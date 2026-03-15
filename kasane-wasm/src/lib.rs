mod adapter;
pub mod capability;
mod convert;
mod host;

mod bindings {
    wasmtime::component::bindgen!({
        world: "kasane-plugin",
        path: "wit",
    });
}

pub use adapter::WasmPlugin;
pub use capability::WasiCapabilityConfig;

use std::path::Path;

use kasane_core::plugin::Plugin;
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
        Ok(Self { engine, linker })
    }

    /// Load a WASM plugin from raw bytes with WASI capability configuration.
    ///
    /// The plugin is first instantiated with an empty WASI context to query
    /// its ID and requested capabilities. Then a proper WASI context is built
    /// based on the plugin's requests and user configuration, and swapped in
    /// before returning.
    pub fn load(
        &self,
        wasm_bytes: &[u8],
        wasi_config: &WasiCapabilityConfig,
    ) -> anyhow::Result<WasmPlugin> {
        let component = Component::new(&self.engine, wasm_bytes)?;
        let host_state = host::HostState::default();
        let mut store = wasmtime::Store::new(&self.engine, host_state);
        let instance = bindings::KasanePlugin::instantiate(&mut store, &component, &self.linker)?;

        let plugin_api = instance.kasane_plugin_plugin_api();
        let id = plugin_api.call_get_id(&mut store)?;

        // Query requested capabilities and build per-plugin WasiCtx
        let requested = plugin_api.call_requested_capabilities(&mut store)?;
        let process_allowed = capability::is_capability_granted(
            &id,
            &crate::bindings::kasane::plugin::types::Capability::Process,
            &requested,
            wasi_config,
        );
        if !requested.is_empty() {
            let wasi_ctx = capability::build_wasi_ctx(&id, &requested, wasi_config)?;
            let data = store.data_mut();
            data.wasi = wasi_ctx;
            data.table = wasmtime::component::ResourceTable::new();
        }

        Ok(WasmPlugin::new(store, instance, id, process_allowed))
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

/// Register bundled WASM plugins that are embedded in the binary.
///
/// Bundled plugins are only loaded when explicitly listed in `plugins.enabled`.
/// This is opt-in: by default no bundled plugins are registered.
/// Later registrations with the same ID (e.g. from filesystem discovery)
/// will replace bundled versions.
pub fn register_bundled_plugins(
    plugins_config: &kasane_core::config::PluginsConfig,
    registry: &mut kasane_core::plugin::PluginRegistry,
) {
    let bundled = [
        ("cursor_line", BUNDLED_CURSOR_LINE),
        ("color_preview", BUNDLED_COLOR_PREVIEW),
        ("sel_badge", BUNDLED_SEL_BADGE),
        ("fuzzy_finder", BUNDLED_FUZZY_FINDER),
    ];

    // Early return if no bundled plugins are enabled
    if !bundled
        .iter()
        .any(|(name, _)| plugins_config.is_bundled_enabled(name))
    {
        return;
    }

    let loader = match WasmPluginLoader::new() {
        Ok(l) => l,
        Err(e) => {
            tracing::error!("failed to create WASM plugin loader for bundled plugins: {e}");
            return;
        }
    };

    let wasi_config = WasiCapabilityConfig::from_plugins_config(plugins_config);

    for (name, bytes) in bundled {
        if !plugins_config.is_bundled_enabled(name) {
            continue;
        }
        match loader.load(bytes, &wasi_config) {
            Ok(plugin) => {
                tracing::info!("loaded bundled WASM plugin {name}");
                registry.register(Box::new(plugin));
            }
            Err(e) => {
                tracing::error!("failed to load bundled WASM plugin {name}: {e}");
            }
        }
    }
}

/// Discover and register WASM plugins from the plugins directory.
///
/// Scans `plugins_config.plugins_dir()` for `*.wasm` files, loads each one,
/// and registers it with the given `PluginRegistry`. Plugins whose ID appears
/// in `plugins_config.disabled` are skipped. Errors are logged and do not
/// prevent other plugins from loading.
pub fn discover_and_register(
    plugins_config: &kasane_core::config::PluginsConfig,
    registry: &mut kasane_core::plugin::PluginRegistry,
) {
    if !plugins_config.auto_discover {
        return;
    }

    let plugins_dir = plugins_config.plugins_dir();
    let entries = match std::fs::read_dir(&plugins_dir) {
        Ok(entries) => entries,
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

    // Collect and sort .wasm files for deterministic load order
    let mut wasm_files: Vec<_> = entries
        .filter_map(|entry| {
            let entry = entry.ok()?;
            let path = entry.path();
            if path.extension().and_then(|e| e.to_str()) == Some("wasm") {
                Some(path)
            } else {
                None
            }
        })
        .collect();
    wasm_files.sort();

    if wasm_files.is_empty() {
        return;
    }

    let loader = match WasmPluginLoader::new() {
        Ok(l) => l,
        Err(e) => {
            tracing::error!("failed to create WASM plugin loader: {e}");
            return;
        }
    };

    let wasi_config = WasiCapabilityConfig::from_plugins_config(plugins_config);

    for path in &wasm_files {
        let file_name = path.file_name().unwrap_or_default().to_string_lossy();

        match loader.load_file(path, &wasi_config) {
            Ok(plugin) => {
                let id = plugin.id();
                if plugins_config.disabled.contains(&id.0) {
                    tracing::info!("WASM plugin {id:?} ({file_name}) disabled by config");
                    continue;
                }
                tracing::info!("loaded WASM plugin {id:?} from {file_name}");
                registry.register(Box::new(plugin));
            }
            Err(e) => {
                tracing::error!("failed to load WASM plugin {file_name}: {e}");
            }
        }
    }
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
