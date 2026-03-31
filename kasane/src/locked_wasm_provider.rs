use std::collections::HashMap;
use std::sync::Arc;

use anyhow::Result;
use kasane_core::plugin::{
    PluginBackend, PluginCollect, PluginDescriptor, PluginDiagnostic, PluginFactory, PluginId,
    PluginProvider, PluginRank, PluginRevision, PluginSource, ProviderArtifactStage,
    plugin_factory,
};
use kasane_plugin_package::package;
use kasane_wasm::{WasiCapabilityConfig, WasmPluginLoader};

use crate::plugin_lock::{LockedPluginEntry, PluginsLock};
use crate::plugin_store::PluginStore;

const LOCKED_WASM_PROVIDER_NAME: &str = "kasane::LockedWasmPluginProvider";

pub struct LockedWasmPluginProvider {
    lock: std::result::Result<PluginsLock, String>,
    store: PluginStore,
    plugins_config: kasane_core::config::PluginsConfig,
    config_settings: HashMap<String, toml::Table>,
}

impl LockedWasmPluginProvider {
    pub fn new(
        lock: Result<PluginsLock>,
        plugins_config: kasane_core::config::PluginsConfig,
        config_settings: HashMap<String, toml::Table>,
    ) -> Self {
        let store = PluginStore::from_plugins_dir(plugins_config.plugins_dir());
        Self {
            lock: lock.map_err(|err| err.to_string()),
            store,
            plugins_config,
            config_settings,
        }
    }

    pub fn has_lock_entries(&self) -> bool {
        self.lock
            .as_ref()
            .is_ok_and(|lock| !lock.plugins.is_empty())
    }

    pub fn should_prefer_locked_runtime(&self) -> bool {
        self.lock.is_err() || self.has_lock_entries()
    }
}

impl PluginProvider for LockedWasmPluginProvider {
    fn name(&self) -> &'static str {
        LOCKED_WASM_PROVIDER_NAME
    }

    fn collect(&self) -> Result<PluginCollect> {
        let mut resolved = PluginCollect::default();

        let lock = match &self.lock {
            Ok(lock) => lock,
            Err(err) => {
                resolved
                    .diagnostics
                    .push(PluginDiagnostic::provider_collect_failed(
                        self.name(),
                        err.clone(),
                    ));
                return Ok(resolved);
            }
        };

        if lock.plugins.is_empty() {
            return Ok(resolved);
        }

        let _loader = match WasmPluginLoader::new() {
            Ok(loader) => loader,
            Err(err) => {
                resolved
                    .diagnostics
                    .push(PluginDiagnostic::provider_collect_failed(
                        self.name(),
                        err.to_string(),
                    ));
                return Ok(resolved);
            }
        };
        let wasi_config = WasiCapabilityConfig::from_plugins_config(&self.plugins_config);

        for (key, entry) in &lock.plugins {
            collect_locked_plugin(
                key,
                entry,
                &self.store,
                &self.config_settings,
                &wasi_config,
                &mut resolved,
            );
        }

        Ok(resolved)
    }
}

fn collect_locked_plugin(
    key: &str,
    entry: &LockedPluginEntry,
    store: &PluginStore,
    config_settings: &HashMap<String, toml::Table>,
    wasi_config: &WasiCapabilityConfig,
    resolved: &mut PluginCollect,
) {
    if key != entry.plugin_id {
        resolved
            .diagnostics
            .push(PluginDiagnostic::provider_artifact_failed(
                LOCKED_WASM_PROVIDER_NAME,
                key,
                ProviderArtifactStage::Manifest,
                format!(
                    "plugins.lock entry key `{key}` does not match plugin_id `{}`",
                    entry.plugin_id
                ),
            ));
        return;
    }

    let package_path = match store.path_for(&entry.artifact_digest) {
        Ok(path) => path,
        Err(err) => {
            resolved
                .diagnostics
                .push(PluginDiagnostic::provider_artifact_failed(
                    LOCKED_WASM_PROVIDER_NAME,
                    key,
                    ProviderArtifactStage::Manifest,
                    err.to_string(),
                ));
            return;
        }
    };

    if !package_path.exists() {
        resolved
            .diagnostics
            .push(PluginDiagnostic::provider_artifact_failed(
                LOCKED_WASM_PROVIDER_NAME,
                key,
                ProviderArtifactStage::Read,
                format!(
                    "lock points to missing store artifact {}",
                    package_path.display()
                ),
            ));
        return;
    }

    let inspected = match package::inspect_package_file(&package_path) {
        Ok(inspected) => inspected,
        Err(err) => {
            resolved
                .diagnostics
                .push(PluginDiagnostic::provider_artifact_failed(
                    LOCKED_WASM_PROVIDER_NAME,
                    key,
                    ProviderArtifactStage::Manifest,
                    err.to_string(),
                ));
            return;
        }
    };

    if inspected.header.plugin.id != entry.plugin_id {
        resolved
            .diagnostics
            .push(PluginDiagnostic::provider_artifact_failed(
                LOCKED_WASM_PROVIDER_NAME,
                key,
                ProviderArtifactStage::Manifest,
                format!(
                    "plugins.lock plugin_id `{}` does not match package plugin id `{}`",
                    entry.plugin_id, inspected.header.plugin.id
                ),
            ));
        return;
    }

    if inspected.header.digests.artifact != entry.artifact_digest {
        resolved
            .diagnostics
            .push(PluginDiagnostic::provider_artifact_failed(
                LOCKED_WASM_PROVIDER_NAME,
                key,
                ProviderArtifactStage::Manifest,
                format!(
                    "plugins.lock artifact digest `{}` does not match package digest `{}`",
                    entry.artifact_digest, inspected.header.digests.artifact
                ),
            ));
        return;
    }

    if inspected.header.digests.code != entry.code_digest {
        resolved
            .diagnostics
            .push(PluginDiagnostic::provider_artifact_failed(
                LOCKED_WASM_PROVIDER_NAME,
                key,
                ProviderArtifactStage::Manifest,
                format!(
                    "plugins.lock code digest `{}` does not match package digest `{}`",
                    entry.code_digest, inspected.header.digests.code
                ),
            ));
        return;
    }

    let manifest = inspected.header.to_manifest();
    if let Err(err) = manifest.validate() {
        resolved
            .diagnostics
            .push(PluginDiagnostic::provider_artifact_failed(
                LOCKED_WASM_PROVIDER_NAME,
                key,
                ProviderArtifactStage::Manifest,
                err.to_string(),
            ));
        return;
    }

    let plugin_id = PluginId(manifest.plugin.id.clone());
    let settings = resolve_plugin_settings(&manifest, config_settings);
    if !settings.is_empty() {
        resolved
            .initial_settings
            .insert(plugin_id.clone(), settings);
    }

    let descriptor = PluginDescriptor {
        id: plugin_id,
        source: PluginSource::FilesystemWasm {
            path: package_path.clone(),
        },
        revision: PluginRevision(format!(
            "{}+{}",
            inspected.header.digests.artifact, inspected.header.digests.code
        )),
        rank: PluginRank::FILESYSTEM_WASM,
    };

    let factory = locked_wasm_package_factory(descriptor, package_path, wasi_config.clone());
    upsert_resolved_factory(&mut resolved.factories, factory);
}

fn resolve_plugin_settings(
    manifest: &kasane_plugin_package::manifest::PluginManifest,
    config_settings: &HashMap<String, toml::Table>,
) -> HashMap<String, kasane_core::plugin::setting::SettingValue> {
    let mut settings = manifest.resolve_setting_defaults();
    if let Some(config_table) = config_settings.get(&manifest.plugin.id) {
        let (overrides, warnings) = manifest.validate_config_settings(config_table);
        for warning in warnings {
            tracing::warn!("{}", warning);
        }
        settings.extend(overrides);
    }
    settings
}

fn locked_wasm_package_factory(
    descriptor: PluginDescriptor,
    package_path: std::path::PathBuf,
    wasi_config: WasiCapabilityConfig,
) -> Arc<dyn PluginFactory> {
    plugin_factory(descriptor, move || {
        let loader = WasmPluginLoader::new()?;
        let plugin = loader
            .load_package_file(&package_path, &wasi_config)
            .map_err(|(_, err)| err)?;
        Ok(Box::new(plugin) as Box<dyn PluginBackend>)
    })
}

fn upsert_resolved_factory(
    target: &mut Vec<Arc<dyn PluginFactory>>,
    factory: Arc<dyn PluginFactory>,
) {
    let plugin_id = factory.descriptor().id.clone();
    if let Some(pos) = target
        .iter()
        .position(|existing| existing.descriptor().id == plugin_id)
    {
        target[pos] = factory;
    } else {
        target.push(factory);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::path::Path;

    use kasane_plugin_package::manifest::PluginManifest;
    use kasane_plugin_package::package::{BuildInput, write_package};

    fn build_fixture_package(
        root: &Path,
        plugin_id: &str,
        package_name: &str,
    ) -> std::path::PathBuf {
        let source_path = root.join(format!("{plugin_id}.kpk"));
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
            package_version: "0.1.0".to_string(),
            component_entry: "plugin.wasm".to_string(),
            component: fs::read(
                Path::new(env!("CARGO_MANIFEST_DIR"))
                    .join("../kasane-wasm/fixtures/sel-badge.wasm"),
            )
            .unwrap(),
            manifest,
            assets: Vec::new(),
        })
        .unwrap();
        write_package(&source_path, &output).unwrap();
        source_path
    }

    #[test]
    fn collect_loads_only_locked_store_packages() {
        let tmp = tempfile::tempdir().unwrap();
        let plugins_dir = tmp.path().join("plugins");
        let store = PluginStore::from_plugins_dir(&plugins_dir);

        let selected_source = build_fixture_package(tmp.path(), "sel_badge", "example/sel-badge");
        let selected_artifact = store.put_verified_package(&selected_source).unwrap();

        let ignored_source =
            build_fixture_package(tmp.path(), "cursor_line", "example/cursor-line");
        let _ignored_artifact = store.put_verified_package(&ignored_source).unwrap();

        let mut lock = PluginsLock::new();
        lock.plugins.insert(
            selected_artifact.plugin_id.clone(),
            LockedPluginEntry {
                plugin_id: selected_artifact.plugin_id.clone(),
                package: Some(selected_artifact.package_name.clone()),
                version: Some(selected_artifact.package_version.clone()),
                artifact_digest: selected_artifact.artifact_digest.clone(),
                code_digest: selected_artifact.code_digest.clone(),
                source_kind: "filesystem".to_string(),
                abi_version: Some(selected_artifact.abi_version.clone()),
            },
        );

        let provider = LockedWasmPluginProvider::new(
            Ok(lock),
            kasane_core::config::PluginsConfig {
                path: Some(plugins_dir.to_string_lossy().into_owned()),
                ..Default::default()
            },
            HashMap::new(),
        );
        let collected = provider.collect().unwrap();
        assert_eq!(collected.factories.len(), 1);
        assert!(collected.diagnostics.is_empty());
        assert_eq!(
            collected.factories[0].descriptor().id,
            PluginId("sel_badge".to_string())
        );
    }

    #[test]
    fn collect_reports_digest_mismatch() {
        let tmp = tempfile::tempdir().unwrap();
        let plugins_dir = tmp.path().join("plugins");
        let store = PluginStore::from_plugins_dir(&plugins_dir);

        let selected_source = build_fixture_package(tmp.path(), "sel_badge", "example/sel-badge");
        let selected_artifact = store.put_verified_package(&selected_source).unwrap();

        let mut lock = PluginsLock::new();
        lock.plugins.insert(
            selected_artifact.plugin_id.clone(),
            LockedPluginEntry {
                plugin_id: selected_artifact.plugin_id.clone(),
                package: Some(selected_artifact.package_name.clone()),
                version: Some(selected_artifact.package_version.clone()),
                artifact_digest: "sha256:deadbeef".to_string(),
                code_digest: selected_artifact.code_digest.clone(),
                source_kind: "filesystem".to_string(),
                abi_version: Some(selected_artifact.abi_version.clone()),
            },
        );

        let provider = LockedWasmPluginProvider::new(
            Ok(lock),
            kasane_core::config::PluginsConfig {
                path: Some(plugins_dir.to_string_lossy().into_owned()),
                ..Default::default()
            },
            HashMap::new(),
        );
        let collected = provider.collect().unwrap();
        assert!(collected.factories.is_empty());
        assert_eq!(collected.diagnostics.len(), 1);
    }
}
