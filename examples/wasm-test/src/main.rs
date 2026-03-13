fn main() {
    kasane::run(|registry| {
        let fixtures = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../kasane-wasm/fixtures");
        let loader = kasane::kasane_wasm::WasmPluginLoader::new().unwrap();

        let cursor_line = loader.load_file(&fixtures.join("cursor-line.wasm")).unwrap();
        registry.register(Box::new(cursor_line));

        let line_numbers = loader.load_file(&fixtures.join("line-numbers.wasm")).unwrap();
        registry.register(Box::new(line_numbers));
    });
}
