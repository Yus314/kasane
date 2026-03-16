use std::fs;

use anyhow::{Context, Result};
use kasane_core::config::Config;

pub fn run() -> Result<()> {
    let config = Config::load();
    let plugins_dir = config.plugins.plugins_dir();

    let entries = match fs::read_dir(&plugins_dir) {
        Ok(entries) => entries,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            println!("No plugins installed in {}", plugins_dir.display());
            return Ok(());
        }
        Err(e) => {
            return Err(e).context(format!(
                "failed to read plugins directory: {}",
                plugins_dir.display()
            ));
        }
    };

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
        println!("No plugins installed in {}", plugins_dir.display());
        return Ok(());
    }

    println!("Installed plugins ({}):", plugins_dir.display());

    for path in &wasm_files {
        let filename = path.file_name().unwrap_or_default().to_string_lossy();
        let size = fs::metadata(path).map(|m| m.len()).unwrap_or(0);
        let id = load_plugin_id(path);

        match id {
            Ok(id) => {
                println!("  {id:<20} {filename:<30} ({} KiB)", size / 1024);
            }
            Err(detail) => {
                let msg = format!("(error: {detail})");
                println!("  {msg:<20} {filename:<30} ({} KiB)", size / 1024);
            }
        }
    }

    Ok(())
}

#[cfg(feature = "wasm-plugins")]
fn load_plugin_id(path: &std::path::Path) -> std::result::Result<String, String> {
    use kasane_core::plugin::PluginBackend;
    use kasane_wasm::{WasiCapabilityConfig, WasmPluginLoader};

    let loader = WasmPluginLoader::new().map_err(|e| e.to_string())?;
    let wasi_config = WasiCapabilityConfig::default();
    let plugin = loader.load_file(path, &wasi_config).map_err(|e| {
        e.to_string()
            .lines()
            .next()
            .unwrap_or("unknown error")
            .to_string()
    })?;
    Ok(plugin.id().0)
}

#[cfg(not(feature = "wasm-plugins"))]
fn load_plugin_id(_path: &std::path::Path) -> std::result::Result<String, String> {
    Err("wasm-plugins feature not enabled".to_string())
}
