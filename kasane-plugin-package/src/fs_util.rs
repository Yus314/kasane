use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};

/// Recursively collect all `.kpk` package paths under `root`.
pub fn collect_kpk_paths(root: &Path) -> Result<Vec<PathBuf>> {
    let mut paths = Vec::new();
    collect_recursive(root, &mut paths)?;
    Ok(paths)
}

fn collect_recursive(root: &Path, out: &mut Vec<PathBuf>) -> Result<()> {
    let entries = match fs::read_dir(root) {
        Ok(entries) => entries,
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(err) => return Err(err).with_context(|| format!("failed to read {}", root.display())),
    };

    for entry in entries {
        let entry = entry.with_context(|| format!("failed to read {}", root.display()))?;
        let path = entry.path();
        let file_type = entry
            .file_type()
            .with_context(|| format!("failed to inspect {}", path.display()))?;
        if file_type.is_dir() {
            collect_recursive(&path, out)?;
        } else if path.extension().and_then(std::ffi::OsStr::to_str) == Some("kpk") {
            out.push(path);
        }
    }

    Ok(())
}
