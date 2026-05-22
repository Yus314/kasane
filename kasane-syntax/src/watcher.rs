//! Filesystem watcher for ADR-051 external Salsa inputs.
//!
//! Wraps `notify-debouncer-full` to deliver coalesced file-change events for
//! one watched file. Robust to editor save-dance (write-temp + rename) by
//! watching the parent directory non-recursively and filtering events whose
//! canonical path matches the target.
//!
//! Production wiring lands in a follow-up chunk; this module is currently
//! standalone with tests gated behind `KASANE_RUN_FS_WATCH_TESTS=1`.

use std::path::{Path, PathBuf};
use std::sync::mpsc::{self, Receiver, Sender};
use std::time::Duration;

use notify::{RecommendedWatcher, RecursiveMode};
use notify_debouncer_full::{DebounceEventResult, Debouncer, RecommendedCache, new_debouncer};
use thiserror::Error;

const DEBOUNCE_TIMEOUT: Duration = Duration::from_millis(150);

#[derive(Debug, Error)]
pub enum FileWatcherError {
    #[error("watcher init failed: {0}")]
    Init(#[source] notify::Error),
    #[error("watch failed for {path}: {source}")]
    Watch {
        path: PathBuf,
        #[source]
        source: notify::Error,
    },
    #[error("unwatch failed: {0}")]
    Unwatch(#[source] notify::Error),
    #[error("canonicalize failed for {path}: {source}")]
    Canonicalize {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
    #[error("path {0} has no parent directory")]
    NoParent(PathBuf),
}

pub struct FileWatcher {
    tx: Sender<PathBuf>,
    rx: Receiver<PathBuf>,
    active: Option<Active>,
}

struct Active {
    canonical: PathBuf,
    _debouncer: Debouncer<RecommendedWatcher, RecommendedCache>,
}

impl FileWatcher {
    pub fn new() -> Result<Self, FileWatcherError> {
        let (tx, rx) = mpsc::channel();
        Ok(Self {
            tx,
            rx,
            active: None,
        })
    }

    /// Watch a single file. Replaces any previous watch.
    pub fn watch(&mut self, path: &Path) -> Result<(), FileWatcherError> {
        self.active = None;
        self.flush_channel();

        let canonical = path
            .canonicalize()
            .map_err(|source| FileWatcherError::Canonicalize {
                path: path.to_path_buf(),
                source,
            })?;
        let parent = canonical
            .parent()
            .ok_or_else(|| FileWatcherError::NoParent(canonical.clone()))?
            .to_path_buf();

        let target = canonical.clone();
        let tx = self.tx.clone();

        let mut debouncer = new_debouncer(
            DEBOUNCE_TIMEOUT,
            None,
            move |res: DebounceEventResult| match res {
                Ok(events) => {
                    for ev in events {
                        let mut matched = false;
                        for p in &ev.event.paths {
                            if path_matches(p, &target) {
                                matched = true;
                                break;
                            }
                        }
                        if matched {
                            // One message per debounced batch is sufficient.
                            let _ = tx.send(target.clone());
                        }
                    }
                }
                Err(errors) => {
                    for e in errors {
                        tracing::warn!(error = %e, "file watcher error");
                    }
                }
            },
        )
        .map_err(FileWatcherError::Init)?;

        debouncer
            .watch(&parent, RecursiveMode::NonRecursive)
            .map_err(|source| FileWatcherError::Watch {
                path: parent.clone(),
                source,
            })?;

        self.active = Some(Active {
            canonical,
            _debouncer: debouncer,
        });
        Ok(())
    }

    /// Stop watching. Idempotent.
    pub fn unwatch(&mut self) -> Result<(), FileWatcherError> {
        self.active = None;
        self.flush_channel();
        Ok(())
    }

    /// Drain and discard any pending messages on the channel. Used to
    /// prevent events from a previous target from being delivered to a
    /// caller after a `watch()` / `unwatch()` transition.
    fn flush_channel(&mut self) {
        while self.rx.try_recv().is_ok() {}
    }

    /// Return the path currently watched (canonical form), if any.
    pub fn watched(&self) -> Option<&Path> {
        self.active.as_ref().map(|a| a.canonical.as_path())
    }

    /// Drain all pending change notifications without blocking.
    ///
    /// Successive duplicates are collapsed; the returned `Vec` contains at
    /// most one entry per distinct target observed in the drain window.
    pub fn try_recv_all(&mut self) -> Vec<PathBuf> {
        let mut out: Vec<PathBuf> = Vec::new();
        while let Ok(p) = self.rx.try_recv() {
            if out.last() != Some(&p) {
                out.push(p);
            }
        }
        out
    }

    /// Explicit shutdown that drops the watcher thread.
    ///
    /// `Drop` performs the same teardown, but `stop` makes the intent
    /// explicit and lets callers observe completion (G6).
    pub fn stop(mut self) {
        self.active = None;
    }
}

fn path_matches(observed: &Path, target: &Path) -> bool {
    if observed == target {
        return true;
    }
    // Removed/renamed paths may no longer canonicalize; fall back to a
    // direct comparison above. For existing paths we canonicalize so that
    // symlinks and `.`/`..` segments do not mask a match (G4).
    matches!(observed.canonicalize(), Ok(c) if c == target)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::time::{Duration, Instant};
    use tempfile::tempdir;

    fn fs_tests_enabled() -> bool {
        std::env::var("KASANE_RUN_FS_WATCH_TESTS").as_deref() == Ok("1")
    }

    fn skip_unless_enabled(name: &str) -> bool {
        if fs_tests_enabled() {
            false
        } else {
            eprintln!("[skip] {name}: set KASANE_RUN_FS_WATCH_TESTS=1 to run");
            true
        }
    }

    fn wait_for_event(watcher: &mut FileWatcher, timeout: Duration) -> Vec<PathBuf> {
        let deadline = Instant::now() + timeout;
        loop {
            let events = watcher.try_recv_all();
            if !events.is_empty() {
                return events;
            }
            if Instant::now() >= deadline {
                return Vec::new();
            }
            std::thread::sleep(Duration::from_millis(25));
        }
    }

    #[test]
    fn happy_path_modify_emits_event() {
        if skip_unless_enabled("happy_path_modify_emits_event") {
            return;
        }
        let dir = tempdir().expect("tempdir");
        let file = dir.path().join("buf.txt");
        fs::write(&file, b"hello").expect("write");

        let mut w = FileWatcher::new().expect("new");
        w.watch(&file).expect("watch");

        // notify needs a moment to install the watch on some platforms.
        std::thread::sleep(Duration::from_millis(100));

        fs::write(&file, b"world").expect("modify");

        let events = wait_for_event(&mut w, Duration::from_secs(3));
        assert!(!events.is_empty(), "no event observed within timeout");
        let canonical = file.canonicalize().unwrap();
        assert!(
            events.iter().all(|p| *p == canonical),
            "unexpected paths in events: {events:?}"
        );
    }

    #[test]
    fn save_dance_rename_emits_event() {
        if skip_unless_enabled("save_dance_rename_emits_event") {
            return;
        }
        let dir = tempdir().expect("tempdir");
        let file = dir.path().join("buf.txt");
        let tmp = dir.path().join("buf.txt.tmp");
        fs::write(&file, b"v1").expect("write initial");

        let mut w = FileWatcher::new().expect("new");
        w.watch(&file).expect("watch");

        std::thread::sleep(Duration::from_millis(100));

        fs::write(&tmp, b"v2").expect("write tmp");
        fs::rename(&tmp, &file).expect("atomic rename");

        let events = wait_for_event(&mut w, Duration::from_secs(3));
        assert!(
            !events.is_empty(),
            "rename did not produce a target-path event"
        );
        let canonical = file.canonicalize().unwrap();
        assert!(events.contains(&canonical));
    }

    #[test]
    fn watch_missing_path_returns_error_not_panic() {
        // This test is cheap and platform-portable; run unconditionally so
        // the error surface stays covered without the FS-events env gate.
        let mut w = FileWatcher::new().expect("new");
        let missing = PathBuf::from("/nonexistent/kasane-watcher/path/file.txt");
        let err = w.watch(&missing).expect_err("expected error");
        assert!(
            matches!(err, FileWatcherError::Canonicalize { .. }),
            "unexpected error variant: {err:?}"
        );
    }

    #[test]
    fn stop_completes_promptly() {
        if skip_unless_enabled("stop_completes_promptly") {
            return;
        }
        let dir = tempdir().expect("tempdir");
        let file = dir.path().join("buf.txt");
        fs::write(&file, b"x").expect("write");

        let mut w = FileWatcher::new().expect("new");
        w.watch(&file).expect("watch");

        let start = Instant::now();
        w.stop();
        let elapsed = start.elapsed();
        assert!(
            elapsed < Duration::from_secs(2),
            "stop() took {elapsed:?}; expected prompt shutdown (G6)"
        );
    }
}
