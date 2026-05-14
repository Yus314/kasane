//! Hooks the configuration hot-reload path into the plugin resolve pipeline.
//!
//! `kasane-core` knows how to detect that `plugins`/`settings` changed in
//! `kasane.kdl`, but resolving installed packages and writing `plugins.lock`
//! lives in the `kasane` binary crate (it depends on `kasane_wasm` and
//! `kasane_plugin_package`). Implementations of this trait bridge the two
//! sides without forcing `kasane-core` to depend on the package layer.

use crate::config::Config;
use crate::error::DynError;
use crate::plugin::PluginDiagnostic;

/// Outcome of an attempted resolve + lock-update + sentinel touch.
#[derive(Default)]
pub struct ResolveOutcome {
    /// Diagnostics surfaced during resolution (unresolved IDs, version
    /// conflicts, invalid packages, etc.).
    pub diagnostics: Vec<PluginDiagnostic>,
    /// True if the lock file was updated and the reload sentinel touched —
    /// the caller can use this to decide whether to await a `PluginReload`
    /// event or skip the reload entirely.
    pub touched_sentinel: bool,
}

/// Bridges the kdl-watcher hot-reload path into the resolve + lock pipeline.
///
/// The default implementation lives in the `kasane` binary crate. Tests in
/// `kasane-core` can use a trivial mock.
pub trait ReloadOrchestrator: Send {
    /// Run resolve, write `plugins.lock`, and touch the reload sentinel so
    /// the existing sentinel watcher will trigger `Event::PluginReload`.
    ///
    /// Implementations should be idempotent and safe to call frequently
    /// (the watcher already debounces, but a no-op when nothing changed
    /// keeps the cost low).
    fn resolve_and_signal_reload(&self, config: &Config) -> Result<ResolveOutcome, DynError>;
}

/// No-op orchestrator. Used by backends that don't ship the WASM resolve
/// pipeline (e.g. unit tests).
pub struct NoopReloadOrchestrator;

impl ReloadOrchestrator for NoopReloadOrchestrator {
    fn resolve_and_signal_reload(&self, _config: &Config) -> Result<ResolveOutcome, DynError> {
        Ok(ResolveOutcome::default())
    }
}
