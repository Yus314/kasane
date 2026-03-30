mod adapter;
mod authority;
pub mod capability;
mod convert;
mod host;
pub mod manifest;

mod bindings {
    wasmtime::component::bindgen!({
        world: "kasane-plugin",
        path: "wit",
    });
}

pub use adapter::WasmPlugin;
pub use capability::WasiCapabilityConfig;

use std::collections::BTreeMap;
use std::path::Path;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::SystemTime;

use kasane_core::plugin::{
    PluginBackend, PluginCollect, PluginDescriptor, PluginDiagnostic, PluginFactory, PluginId,
    PluginProvider, PluginRank, PluginRevision, PluginSource, ProviderArtifactStage,
    plugin_factory,
};
use wasmtime::component::{Component, HasSelf, Linker};
use wasmtime::{Config, Engine};

/// Loads and instantiates WASM plugins.
///
/// A single loader holds a shared `Engine` and pre-configured `Linker`,
/// allowing multiple plugins to be loaded efficiently.
pub struct WasmPluginLoader {
    engine: Engine,
    linker: Linker<host::HostState>,
}

impl WasmPluginLoader {
    pub fn new() -> anyhow::Result<Self> {
        let mut config = Config::new();
        config.wasm_component_model(true);
        let engine = Engine::new(&config)?;
        let mut linker: Linker<host::HostState> = Linker::new(&engine);
        wasmtime_wasi::p2::add_to_linker_sync(&mut linker)?;
        bindings::kasane::plugin::host_state::add_to_linker::<
            host::HostState,
            HasSelf<host::HostState>,
        >(&mut linker, |state| state)?;
        bindings::kasane::plugin::element_builder::add_to_linker::<
            host::HostState,
            HasSelf<host::HostState>,
        >(&mut linker, |state| state)?;
        bindings::kasane::plugin::host_log::add_to_linker::<
            host::HostState,
            HasSelf<host::HostState>,
        >(&mut linker, |state| state)?;
        Ok(Self { engine, linker })
    }

    /// Load a WASM plugin from raw bytes with WASI capability configuration.
    ///
    /// The plugin is first instantiated with an empty WASI context to query
    /// its ID, requested WASI capabilities, and requested host authorities.
    /// Then a proper WASI context is built based on the plugin's requests and
    /// user configuration, and swapped in before returning.
    pub fn load_staged(
        &self,
        wasm_bytes: &[u8],
        wasi_config: &WasiCapabilityConfig,
    ) -> Result<WasmPlugin, (ProviderArtifactStage, anyhow::Error)> {
        let component = Component::new(&self.engine, wasm_bytes)
            .map_err(|err| (ProviderArtifactStage::Load, err.into()))?;
        self.instantiate_component(component, wasi_config)
            .map_err(|err| (ProviderArtifactStage::Instantiate, err))
    }

    pub fn load(
        &self,
        wasm_bytes: &[u8],
        wasi_config: &WasiCapabilityConfig,
    ) -> anyhow::Result<WasmPlugin> {
        self.load_staged(wasm_bytes, wasi_config)
            .map_err(|(_, err)| err)
    }

    fn instantiate_component(
        &self,
        component: Component,
        wasi_config: &WasiCapabilityConfig,
    ) -> anyhow::Result<WasmPlugin> {
        let host_state = host::HostState::default();
        let mut store = wasmtime::Store::new(&self.engine, host_state);
        let instance = bindings::KasanePlugin::instantiate(&mut store, &component, &self.linker)?;

        let plugin_api = instance.kasane_plugin_plugin_api();
        let id = plugin_api.call_get_id(&mut store)?;

        // Query requested capabilities/authorities and build per-plugin WasiCtx.
        let requested = plugin_api.call_requested_capabilities(&mut store)?;
        let requested_authorities = plugin_api.call_requested_authorities(&mut store)?;
        let process_allowed = capability::is_capability_granted(
            &id,
            &crate::bindings::kasane::plugin::types::Capability::Process,
            &requested,
            wasi_config,
        );
        let resolved_authorities =
            authority::resolve_authorities(&id, &requested_authorities, wasi_config);
        if !requested.is_empty() {
            let wasi_ctx = capability::build_wasi_ctx(&id, &requested, wasi_config)?;
            let data = store.data_mut();
            data.wasi = wasi_ctx;
            data.table = wasmtime::component::ResourceTable::new();
        }

        Ok(WasmPlugin::new(
            store,
            instance,
            id,
            process_allowed,
            resolved_authorities,
        ))
    }

    /// Load a WASM plugin using manifest-provided metadata.
    ///
    /// The manifest supplies plugin identity, capabilities, authorities,
    /// handler flags, and view deps. The WASM module is instantiated with
    /// a WasiCtx built from the manifest (not from the plugin's self-report).
    pub fn load_with_manifest(
        &self,
        wasm_bytes: &[u8],
        manifest: &manifest::PluginManifest,
        wasi_config: &WasiCapabilityConfig,
    ) -> Result<WasmPlugin, (ProviderArtifactStage, anyhow::Error)> {
        let component = Component::new(&self.engine, wasm_bytes)
            .map_err(|err| (ProviderArtifactStage::Load, err.into()))?;
        self.instantiate_with_manifest(component, manifest, wasi_config)
            .map_err(|err| (ProviderArtifactStage::Instantiate, err))
    }

    fn instantiate_with_manifest(
        &self,
        component: Component,
        manifest: &manifest::PluginManifest,
        wasi_config: &WasiCapabilityConfig,
    ) -> anyhow::Result<WasmPlugin> {
        let plugin_id = &manifest.plugin.id;

        // Build WasiCtx from manifest capabilities BEFORE instantiation.
        let wasi_ctx = if !manifest.wasi_capabilities().is_empty() {
            capability::build_wasi_ctx_from_manifest(
                plugin_id,
                manifest.wasi_capabilities(),
                wasi_config,
            )?
        } else {
            wasmtime_wasi::WasiCtxBuilder::new().build()
        };

        let host_state = host::HostState {
            wasi: wasi_ctx,
            ..Default::default()
        };

        let mut store = wasmtime::Store::new(&self.engine, host_state);
        let instance = bindings::KasanePlugin::instantiate(&mut store, &component, &self.linker)?;

        // Verify WASM module's self-reported ID matches manifest
        let wasm_id = instance
            .kasane_plugin_plugin_api()
            .call_get_id(&mut store)?;
        if wasm_id != *plugin_id {
            anyhow::bail!(
                "manifest-WASM ID mismatch: manifest declares `{plugin_id}`, WASM reports `{wasm_id}`"
            );
        }

        let process_allowed = capability::is_process_allowed_by_manifest(
            plugin_id,
            manifest.wasi_capabilities(),
            wasi_config,
        );
        let resolved_authorities = authority::resolve_authorities_from_manifest(
            plugin_id,
            manifest.host_authorities(),
            wasi_config,
        );
        let cached_capabilities = manifest.plugin_capabilities();
        let cached_view_deps = manifest.dirty_flags();

        let manifest_descriptor = Some(manifest.capability_descriptor());
        let publish_topics = manifest.handlers.publish_topics.clone();
        let subscribe_topics = manifest.handlers.subscribe_topics.clone();
        let extensions_consumed = manifest.handlers.extensions_consumed.clone();
        let extension_defs = manifest
            .handlers
            .extensions_defined
            .iter()
            .map(|name| {
                kasane_core::plugin::extension_point::ExtensionDefinition::metadata_only(
                    kasane_core::plugin::extension_point::ExtensionPointId::new(name.clone()),
                    kasane_core::plugin::extension_point::CompositionRule::Merge,
                )
            })
            .collect();

        Ok(WasmPlugin::new_from_manifest(
            store,
            instance,
            plugin_id.clone(),
            process_allowed,
            resolved_authorities,
            cached_capabilities,
            cached_view_deps,
            manifest_descriptor,
            publish_topics,
            subscribe_topics,
            extensions_consumed,
            extension_defs,
        ))
    }

    /// Load a WASM plugin from a file path.
    pub fn load_file(
        &self,
        path: &Path,
        wasi_config: &WasiCapabilityConfig,
    ) -> anyhow::Result<WasmPlugin> {
        let bytes = std::fs::read(path)?;
        self.load(&bytes, wasi_config)
    }
}

// ---------------------------------------------------------------------------
// Bundled WASM plugins (embedded via include_bytes!)
// ---------------------------------------------------------------------------

const BUNDLED_CURSOR_LINE: &[u8] = include_bytes!("../bundled/cursor-line.wasm");
const BUNDLED_COLOR_PREVIEW: &[u8] = include_bytes!("../bundled/color-preview.wasm");
const BUNDLED_SEL_BADGE: &[u8] = include_bytes!("../bundled/sel-badge.wasm");
const BUNDLED_FUZZY_FINDER: &[u8] = include_bytes!("../bundled/fuzzy-finder.wasm");
const BUNDLED_PANE_MANAGER: &[u8] = include_bytes!("../bundled/pane-manager.wasm");

const BUNDLED_CURSOR_LINE_MANIFEST: &str = include_str!("../bundled/cursor-line.toml");
const BUNDLED_COLOR_PREVIEW_MANIFEST: &str = include_str!("../bundled/color-preview.toml");
const BUNDLED_SEL_BADGE_MANIFEST: &str = include_str!("../bundled/sel-badge.toml");
const BUNDLED_FUZZY_FINDER_MANIFEST: &str = include_str!("../bundled/fuzzy-finder.toml");
const BUNDLED_PANE_MANAGER_MANIFEST: &str = include_str!("../bundled/pane-manager.toml");

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum WasmPluginOrigin {
    Bundled(&'static str),
    Filesystem(PathBuf),
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum WasmPluginFingerprint {
    Bundled(&'static str),
    Filesystem {
        len: u64,
        modified_ns: Option<u128>,
        manifest_modified_ns: Option<u128>,
    },
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct WasmPluginRevision {
    pub origin: WasmPluginOrigin,
    pub fingerprint: WasmPluginFingerprint,
}

pub struct ResolvedWasmPlugin {
    pub id: PluginId,
    pub revision: WasmPluginRevision,
    plugin: WasmPlugin,
}

impl ResolvedWasmPlugin {
    pub fn into_backend(self) -> Box<dyn PluginBackend> {
        Box::new(self.plugin)
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ResolvedWasmSnapshot {
    revisions: BTreeMap<PluginId, WasmPluginRevision>,
}

impl ResolvedWasmSnapshot {
    pub fn contains(&self, id: &PluginId) -> bool {
        self.revisions.contains_key(id)
    }

    pub fn plugin_ids(&self) -> impl Iterator<Item = &PluginId> {
        self.revisions.keys()
    }

    pub fn revision(&self, id: &PluginId) -> Option<&WasmPluginRevision> {
        self.revisions.get(id)
    }
}

#[derive(Default)]
pub struct ResolvedWasmPlugins {
    plugins: Vec<ResolvedWasmPlugin>,
}

impl ResolvedWasmPlugins {
    pub fn len(&self) -> usize {
        self.plugins.len()
    }

    pub fn is_empty(&self) -> bool {
        self.plugins.is_empty()
    }

    pub fn snapshot(&self) -> ResolvedWasmSnapshot {
        let revisions = self
            .plugins
            .iter()
            .map(|plugin| (plugin.id.clone(), plugin.revision.clone()))
            .collect();
        ResolvedWasmSnapshot { revisions }
    }

    pub fn into_plugins(self) -> Vec<ResolvedWasmPlugin> {
        self.plugins
    }

    pub fn register_into(self, registry: &mut kasane_core::plugin::PluginRuntime) {
        for plugin in self.plugins {
            registry.register_backend(plugin.into_backend());
        }
    }
}

pub struct WasmPluginProvider {
    plugins_config: kasane_core::config::PluginsConfig,
}

impl WasmPluginProvider {
    pub fn new(plugins_config: kasane_core::config::PluginsConfig) -> Self {
        Self { plugins_config }
    }
}

impl PluginProvider for WasmPluginProvider {
    fn collect(&self) -> anyhow::Result<PluginCollect> {
        let loader = match WasmPluginLoader::new() {
            Ok(loader) => loader,
            Err(err) => {
                return Ok(PluginCollect {
                    factories: vec![],
                    diagnostics: vec![PluginDiagnostic::provider_collect_failed(
                        self.name(),
                        err.to_string(),
                    )],
                });
            }
        };
        let wasi_config = WasiCapabilityConfig::from_plugins_config(&self.plugins_config);
        let mut resolved = PluginCollect::default();
        resolve_bundled_plugins_with_factories(
            &self.plugins_config,
            &loader,
            &wasi_config,
            &mut resolved,
        );
        resolve_filesystem_plugins_with_factories(
            &self.plugins_config,
            &loader,
            &wasi_config,
            &mut resolved,
        );
        Ok(resolved)
    }
}

const WASM_PROVIDER_NAME: &str = "kasane_wasm::WasmPluginProvider";

fn mtime_ns(path: &Path) -> Option<u128> {
    std::fs::metadata(path)
        .ok()
        .and_then(|meta| meta.modified().ok())
        .and_then(|time: SystemTime| time.duration_since(SystemTime::UNIX_EPOCH).ok())
        .map(|duration| duration.as_nanos())
}

fn filesystem_fingerprint(
    wasm_path: &Path,
    wasm_len: u64,
    manifest_path: Option<&Path>,
) -> WasmPluginFingerprint {
    WasmPluginFingerprint::Filesystem {
        len: wasm_len,
        modified_ns: mtime_ns(wasm_path),
        manifest_modified_ns: manifest_path.and_then(mtime_ns),
    }
}

fn upsert_resolved_plugin(target: &mut Vec<ResolvedWasmPlugin>, plugin: ResolvedWasmPlugin) {
    if let Some(pos) = target.iter().position(|existing| existing.id == plugin.id) {
        target[pos] = plugin;
    } else {
        target.push(plugin);
    }
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

fn descriptor_from_wasm_revision(id: PluginId, revision: WasmPluginRevision) -> PluginDescriptor {
    let (source, rank) = match revision.origin {
        WasmPluginOrigin::Bundled(name) => (
            PluginSource::BundledWasm {
                name: name.to_string(),
            },
            PluginRank::BUNDLED_WASM,
        ),
        WasmPluginOrigin::Filesystem(path) => (
            PluginSource::FilesystemWasm { path },
            PluginRank::FILESYSTEM_WASM,
        ),
    };
    PluginDescriptor {
        id,
        source,
        revision: PluginRevision(format!("{:?}", revision.fingerprint)),
        rank,
    }
}

fn wasm_manifest_factory(
    descriptor: PluginDescriptor,
    bytes: Vec<u8>,
    manifest: manifest::PluginManifest,
    wasi_config: WasiCapabilityConfig,
) -> Arc<dyn PluginFactory> {
    plugin_factory(descriptor, move || {
        let loader = WasmPluginLoader::new()?;
        let plugin = loader
            .load_with_manifest(&bytes, &manifest, &wasi_config)
            .map_err(|(_, err)| err)?;
        Ok(Box::new(plugin))
    })
}

struct BundledPluginSpec {
    name: &'static str,
    wasm_bytes: &'static [u8],
    manifest_toml: &'static str,
    default_enabled: bool,
}

/// Plugins with `default_enabled = true` are loaded unless explicitly disabled.
/// Plugins with `default_enabled = false` require opt-in via `plugins.enabled`.
fn bundled_plugin_specs() -> &'static [BundledPluginSpec] {
    &[
        BundledPluginSpec {
            name: "cursor_line",
            wasm_bytes: BUNDLED_CURSOR_LINE,
            manifest_toml: BUNDLED_CURSOR_LINE_MANIFEST,
            default_enabled: false,
        },
        BundledPluginSpec {
            name: "color_preview",
            wasm_bytes: BUNDLED_COLOR_PREVIEW,
            manifest_toml: BUNDLED_COLOR_PREVIEW_MANIFEST,
            default_enabled: false,
        },
        BundledPluginSpec {
            name: "sel_badge",
            wasm_bytes: BUNDLED_SEL_BADGE,
            manifest_toml: BUNDLED_SEL_BADGE_MANIFEST,
            default_enabled: false,
        },
        BundledPluginSpec {
            name: "fuzzy_finder",
            wasm_bytes: BUNDLED_FUZZY_FINDER,
            manifest_toml: BUNDLED_FUZZY_FINDER_MANIFEST,
            default_enabled: false,
        },
        BundledPluginSpec {
            name: "pane_manager",
            wasm_bytes: BUNDLED_PANE_MANAGER,
            manifest_toml: BUNDLED_PANE_MANAGER_MANIFEST,
            default_enabled: true,
        },
    ]
}

/// A discovered plugin artifact from the filesystem (manifest + WASM pair).
struct DiscoveredPlugin {
    manifest: manifest::PluginManifest,
    wasm_path: PathBuf,
    manifest_path: PathBuf,
}

/// Scan a plugins directory for `.toml` manifest files and their sibling `.wasm` files.
fn discover_plugin_artifacts(plugins_dir: &Path) -> Result<Vec<DiscoveredPlugin>, std::io::Error> {
    let entries = std::fs::read_dir(plugins_dir)?;
    let mut toml_files: Vec<PathBuf> = entries
        .filter_map(|entry| {
            let entry = entry.ok()?;
            let path = entry.path();
            if path.extension().and_then(|e| e.to_str()) == Some("toml") {
                Some(path)
            } else {
                None
            }
        })
        .collect();
    toml_files.sort();

    let mut plugins = Vec::new();
    for toml_path in toml_files {
        let wasm_path = toml_path.with_extension("wasm");
        if !wasm_path.exists() {
            tracing::warn!("manifest {} has no sibling .wasm file", toml_path.display());
            continue;
        }
        let toml_str = match std::fs::read_to_string(&toml_path) {
            Ok(s) => s,
            Err(e) => {
                tracing::error!("failed to read manifest {}: {e}", toml_path.display());
                continue;
            }
        };
        match manifest::PluginManifest::parse(&toml_str) {
            Ok(m) => {
                plugins.push(DiscoveredPlugin {
                    manifest: m,
                    wasm_path,
                    manifest_path: toml_path.clone(),
                });
            }
            Err(e) => {
                tracing::error!("failed to parse manifest {}: {e}", toml_path.display());
            }
        }
    }
    Ok(plugins)
}

fn is_plugin_disabled(plugins_config: &kasane_core::config::PluginsConfig, name: &str) -> bool {
    plugins_config.disabled.iter().any(|d| d == name)
}

fn resolve_bundled_plugins(
    plugins_config: &kasane_core::config::PluginsConfig,
    loader: &WasmPluginLoader,
    wasi_config: &WasiCapabilityConfig,
    resolved: &mut Vec<ResolvedWasmPlugin>,
) {
    for spec in bundled_plugin_specs() {
        let should_load = if spec.default_enabled {
            !is_plugin_disabled(plugins_config, spec.name)
        } else {
            plugins_config.is_bundled_enabled(spec.name)
                && !is_plugin_disabled(plugins_config, spec.name)
        };
        if !should_load {
            continue;
        }
        let manifest = match manifest::PluginManifest::parse(spec.manifest_toml) {
            Ok(m) => m,
            Err(e) => {
                tracing::error!("failed to parse bundled manifest for {}: {e}", spec.name);
                continue;
            }
        };
        // Check disable by manifest plugin ID (before WASM compilation)
        if is_plugin_disabled(plugins_config, &manifest.plugin.id) {
            continue;
        }
        match loader.load_with_manifest(spec.wasm_bytes, &manifest, wasi_config) {
            Ok(plugin) => {
                tracing::info!("loaded bundled WASM plugin {}", spec.name);
                let id = plugin.id();
                upsert_resolved_plugin(
                    resolved,
                    ResolvedWasmPlugin {
                        id,
                        revision: WasmPluginRevision {
                            origin: WasmPluginOrigin::Bundled(spec.name),
                            fingerprint: WasmPluginFingerprint::Bundled(spec.name),
                        },
                        plugin,
                    },
                );
            }
            Err((_, e)) => {
                tracing::error!("failed to load bundled WASM plugin {}: {e}", spec.name);
            }
        }
    }
}

fn resolve_bundled_plugins_with_factories(
    plugins_config: &kasane_core::config::PluginsConfig,
    loader: &WasmPluginLoader,
    wasi_config: &WasiCapabilityConfig,
    resolved: &mut PluginCollect,
) {
    for spec in bundled_plugin_specs() {
        let should_load = if spec.default_enabled {
            !is_plugin_disabled(plugins_config, spec.name)
        } else {
            plugins_config.is_bundled_enabled(spec.name)
                && !is_plugin_disabled(plugins_config, spec.name)
        };
        if !should_load {
            continue;
        }
        let manifest = match manifest::PluginManifest::parse(spec.manifest_toml) {
            Ok(m) => m,
            Err(e) => {
                resolved
                    .diagnostics
                    .push(PluginDiagnostic::provider_artifact_failed(
                        WASM_PROVIDER_NAME,
                        format!("bundled:{}", spec.name),
                        ProviderArtifactStage::Manifest,
                        e.to_string(),
                    ));
                continue;
            }
        };
        if is_plugin_disabled(plugins_config, &manifest.plugin.id) {
            continue;
        }
        match loader.load_with_manifest(spec.wasm_bytes, &manifest, wasi_config) {
            Ok(plugin) => {
                let descriptor = descriptor_from_wasm_revision(
                    plugin.id(),
                    WasmPluginRevision {
                        origin: WasmPluginOrigin::Bundled(spec.name),
                        fingerprint: WasmPluginFingerprint::Bundled(spec.name),
                    },
                );
                upsert_resolved_factory(
                    &mut resolved.factories,
                    wasm_manifest_factory(
                        descriptor,
                        spec.wasm_bytes.to_vec(),
                        manifest,
                        wasi_config.clone(),
                    ),
                );
            }
            Err((stage, err)) => {
                resolved
                    .diagnostics
                    .push(PluginDiagnostic::provider_artifact_failed(
                        WASM_PROVIDER_NAME,
                        format!("bundled:{}", spec.name),
                        stage,
                        err.to_string(),
                    ));
            }
        }
    }
}

fn resolve_filesystem_plugins(
    plugins_config: &kasane_core::config::PluginsConfig,
    loader: &WasmPluginLoader,
    wasi_config: &WasiCapabilityConfig,
    resolved: &mut Vec<ResolvedWasmPlugin>,
) {
    if !plugins_config.auto_discover {
        return;
    }

    let plugins_dir = plugins_config.plugins_dir();
    let artifacts = match discover_plugin_artifacts(&plugins_dir) {
        Ok(a) => a,
        Err(e) => {
            if e.kind() != std::io::ErrorKind::NotFound {
                tracing::warn!(
                    "failed to read plugins directory {}: {e}",
                    plugins_dir.display()
                );
            }
            return;
        }
    };

    for artifact in &artifacts {
        let file_name = artifact
            .wasm_path
            .file_name()
            .unwrap_or_default()
            .to_string_lossy();

        if let Err(e) = artifact.manifest.validate() {
            tracing::error!("manifest validation failed for {file_name}: {e}");
            continue;
        }

        let plugin_id = &artifact.manifest.plugin.id;
        if is_plugin_disabled(plugins_config, plugin_id) {
            tracing::info!("WASM plugin {plugin_id} ({file_name}) disabled by config");
            continue;
        }

        let bytes = match std::fs::read(&artifact.wasm_path) {
            Ok(bytes) => bytes,
            Err(e) => {
                tracing::error!("failed to read WASM plugin {file_name}: {e}");
                continue;
            }
        };

        match loader.load_with_manifest(&bytes, &artifact.manifest, wasi_config) {
            Ok(plugin) => {
                let id = plugin.id();
                tracing::info!("loaded WASM plugin {id:?} from {file_name}");
                upsert_resolved_plugin(
                    resolved,
                    ResolvedWasmPlugin {
                        id,
                        revision: WasmPluginRevision {
                            origin: WasmPluginOrigin::Filesystem(artifact.wasm_path.clone()),
                            fingerprint: filesystem_fingerprint(
                                &artifact.wasm_path,
                                bytes.len() as u64,
                                Some(&artifact.manifest_path),
                            ),
                        },
                        plugin,
                    },
                );
            }
            Err((_, e)) => {
                tracing::error!("failed to load WASM plugin {file_name}: {e}");
            }
        }
    }
}

fn resolve_filesystem_plugins_with_factories(
    plugins_config: &kasane_core::config::PluginsConfig,
    loader: &WasmPluginLoader,
    wasi_config: &WasiCapabilityConfig,
    resolved: &mut PluginCollect,
) {
    if !plugins_config.auto_discover {
        return;
    }

    let plugins_dir = plugins_config.plugins_dir();
    let artifacts = match discover_plugin_artifacts(&plugins_dir) {
        Ok(a) => a,
        Err(e) => {
            if e.kind() != std::io::ErrorKind::NotFound {
                resolved
                    .diagnostics
                    .push(PluginDiagnostic::provider_collect_failed(
                        WASM_PROVIDER_NAME,
                        format!(
                            "failed to read plugins directory {}: {e}",
                            plugins_dir.display()
                        ),
                    ));
            }
            return;
        }
    };

    for artifact in &artifacts {
        let file_name = artifact
            .wasm_path
            .file_name()
            .unwrap_or_default()
            .to_string_lossy();

        if let Err(e) = artifact.manifest.validate() {
            resolved
                .diagnostics
                .push(PluginDiagnostic::provider_artifact_failed(
                    WASM_PROVIDER_NAME,
                    file_name.as_ref(),
                    ProviderArtifactStage::Manifest,
                    e.to_string(),
                ));
            continue;
        }

        let plugin_id = &artifact.manifest.plugin.id;
        if is_plugin_disabled(plugins_config, plugin_id) {
            tracing::info!("WASM plugin {plugin_id} ({file_name}) disabled by config");
            continue;
        }

        let bytes = match std::fs::read(&artifact.wasm_path) {
            Ok(bytes) => bytes,
            Err(e) => {
                resolved
                    .diagnostics
                    .push(PluginDiagnostic::provider_artifact_failed(
                        WASM_PROVIDER_NAME,
                        file_name.as_ref(),
                        ProviderArtifactStage::Read,
                        e.to_string(),
                    ));
                continue;
            }
        };

        match loader.load_with_manifest(&bytes, &artifact.manifest, wasi_config) {
            Ok(plugin) => {
                let id = plugin.id();
                let descriptor = descriptor_from_wasm_revision(
                    id,
                    WasmPluginRevision {
                        origin: WasmPluginOrigin::Filesystem(artifact.wasm_path.clone()),
                        fingerprint: filesystem_fingerprint(
                            &artifact.wasm_path,
                            bytes.len() as u64,
                            Some(&artifact.manifest_path),
                        ),
                    },
                );
                upsert_resolved_factory(
                    &mut resolved.factories,
                    wasm_manifest_factory(
                        descriptor,
                        bytes,
                        artifact.manifest.clone(),
                        wasi_config.clone(),
                    ),
                );
            }
            Err((stage, err)) => {
                resolved
                    .diagnostics
                    .push(PluginDiagnostic::provider_artifact_failed(
                        WASM_PROVIDER_NAME,
                        file_name.as_ref(),
                        stage,
                        err.to_string(),
                    ));
            }
        }
    }
}

pub fn resolve_wasm_plugins(
    plugins_config: &kasane_core::config::PluginsConfig,
) -> anyhow::Result<ResolvedWasmPlugins> {
    let loader = WasmPluginLoader::new()?;
    let wasi_config = WasiCapabilityConfig::from_plugins_config(plugins_config);
    let mut resolved = Vec::new();
    resolve_bundled_plugins(plugins_config, &loader, &wasi_config, &mut resolved);
    resolve_filesystem_plugins(plugins_config, &loader, &wasi_config, &mut resolved);
    Ok(ResolvedWasmPlugins { plugins: resolved })
}

/// Register bundled WASM plugins that are embedded in the binary.
///
/// Bundled plugins are only loaded when explicitly listed in `plugins.enabled`.
/// This is opt-in: by default no bundled plugins are registered.
/// Later registrations with the same ID (e.g. from filesystem discovery)
/// will replace bundled versions.
pub fn register_bundled_plugins(
    plugins_config: &kasane_core::config::PluginsConfig,
    registry: &mut kasane_core::plugin::PluginRuntime,
) {
    let loader = match WasmPluginLoader::new() {
        Ok(l) => l,
        Err(e) => {
            tracing::error!("failed to create WASM plugin loader for bundled plugins: {e}");
            return;
        }
    };

    let wasi_config = WasiCapabilityConfig::from_plugins_config(plugins_config);
    let mut resolved = Vec::new();
    resolve_bundled_plugins(plugins_config, &loader, &wasi_config, &mut resolved);
    ResolvedWasmPlugins { plugins: resolved }.register_into(registry);
}

/// Discover and register WASM plugins from the plugins directory.
///
/// Scans `plugins_config.plugins_dir()` for `*.wasm` files, loads each one,
/// and registers it with the given `PluginRuntime`. Plugins whose ID appears
/// in `plugins_config.disabled` are skipped. Errors are logged and do not
/// prevent other plugins from loading.
pub fn discover_and_register(
    plugins_config: &kasane_core::config::PluginsConfig,
    registry: &mut kasane_core::plugin::PluginRuntime,
) {
    let loader = match WasmPluginLoader::new() {
        Ok(l) => l,
        Err(e) => {
            tracing::error!("failed to create WASM plugin loader: {e}");
            return;
        }
    };

    let wasi_config = WasiCapabilityConfig::from_plugins_config(plugins_config);
    let mut resolved = Vec::new();
    resolve_filesystem_plugins(plugins_config, &loader, &wasi_config, &mut resolved);
    ResolvedWasmPlugins { plugins: resolved }.register_into(registry);
}

/// Load a pre-built .wasm file from the fixtures directory (for tests).
#[doc(hidden)]
pub fn load_wasm_fixture(name: &str) -> anyhow::Result<Vec<u8>> {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("fixtures")
        .join(name);
    Ok(std::fs::read(path)?)
}

#[cfg(test)]
mod tests;
