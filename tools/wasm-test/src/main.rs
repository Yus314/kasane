fn main() {
    kasane::run(|registry| {
        let fixtures = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../kasane-wasm/fixtures");
        let loader = kasane::kasane_wasm::WasmPluginLoader::new().unwrap();
        let wasi_config = kasane::kasane_wasm::WasiCapabilityConfig::default();

        let cursor_line = loader
            .load_file(&fixtures.join("cursor-line.wasm"), &wasi_config)
            .unwrap();
        registry.register_backend(Box::new(cursor_line));

        let line_numbers = loader
            .load_file(&fixtures.join("line-numbers.wasm"), &wasi_config)
            .unwrap();
        registry.register_backend(Box::new(line_numbers));
    });
}
