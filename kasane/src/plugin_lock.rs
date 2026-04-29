use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use anyhow::{Context, Result, bail};
use serde::{Deserialize, Serialize};

const PLUGINS_LOCK_VERSION: u32 = 1;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PluginsLock {
    #[serde(default = "plugins_lock_version")]
    pub version: u32,
    #[serde(default)]
    pub plugins: BTreeMap<String, LockedPluginEntry>,
}

impl Default for PluginsLock {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LockedPluginEntry {
    pub plugin_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub package: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub version: Option<String>,
    pub artifact_digest: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub code_digest: Option<String>,
    pub source_kind: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub abi_version: Option<String>,
}

impl PluginsLock {
    pub fn new() -> Self {
        Self {
            version: plugins_lock_version(),
            plugins: BTreeMap::new(),
        }
    }

    pub fn load() -> Result<Self> {
        Self::load_from_path(plugins_lock_path())
    }

    pub fn load_from_path(path: impl AsRef<Path>) -> Result<Self> {
        let path = path.as_ref();
        let contents = match fs::read_to_string(path) {
            Ok(contents) => contents,
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => return Ok(Self::new()),
            Err(err) => {
                return Err(err).with_context(|| format!("failed to read {}", path.display()));
            }
        };
        let lock: Self = toml::from_str(&contents)
            .with_context(|| format!("failed to parse {}", path.display()))?;
        lock.validate()
            .with_context(|| format!("failed to validate {}", path.display()))?;
        Ok(lock)
    }

    pub fn save(&self) -> Result<PathBuf> {
        let path = plugins_lock_path();
        self.save_to_path(&path)?;
        Ok(path)
    }

    pub fn save_to_path(&self, path: impl AsRef<Path>) -> Result<()> {
        self.validate()
            .context("failed to save invalid plugins lock")?;

        let path = path.as_ref();
        if let Some(parent) = path.parent()
            && !parent.as_os_str().is_empty()
        {
            fs::create_dir_all(parent)
                .with_context(|| format!("failed to create {}", parent.display()))?;
        }

        let normalized = self.normalized();
        let contents =
            toml::to_string_pretty(&normalized).context("failed to serialize plugins lock")?;
        let existing_contents = match fs::read_to_string(path) {
            Ok(contents) => Some(contents),
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => None,
            Err(err) => {
                return Err(err).with_context(|| format!("failed to read {}", path.display()));
            }
        };
        if existing_contents.as_deref() == Some(contents.as_str()) {
            return Ok(());
        }

        let temp_path = temp_lock_path(path);
        fs::write(&temp_path, contents)
            .with_context(|| format!("failed to write {}", temp_path.display()))?;
        if existing_contents.is_some() {
            archive_lock_generation(path)?;
        }
        fs::rename(&temp_path, path).with_context(|| {
            format!(
                "failed to atomically replace {} with {}",
                path.display(),
                temp_path.display()
            )
        })?;
        Ok(())
    }

    fn validate(&self) -> Result<()> {
        if self.version != plugins_lock_version() {
            bail!(
                "unsupported plugins.lock version {} (expected {})",
                self.version,
                plugins_lock_version()
            );
        }

        for (key, entry) in &self.plugins {
            if entry.plugin_id.is_empty() {
                bail!("plugin entry `{key}` has an empty plugin_id");
            }
            if key != &entry.plugin_id {
                bail!(
                    "plugin entry `{key}` does not match nested plugin_id `{}`",
                    entry.plugin_id
                );
            }
            if entry.artifact_digest.is_empty() {
                bail!("plugin entry `{key}` has an empty artifact_digest");
            }
            if entry.source_kind.is_empty() {
                bail!("plugin entry `{key}` has an empty source_kind");
            }
        }

        Ok(())
    }

    /// Return a copy with derived fields stripped from each entry.
    /// Only independent fields (plugin_id, artifact_digest, source_kind)
    /// are preserved. Derived fields (package, version, code_digest,
    /// abi_version) are set to None.
    fn normalized(&self) -> Self {
        let plugins = self
            .plugins
            .iter()
            .map(|(key, entry)| {
                (
                    key.clone(),
                    LockedPluginEntry {
                        plugin_id: entry.plugin_id.clone(),
                        package: None,
                        version: None,
                        artifact_digest: entry.artifact_digest.clone(),
                        code_digest: None,
                        source_kind: entry.source_kind.clone(),
                        abi_version: None,
                    },
                )
            })
            .collect();
        Self {
            version: self.version,
            plugins,
        }
    }
}

pub fn plugins_lock_path() -> PathBuf {
    plugins_lock_path_from_env(
        std::env::var_os("XDG_CONFIG_HOME").map(PathBuf::from),
        std::env::var_os("HOME").map(PathBuf::from),
    )
}

pub fn plugins_lock_history_dir() -> PathBuf {
    plugins_lock_history_dir_from_path(&plugins_lock_path())
}

pub fn latest_plugins_lock_history_path() -> Result<Option<PathBuf>> {
    latest_plugins_lock_history_path_from_path(plugins_lock_path())
}

pub fn plugins_lock_history_paths() -> Result<Vec<PathBuf>> {
    plugins_lock_history_paths_from_path(plugins_lock_path())
}

pub fn rollback_plugins_lock() -> Result<Option<PathBuf>> {
    rollback_plugins_lock_from_path(plugins_lock_path())
}

pub fn prune_plugins_lock_history(keep: usize) -> Result<Vec<PathBuf>> {
    prune_plugins_lock_history_from_path(plugins_lock_path(), keep)
}

fn plugins_lock_version() -> u32 {
    PLUGINS_LOCK_VERSION
}

fn plugins_lock_path_from_env(xdg_config_home: Option<PathBuf>, home: Option<PathBuf>) -> PathBuf {
    if let Some(xdg) = xdg_config_home {
        xdg.join("kasane").join("plugins.lock")
    } else if let Some(home) = home {
        home.join(".config").join("kasane").join("plugins.lock")
    } else {
        PathBuf::from("plugins.lock")
    }
}

fn plugins_lock_history_dir_from_path(path: &Path) -> PathBuf {
    if let Some(parent) = path.parent() {
        parent.join("locks")
    } else {
        PathBuf::from("locks")
    }
}

fn temp_lock_path(path: &Path) -> PathBuf {
    let file_name = path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("plugins.lock");
    let stamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    let pid = std::process::id();
    path.with_file_name(format!(".{file_name}.{pid}.{stamp}.tmp"))
}

fn history_lock_path(path: &Path) -> PathBuf {
    let history_dir = plugins_lock_history_dir_from_path(path);
    let stamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    let pid = std::process::id();
    history_dir.join(format!("plugins-{stamp:020}-{pid}.lock"))
}

fn archive_lock_generation(path: &Path) -> Result<PathBuf> {
    let history_path = history_lock_path(path);
    if let Some(parent) = history_path.parent() {
        fs::create_dir_all(parent)
            .with_context(|| format!("failed to create {}", parent.display()))?;
    }
    fs::copy(path, &history_path).with_context(|| {
        format!(
            "failed to archive {} to {}",
            path.display(),
            history_path.display()
        )
    })?;
    Ok(history_path)
}

fn plugins_lock_history_paths_from_path(path: impl AsRef<Path>) -> Result<Vec<PathBuf>> {
    let history_dir = plugins_lock_history_dir_from_path(path.as_ref());
    let entries = match fs::read_dir(&history_dir) {
        Ok(entries) => entries,
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(err) => {
            return Err(err).with_context(|| format!("failed to read {}", history_dir.display()));
        }
    };

    let mut paths = Vec::new();
    for entry in entries {
        let entry = entry.with_context(|| format!("failed to read {}", history_dir.display()))?;
        let path = entry.path();
        let file_type = entry
            .file_type()
            .with_context(|| format!("failed to inspect {}", path.display()))?;
        if file_type.is_file() && path.extension().and_then(|ext| ext.to_str()) == Some("lock") {
            paths.push(path);
        }
    }
    paths.sort();
    Ok(paths)
}

fn latest_plugins_lock_history_path_from_path(path: impl AsRef<Path>) -> Result<Option<PathBuf>> {
    let mut paths = plugins_lock_history_paths_from_path(path)?;
    Ok(paths.pop())
}

fn rollback_plugins_lock_from_path(path: impl AsRef<Path>) -> Result<Option<PathBuf>> {
    let path = path.as_ref();
    let Some(previous) = latest_plugins_lock_history_path_from_path(path)? else {
        return Ok(None);
    };

    if path.exists() {
        archive_lock_generation(path)?;
    } else if let Some(parent) = path.parent()
        && !parent.as_os_str().is_empty()
    {
        fs::create_dir_all(parent)
            .with_context(|| format!("failed to create {}", parent.display()))?;
    }

    let temp_path = temp_lock_path(path);
    fs::copy(&previous, &temp_path).with_context(|| {
        format!(
            "failed to copy {} to {}",
            previous.display(),
            temp_path.display()
        )
    })?;
    fs::rename(&temp_path, path).with_context(|| {
        format!(
            "failed to atomically restore {} from {}",
            path.display(),
            previous.display()
        )
    })?;
    Ok(Some(previous))
}

fn prune_plugins_lock_history_from_path(
    path: impl AsRef<Path>,
    keep: usize,
) -> Result<Vec<PathBuf>> {
    let mut history_paths = plugins_lock_history_paths_from_path(path)?;
    if history_paths.len() <= keep {
        return Ok(Vec::new());
    }

    let remove_count = history_paths.len() - keep;
    let removed: Vec<_> = history_paths.drain(..remove_count).collect();
    for path in &removed {
        fs::remove_file(path).with_context(|| format!("failed to remove {}", path.display()))?;
    }
    Ok(removed)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plugins_lock_path_prefers_xdg_config_home() {
        let path = plugins_lock_path_from_env(
            Some(PathBuf::from("/tmp/xdg-config")),
            Some(PathBuf::from("/tmp/home")),
        );
        assert_eq!(path, PathBuf::from("/tmp/xdg-config/kasane/plugins.lock"));
    }

    #[test]
    fn plugins_lock_path_falls_back_to_home() {
        let path = plugins_lock_path_from_env(None, Some(PathBuf::from("/tmp/home")));
        assert_eq!(path, PathBuf::from("/tmp/home/.config/kasane/plugins.lock"));
    }

    #[test]
    fn load_missing_plugins_lock_returns_empty_lock() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("missing.lock");

        let lock = PluginsLock::load_from_path(&path).unwrap();
        assert_eq!(lock, PluginsLock::new());
    }

    #[test]
    fn save_and_load_round_trip() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("nested").join("plugins.lock");

        let mut lock = PluginsLock::new();
        lock.plugins.insert(
            "sel_badge".to_string(),
            LockedPluginEntry {
                plugin_id: "sel_badge".to_string(),
                package: Some("yus314/sel-badge".to_string()),
                version: Some("0.1.0".to_string()),
                artifact_digest: "sha256:artifact".to_string(),
                code_digest: Some("sha256:code".to_string()),
                source_kind: "filesystem".to_string(),
                abi_version: Some("1.0.0".to_string()),
            },
        );

        lock.save_to_path(&path).unwrap();
        let loaded = PluginsLock::load_from_path(&path).unwrap();
        // Normalized on save: derived fields are stripped
        assert_eq!(loaded, lock.normalized());
    }

    fn make_lock(plugin_id: &str, digest: &str) -> PluginsLock {
        let mut lock = PluginsLock::new();
        lock.plugins.insert(
            plugin_id.to_string(),
            LockedPluginEntry {
                plugin_id: plugin_id.to_string(),
                package: Some(format!("example/{plugin_id}")),
                version: Some("0.1.0".to_string()),
                artifact_digest: digest.to_string(),
                code_digest: Some(format!("{digest}-code")),
                source_kind: "filesystem".to_string(),
                abi_version: Some("1.0.0".to_string()),
            },
        );
        lock
    }

    #[test]
    fn save_to_path_archives_previous_generation() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("plugins.lock");
        let lock1 = make_lock("sel_badge", "sha256:one");
        let lock2 = make_lock("sel_badge", "sha256:two");

        lock1.save_to_path(&path).unwrap();
        lock2.save_to_path(&path).unwrap();

        let latest = latest_plugins_lock_history_path_from_path(&path)
            .unwrap()
            .expect("expected archived lock");
        let archived = PluginsLock::load_from_path(&latest).unwrap();
        assert_eq!(archived, lock1.normalized());
        assert_eq!(
            PluginsLock::load_from_path(&path).unwrap(),
            lock2.normalized()
        );
    }

    #[test]
    fn rollback_restores_latest_archived_generation() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("plugins.lock");
        let lock1 = make_lock("sel_badge", "sha256:one");
        let lock2 = make_lock("sel_badge", "sha256:two");

        lock1.save_to_path(&path).unwrap();
        lock2.save_to_path(&path).unwrap();

        let restored_from = rollback_plugins_lock_from_path(&path)
            .unwrap()
            .expect("expected rollback history");
        let restored = PluginsLock::load_from_path(&path).unwrap();
        assert_eq!(restored, lock1.normalized());
        assert_eq!(
            PluginsLock::load_from_path(&restored_from).unwrap(),
            lock1.normalized()
        );

        let latest = latest_plugins_lock_history_path_from_path(&path)
            .unwrap()
            .expect("expected current lock to be archived");
        let archived_current = PluginsLock::load_from_path(&latest).unwrap();
        assert_eq!(archived_current, lock2.normalized());
    }

    #[test]
    fn prune_history_keeps_latest_generations() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("plugins.lock");
        let lock1 = make_lock("sel_badge", "sha256:one");
        let lock2 = make_lock("sel_badge", "sha256:two");
        let lock3 = make_lock("sel_badge", "sha256:three");

        lock1.save_to_path(&path).unwrap();
        lock2.save_to_path(&path).unwrap();
        lock3.save_to_path(&path).unwrap();

        let removed = prune_plugins_lock_history_from_path(&path, 1).unwrap();
        assert_eq!(removed.len(), 1);

        let remaining = plugins_lock_history_paths_from_path(&path).unwrap();
        assert_eq!(remaining.len(), 1);
        let remaining_lock = PluginsLock::load_from_path(&remaining[0]).unwrap();
        assert_eq!(remaining_lock, lock2.normalized());
    }

    #[test]
    fn load_tolerates_old_format_with_derived_fields() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("plugins.lock");
        // Old format includes derived fields — load should accept them
        let contents = r#"
version = 1

[plugins.sel_badge]
plugin_id = "sel_badge"
package = "example/sel-badge"
version = "0.1.0"
artifact_digest = "sha256:artifact"
code_digest = "sha256:code"
source_kind = "filesystem"
abi_version = "1.1.0"
"#;
        fs::write(&path, contents).unwrap();
        let lock = PluginsLock::load_from_path(&path).unwrap();
        assert_eq!(lock.plugins.len(), 1);
        let entry = lock.plugins.get("sel_badge").unwrap();
        assert_eq!(entry.code_digest.as_deref(), Some("sha256:code"));
        assert_eq!(entry.package.as_deref(), Some("example/sel-badge"));
    }
}
