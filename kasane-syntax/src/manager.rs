//! [`SyntaxManager`] — lifecycle management for per-buffer syntax providers.

use std::path::PathBuf;
use std::sync::Arc;
use std::time::SystemTime;

use kasane_core::state::AppState;

use crate::grammar::{GrammarRegistry, language_for_extension};
use crate::provider::TreeSitterProvider;
use crate::watcher::FileWatcher;

/// Manages the active syntax provider for the current buffer.
///
/// Detects buffer changes (via `ui_options["buffile"]`), reads the file from
/// disk, and triggers tree-sitter re-parse when content changes. Reparse is
/// driven by two parallel signals (per ADR-051 chunk 3b):
///
/// - A `notify`-backed [`FileWatcher`] when available (primary).
/// - `std::fs::metadata` mtime polling (fallback for NFS/FUSE and the
///   initial baseline against which the watcher is cross-validated).
///
/// When the two signals disagree, the divergence is logged via `tracing`
/// at warn level. The reparse fires whenever *either* signal indicates a
/// change so a missed event on one channel cannot starve the parser.
pub struct SyntaxManager {
    registry: GrammarRegistry,
    active: Option<ActiveBuffer>,
    watcher: Option<FileWatcher>,
}

struct ActiveBuffer {
    buffile: PathBuf,
    language: String,
    provider: Arc<TreeSitterProvider>,
    file_mtime: SystemTime,
}

impl SyntaxManager {
    /// Create a new syntax manager with default grammar search paths.
    pub fn new() -> Self {
        let watcher = match FileWatcher::new() {
            Ok(w) => Some(w),
            Err(e) => {
                tracing::warn!(error = %e, "FileWatcher init failed; falling back to mtime polling");
                None
            }
        };
        Self {
            registry: GrammarRegistry::new(),
            active: None,
            watcher,
        }
    }

    /// Update the syntax provider based on current application state.
    ///
    /// Reads `ui_options["buffile"]` to detect buffer identity, drains the
    /// file-watcher channel, checks mtime, and re-parses on either signal.
    /// Sets `state.runtime.syntax_provider` with the current provider.
    pub fn update(&mut self, state: &mut AppState) {
        let buffile = match state.observed.ui_options.get("buffile") {
            Some(path) if !path.is_empty() && path != "*scratch*" => PathBuf::from(path),
            _ => {
                self.clear(state);
                return;
            }
        };

        let ext = buffile.extension().and_then(|e| e.to_str()).unwrap_or("");
        let Some(lang_name) = language_for_extension(ext) else {
            self.clear(state);
            return;
        };

        // Drain the watcher *before* touching `self.active` so the
        // borrow does not conflict with the let-chain below.
        let watcher_fired = self.drain_watcher();

        if let Some(active) = &mut self.active
            && active.buffile == buffile
            && active.language == lang_name
        {
            let current_mtime = std::fs::metadata(&buffile).and_then(|m| m.modified()).ok();
            let mtime_changed = current_mtime.is_some_and(|m| m != active.file_mtime);

            match (watcher_fired, mtime_changed) {
                (true, false) => tracing::warn!(
                    buffile = %buffile.display(),
                    "watcher fired but mtime unchanged"
                ),
                (false, true) => tracing::warn!(
                    buffile = %buffile.display(),
                    "mtime changed without watcher event (NFS/FUSE fallback?)"
                ),
                _ => {}
            }

            if (watcher_fired || mtime_changed)
                && let Ok(source) = std::fs::read(&buffile)
                && let Some(provider) = Arc::get_mut(&mut active.provider)
            {
                provider.update(&source);
                if let Some(m) = current_mtime {
                    active.file_mtime = m;
                }
                state.runtime.syntax_provider = Some(active.provider.clone());
            }
            return;
        }

        // New file or language change — create a new provider and rewatch.
        let Some(entry) = self.registry.get_or_load(lang_name) else {
            self.clear(state);
            return;
        };

        let fold_query = entry.make_fold_query();
        let declaration_query = entry.make_declaration_query();

        let mut provider = TreeSitterProvider::new(
            entry.language.clone(),
            lang_name.to_string(),
            fold_query,
            declaration_query,
        );

        let file_mtime = std::fs::metadata(&buffile)
            .and_then(|m| m.modified())
            .unwrap_or(SystemTime::UNIX_EPOCH);

        if let Ok(source) = std::fs::read(&buffile) {
            provider.update(&source);
        }

        if let Some(w) = &mut self.watcher
            && let Err(e) = w.watch(&buffile)
        {
            tracing::warn!(
                buffile = %buffile.display(),
                error = %e,
                "FileWatcher::watch failed; mtime polling will drive reparse"
            );
        }

        let provider = Arc::new(provider);
        state.runtime.syntax_provider = Some(provider.clone());

        self.active = Some(ActiveBuffer {
            buffile,
            language: lang_name.to_string(),
            provider,
            file_mtime,
        });
    }

    fn clear(&mut self, state: &mut AppState) {
        if self.active.is_some() {
            self.active = None;
            state.runtime.syntax_provider = None;
        }
        if let Some(w) = &mut self.watcher {
            let _ = w.unwatch();
        }
    }

    fn drain_watcher(&mut self) -> bool {
        self.watcher
            .as_mut()
            .map(|w| !w.try_recv_all().is_empty())
            .unwrap_or(false)
    }
}

impl Default for SyntaxManager {
    fn default() -> Self {
        Self::new()
    }
}

impl kasane_core::event_loop::PreRenderHook for SyntaxManager {
    fn pre_render(&mut self, state: &mut AppState) {
        self.update(state);
    }
}
