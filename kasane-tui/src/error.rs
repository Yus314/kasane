//! Typed error surface for the TUI runner.
//!
//! `run_tui` and its private helpers return `Result<_, TuiError>` so the
//! binary boundary in the `kasane` crate can `?`-bubble TUI failures
//! through its `anyhow::Result<()>` top-level handler via
//! `anyhow::Error::from_boxed` (since `TuiError: std::error::Error`).

use kasane_core::error::DynError;
use kasane_core::session::SessionManagerError;
use thiserror::Error;

/// Errors raised by the TUI runner (`run_tui` + helpers).
#[derive(Debug, Error)]
pub enum TuiError {
    /// `SessionManager::active_session_id` returned `None` — should never
    /// happen because the primary session is inserted before `run_tui`
    /// is called.
    #[error("missing primary session id")]
    MissingPrimarySession,

    /// `SessionManager::take_active_reader` failed — usually means the
    /// reader was already consumed.
    #[error("failed to acquire primary session: {0}")]
    AcquirePrimarySession(#[from] SessionManagerError),

    /// I/O failure during terminal init or per-frame render (crossterm,
    /// write_all, flush).
    #[error("terminal I/O error: {0}")]
    Io(#[from] std::io::Error),

    /// Plugin loading failure surfaced from `PluginManager::{initialize,
    /// reload}`. Carries the user-defined trait-surface boundary error
    /// box; `Display` formats the inner Display.
    #[error("plugin manager error: {0}")]
    PluginManager(DynError),
}
