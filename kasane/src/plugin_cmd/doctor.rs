use std::path::Path;

use anyhow::Result;
use kasane_core::config::Config;

use crate::plugin_lock::PluginsLock;

pub fn run(fix: bool) -> Result<()> {
    println!("kasane plugin doctor");
    println!();

    let mut all_ok = true;

    all_ok &= check_wasm_target(fix);
    all_ok &= check_sdk_version();
    all_ok &= check_plugins_directory(fix);
    all_ok &= check_plugins_lock();
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
    print!("  package directory ... ");
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

fn check_plugins_lock() -> bool {
    print!("  plugins lock ... ");
    let config = Config::load();
    let plugins_dir = config.plugins.plugins_dir();
    let lock = match PluginsLock::load() {
        Ok(lock) => lock,
        Err(e) => {
            println!("ERROR ({e:#})");
            return false;
        }
    };

    if lock.plugins.is_empty() {
        println!("empty");
        return true;
    }

    let packages = match super::package_artifact::discover_installed_packages(&plugins_dir) {
        Ok(packages) => packages,
        Err(e) => {
            println!("ERROR ({e:#})");
            return false;
        }
    };

    let mut installed = std::collections::HashMap::new();
    for package in packages {
        if let super::package_artifact::DiscoveredPackage::Valid { inspected, .. } = package {
            installed.insert(
                inspected.header.plugin.id.clone(),
                inspected.header.digests.artifact.clone(),
            );
        }
    }

    let mut all_ok = true;
    println!("{} entries", lock.plugins.len());
    for (plugin_id, entry) in &lock.plugins {
        match installed.get(plugin_id) {
            Some(digest) if digest == &entry.artifact_digest => {
                println!("    {plugin_id}: ok");
            }
            Some(digest) => {
                println!(
                    "    {plugin_id}: STALE (lock={}, installed={digest})",
                    entry.artifact_digest
                );
                all_ok = false;
            }
            None => {
                println!("    {plugin_id}: MISSING ({})", entry.artifact_digest);
                all_ok = false;
            }
        }
    }
    all_ok
}

fn check_installed_plugins() -> bool {
    print!("  installed packages ... ");
    let config = Config::load();
    let plugins_dir = config.plugins.plugins_dir();

    let packages = match super::package_artifact::discover_installed_packages(&plugins_dir) {
        Ok(packages) => packages,
        Err(_) => {
            println!("none (directory not found)");
            return true;
        }
    };

    if packages.is_empty() {
        println!("none");
        return true;
    }

    println!("{} found", packages.len());
    let mut all_ok = true;
    for package in packages {
        match package {
            super::package_artifact::DiscoveredPackage::Valid { path, inspected } => {
                let filename = path.file_name().unwrap_or_default().to_string_lossy();
                println!(
                    "    {filename}: ok ({} / {})",
                    inspected.header.plugin.id,
                    super::package_artifact::package_label(&inspected)
                );
            }
            super::package_artifact::DiscoveredPackage::Invalid { path, error } => {
                let filename = path.file_name().unwrap_or_default().to_string_lossy();
                println!("    {filename}: ERROR ({error})");
                all_ok = false;
            }
        }
    }
    all_ok
}
