use anyhow::Result;

use crate::plugin_lock::{PluginsLock, plugins_lock_path, rollback_plugins_lock};

pub fn run() -> Result<()> {
    let lock_path = plugins_lock_path();
    match rollback_plugins_lock()? {
        Some(restored_from) => {
            let active = PluginsLock::load()?;
            if let Ok(config) = kasane_core::config::Config::try_load() {
                super::package_artifact::touch_reload_sentinel(&config.plugins.plugins_dir());
            }
            println!("Rolled back plugins lock.");
            println!("Restored from: {}", restored_from.display());
            println!("Lock: {}", lock_path.display());
            println!("Active plugins: {}", active.plugins.len());
        }
        None => {
            println!("No previous plugins.lock generation is available.");
            println!("Lock: {}", lock_path.display());
        }
    }

    Ok(())
}
