use anyhow::Result;
use kasane_core::config::Config;

use crate::plugin_lock::PluginsLock;
use crate::plugin_store::PluginStore;

pub fn run() -> Result<()> {
    let config = Config::try_load()?;
    let lock = PluginsLock::load()?;
    let store = PluginStore::from_plugins_dir(config.plugins.plugins_dir());
    let gc = store.garbage_collect(&lock)?;

    if gc.removed_paths.is_empty() {
        println!("No unreferenced packages.");
        return Ok(());
    }

    println!(
        "Removed {} unreferenced package(s), reclaimed {} bytes.",
        gc.removed_paths.len(),
        gc.removed_bytes
    );
    for path in gc.removed_paths {
        println!("  {}", path.display());
    }

    Ok(())
}
