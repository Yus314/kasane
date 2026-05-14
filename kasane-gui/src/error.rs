//! Typed error surface for the GUI runner.
//!
//! `run_gui` and its private helpers return `Result<_, GuiError>` so the
//! binary boundary in the `kasane` crate can `?`-bubble GUI failures
//! through its `anyhow::Result<()>` top-level handler via
//! `anyhow::Error::new` (since `GuiError: std::error::Error`).

use kasane_core::error::DynError;
use kasane_core::session::SessionManagerError;
use thiserror::Error;

/// Errors raised by the GUI runner (`run_gui` + helpers).
#[derive(Debug, Error)]
pub enum GuiError {
    /// `SessionManager::active_session_id` returned `None` — should never
    /// happen because the primary session is inserted before `run_gui`
    /// is called.
    #[error("missing primary session id")]
    MissingPrimarySession,

    /// `SessionManager::take_active_reader` failed — usually means the
    /// reader was already consumed.
    #[error("failed to acquire primary session: {0}")]
    AcquirePrimarySession(#[from] SessionManagerError),

    /// Winit / event-loop initialization or run-loop failure.
    #[error("winit event-loop error: {0}")]
    EventLoop(#[from] winit::error::EventLoopError),

    /// I/O failure during widget reload watcher setup or filesystem I/O.
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),

    /// Plugin loading failure surfaced from `PluginManager::{initialize,
    /// reload}`. Carries the user-defined trait-surface boundary error
    /// box; `Display` formats the inner Display.
    #[error("plugin manager error: {0}")]
    PluginManager(DynError),
}
