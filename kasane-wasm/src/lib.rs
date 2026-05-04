mod adapter;
mod authority;
mod cache;
pub mod capability;
mod convert;
pub mod error;
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
pub use error::WasmPluginError;

use kasane_core::plugin::ProviderArtifactStage;
use kasane_plugin_package::package;
use sha2::{Digest, Sha256};
use std::path::Path;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use wasmtime::component::{Component, HasSelf, Linker};
use wasmtime::{Config, Engine};

/// Background thread that periodically increments the engine's epoch counter.
///
/// The ticker is reference-counted: it keeps running as long as any
/// `Arc<EpochTicker>` clone is alive (shared between the loader and all
/// plugins it creates). When the last reference is dropped, the thread stops.
struct EpochTicker {
    stop: Arc<AtomicBool>,
}

impl EpochTicker {
    fn start(engine: &Engine) -> Arc<Self> {
        let stop = Arc::new(AtomicBool::new(false));
        let flag = stop.clone();
        let engine = engine.clone();
        std::thread::Builder::new()
            .name("wasm-epoch-ticker".into())
            .spawn(move || {
                while !flag.load(Ordering::Relaxed) {
                    std::thread::sleep(std::time::Duration::from_millis(10));
                    engine.increment_epoch();
                }
            })
            .expect("failed to spawn epoch ticker thread");
        Arc::new(Self { stop })
    }
}

impl Drop for EpochTicker {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
    }
}

/// Loads and instantiates WASM plugins.
///
/// A single loader holds a shared `Engine` and pre-configured `Linker`,
/// allowing multiple plugins to be loaded efficiently.
pub struct WasmPluginLoader {
    engine: Engine,
    linker: Linker<host::HostState>,
    cache: Option<cache::ComponentCache>,
    epoch_ticker: Arc<EpochTicker>,
}

impl WasmPluginLoader {
    pub fn new() -> Result<Self, WasmPluginError> {
        // Tests opt into panic-on-trap so a WASM call failure surfaces as a
        // loud panic instead of an empty default that masquerades as a
        // legitimate "no contribution" result. Production keeps the default
        // (graceful degradation) so a buggy plugin can't crash the editor.
        #[cfg(test)]
        adapter::set_panic_on_trap(true);

        let (engine, linker) =
            Self::create_engine_and_linker().map_err(WasmPluginError::EngineInit)?;
        let cache = cache::ComponentCache::new(&engine);
        let epoch_ticker = EpochTicker::start(&engine);
        Ok(Self {
            engine,
            linker,
            cache,
            epoch_ticker,
        })
    }

    /// Create a loader with a custom cache base directory (for testing).
    #[doc(hidden)]
    pub fn new_with_cache_base(cache_base: &std::path::Path) -> Result<Self, WasmPluginError> {
        let (engine, linker) =
            Self::create_engine_and_linker().map_err(WasmPluginError::EngineInit)?;
        let cache = cache::ComponentCache::new_with_base(&engine, cache_base);
        let epoch_ticker = EpochTicker::start(&engine);
        Ok(Self {
            engine,
            linker,
            cache,
            epoch_ticker,
        })
    }

    fn create_engine_and_linker() -> anyhow::Result<(Engine, Linker<host::HostState>)> {
        let mut config = Config::new();
        config.wasm_component_model(true);
        config.epoch_interruption(true);
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
        // WIT 3.0 (ADR-035 §2): history interface
        bindings::kasane::plugin::history::add_to_linker::<
            host::HostState,
            HasSelf<host::HostState>,
        >(&mut linker, |state| state)?;
        Ok((engine, linker))
    }

    fn load_component(&self, wasm_bytes: &[u8]) -> anyhow::Result<Component> {
        if let Some(ref cache) = self.cache
            && let Some(component) = cache.get(wasm_bytes, &self.engine)
        {
            return Ok(component);
        }
        let component = Component::new(&self.engine, wasm_bytes)?;
        if let Some(ref cache) = self.cache {
            cache.put(wasm_bytes, &component);
        }
        Ok(component)
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
    ) -> Result<WasmPlugin, (ProviderArtifactStage, WasmPluginError)> {
        let component = self.load_component(wasm_bytes).map_err(|err| {
            (
                ProviderArtifactStage::Load,
                WasmPluginError::ComponentLoad(err),
            )
        })?;
        self.instantiate_component(component, wasi_config)
            .map_err(|err| {
                (
                    ProviderArtifactStage::Instantiate,
                    WasmPluginError::Instantiate(err),
                )
            })
    }

    pub fn load(
        &self,
        wasm_bytes: &[u8],
        wasi_config: &WasiCapabilityConfig,
    ) -> Result<WasmPlugin, WasmPluginError> {
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
        store.limiter(|data| &mut data.store_limits);
        // Generous deadline during instantiation (≈1s at 10ms tick).
        // Runtime calls use a tighter deadline (1 epoch ≈ 10ms) via with_runtime().
        store.set_epoch_deadline(100);
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
            Arc::clone(&self.epoch_ticker),
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
    ) -> Result<WasmPlugin, (ProviderArtifactStage, WasmPluginError)> {
        let component = self.load_component(wasm_bytes).map_err(|err| {
            (
                ProviderArtifactStage::Load,
                WasmPluginError::ComponentLoad(err),
            )
        })?;
        self.instantiate_with_manifest(component, manifest, wasi_config)
            .map_err(|err| {
                (
                    ProviderArtifactStage::Instantiate,
                    WasmPluginError::Instantiate(err),
                )
            })
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
            capability::build_wasi_ctx_from_manifest_with_env(
                plugin_id,
                manifest.wasi_capabilities(),
                &manifest.capabilities.env_vars,
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
        store.limiter(|data| &mut data.store_limits);
        // Generous deadline during instantiation (≈1s at 10ms tick).
        store.set_epoch_deadline(100);
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
            Arc::clone(&self.epoch_ticker),
        ))
    }

    /// Load a WASM plugin from a file path.
    pub fn load_file(
        &self,
        path: &Path,
        wasi_config: &WasiCapabilityConfig,
    ) -> Result<WasmPlugin, WasmPluginError> {
        let bytes = std::fs::read(path).map_err(|err| WasmPluginError::Other(err.into()))?;
        self.load(&bytes, wasi_config)
    }

    pub fn load_package_bytes(
        &self,
        package_bytes: &[u8],
        wasi_config: &WasiCapabilityConfig,
    ) -> Result<WasmPlugin, (ProviderArtifactStage, WasmPluginError)> {
        let inspected = package::verify_package(package_bytes).map_err(|err| {
            (
                ProviderArtifactStage::Manifest,
                WasmPluginError::Package(err),
            )
        })?;
        let manifest = inspected.header.to_manifest();
        let component =
            package::entry_bytes(package_bytes, &inspected, &inspected.header.plugin.entry)
                .map_err(|err| (ProviderArtifactStage::Read, WasmPluginError::Package(err)))?;
        self.load_with_manifest(component, &manifest, wasi_config)
    }

    pub fn load_package_file(
        &self,
        path: &Path,
        wasi_config: &WasiCapabilityConfig,
    ) -> Result<WasmPlugin, (ProviderArtifactStage, WasmPluginError)> {
        let bytes = std::fs::read(path).map_err(|err| {
            (
                ProviderArtifactStage::Read,
                WasmPluginError::Other(err.into()),
            )
        })?;
        self.load_package_bytes(&bytes, wasi_config)
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
const BUNDLED_SMOOTH_SCROLL: &[u8] = include_bytes!("../bundled/smooth-scroll.wasm");

const BUNDLED_CURSOR_LINE_MANIFEST: &str = include_str!("../bundled/cursor-line.toml");
const BUNDLED_COLOR_PREVIEW_MANIFEST: &str = include_str!("../bundled/color-preview.toml");
const BUNDLED_SEL_BADGE_MANIFEST: &str = include_str!("../bundled/sel-badge.toml");
const BUNDLED_FUZZY_FINDER_MANIFEST: &str = include_str!("../bundled/fuzzy-finder.toml");
const BUNDLED_PANE_MANAGER_MANIFEST: &str = include_str!("../bundled/pane-manager.toml");
const BUNDLED_SMOOTH_SCROLL_MANIFEST: &str = include_str!("../bundled/smooth-scroll.toml");

struct BundledPluginSpec {
    name: &'static str,
    wasm_bytes: &'static [u8],
    manifest_toml: &'static str,
    default_enabled: bool,
}

#[derive(Clone, Debug)]
pub struct BundledPluginArtifact {
    pub name: &'static str,
    pub plugin_id: String,
    pub package_name: String,
    pub package_version: String,
    pub artifact_digest: String,
    pub code_digest: String,
    pub abi_version: String,
    pub default_enabled: bool,
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
        BundledPluginSpec {
            name: "smooth_scroll",
            wasm_bytes: BUNDLED_SMOOTH_SCROLL,
            manifest_toml: BUNDLED_SMOOTH_SCROLL_MANIFEST,
            default_enabled: false,
        },
    ]
}

pub fn bundled_plugin_artifacts() -> Result<Vec<BundledPluginArtifact>, WasmPluginError> {
    bundled_plugin_specs()
        .iter()
        .map(bundled_plugin_artifact_from_spec)
        .collect()
}

pub fn bundled_plugin_artifact_by_plugin_id(
    plugin_id: &str,
) -> Result<Option<BundledPluginArtifact>, WasmPluginError> {
    for spec in bundled_plugin_specs() {
        let artifact = bundled_plugin_artifact_from_spec(spec)?;
        if artifact.plugin_id == plugin_id {
            return Ok(Some(artifact));
        }
    }
    Ok(None)
}

pub fn bundled_plugin_manifest_by_plugin_id(
    plugin_id: &str,
) -> Result<Option<manifest::PluginManifest>, WasmPluginError> {
    for spec in bundled_plugin_specs() {
        let manifest = manifest::PluginManifest::parse(spec.manifest_toml)
            .map_err(|err| WasmPluginError::Other(anyhow::anyhow!(err)))?;
        if manifest.plugin.id == plugin_id {
            return Ok(Some(manifest));
        }
    }
    Ok(None)
}

pub fn load_bundled_plugin_by_plugin_id(
    plugin_id: &str,
    wasi_config: &WasiCapabilityConfig,
) -> Result<WasmPlugin, (ProviderArtifactStage, WasmPluginError)> {
    let loader =
        WasmPluginLoader::new().map_err(|err| (ProviderArtifactStage::Instantiate, err))?;
    for spec in bundled_plugin_specs() {
        let manifest = manifest::PluginManifest::parse(spec.manifest_toml).map_err(|err| {
            (
                ProviderArtifactStage::Manifest,
                WasmPluginError::Other(anyhow::anyhow!(err)),
            )
        })?;
        if manifest.plugin.id != plugin_id {
            continue;
        }
        return loader.load_with_manifest(spec.wasm_bytes, &manifest, wasi_config);
    }
    Err((
        ProviderArtifactStage::Manifest,
        WasmPluginError::UnknownBundledPlugin(plugin_id.to_string()),
    ))
}

fn bundled_plugin_artifact_from_spec(
    spec: &BundledPluginSpec,
) -> Result<BundledPluginArtifact, WasmPluginError> {
    let manifest = manifest::PluginManifest::parse(spec.manifest_toml)
        .map_err(|err| WasmPluginError::Other(anyhow::anyhow!(err)))?;
    Ok(BundledPluginArtifact {
        name: spec.name,
        plugin_id: manifest.plugin.id.clone(),
        package_name: bundled_package_name(spec.name),
        package_version: env!("CARGO_PKG_VERSION").to_string(),
        artifact_digest: bundled_artifact_digest(spec.manifest_toml, spec.wasm_bytes),
        code_digest: sha256_prefixed(spec.wasm_bytes),
        abi_version: manifest.plugin.abi_version.clone(),
        default_enabled: spec.default_enabled,
    })
}

fn bundled_package_name(name: &str) -> String {
    format!("builtin/{}", name.replace('_', "-"))
}

fn bundled_artifact_digest(manifest_toml: &str, wasm_bytes: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(manifest_toml.as_bytes());
    hasher.update([0]);
    hasher.update(wasm_bytes);
    format!("sha256:{}", hex_encode(&hasher.finalize()))
}

fn sha256_prefixed(bytes: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    format!("sha256:{}", hex_encode(&hasher.finalize()))
}

fn hex_encode(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
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
