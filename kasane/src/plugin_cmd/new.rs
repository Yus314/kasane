use std::fs;
use std::path::Path;
use std::process::Command;

use anyhow::{Result, bail};

use crate::cli::PluginTemplate;

use super::templates;

pub fn run(name: &str, template: PluginTemplate) -> Result<()> {
    validate_name(name)?;

    let dir = Path::new(name);
    if dir.exists() {
        bail!("directory '{name}' already exists");
    }

    check_wasm_target();

    let src_dir = dir.join("src");
    fs::create_dir_all(&src_dir)?;

    fs::write(dir.join("Cargo.toml"), templates::cargo_toml(name))?;
    fs::write(src_dir.join("lib.rs"), templates::lib_rs(name, template))?;
    fs::write(dir.join(".gitignore"), templates::gitignore())?;

    println!("Created plugin \"{name}\" at ./{name}/");
    println!();
    println!("Next steps:");
    println!("  cd {name}");
    println!("  kasane plugin build        Build the plugin");
    println!("  kasane plugin install      Build, validate, and install");

    Ok(())
}

fn validate_name(name: &str) -> Result<()> {
    if name.is_empty() {
        bail!("plugin name cannot be empty");
    }
    if name.starts_with('-') {
        bail!("plugin name cannot start with '-'");
    }
    if !name.chars().all(|c| c.is_ascii_alphanumeric() || c == '-') {
        bail!("plugin name must contain only ASCII alphanumeric characters and '-'");
    }
    Ok(())
}

fn check_wasm_target() {
    let output = Command::new("rustup")
        .args(["target", "list", "--installed"])
        .output();

    match output {
        Ok(out) => {
            let stdout = String::from_utf8_lossy(&out.stdout);
            if !stdout.lines().any(|l| l.trim() == "wasm32-wasip2") {
                eprintln!(
                    "hint: wasm32-wasip2 target not found. Run: rustup target add wasm32-wasip2"
                );
            }
        }
        Err(_) => {
            eprintln!("hint: rustup not found. Ensure wasm32-wasip2 target is available.");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_validate_name_ok() {
        assert!(validate_name("my-widget").is_ok());
        assert!(validate_name("simple").is_ok());
        assert!(validate_name("a123").is_ok());
    }

    #[test]
    fn test_validate_name_empty() {
        assert!(validate_name("").is_err());
    }

    #[test]
    fn test_validate_name_starts_with_dash() {
        assert!(validate_name("-bad").is_err());
    }

    #[test]
    fn test_validate_name_invalid_chars() {
        assert!(validate_name("my_widget").is_err());
        assert!(validate_name("my widget").is_err());
        assert!(validate_name("foo.bar").is_err());
    }
}
