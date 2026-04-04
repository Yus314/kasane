//! Advisory file locking for serializing Store and Lock mutations.
//!
//! Prevents TOCTOU races between concurrent CLI invocations (e.g., `gc ∥ install`).
//! Uses `flock(LOCK_EX)` via the `fs2` crate. The lock is released when
//! the returned guard is dropped.

use std::fs::{self, File};
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use fs2::FileExt;

/// RAII guard that holds an exclusive advisory lock on the workspace lockfile.
///
/// The lock is released when this guard is dropped (the file is closed).
pub struct WorkspaceLockGuard {
    _file: File,
    path: PathBuf,
}

impl WorkspaceLockGuard {
    pub fn path(&self) -> &Path {
        &self.path
    }
}

impl std::fmt::Debug for WorkspaceLockGuard {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("WorkspaceLockGuard")
            .field("path", &self.path)
            .finish()
    }
}

/// Acquire an exclusive advisory lock on the plugins workspace.
///
/// The lock file is placed alongside the plugins.lock file as `.kasane-plugins.lk`.
/// Blocks until the lock is acquired.
pub fn acquire_workspace_lock(plugins_dir: &Path) -> Result<WorkspaceLockGuard> {
    let lock_path = plugins_dir.join(".kasane-plugins.lk");
    if let Some(parent) = lock_path.parent() {
        fs::create_dir_all(parent)
            .with_context(|| format!("failed to create {}", parent.display()))?;
    }
    let file = File::options()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .open(&lock_path)
        .with_context(|| format!("failed to open lock file {}", lock_path.display()))?;
    file.lock_exclusive().with_context(|| {
        format!(
            "failed to acquire exclusive lock on {}",
            lock_path.display()
        )
    })?;
    Ok(WorkspaceLockGuard {
        _file: file,
        path: lock_path,
    })
}

/// Try to acquire an exclusive advisory lock without blocking.
///
/// Returns `None` if another process holds the lock.
pub fn try_acquire_workspace_lock(plugins_dir: &Path) -> Result<Option<WorkspaceLockGuard>> {
    let lock_path = plugins_dir.join(".kasane-plugins.lk");
    if let Some(parent) = lock_path.parent() {
        fs::create_dir_all(parent)
            .with_context(|| format!("failed to create {}", parent.display()))?;
    }
    let file = File::options()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .open(&lock_path)
        .with_context(|| format!("failed to open lock file {}", lock_path.display()))?;
    match file.try_lock_exclusive() {
        Ok(()) => Ok(Some(WorkspaceLockGuard {
            _file: file,
            path: lock_path,
        })),
        Err(err) if err.kind() == std::io::ErrorKind::WouldBlock => Ok(None),
        Err(err) => {
            Err(err).with_context(|| format!("failed to acquire lock on {}", lock_path.display()))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn acquire_and_release_workspace_lock() {
        let tmp = tempfile::tempdir().unwrap();
        let guard = acquire_workspace_lock(tmp.path()).unwrap();
        assert!(guard.path().exists());
        drop(guard);
    }

    #[test]
    fn try_acquire_fails_when_held() {
        let tmp = tempfile::tempdir().unwrap();
        let _guard = acquire_workspace_lock(tmp.path()).unwrap();
        let result = try_acquire_workspace_lock(tmp.path()).unwrap();
        assert!(result.is_none());
    }

    #[test]
    fn try_acquire_succeeds_when_free() {
        let tmp = tempfile::tempdir().unwrap();
        let result = try_acquire_workspace_lock(tmp.path()).unwrap();
        assert!(result.is_some());
    }
}
