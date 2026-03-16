use anyhow::{Context, Result};
use kasane_core::config::Config;

use super::build;

pub fn run(path: Option<&str>, release: bool) -> Result<()> {
    let project_dir = path.unwrap_or(".");
    let src_dir = std::path::Path::new(project_dir).join("src");

    if !src_dir.exists() {
        anyhow::bail!("no src/ directory found in '{project_dir}'. Is this a plugin project?");
    }

    if !release {
        println!("(debug build — use `kasane plugin build` for optimized release)");
    }

    // Initial build
    println!("Building plugin...");
    match build_and_install(project_dir, release) {
        Ok(()) => println!("Initial build succeeded. Watching for changes..."),
        Err(e) => eprintln!("Initial build failed: {e:#}"),
    }

    // Watch for changes
    watch_and_rebuild(project_dir, &src_dir, release)
}

fn build_and_install(project_dir: &str, release: bool) -> Result<()> {
    let wasm_path = build::build_plugin(project_dir, release)?;

    let config = Config::load();
    let plugins_dir = config.plugins.plugins_dir();
    std::fs::create_dir_all(&plugins_dir).with_context(|| {
        format!(
            "failed to create plugins directory: {}",
            plugins_dir.display()
        )
    })?;

    let filename = wasm_path
        .file_name()
        .expect("wasm_path should have a filename");
    let dest = plugins_dir.join(filename);
    std::fs::copy(&wasm_path, &dest)
        .with_context(|| format!("failed to copy to {}", dest.display()))?;

    // Touch sentinel file to signal reload
    let reload_sentinel = plugins_dir.join(".reload");
    std::fs::write(&reload_sentinel, "").ok();

    let size = std::fs::metadata(&dest)?.len();
    println!("Installed {} ({} KiB)", dest.display(), size / 1024);

    Ok(())
}

#[cfg(feature = "wasm-plugins")]
fn watch_and_rebuild(project_dir: &str, src_dir: &std::path::Path, release: bool) -> Result<()> {
    use notify::{EventKind, RecursiveMode, Watcher};

    let (tx, rx) = std::sync::mpsc::channel();

    let mut watcher = notify::recommended_watcher(move |res: notify::Result<notify::Event>| {
        if let Ok(event) = res {
            match event.kind {
                EventKind::Create(_) | EventKind::Modify(_) | EventKind::Remove(_) => {
                    let _ = tx.send(());
                }
                _ => {}
            }
        }
    })
    .context("failed to create file watcher")?;

    watcher
        .watch(src_dir, RecursiveMode::Recursive)
        .with_context(|| format!("failed to watch {}", src_dir.display()))?;

    // Also watch Cargo.toml
    let cargo_toml = std::path::Path::new(project_dir).join("Cargo.toml");
    if cargo_toml.exists() {
        watcher.watch(&cargo_toml, RecursiveMode::NonRecursive).ok();
    }

    println!(
        "Watching {} for changes. Press Ctrl+C to stop.",
        src_dir.display()
    );
    println!();

    loop {
        // Wait for first change event
        rx.recv().context("file watcher channel closed")?;

        // Debounce: drain any additional events within 200ms
        let debounce = std::time::Duration::from_millis(200);
        while rx.recv_timeout(debounce).is_ok() {}

        println!("Change detected. Rebuilding...");
        match build_and_install(project_dir, release) {
            Ok(()) => println!("Rebuild succeeded."),
            Err(e) => eprintln!("Rebuild failed: {e:#}"),
        }
        println!();
    }
}

#[cfg(not(feature = "wasm-plugins"))]
fn watch_and_rebuild(_project_dir: &str, _src_dir: &std::path::Path, _release: bool) -> Result<()> {
    anyhow::bail!("wasm-plugins feature not enabled; `kasane plugin dev` requires it")
}
