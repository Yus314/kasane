//! [`SyntaxManager`] — lifecycle management for per-buffer syntax providers.

use std::path::PathBuf;
use std::sync::Arc;
use std::time::SystemTime;

use kasane_core::state::AppState;

use crate::grammar::{GrammarRegistry, language_for_extension};
use crate::provider::TreeSitterProvider;

/// Manages the active syntax provider for the current buffer.
///
/// Detects buffer changes (via `ui_options["buffile"]`), reads the file from
/// disk, and triggers tree-sitter re-parse when content changes.
pub struct SyntaxManager {
    registry: GrammarRegistry,
    active: Option<ActiveBuffer>,
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
        Self {
            registry: GrammarRegistry::new(),
            active: None,
        }
    }

    /// Update the syntax provider based on current application state.
    ///
    /// Reads `ui_options["buffile"]` to detect buffer identity, checks
    /// file modification time, and re-parses if the content changed.
    /// Sets `state.runtime.syntax_provider` with the current provider.
    pub fn update(&mut self, state: &mut AppState) {
        let buffile = match state.observed.ui_options.get("buffile") {
            Some(path) if !path.is_empty() && path != "*scratch*" => PathBuf::from(path),
            _ => {
                // No file — clear provider.
                if self.active.is_some() {
                    self.active = None;
                    state.runtime.syntax_provider = None;
                }
                return;
            }
        };

        // Detect language from extension.
        let ext = buffile.extension().and_then(|e| e.to_str()).unwrap_or("");
        let Some(lang_name) = language_for_extension(ext) else {
            // Unsupported extension — clear provider.
            if self.active.is_some() {
                self.active = None;
                state.runtime.syntax_provider = None;
            }
            return;
        };

        // Check if we're already tracking this file+language.
        if let Some(active) = &mut self.active
            && active.buffile == buffile
            && active.language == lang_name
        {
            // Same file — check mtime for re-parse.
            if let Ok(meta) = std::fs::metadata(&buffile)
                && let Ok(mtime) = meta.modified()
                && mtime != active.file_mtime
                && let Ok(source) = std::fs::read(&buffile)
                && let Some(provider) = Arc::get_mut(&mut active.provider)
            {
                provider.update(&source);
                active.file_mtime = mtime;
                state.runtime.syntax_provider = Some(active.provider.clone());
            }
            return;
        }

        // New file or language change — create a new provider.
        let Some(entry) = self.registry.get_or_load(lang_name) else {
            self.active = None;
            state.runtime.syntax_provider = None;
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

        // Initial parse.
        if let Ok(source) = std::fs::read(&buffile) {
            provider.update(&source);
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
