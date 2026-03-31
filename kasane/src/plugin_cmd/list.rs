use anyhow::Result;
use kasane_core::config::Config;

use crate::plugin_lock::PluginsLock;

pub fn run() -> Result<()> {
    let config = Config::load();
    let plugins_dir = config.plugins.plugins_dir();
    let packages = super::package_artifact::discover_installed_packages(&plugins_dir)?;
    let lock = PluginsLock::load()?;

    if packages.is_empty() {
        println!("No plugins installed in {}", plugins_dir.display());
        return Ok(());
    }

    println!("Installed packages ({}):", plugins_dir.display());

    for package in packages {
        match package {
            super::package_artifact::DiscoveredPackage::Valid { path, inspected } => {
                let size = std::fs::metadata(&path).map(|m| m.len()).unwrap_or(0);
                let state = lock
                    .plugins
                    .get(&inspected.header.plugin.id)
                    .filter(|entry| entry.artifact_digest == inspected.header.digests.artifact)
                    .map_or("installed", |_| "active");
                println!(
                    "  {:<20} {:<30} {:<10} ({} KiB)",
                    inspected.header.plugin.id,
                    super::package_artifact::package_label(&inspected),
                    state,
                    size / 1024
                );
            }
            super::package_artifact::DiscoveredPackage::Invalid { path, error } => {
                let filename = path.file_name().unwrap_or_default().to_string_lossy();
                println!("  {:<20} {:<30} invalid", "(invalid)", filename);
                println!("    reason: {error}");
            }
        }
    }

    Ok(())
}
