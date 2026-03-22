use std::io::BufRead;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use anyhow::{Context, Result, bail};

pub fn run(path: Option<&str>) -> Result<()> {
    let wasm_path = build_plugin(path.unwrap_or("."), true)?;
    let size = std::fs::metadata(&wasm_path)?.len();
    println!("Built {} ({} KiB)", wasm_path.display(), size / 1024);
    Ok(())
}

/// Build the plugin and return the path to the output .wasm file.
///
/// This is public so `install` and `dev` can reuse it.
/// When `release` is false, builds in debug mode for faster iteration.
pub fn build_plugin(project_dir: &str, release: bool) -> Result<PathBuf> {
    let project_path = Path::new(project_dir);
    if !project_path.join("Cargo.toml").exists() {
        bail!(
            "no Cargo.toml found in '{}'. Is this a plugin project?",
            project_path.display()
        );
    }

    let mut args = vec![
        "build",
        "--target",
        "wasm32-wasip2",
        "--message-format=json",
    ];
    if release {
        args.push("--release");
    }

    let mut child = Command::new("cargo")
        .args(&args)
        .current_dir(project_path)
        .stdout(Stdio::piped())
        .stderr(Stdio::inherit())
        .spawn()
        .context("failed to run `cargo build`")?;

    let stdout = child.stdout.take().expect("stdout piped in Command setup");
    let reader = std::io::BufReader::new(stdout);

    let mut wasm_path: Option<PathBuf> = None;

    for line in reader.lines() {
        let line = line?;
        let Ok(val) = serde_json::from_str::<serde_json::Value>(&line) else {
            continue;
        };
        if val.get("reason").and_then(|r| r.as_str()) != Some("compiler-artifact") {
            continue;
        }
        let is_cdylib = val
            .get("target")
            .and_then(|t| t.get("crate_types"))
            .and_then(|ct| ct.as_array())
            .is_some_and(|arr| arr.iter().any(|v| v.as_str() == Some("cdylib")));
        if !is_cdylib {
            continue;
        }
        if let Some(filenames) = val.get("filenames").and_then(|f| f.as_array()) {
            for f in filenames {
                if let Some(s) = f.as_str()
                    && s.ends_with(".wasm")
                {
                    wasm_path = Some(PathBuf::from(s));
                }
            }
        }
    }

    let status = child.wait()?;
    if !status.success() {
        eprintln!();
        eprintln!("hint: Run `kasane plugin doctor` to diagnose your environment");
        eprintln!("hint: Common cause: missing target. Run `rustup target add wasm32-wasip2`");
        bail!("cargo build failed");
    }

    // Fallback: scan target directory if JSON didn't yield the path
    if wasm_path.is_none() {
        let profile = if release { "release" } else { "debug" };
        let target_dir = project_path.join(format!("target/wasm32-wasip2/{profile}"));
        if target_dir.exists()
            && let Ok(entries) = std::fs::read_dir(&target_dir)
        {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.extension().and_then(|e| e.to_str()) == Some("wasm") {
                    wasm_path = Some(path);
                    break;
                }
            }
        }
    }

    wasm_path.ok_or_else(|| anyhow::anyhow!("no .wasm file found after build"))
}
