mod adapter;
mod convert;
mod host;

mod bindings {
    wasmtime::component::bindgen!({
        world: "kasane-plugin",
        path: "wit",
    });
}

pub use adapter::WasmPlugin;

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

    /// Load a WASM plugin from raw bytes.
    pub fn load(&self, wasm_bytes: &[u8]) -> anyhow::Result<WasmPlugin> {
        let component = Component::new(&self.engine, wasm_bytes)?;
        let host_state = host::HostState::default();
        let mut store = wasmtime::Store::new(&self.engine, host_state);
        let instance = bindings::KasanePlugin::instantiate(&mut store, &component, &self.linker)?;

        let plugin_api = instance.kasane_plugin_plugin_api();
        let id = plugin_api.call_get_id(&mut store)?;

        Ok(WasmPlugin::new(store, instance, id))
    }

    /// Load a WASM plugin from a file path.
    pub fn load_file(&self, path: &Path) -> anyhow::Result<WasmPlugin> {
        let bytes = std::fs::read(path)?;
        self.load(&bytes)
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

    for path in &wasm_files {
        let file_name = path.file_name().unwrap_or_default().to_string_lossy();

        match loader.load_file(path) {
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
