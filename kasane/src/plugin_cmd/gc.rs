use anyhow::Result;
use kasane_core::config::Config;

use crate::plugin_lock::{PluginsLock, prune_plugins_lock_history};
use crate::plugin_store::PluginStore;

pub fn run(prune_history: bool, keep_generations: usize) -> Result<()> {
    let config = Config::try_load()?;
    let _guard = crate::workspace_lock::acquire_workspace_lock(&config.plugins.plugins_dir())?;
    let removed_history = if prune_history {
        prune_plugins_lock_history(keep_generations)?
    } else {
        Vec::new()
    };
    let lock = PluginsLock::load()?;
    let store = PluginStore::from_plugins_dir(config.plugins.plugins_dir());
    let gc = store.garbage_collect(&lock)?;

    if gc.removed_paths.is_empty() && removed_history.is_empty() {
        println!("No unreferenced packages.");
        return Ok(());
    }

    if !removed_history.is_empty() {
        println!(
            "Pruned {} archived lock generation(s), keeping the latest {}.",
            removed_history.len(),
            keep_generations
        );
        for path in removed_history {
            println!("  {}", path.display());
        }
    }

    if !gc.removed_paths.is_empty() {
        println!(
            "Removed {} unreferenced package(s), reclaimed {} bytes.",
            gc.removed_paths.len(),
            gc.removed_bytes
        );
        for path in gc.removed_paths {
            println!("  {}", path.display());
        }
    }

    Ok(())
}
