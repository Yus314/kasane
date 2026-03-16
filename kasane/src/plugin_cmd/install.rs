use std::fs;
use std::path::Path;

use anyhow::{Context, Result, bail};
use kasane_core::config::Config;

use super::build;

pub fn run(path: Option<&str>) -> Result<()> {
    let project_dir = path.unwrap_or(".");
    let wasm_path = build::build_plugin(project_dir, true)?;

    let plugin_id = validate_wasm(&wasm_path)?;

    let config = Config::load();
    let plugins_dir = config.plugins.plugins_dir();
    fs::create_dir_all(&plugins_dir).with_context(|| {
        format!(
            "failed to create plugins directory: {}",
            plugins_dir.display()
        )
    })?;

    let filename = wasm_path
        .file_name()
        .expect("wasm_path should have a filename");
    let dest = plugins_dir.join(filename);
    fs::copy(&wasm_path, &dest).with_context(|| format!("failed to copy to {}", dest.display()))?;

    let size = fs::metadata(&dest)?.len();
    let id_display = plugin_id.as_deref().unwrap_or("(unknown)");
    println!(
        "Installed \"{id_display}\" to {} ({} KiB)",
        dest.display(),
        size / 1024
    );

    Ok(())
}

/// Validate the WASM file by loading it with the WASM runtime (if available).
/// Returns the plugin ID on success.
#[cfg(feature = "wasm-plugins")]
fn validate_wasm(wasm_path: &Path) -> Result<Option<String>> {
    use kasane_core::plugin::PluginBackend;
    use kasane_wasm::{WasiCapabilityConfig, WasmPluginLoader};

    let loader = WasmPluginLoader::new().context("failed to create WASM plugin loader")?;
    let wasi_config = WasiCapabilityConfig::default();
    match loader.load_file(wasm_path, &wasi_config) {
        Ok(plugin) => Ok(Some(plugin.id().0)),
        Err(e) => {
            let detail = format!("{e:#}");
            if detail.contains("type mismatch") || detail.contains("import") {
                bail!(
                    "Plugin incompatible with this Kasane version (v{}).\n\
                     Rebuild with latest SDK: kasane plugin build\n\
                     Diagnose: kasane plugin doctor",
                    env!("CARGO_PKG_VERSION")
                );
            }
            bail!("Plugin validation failed: {e:#}");
        }
    }
}

#[cfg(not(feature = "wasm-plugins"))]
fn validate_wasm(_wasm_path: &Path) -> Result<Option<String>> {
    eprintln!("warning: wasm-plugins feature not enabled, skipping validation");
    Ok(None)
}
