use kasane::kasane_core::plugin::{
    plugin_factory, PluginDescriptor, PluginRank, PluginRevision, PluginSource,
};
use kasane::kasane_wasm::{WasiCapabilityConfig, WasmPluginLoader};

fn main() {
    let fixtures =
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../kasane-wasm/fixtures");
    let wasi_config = WasiCapabilityConfig::default();

    let cursor_line_path = fixtures.join("cursor-line.wasm");

    kasane::run_with_factories([plugin_factory(
        PluginDescriptor {
            id: kasane::kasane_core::plugin::PluginId("cursor_line".to_string()),
            source: PluginSource::FilesystemWasm {
                path: cursor_line_path.clone(),
            },
            revision: PluginRevision("static".to_string()),
            rank: PluginRank::FILESYSTEM_WASM,
        },
        {
            let wasi_config = wasi_config.clone();
            move || {
                let loader = WasmPluginLoader::new()?;
                let plugin = loader.load_file(&cursor_line_path, &wasi_config)?;
                Ok(Box::new(plugin))
            }
        },
    )]);
}
