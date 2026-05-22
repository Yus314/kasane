//! [`SyntaxManager`] — lifecycle management for per-buffer syntax providers.

use std::path::PathBuf;
use std::sync::Arc;
use std::time::SystemTime;

use kasane_core::event_loop::FrameSyncHook;
use kasane_core::salsa_inputs::external::{
    BackPressurePolicy, ExternalInputId, ExternalInputRegistry,
};
use kasane_core::state::AppState;

use crate::grammar::{GrammarRegistry, language_for_extension};
use crate::provider::TreeSitterProvider;
use crate::watcher::FileWatcher;

/// Manages the active syntax provider for the current buffer.
///
/// Lifecycle and reparse are split across two phases per ADR-051 chunk 3c:
///
/// - **`pre_render`** (publish phase): detect buffer/language changes,
///   create or tear down the [`TreeSitterProvider`], and drain the
///   [`FileWatcher`] channel into the host's
///   [`ExternalInputRegistry`].
/// - **`post_sync`** (consume phase): after the frame-boundary drain,
///   inspect the registry slot; if it is dirty (or the mtime fallback
///   path detected change) reparse the file. Cross-validation between
///   the watcher and the mtime poll is logged at `tracing::warn`.
pub struct SyntaxManager {
    registry: GrammarRegistry,
    active: Option<ActiveBuffer>,
    watcher: Option<FileWatcher>,
    syntax_reload_id: Option<ExternalInputId<PathBuf>>,
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
            syntax_reload_id: None,
        }
    }

    fn ensure_registered(
        &mut self,
        external: &mut ExternalInputRegistry,
    ) -> ExternalInputId<PathBuf> {
        if let Some(id) = self.syntax_reload_id {
            return id;
        }
        let id = external.register::<PathBuf>("syntax.reload", BackPressurePolicy::Coalesce);
        self.syntax_reload_id = Some(id);
        id
    }

    /// Pre-render: lifecycle (provider setup, watch/unwatch) and publish
    /// pending watcher events into the registry.
    fn run_pre_render(&mut self, state: &mut AppState, external: &mut ExternalInputRegistry) {
        let handle = self.ensure_registered(external);

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

        // Publish any pending watcher events for the currently-watched
        // file. The post_sync consumer reads the resulting registry
        // slot to decide whether to reparse.
        if let Some(w) = &mut self.watcher {
            for path in w.try_recv_all() {
                external.commit(handle, path);
            }
        }

        // Same file + language: nothing else to do here. Reparse decision
        // lives in post_sync once the registry has drained.
        if let Some(active) = &self.active
            && active.buffile == buffile
            && active.language == lang_name
        {
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

    /// Post-sync: read the registry slot drained this frame; reparse if
    /// either the watcher fired or the mtime fallback detected change.
    fn run_post_sync(&mut self, state: &mut AppState, external: &ExternalInputRegistry) {
        let Some(handle) = self.syntax_reload_id else {
            return;
        };
        let Some(active) = self.active.as_mut() else {
            return;
        };

        // The registry slot is global; verify the most recently committed
        // path matches our currently-active buffer before treating
        // `is_dirty` as authoritative for this file.
        let registry_path = external.last(handle).cloned();
        let canonical_active = active.buffile.canonicalize().ok();
        let registry_matches = registry_path
            .as_ref()
            .zip(canonical_active.as_ref())
            .is_some_and(|(r, a)| r == a);
        let registry_fired = registry_matches && external.is_dirty(handle);

        let current_mtime = std::fs::metadata(&active.buffile)
            .and_then(|m| m.modified())
            .ok();
        let mtime_changed = current_mtime.is_some_and(|m| m != active.file_mtime);

        match (registry_fired, mtime_changed) {
            (true, false) => tracing::warn!(
                buffile = %active.buffile.display(),
                "registry fired but mtime unchanged"
            ),
            (false, true) => tracing::warn!(
                buffile = %active.buffile.display(),
                "mtime changed without registry event (NFS/FUSE fallback?)"
            ),
            _ => {}
        }

        if (registry_fired || mtime_changed)
            && let Ok(source) = std::fs::read(&active.buffile)
            && let Some(provider) = Arc::get_mut(&mut active.provider)
        {
            provider.update(&source);
            if let Some(m) = current_mtime {
                active.file_mtime = m;
            }
            state.runtime.syntax_provider = Some(active.provider.clone());
        }
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
}

impl Default for SyntaxManager {
    fn default() -> Self {
        Self::new()
    }
}

impl FrameSyncHook for SyntaxManager {
    fn pre_render(&mut self, state: &mut AppState, external: &mut ExternalInputRegistry) {
        self.run_pre_render(state, external);
    }

    fn post_sync(&mut self, state: &mut AppState, external: &ExternalInputRegistry) {
        self.run_post_sync(state, external);
    }
}
