use std::cmp::Ordering;
use std::collections::{BTreeMap, BTreeSet};
use std::path::PathBuf;

use anyhow::{Result, bail};
use kasane_core::config::{Config, PluginSelection};
use kasane_plugin_package::manifest::{HOST_ABI_VERSION, abi_compatible};
use kasane_plugin_package::package::{self, InspectedPackage};
use kasane_wasm::{BundledPluginArtifact, bundled_plugin_artifacts};
use semver::Version;

use crate::plugin_lock::{LockedPluginEntry, PluginsLock};
use crate::plugin_store::{PluginStore, StoredArtifact};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum ResolveMode {
    Reconcile,
    Update,
}

#[derive(Debug, Clone)]
pub(super) struct ResolveOptions {
    mode: ResolveMode,
    requested: BTreeMap<String, String>,
}

impl ResolveOptions {
    pub(super) fn reconcile() -> Self {
        Self {
            mode: ResolveMode::Reconcile,
            requested: BTreeMap::new(),
        }
    }

    pub(super) fn update() -> Self {
        Self {
            mode: ResolveMode::Update,
            requested: BTreeMap::new(),
        }
    }

    pub(super) fn request_artifact(
        mut self,
        plugin_id: impl Into<String>,
        digest: impl Into<String>,
    ) -> Self {
        self.requested.insert(plugin_id.into(), digest.into());
        self
    }
}

#[derive(Debug, Clone)]
pub(super) struct ResolveResult {
    pub(super) lock: PluginsLock,
    pub(super) selected: Vec<ResolvedPlugin>,
    pub(super) issues: Vec<ResolutionIssue>,
    pub(super) invalid_packages: Vec<InvalidPackage>,
}

#[derive(Debug, Clone)]
pub(super) struct SavedResolveResult {
    pub(super) result: ResolveResult,
    pub(super) lock_path: PathBuf,
}

#[derive(Debug, Clone)]
pub(super) struct ResolvedPlugin {
    pub(super) plugin_id: String,
    pub(super) package: String,
    pub(super) version: String,
    pub(super) artifact_digest: String,
    pub(super) reason: ResolveReason,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum ResolveReason {
    Requested,
    ExistingLock,
    AutoSingle,
    Updated,
    PinnedDigest,
    PinnedPackage,
    BundledDefault,
    BundledEnabled,
}

#[derive(Debug, Clone)]
pub(super) struct ResolutionIssue {
    pub(super) plugin_id: String,
    pub(super) reason: String,
    pub(super) candidates: Vec<CandidateSummary>,
}

#[derive(Debug, Clone)]
pub(super) struct CandidateSummary {
    pub(super) package: String,
    pub(super) version: String,
    pub(super) artifact_digest: String,
}

#[derive(Debug, Clone)]
pub(super) struct InvalidPackage {
    pub(super) path: PathBuf,
    pub(super) error: String,
}

#[derive(Debug, Clone)]
struct PackageCandidate {
    path: PathBuf,
    inspected: InspectedPackage,
}

pub fn run() -> Result<()> {
    let config = Config::try_load()?;
    let saved = resolve_and_save(&config, ResolveOptions::reconcile())?;
    print_saved_resolution("Resolved plugins", &saved);
    Ok(())
}

pub fn run_update() -> Result<()> {
    let config = Config::try_load()?;
    let old_lock = PluginsLock::load()?;
    let saved = resolve_and_save(&config, ResolveOptions::update())?;
    print_saved_resolution("Updated plugins", &saved);

    let changed = saved
        .result
        .selected
        .iter()
        .filter(|plugin| {
            old_lock
                .plugins
                .get(&plugin.plugin_id)
                .map(|entry| entry.artifact_digest != plugin.artifact_digest)
                .unwrap_or(true)
        })
        .count();
    if changed == 0 {
        println!("No plugin updates.");
    }

    Ok(())
}

pub fn run_pin(
    plugin_id: &str,
    digest: Option<&str>,
    package: Option<&str>,
    version: Option<&str>,
) -> Result<()> {
    let mut config = Config::try_load()?;
    let selection = match (digest, package) {
        (Some(digest), None) => PluginSelection::PinDigest {
            digest: digest.to_string(),
        },
        (None, Some(package)) => PluginSelection::PinPackage {
            package: package.to_string(),
            version: version.map(str::to_string),
        },
        _ => bail!("pin requires exactly one of --digest or --package"),
    };

    config
        .plugins
        .selection
        .insert(plugin_id.to_string(), selection.clone());
    let saved = resolve_and_save(&config, ResolveOptions::reconcile())?;
    require_resolved(&saved.result, plugin_id)?;
    let config_path = config.save()?;

    println!("Pinned plugin: {plugin_id}");
    match selection {
        PluginSelection::PinDigest { digest } => println!("Selection: digest {digest}"),
        PluginSelection::PinPackage { package, version } => match version {
            Some(version) => println!("Selection: package {package}@{version}"),
            None => println!("Selection: package {package}"),
        },
        PluginSelection::Auto => {}
    }
    println!("Config: {}", config_path.display());
    print_saved_resolution("Resolved plugins", &saved);
    Ok(())
}

pub fn run_unpin(plugin_id: &str) -> Result<()> {
    let mut config = Config::try_load()?;
    config.plugins.selection.remove(plugin_id);
    let saved = resolve_and_save(&config, ResolveOptions::reconcile())?;
    let config_path = config.save()?;

    println!("Unpinned plugin: {plugin_id}");
    println!("Config: {}", config_path.display());
    print_saved_resolution("Resolved plugins", &saved);
    Ok(())
}

pub(super) fn resolve_and_save(
    config: &Config,
    options: ResolveOptions,
) -> Result<SavedResolveResult> {
    let _guard = crate::workspace_lock::acquire_workspace_lock(&config.plugins.plugins_dir())?;
    let result = preview_resolution(config, options)?;
    // Partial resolution: save successfully resolved plugins even when some
    // plugins have issues. Callers that require specific plugins to succeed
    // (e.g., pin, install) check for their specific issues after this call.
    let lock_path = result.lock.save()?;
    super::package_artifact::touch_reload_sentinel(&config.plugins.plugins_dir());
    Ok(SavedResolveResult { result, lock_path })
}

/// Check if a specific plugin has unresolved issues and bail if so.
pub(super) fn require_resolved(result: &ResolveResult, plugin_id: &str) -> Result<()> {
    for issue in &result.issues {
        if issue.plugin_id == plugin_id {
            bail!("failed to resolve plugin `{plugin_id}`: {}", issue.reason);
        }
    }
    Ok(())
}

pub(super) fn preview_resolution(
    config: &Config,
    options: ResolveOptions,
) -> Result<ResolveResult> {
    let existing_lock = PluginsLock::load()?;
    resolve_with_existing_lock(config, options, &existing_lock)
}

fn resolve_with_existing_lock(
    config: &Config,
    options: ResolveOptions,
    existing_lock: &PluginsLock,
) -> Result<ResolveResult> {
    let store = PluginStore::from_plugins_dir(config.plugins.plugins_dir());
    let package_paths = store.discover_package_paths()?;

    let mut invalid_packages = Vec::new();
    let mut grouped: BTreeMap<String, Vec<PackageCandidate>> = BTreeMap::new();
    for path in package_paths {
        match package::inspect_package_file(&path) {
            Ok(inspected) => {
                grouped
                    .entry(inspected.header.plugin.id.clone())
                    .or_default()
                    .push(PackageCandidate { path, inspected });
            }
            Err(err) => invalid_packages.push(InvalidPackage {
                path,
                error: err.to_string(),
            }),
        }
    }

    for candidates in grouped.values_mut() {
        candidates.retain(|candidate| {
            let abi = &candidate.inspected.header.plugin.abi_version;
            if abi_compatible(abi, HOST_ABI_VERSION) {
                true
            } else {
                invalid_packages.push(InvalidPackage {
                    path: candidate.path.clone(),
                    error: format!(
                        "ABI version {abi} is incompatible with host ABI {HOST_ABI_VERSION}"
                    ),
                });
                false
            }
        });
        candidates.sort_by(|left, right| left.path.cmp(&right.path));
    }
    grouped.retain(|_, candidates| !candidates.is_empty());

    let mut lock = PluginsLock::new();
    for (plugin_id, entry) in &existing_lock.plugins {
        if entry.source_kind != "filesystem" && entry.source_kind != "bundled" {
            lock.plugins.insert(plugin_id.clone(), entry.clone());
        }
    }

    let mut selected = Vec::new();
    let mut issues = Vec::new();
    for (plugin_id, candidates) in &grouped {
        if let Some(digest) = options.requested.get(plugin_id) {
            match candidates
                .iter()
                .find(|candidate| candidate.inspected.header.digests.artifact == *digest)
            {
                Some(candidate) => select_candidate(
                    &mut lock,
                    &mut selected,
                    candidate,
                    ResolveReason::Requested,
                ),
                None => issues.push(ResolutionIssue {
                    plugin_id: plugin_id.clone(),
                    reason: format!("requested artifact `{digest}` is not installed"),
                    candidates: candidate_summaries(candidates),
                }),
            }
            continue;
        }

        if config.plugins.is_disabled(plugin_id) {
            continue;
        }

        let existing = existing_lock.plugins.get(plugin_id);
        match config.plugins.selection_for(plugin_id) {
            PluginSelection::Auto => {
                match resolve_auto(plugin_id, candidates, existing, options.mode) {
                    Ok(Some((candidate, reason))) => {
                        select_candidate(&mut lock, &mut selected, candidate, reason)
                    }
                    Ok(None) => {}
                    Err(issue) => issues.push(issue),
                }
            }
            PluginSelection::PinDigest { digest } => {
                match candidates
                    .iter()
                    .find(|candidate| candidate.inspected.header.digests.artifact == digest)
                {
                    Some(candidate) => select_candidate(
                        &mut lock,
                        &mut selected,
                        candidate,
                        ResolveReason::PinnedDigest,
                    ),
                    None => issues.push(ResolutionIssue {
                        plugin_id: plugin_id.clone(),
                        reason: format!("pinned digest `{digest}` is not installed"),
                        candidates: candidate_summaries(candidates),
                    }),
                }
            }
            PluginSelection::PinPackage { package, version } => {
                let matching: Vec<_> = candidates
                    .iter()
                    .filter(|candidate| candidate.inspected.header.package.name == package)
                    .collect();
                if matching.is_empty() {
                    issues.push(ResolutionIssue {
                        plugin_id: plugin_id.clone(),
                        reason: match &version {
                            Some(version) => {
                                format!("pinned package `{package}@{version}` is not installed")
                            }
                            None => format!("pinned package `{package}` is not installed"),
                        },
                        candidates: candidate_summaries(candidates),
                    });
                    continue;
                }

                let filtered: Vec<_> = match version.as_deref() {
                    Some(version) => matching
                        .into_iter()
                        .filter(|candidate| candidate.inspected.header.package.version == version)
                        .collect(),
                    None => matching,
                };
                if filtered.is_empty() {
                    issues.push(ResolutionIssue {
                        plugin_id: plugin_id.clone(),
                        reason: format!("pinned package `{package}` is installed, but requested version is missing"),
                        candidates: candidate_summaries(candidates),
                    });
                    continue;
                }

                match latest_unique_candidate(plugin_id, filtered, "pinned package selection") {
                    Ok(candidate) => select_candidate(
                        &mut lock,
                        &mut selected,
                        candidate,
                        ResolveReason::PinnedPackage,
                    ),
                    Err(issue) => issues.push(issue),
                }
            }
        }
    }

    for (plugin_id, digest) in &options.requested {
        if grouped.contains_key(plugin_id) {
            continue;
        }
        issues.push(ResolutionIssue {
            plugin_id: plugin_id.clone(),
            reason: format!("requested artifact `{digest}` is not installed"),
            candidates: Vec::new(),
        });
    }

    for (plugin_id, selection) in &config.plugins.selection {
        if grouped.contains_key(plugin_id) || config.plugins.is_disabled(plugin_id) {
            continue;
        }
        match selection {
            PluginSelection::Auto => {}
            PluginSelection::PinDigest { digest } => issues.push(ResolutionIssue {
                plugin_id: plugin_id.clone(),
                reason: format!("pinned digest `{digest}` is not installed"),
                candidates: Vec::new(),
            }),
            PluginSelection::PinPackage { package, version } => issues.push(ResolutionIssue {
                plugin_id: plugin_id.clone(),
                reason: match version {
                    Some(version) => {
                        format!("pinned package `{package}@{version}` is not installed")
                    }
                    None => format!("pinned package `{package}` is not installed"),
                },
                candidates: Vec::new(),
            }),
        }
    }

    let issue_ids: BTreeSet<_> = issues.iter().map(|issue| issue.plugin_id.clone()).collect();
    append_bundled_fallbacks(config, &mut lock, &mut selected, &issue_ids)?;

    selected.sort_by(|left, right| left.plugin_id.cmp(&right.plugin_id));
    invalid_packages.sort_by(|left, right| left.path.cmp(&right.path));

    Ok(ResolveResult {
        lock,
        selected,
        issues,
        invalid_packages,
    })
}

fn resolve_auto<'a>(
    plugin_id: &str,
    candidates: &'a [PackageCandidate],
    existing: Option<&LockedPluginEntry>,
    mode: ResolveMode,
) -> std::result::Result<Option<(&'a PackageCandidate, ResolveReason)>, ResolutionIssue> {
    if candidates.is_empty() {
        return Ok(None);
    }

    if mode == ResolveMode::Reconcile
        && let Some(existing) = existing
        && let Some(candidate) = find_candidate_by_digest(candidates, &existing.artifact_digest)
    {
        return Ok(Some((candidate, ResolveReason::ExistingLock)));
    }

    if candidates.len() == 1 {
        let reason = if mode == ResolveMode::Update {
            ResolveReason::Updated
        } else {
            ResolveReason::AutoSingle
        };
        return Ok(Some((&candidates[0], reason)));
    }

    if mode == ResolveMode::Update
        && let Some(existing) = existing
    {
        let lineage: Vec<_> = match &existing.package {
            Some(package_name) => candidates
                .iter()
                .filter(|candidate| candidate.inspected.header.package.name == *package_name)
                .collect(),
            None => Vec::new(),
        };
        if !lineage.is_empty() {
            let candidate = latest_unique_candidate(plugin_id, lineage, "update target")?;
            let reason = if candidate.inspected.header.digests.artifact == existing.artifact_digest
            {
                ResolveReason::ExistingLock
            } else {
                ResolveReason::Updated
            };
            return Ok(Some((candidate, reason)));
        }
    }

    Err(ResolutionIssue {
        plugin_id: plugin_id.to_string(),
        reason: "multiple installed packages provide the same plugin id".to_string(),
        candidates: candidate_summaries(candidates),
    })
}

fn select_candidate(
    lock: &mut PluginsLock,
    selected: &mut Vec<ResolvedPlugin>,
    candidate: &PackageCandidate,
    reason: ResolveReason,
) {
    let stored = StoredArtifact::from_inspected(candidate.path.clone(), &candidate.inspected);
    lock.plugins.insert(
        stored.plugin_id.clone(),
        LockedPluginEntry {
            plugin_id: stored.plugin_id.clone(),
            package: Some(stored.package_name.clone()),
            version: Some(stored.package_version.clone()),
            artifact_digest: stored.artifact_digest.clone(),
            code_digest: Some(stored.code_digest.clone()),
            source_kind: "filesystem".to_string(),
            abi_version: Some(stored.abi_version.clone()),
        },
    );
    selected.push(ResolvedPlugin {
        plugin_id: stored.plugin_id,
        package: stored.package_name,
        version: stored.package_version,
        artifact_digest: stored.artifact_digest,
        reason,
    });
}

fn append_bundled_fallbacks(
    config: &Config,
    lock: &mut PluginsLock,
    selected: &mut Vec<ResolvedPlugin>,
    issue_ids: &BTreeSet<String>,
) -> Result<()> {
    let mut bundled = bundled_plugin_artifacts()?;
    bundled.sort_by(|left, right| left.plugin_id.cmp(&right.plugin_id));

    for artifact in bundled {
        if !should_enable_bundled(config, &artifact) {
            continue;
        }
        if issue_ids.contains(&artifact.plugin_id) {
            continue;
        }
        if lock.plugins.contains_key(&artifact.plugin_id) {
            continue;
        }

        lock.plugins.insert(
            artifact.plugin_id.clone(),
            LockedPluginEntry {
                plugin_id: artifact.plugin_id.clone(),
                package: Some(artifact.package_name.clone()),
                version: Some(artifact.package_version.clone()),
                artifact_digest: artifact.artifact_digest.clone(),
                code_digest: Some(artifact.code_digest.clone()),
                source_kind: "bundled".to_string(),
                abi_version: Some(artifact.abi_version.clone()),
            },
        );
        selected.push(ResolvedPlugin {
            plugin_id: artifact.plugin_id,
            package: artifact.package_name,
            version: artifact.package_version,
            artifact_digest: artifact.artifact_digest,
            reason: if artifact.default_enabled {
                ResolveReason::BundledDefault
            } else {
                ResolveReason::BundledEnabled
            },
        });
    }

    Ok(())
}

fn should_enable_bundled(config: &Config, artifact: &BundledPluginArtifact) -> bool {
    let disabled = config.plugins.is_disabled(artifact.name)
        || config.plugins.is_disabled(&artifact.plugin_id);
    if disabled {
        return false;
    }

    artifact.default_enabled
        || config.plugins.is_bundled_enabled(artifact.name)
        || config.plugins.is_bundled_enabled(&artifact.plugin_id)
}

fn find_candidate_by_digest<'a>(
    candidates: &'a [PackageCandidate],
    digest: &str,
) -> Option<&'a PackageCandidate> {
    candidates
        .iter()
        .find(|candidate| candidate.inspected.header.digests.artifact == digest)
}

fn latest_unique_candidate<'a>(
    plugin_id: &str,
    candidates: Vec<&'a PackageCandidate>,
    context: &str,
) -> std::result::Result<&'a PackageCandidate, ResolutionIssue> {
    let mut candidates = candidates;
    candidates.sort_by(|left, right| compare_candidate_versions(left, right).reverse());

    let Some(best) = candidates.first().copied() else {
        return Err(ResolutionIssue {
            plugin_id: plugin_id.to_string(),
            reason: format!("no candidates available for {context}"),
            candidates: Vec::new(),
        });
    };
    if let Some(second) = candidates.get(1)
        && compare_candidate_versions(best, second) == Ordering::Equal
    {
        return Err(ResolutionIssue {
            plugin_id: plugin_id.to_string(),
            reason: format!("cannot choose a unique latest package for {context}"),
            candidates: candidate_summaries(candidates.iter().copied()),
        });
    }

    Ok(best)
}

fn compare_candidate_versions(left: &PackageCandidate, right: &PackageCandidate) -> Ordering {
    let left_version = Version::parse(&left.inspected.header.package.version).ok();
    let right_version = Version::parse(&right.inspected.header.package.version).ok();
    match (left_version, right_version) {
        (Some(left_version), Some(right_version)) => left_version
            .cmp(&right_version)
            .then_with(|| {
                left.inspected
                    .header
                    .package
                    .name
                    .cmp(&right.inspected.header.package.name)
            })
            .then_with(|| {
                left.inspected
                    .header
                    .digests
                    .artifact
                    .cmp(&right.inspected.header.digests.artifact)
            }),
        (Some(_), None) => Ordering::Greater,
        (None, Some(_)) => Ordering::Less,
        (None, None) => left
            .inspected
            .header
            .package
            .version
            .cmp(&right.inspected.header.package.version)
            .then_with(|| {
                left.inspected
                    .header
                    .package
                    .name
                    .cmp(&right.inspected.header.package.name)
            })
            .then_with(|| {
                left.inspected
                    .header
                    .digests
                    .artifact
                    .cmp(&right.inspected.header.digests.artifact)
            }),
    }
}

fn candidate_summaries<'a>(
    candidates: impl IntoIterator<Item = &'a PackageCandidate>,
) -> Vec<CandidateSummary> {
    candidates
        .into_iter()
        .map(|candidate| CandidateSummary {
            package: candidate.inspected.header.package.name.clone(),
            version: candidate.inspected.header.package.version.clone(),
            artifact_digest: candidate.inspected.header.digests.artifact.clone(),
        })
        .collect()
}

fn print_saved_resolution(header: &str, saved: &SavedResolveResult) {
    println!("{header}:");
    if saved.result.selected.is_empty() {
        println!("  (no plugins selected)");
    } else {
        for plugin in &saved.result.selected {
            println!(
                "  {} -> {}@{} ({})",
                plugin.plugin_id,
                plugin.package,
                plugin.version,
                match plugin.reason {
                    ResolveReason::Requested => "requested",
                    ResolveReason::ExistingLock => "locked",
                    ResolveReason::AutoSingle => "auto",
                    ResolveReason::Updated => "updated",
                    ResolveReason::PinnedDigest => "pinned-digest",
                    ResolveReason::PinnedPackage => "pinned-package",
                    ResolveReason::BundledDefault => "bundled-default",
                    ResolveReason::BundledEnabled => "bundled-enabled",
                }
            );
        }
    }
    if !saved.result.issues.is_empty() {
        println!("Warnings:");
        for issue in &saved.result.issues {
            println!("  {}: {}", issue.plugin_id, issue.reason);
            for candidate in &issue.candidates {
                println!(
                    "    - {}@{} ({})",
                    candidate.package, candidate.version, candidate.artifact_digest
                );
            }
        }
    }
    if !saved.result.invalid_packages.is_empty() {
        println!("Invalid packages:");
        for invalid in &saved.result.invalid_packages {
            println!("  {} ({})", invalid.path.display(), invalid.error);
        }
    }
    println!("Lock: {}", saved.lock_path.display());
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::path::Path;

    use kasane_plugin_package::manifest::PluginManifest;
    use kasane_plugin_package::package::{BuildInput, build_package_unchecked, write_package};

    fn build_fixture_package(
        root: &Path,
        plugin_id: &str,
        package_name: &str,
        version: &str,
    ) -> std::path::PathBuf {
        let source_path = root.join(format!("{plugin_id}-{version}.kpk"));
        let manifest = PluginManifest::parse(&format!(
            r#"
[plugin]
id = "{plugin_id}"
abi_version = "0.25.0"

[handlers]
flags = ["contributor"]
"#
        ))
        .unwrap();
        let output = package::build_package(BuildInput {
            package_name: package_name.to_string(),
            package_version: version.to_string(),
            component_entry: "plugin.wasm".to_string(),
            component: format!("component-{plugin_id}-{version}").into_bytes(),
            manifest,
            assets: Vec::new(),
        })
        .unwrap();
        write_package(&source_path, &output).unwrap();
        source_path
    }

    fn config_with_paths(data_home: &Path, _config_home: &Path) -> Config {
        let mut config = Config::default();
        config.plugins.path = Some(data_home.join("plugins").display().to_string());
        config
    }

    #[test]
    fn reconcile_keeps_existing_lock_when_valid() {
        let tmp = tempfile::tempdir().unwrap();
        let data_home = tmp.path().join("data");
        let config_home = tmp.path().join("config");
        fs::create_dir_all(&data_home).unwrap();
        fs::create_dir_all(&config_home).unwrap();

        let config = config_with_paths(&data_home, &config_home);
        let store = PluginStore::from_plugins_dir(config.plugins.plugins_dir());
        let old_path = build_fixture_package(tmp.path(), "sel_badge", "example/sel-badge", "0.1.0");
        let new_path = build_fixture_package(tmp.path(), "sel_badge", "example/sel-badge", "0.2.0");
        let old = store.put_verified_package(&old_path).unwrap();
        store.put_verified_package(&new_path).unwrap();

        let mut lock = PluginsLock::new();
        lock.plugins.insert(
            "sel_badge".to_string(),
            LockedPluginEntry {
                plugin_id: "sel_badge".to_string(),
                package: Some("example/sel-badge".to_string()),
                version: Some("0.1.0".to_string()),
                artifact_digest: old.artifact_digest.clone(),
                code_digest: Some(old.code_digest.clone()),
                source_kind: "filesystem".to_string(),
                abi_version: Some(old.abi_version.clone()),
            },
        );
        let result =
            resolve_with_existing_lock(&config, ResolveOptions::reconcile(), &lock).unwrap();
        assert!(result.issues.is_empty());
        let sel_badge = result
            .selected
            .iter()
            .find(|plugin| plugin.plugin_id == "sel_badge")
            .unwrap();
        assert_eq!(sel_badge.artifact_digest, old.artifact_digest);
        assert_eq!(sel_badge.reason, ResolveReason::ExistingLock);
    }

    #[test]
    fn reconcile_reports_conflict_without_lock_or_pin() {
        let tmp = tempfile::tempdir().unwrap();
        let data_home = tmp.path().join("data");
        let config_home = tmp.path().join("config");
        fs::create_dir_all(&data_home).unwrap();
        fs::create_dir_all(&config_home).unwrap();

        let config = config_with_paths(&data_home, &config_home);
        let store = PluginStore::from_plugins_dir(config.plugins.plugins_dir());
        let one = build_fixture_package(tmp.path(), "sel_badge", "example/sel-badge", "0.1.0");
        let two = build_fixture_package(tmp.path(), "sel_badge", "example/sel-badge", "0.2.0");
        store.put_verified_package(&one).unwrap();
        store.put_verified_package(&two).unwrap();

        let result =
            resolve_with_existing_lock(&config, ResolveOptions::reconcile(), &PluginsLock::new())
                .unwrap();
        assert_eq!(result.issues.len(), 1);
        assert_eq!(result.issues[0].plugin_id, "sel_badge");
        assert!(!result.lock.plugins.contains_key("sel_badge"));
    }

    #[test]
    fn update_advances_auto_selection_with_existing_lock() {
        let tmp = tempfile::tempdir().unwrap();
        let data_home = tmp.path().join("data");
        let config_home = tmp.path().join("config");
        fs::create_dir_all(&data_home).unwrap();
        fs::create_dir_all(&config_home).unwrap();

        let config = config_with_paths(&data_home, &config_home);
        let store = PluginStore::from_plugins_dir(config.plugins.plugins_dir());
        let old_path = build_fixture_package(tmp.path(), "sel_badge", "example/sel-badge", "0.1.0");
        let new_path = build_fixture_package(tmp.path(), "sel_badge", "example/sel-badge", "0.2.0");
        let old = store.put_verified_package(&old_path).unwrap();
        let new = store.put_verified_package(&new_path).unwrap();

        let mut lock = PluginsLock::new();
        lock.plugins.insert(
            "sel_badge".to_string(),
            LockedPluginEntry {
                plugin_id: "sel_badge".to_string(),
                package: Some("example/sel-badge".to_string()),
                version: Some("0.1.0".to_string()),
                artifact_digest: old.artifact_digest.clone(),
                code_digest: Some(old.code_digest.clone()),
                source_kind: "filesystem".to_string(),
                abi_version: Some(old.abi_version.clone()),
            },
        );
        let result = resolve_with_existing_lock(&config, ResolveOptions::update(), &lock).unwrap();
        assert!(result.issues.is_empty());
        let sel_badge = result
            .selected
            .iter()
            .find(|plugin| plugin.plugin_id == "sel_badge")
            .unwrap();
        assert_eq!(sel_badge.artifact_digest, new.artifact_digest);
        assert_eq!(sel_badge.reason, ResolveReason::Updated);
    }

    #[test]
    fn pin_digest_selects_requested_artifact() {
        let tmp = tempfile::tempdir().unwrap();
        let data_home = tmp.path().join("data");
        let config_home = tmp.path().join("config");
        fs::create_dir_all(&data_home).unwrap();
        fs::create_dir_all(&config_home).unwrap();

        let mut config = config_with_paths(&data_home, &config_home);
        let store = PluginStore::from_plugins_dir(config.plugins.plugins_dir());
        let old_path = build_fixture_package(tmp.path(), "sel_badge", "example/sel-badge", "0.1.0");
        let new_path = build_fixture_package(tmp.path(), "sel_badge", "example/sel-badge", "0.2.0");
        let old = store.put_verified_package(&old_path).unwrap();
        store.put_verified_package(&new_path).unwrap();

        config.plugins.selection.insert(
            "sel_badge".to_string(),
            PluginSelection::PinDigest {
                digest: old.artifact_digest.clone(),
            },
        );

        let result =
            resolve_with_existing_lock(&config, ResolveOptions::reconcile(), &PluginsLock::new())
                .unwrap();
        assert!(result.issues.is_empty());
        let sel_badge = result
            .selected
            .iter()
            .find(|plugin| plugin.plugin_id == "sel_badge")
            .unwrap();
        assert_eq!(sel_badge.artifact_digest, old.artifact_digest);
        assert_eq!(sel_badge.reason, ResolveReason::PinnedDigest);
    }

    #[test]
    fn reconcile_adds_default_enabled_bundled_plugin() {
        let tmp = tempfile::tempdir().unwrap();
        let data_home = tmp.path().join("data");
        let config_home = tmp.path().join("config");
        fs::create_dir_all(&data_home).unwrap();
        fs::create_dir_all(&config_home).unwrap();

        let config = config_with_paths(&data_home, &config_home);
        let result =
            resolve_with_existing_lock(&config, ResolveOptions::reconcile(), &PluginsLock::new())
                .unwrap();

        let pane_manager = result.lock.plugins.get("pane_manager").unwrap();
        assert_eq!(pane_manager.source_kind, "bundled");
        assert!(
            result
                .selected
                .iter()
                .any(|plugin| plugin.plugin_id == "pane_manager"
                    && plugin.reason == ResolveReason::BundledDefault)
        );
    }

    fn build_fixture_package_with_abi(
        root: &Path,
        plugin_id: &str,
        package_name: &str,
        version: &str,
        abi_version: &str,
    ) -> std::path::PathBuf {
        let source_path = root.join(format!("{plugin_id}-{version}-abi{abi_version}.kpk"));
        let manifest = PluginManifest::parse(&format!(
            r#"
[plugin]
id = "{plugin_id}"
abi_version = "{abi_version}"

[handlers]
flags = ["contributor"]
"#
        ))
        .unwrap();
        let output = build_package_unchecked(BuildInput {
            package_name: package_name.to_string(),
            package_version: version.to_string(),
            component_entry: "plugin.wasm".to_string(),
            component: format!("component-{plugin_id}-{version}-{abi_version}").into_bytes(),
            manifest,
            assets: Vec::new(),
        })
        .unwrap();
        write_package(&source_path, &output).unwrap();
        source_path
    }

    #[test]
    fn abi_incompatible_candidates_are_filtered_out() {
        let tmp = tempfile::tempdir().unwrap();
        let data_home = tmp.path().join("data");
        let config_home = tmp.path().join("config");
        fs::create_dir_all(&data_home).unwrap();
        fs::create_dir_all(&config_home).unwrap();

        let config = config_with_paths(&data_home, &config_home);
        let store = PluginStore::from_plugins_dir(config.plugins.plugins_dir());

        // Install a compatible package
        let compatible =
            build_fixture_package(tmp.path(), "sel_badge", "example/sel-badge", "0.1.0");
        store.put_verified_package(&compatible).unwrap();

        // Install an incompatible ABI package
        let incompatible = build_fixture_package_with_abi(
            tmp.path(),
            "incompat_plugin",
            "example/incompat",
            "0.1.0",
            "0.99.0",
        );
        store.put_verified_package(&incompatible).unwrap();

        let result =
            resolve_with_existing_lock(&config, ResolveOptions::reconcile(), &PluginsLock::new())
                .unwrap();

        // Compatible plugin should be resolved
        assert!(result.selected.iter().any(|p| p.plugin_id == "sel_badge"));

        // Incompatible plugin should NOT be resolved, and should appear in invalid_packages
        assert!(
            !result
                .selected
                .iter()
                .any(|p| p.plugin_id == "incompat_plugin")
        );
        assert!(
            result
                .invalid_packages
                .iter()
                .any(|p| p.error.contains("ABI version"))
        );
    }

    #[test]
    fn partial_resolution_saves_resolved_plugins_despite_issues() {
        let tmp = tempfile::tempdir().unwrap();
        let data_home = tmp.path().join("data");
        let config_home = tmp.path().join("config");
        fs::create_dir_all(&data_home).unwrap();
        fs::create_dir_all(&config_home).unwrap();

        let config = config_with_paths(&data_home, &config_home);
        let store = PluginStore::from_plugins_dir(config.plugins.plugins_dir());

        // sel_badge: single candidate → resolves fine
        let good = build_fixture_package(tmp.path(), "sel_badge", "example/sel-badge", "0.1.0");
        store.put_verified_package(&good).unwrap();

        // cursor_line: two candidates, no lock → ambiguity issue
        let conflict_a =
            build_fixture_package(tmp.path(), "cursor_line", "example/cursor-line", "0.1.0");
        let conflict_b =
            build_fixture_package(tmp.path(), "cursor_line", "example/cursor-line", "0.2.0");
        store.put_verified_package(&conflict_a).unwrap();
        store.put_verified_package(&conflict_b).unwrap();

        let result =
            resolve_with_existing_lock(&config, ResolveOptions::reconcile(), &PluginsLock::new())
                .unwrap();

        // cursor_line has an issue
        assert!(
            result
                .issues
                .iter()
                .any(|issue| issue.plugin_id == "cursor_line")
        );

        // sel_badge is still in the lock despite cursor_line's issue
        assert!(result.lock.plugins.contains_key("sel_badge"));
        assert!(result.selected.iter().any(|p| p.plugin_id == "sel_badge"));

        // cursor_line is NOT in the lock
        assert!(!result.lock.plugins.contains_key("cursor_line"));
    }

    #[test]
    fn bundled_fallback_does_not_mask_filesystem_conflict() {
        let tmp = tempfile::tempdir().unwrap();
        let data_home = tmp.path().join("data");
        let config_home = tmp.path().join("config");
        fs::create_dir_all(&data_home).unwrap();
        fs::create_dir_all(&config_home).unwrap();

        let config = config_with_paths(&data_home, &config_home);
        let store = PluginStore::from_plugins_dir(config.plugins.plugins_dir());
        let one =
            build_fixture_package(tmp.path(), "pane_manager", "example/pane-manager", "0.1.0");
        let two =
            build_fixture_package(tmp.path(), "pane_manager", "example/pane-manager", "0.2.0");
        store.put_verified_package(&one).unwrap();
        store.put_verified_package(&two).unwrap();

        let result =
            resolve_with_existing_lock(&config, ResolveOptions::reconcile(), &PluginsLock::new())
                .unwrap();
        assert!(
            result
                .issues
                .iter()
                .any(|issue| issue.plugin_id == "pane_manager")
        );
        assert!(!result.lock.plugins.contains_key("pane_manager"));
    }
}
