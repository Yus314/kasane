use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;

use anyhow::Result;
use kasane_core::plugin::{
    PluginBackend, PluginCollect, PluginDescriptor, PluginDiagnostic, PluginFactory, PluginId,
    PluginProvider, PluginRank, PluginRevision, PluginSource, ProviderArtifactStage,
    plugin_factory,
};
use kasane_plugin_package::package;
use kasane_wasm::{
    WasiCapabilityConfig, WasmPluginLoader, bundled_plugin_artifact_by_plugin_id,
    bundled_plugin_manifest_by_plugin_id, load_bundled_plugin_by_plugin_id,
};

use crate::plugin_lock::{LockedPluginEntry, PluginsLock};
use crate::plugin_store::PluginStore;

const LOCKED_WASM_PROVIDER_NAME: &str = "kasane::LockedWasmPluginProvider";

pub struct LockedWasmPluginProvider {
    lock_path: PathBuf,
    store: PluginStore,
    plugins_config: kasane_core::config::PluginsConfig,
    config_settings: HashMap<String, toml::Table>,
}

impl LockedWasmPluginProvider {
    pub fn new(
        lock_path: impl Into<PathBuf>,
        plugins_config: kasane_core::config::PluginsConfig,
        config_settings: HashMap<String, toml::Table>,
    ) -> Self {
        let store = PluginStore::from_plugins_dir(plugins_config.plugins_dir());
        Self {
            lock_path: lock_path.into(),
            store,
            plugins_config,
            config_settings,
        }
    }

    pub fn from_default_lock_path(
        plugins_config: kasane_core::config::PluginsConfig,
        config_settings: HashMap<String, toml::Table>,
    ) -> Self {
        Self::new(
            crate::plugin_lock::plugins_lock_path(),
            plugins_config,
            config_settings,
        )
    }
}

impl PluginProvider for LockedWasmPluginProvider {
    fn name(&self) -> &'static str {
        LOCKED_WASM_PROVIDER_NAME
    }

    fn collect(&self) -> Result<PluginCollect> {
        let mut resolved = PluginCollect::default();

        let lock = match PluginsLock::load_from_path(&self.lock_path) {
            Ok(lock) => lock,
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

    match entry.source_kind.as_str() {
        "filesystem" => collect_locked_filesystem_plugin(
            key,
            entry,
            store,
            config_settings,
            wasi_config,
            resolved,
        ),
        "bundled" => {
            collect_locked_bundled_plugin(key, entry, config_settings, wasi_config, resolved)
        }
        other => resolved
            .diagnostics
            .push(PluginDiagnostic::provider_artifact_failed(
                LOCKED_WASM_PROVIDER_NAME,
                key,
                ProviderArtifactStage::Manifest,
                format!("unsupported source_kind `{other}` in plugins.lock"),
            )),
    }
}

fn collect_locked_filesystem_plugin(
    key: &str,
    entry: &LockedPluginEntry,
    store: &PluginStore,
    config_settings: &HashMap<String, toml::Table>,
    wasi_config: &WasiCapabilityConfig,
    resolved: &mut PluginCollect,
) {
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

fn collect_locked_bundled_plugin(
    key: &str,
    entry: &LockedPluginEntry,
    config_settings: &HashMap<String, toml::Table>,
    wasi_config: &WasiCapabilityConfig,
    resolved: &mut PluginCollect,
) {
    let artifact = match bundled_plugin_artifact_by_plugin_id(&entry.plugin_id) {
        Ok(Some(artifact)) => artifact,
        Ok(None) => {
            resolved
                .diagnostics
                .push(PluginDiagnostic::provider_artifact_failed(
                    LOCKED_WASM_PROVIDER_NAME,
                    key,
                    ProviderArtifactStage::Manifest,
                    format!("no bundled plugin found for `{}`", entry.plugin_id),
                ));
            return;
        }
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

    if artifact.artifact_digest != entry.artifact_digest {
        resolved
            .diagnostics
            .push(PluginDiagnostic::provider_artifact_failed(
                LOCKED_WASM_PROVIDER_NAME,
                key,
                ProviderArtifactStage::Manifest,
                format!(
                    "plugins.lock artifact digest `{}` does not match bundled digest `{}`",
                    entry.artifact_digest, artifact.artifact_digest
                ),
            ));
        return;
    }

    if artifact.code_digest != entry.code_digest {
        resolved
            .diagnostics
            .push(PluginDiagnostic::provider_artifact_failed(
                LOCKED_WASM_PROVIDER_NAME,
                key,
                ProviderArtifactStage::Manifest,
                format!(
                    "plugins.lock code digest `{}` does not match bundled digest `{}`",
                    entry.code_digest, artifact.code_digest
                ),
            ));
        return;
    }

    if let Some(abi_version) = &entry.abi_version
        && abi_version != &artifact.abi_version
    {
        resolved
            .diagnostics
            .push(PluginDiagnostic::provider_artifact_failed(
                LOCKED_WASM_PROVIDER_NAME,
                key,
                ProviderArtifactStage::Manifest,
                format!(
                    "plugins.lock abi_version `{}` does not match bundled abi `{}`",
                    abi_version, artifact.abi_version
                ),
            ));
        return;
    }

    let manifest = match bundled_plugin_manifest_by_plugin_id(&entry.plugin_id) {
        Ok(Some(manifest)) => manifest,
        Ok(None) => {
            resolved
                .diagnostics
                .push(PluginDiagnostic::provider_artifact_failed(
                    LOCKED_WASM_PROVIDER_NAME,
                    key,
                    ProviderArtifactStage::Manifest,
                    format!("no bundled manifest found for `{}`", entry.plugin_id),
                ));
            return;
        }
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
        id: plugin_id.clone(),
        source: PluginSource::BundledWasm {
            name: artifact.name.to_string(),
        },
        revision: PluginRevision(format!(
            "{}+{}",
            artifact.artifact_digest, artifact.code_digest
        )),
        rank: PluginRank::BUNDLED_WASM,
    };

    let factory = locked_bundled_wasm_factory(descriptor, plugin_id.0.clone(), wasi_config.clone());
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

fn locked_bundled_wasm_factory(
    descriptor: PluginDescriptor,
    plugin_id: String,
    wasi_config: WasiCapabilityConfig,
) -> Arc<dyn PluginFactory> {
    plugin_factory(descriptor, move || {
        let plugin =
            load_bundled_plugin_by_plugin_id(&plugin_id, &wasi_config).map_err(|(_, err)| err)?;
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

        let lock_path = tmp.path().join("plugins.lock");
        lock.save_to_path(&lock_path).unwrap();

        let provider = LockedWasmPluginProvider::new(
            lock_path,
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

        let lock_path = tmp.path().join("plugins.lock");
        lock.save_to_path(&lock_path).unwrap();

        let provider = LockedWasmPluginProvider::new(
            lock_path,
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

    #[test]
    fn collect_rereads_lock_file() {
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
                artifact_digest: selected_artifact.artifact_digest.clone(),
                code_digest: selected_artifact.code_digest.clone(),
                source_kind: "filesystem".to_string(),
                abi_version: Some(selected_artifact.abi_version.clone()),
            },
        );

        let lock_path = tmp.path().join("plugins.lock");
        lock.save_to_path(&lock_path).unwrap();

        let provider = LockedWasmPluginProvider::new(
            lock_path.clone(),
            kasane_core::config::PluginsConfig {
                path: Some(plugins_dir.to_string_lossy().into_owned()),
                ..Default::default()
            },
            HashMap::new(),
        );

        let collected = provider.collect().unwrap();
        assert_eq!(collected.factories.len(), 1);

        PluginsLock::new().save_to_path(&lock_path).unwrap();
        let collected = provider.collect().unwrap();
        assert!(collected.factories.is_empty());
        assert!(collected.diagnostics.is_empty());
    }

    #[test]
    fn collect_loads_bundled_plugins_from_lock() {
        let tmp = tempfile::tempdir().unwrap();
        let artifact = bundled_plugin_artifact_by_plugin_id("pane_manager")
            .unwrap()
            .expect("pane_manager bundled plugin");

        let mut lock = PluginsLock::new();
        lock.plugins.insert(
            artifact.plugin_id.clone(),
            LockedPluginEntry {
                plugin_id: artifact.plugin_id.clone(),
                package: Some(artifact.package_name.clone()),
                version: Some(artifact.package_version.clone()),
                artifact_digest: artifact.artifact_digest.clone(),
                code_digest: artifact.code_digest.clone(),
                source_kind: "bundled".to_string(),
                abi_version: Some(artifact.abi_version.clone()),
            },
        );

        let lock_path = tmp.path().join("plugins.lock");
        lock.save_to_path(&lock_path).unwrap();

        let provider = LockedWasmPluginProvider::new(
            lock_path,
            kasane_core::config::PluginsConfig {
                path: Some(tmp.path().join("plugins").to_string_lossy().into_owned()),
                ..Default::default()
            },
            HashMap::new(),
        );
        let collected = provider.collect().unwrap();
        assert_eq!(collected.factories.len(), 1);
        assert!(collected.diagnostics.is_empty());
        assert_eq!(
            collected.factories[0].descriptor().id,
            PluginId("pane_manager".to_string())
        );
    }
}
