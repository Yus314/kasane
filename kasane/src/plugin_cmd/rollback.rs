use std::fs;
use std::path::{Path, PathBuf};
use std::time::UNIX_EPOCH;

use anyhow::{Context, Result};

use crate::plugin_lock::{
    PluginsLock, plugins_lock_history_paths, plugins_lock_path, rollback_plugins_lock,
};

pub fn run(list: bool) -> Result<()> {
    if list {
        return run_list();
    }

    let config = kasane_core::config::Config::try_load()?;
    let _guard = crate::workspace_lock::acquire_workspace_lock(&config.plugins.plugins_dir())?;

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

fn run_list() -> Result<()> {
    let lock_path = plugins_lock_path();
    let current = PluginsLock::load()?;
    let mut history_paths = plugins_lock_history_paths()?;
    history_paths.reverse();

    println!("Plugins lock history");
    println!("Current: {}", lock_path.display());
    println!("  plugins: {}", current.plugins.len());

    if history_paths.is_empty() {
        println!("History: none");
        return Ok(());
    }

    println!("History:");
    for (index, path) in history_paths.iter().enumerate() {
        let summary = summarize_generation(path, &current)
            .with_context(|| format!("failed to summarize {}", path.display()))?;
        println!("  {}. {}", index + 1, summary.path.display());
        println!(
            "     modified: {}",
            format_unix_timestamp(summary.modified_unix_seconds)
        );
        println!("     plugins: {}", summary.plugin_count);
        println!("     differs from current: {}", summary.changed_plugins);
    }

    Ok(())
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct LockGenerationSummary {
    path: PathBuf,
    modified_unix_seconds: u64,
    plugin_count: usize,
    changed_plugins: usize,
}

fn summarize_generation(path: &Path, current: &PluginsLock) -> Result<LockGenerationSummary> {
    let lock = PluginsLock::load_from_path(path)?;
    let modified_unix_seconds = fs::metadata(path)
        .and_then(|meta| meta.modified())
        .ok()
        .and_then(|modified| modified.duration_since(UNIX_EPOCH).ok())
        .map(|duration| duration.as_secs())
        .unwrap_or(0);

    Ok(LockGenerationSummary {
        path: path.to_path_buf(),
        modified_unix_seconds,
        plugin_count: lock.plugins.len(),
        changed_plugins: count_changed_plugins(current, &lock),
    })
}

fn format_unix_timestamp(secs: u64) -> String {
    // Civil date from epoch via pure arithmetic (no external deps).
    let days = (secs / 86400) as i64;
    let time_secs = secs % 86400;
    let hours = time_secs / 3600;
    let minutes = (time_secs % 3600) / 60;
    let seconds = time_secs % 60;

    // Convert days since 1970-01-01 to civil date (algorithm from Howard Hinnant).
    let z = days + 719468;
    let era = (if z >= 0 { z } else { z - 146096 }) / 146097;
    let doe = (z - era * 146097) as u64;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
    let y = yoe as i64 + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = if m <= 2 { y + 1 } else { y };

    format!("{y:04}-{m:02}-{d:02} {hours:02}:{minutes:02}:{seconds:02} UTC")
}

fn count_changed_plugins(current: &PluginsLock, other: &PluginsLock) -> usize {
    let mut changed = 0;
    for (plugin_id, entry) in &current.plugins {
        match other.plugins.get(plugin_id) {
            Some(other_entry) if other_entry.artifact_digest == entry.artifact_digest => {}
            _ => changed += 1,
        }
    }
    for plugin_id in other.plugins.keys() {
        if !current.plugins.contains_key(plugin_id) {
            changed += 1;
        }
    }
    changed
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::plugin_lock::LockedPluginEntry;

    fn make_lock(plugin_id: &str, digest: &str) -> PluginsLock {
        let mut lock = PluginsLock::new();
        lock.plugins.insert(
            plugin_id.to_string(),
            LockedPluginEntry {
                plugin_id: plugin_id.to_string(),
                package: Some(format!("example/{plugin_id}")),
                version: Some("0.1.0".to_string()),
                artifact_digest: digest.to_string(),
                code_digest: Some(format!("{digest}-code")),
                source_kind: "filesystem".to_string(),
                abi_version: Some("0.25.0".to_string()),
            },
        );
        lock
    }

    #[test]
    fn format_unix_timestamp_produces_correct_civil_date() {
        assert_eq!(format_unix_timestamp(0), "1970-01-01 00:00:00 UTC");
        assert_eq!(format_unix_timestamp(1711878134), "2024-03-31 09:42:14 UTC");
        assert_eq!(format_unix_timestamp(1609459200), "2021-01-01 00:00:00 UTC");
    }

    #[test]
    fn count_changed_plugins_counts_additions_and_digest_changes() {
        let current = make_lock("sel_badge", "sha256:one");
        let mut other = make_lock("sel_badge", "sha256:two");
        other.plugins.insert(
            "cursor_line".to_string(),
            LockedPluginEntry {
                plugin_id: "cursor_line".to_string(),
                package: Some("builtin/cursor-line".to_string()),
                version: Some("0.3.0".to_string()),
                artifact_digest: "sha256:cursor".to_string(),
                code_digest: Some("sha256:cursor-code".to_string()),
                source_kind: "bundled".to_string(),
                abi_version: Some("0.25.0".to_string()),
            },
        );

        assert_eq!(count_changed_plugins(&current, &other), 2);
    }
}
