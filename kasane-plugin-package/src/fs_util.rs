use std::fs;
use std::path::{Path, PathBuf};

/// Errors raised by `fs_util` helpers.
///
/// Each variant pins the offending filesystem path next to the underlying
/// `std::io::Error` so callers logging at the binary boundary can surface
/// "what failed where" without re-wrapping.
#[derive(Debug, thiserror::Error)]
pub enum FsUtilError {
    #[error("failed to read {path}: {source}", path = path.display())]
    ReadDir {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
    #[error("failed to read entry under {path}: {source}", path = path.display())]
    ReadEntry {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
    #[error("failed to inspect {path}: {source}", path = path.display())]
    InspectFileType {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
}

/// Recursively collect all `.kpk` package paths under `root`.
pub fn collect_kpk_paths(root: &Path) -> Result<Vec<PathBuf>, FsUtilError> {
    let mut paths = Vec::new();
    collect_recursive(root, &mut paths)?;
    Ok(paths)
}

fn collect_recursive(root: &Path, out: &mut Vec<PathBuf>) -> Result<(), FsUtilError> {
    let entries = match fs::read_dir(root) {
        Ok(entries) => entries,
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(source) => {
            return Err(FsUtilError::ReadDir {
                path: root.to_path_buf(),
                source,
            });
        }
    };

    for entry in entries {
        let entry = entry.map_err(|source| FsUtilError::ReadEntry {
            path: root.to_path_buf(),
            source,
        })?;
        let path = entry.path();
        let file_type = entry
            .file_type()
            .map_err(|source| FsUtilError::InspectFileType {
                path: path.clone(),
                source,
            })?;
        if file_type.is_dir() {
            collect_recursive(&path, out)?;
        } else if path.extension().and_then(std::ffi::OsStr::to_str) == Some("kpk") {
            out.push(path);
        }
    }

    Ok(())
}
