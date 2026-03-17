use std::path::Path;

use anyhow::Result;
use kasane_core::config::Config;

pub fn run(fix: bool) -> Result<()> {
    println!("kasane plugin doctor");
    println!();

    let mut all_ok = true;

    all_ok &= check_wasm_target(fix);
    all_ok &= check_sdk_version();
    all_ok &= check_plugins_directory(fix);
    all_ok &= check_installed_plugins();

    println!();
    if all_ok {
        println!("All checks passed.");
    } else if !fix {
        println!("Some checks failed. Fix manually or: kasane plugin doctor --fix");
    } else {
        println!("Some checks could not be fixed. See above for details.");
    }

    Ok(())
}

fn check_wasm_target(fix: bool) -> bool {
    print!("  wasm32-wasip2 target ... ");
    let output = std::process::Command::new("rustup")
        .args(["target", "list", "--installed"])
        .output();
    match output {
        Ok(out) => {
            let stdout = String::from_utf8_lossy(&out.stdout);
            if stdout.lines().any(|l| l.trim() == "wasm32-wasip2") {
                println!("ok");
                true
            } else {
                println!("MISSING");
                if fix {
                    print!("    fixing: rustup target add wasm32-wasip2 ... ");
                    let status = std::process::Command::new("rustup")
                        .args(["target", "add", "wasm32-wasip2"])
                        .status();
                    match status {
                        Ok(s) if s.success() => {
                            println!("ok");
                            true
                        }
                        _ => {
                            println!("FAILED");
                            false
                        }
                    }
                } else {
                    println!("    fix: rustup target add wasm32-wasip2");
                    println!("    or: kasane plugin doctor --fix");
                    false
                }
            }
        }
        Err(_) => {
            println!("SKIP (rustup not found)");
            false
        }
    }
}

fn check_sdk_version() -> bool {
    print!("  kasane-plugin-sdk ... ");
    let cargo_toml = Path::new("Cargo.toml");
    if !cargo_toml.exists() {
        println!("SKIP (no Cargo.toml in current directory)");
        return true;
    }
    let contents = match std::fs::read_to_string(cargo_toml) {
        Ok(c) => c,
        Err(_) => {
            println!("SKIP (cannot read Cargo.toml)");
            return true;
        }
    };
    if let Some(line) = contents.lines().find(|l| l.contains("kasane-plugin-sdk")) {
        println!("ok ({line})");
        true
    } else {
        println!("NOT FOUND in Cargo.toml");
        println!("    hint: Is this a kasane plugin project?");
        false
    }
}

fn check_plugins_directory(fix: bool) -> bool {
    print!("  plugins directory ... ");
    let config = Config::load();
    let plugins_dir = config.plugins.plugins_dir();
    if plugins_dir.exists() {
        let writable = std::fs::metadata(&plugins_dir)
            .map(|m| !m.permissions().readonly())
            .unwrap_or(false);
        if writable {
            println!("ok ({})", plugins_dir.display());
            true
        } else {
            println!("NOT WRITABLE ({})", plugins_dir.display());
            false
        }
    } else if fix {
        print!("    creating {} ... ", plugins_dir.display());
        match std::fs::create_dir_all(&plugins_dir) {
            Ok(()) => {
                println!("ok");
                true
            }
            Err(e) => {
                println!("FAILED ({e})");
                false
            }
        }
    } else {
        println!("MISSING ({})", plugins_dir.display());
        println!("    hint: Will be created on first `kasane plugin install`");
        println!("    or: kasane plugin doctor --fix");
        true
    }
}

fn check_installed_plugins() -> bool {
    print!("  installed plugins ... ");
    let config = Config::load();
    let plugins_dir = config.plugins.plugins_dir();

    let entries = match std::fs::read_dir(&plugins_dir) {
        Ok(e) => e,
        Err(_) => {
            println!("none (directory not found)");
            return true;
        }
    };

    let wasm_files: Vec<_> = entries
        .filter_map(|e| {
            let entry = e.ok()?;
            let path = entry.path();
            if path.extension().and_then(|e| e.to_str()) == Some("wasm") {
                Some(path)
            } else {
                None
            }
        })
        .collect();

    if wasm_files.is_empty() {
        println!("none");
        return true;
    }

    println!("{} found", wasm_files.len());
    let mut all_ok = true;
    for path in &wasm_files {
        let filename = path.file_name().unwrap_or_default().to_string_lossy();
        match load_plugin_id(path) {
            Ok(id) => println!("    {filename}: ok ({id})"),
            Err(e) => {
                println!("    {filename}: ERROR ({e})");
                all_ok = false;
            }
        }
    }
    all_ok
}

#[cfg(feature = "wasm-plugins")]
fn load_plugin_id(path: &Path) -> std::result::Result<String, String> {
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
fn load_plugin_id(_path: &Path) -> std::result::Result<String, String> {
    Err("wasm-plugins feature not enabled".to_string())
}
