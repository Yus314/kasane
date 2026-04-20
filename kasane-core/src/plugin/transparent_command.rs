//! Kakoune-transparent command projection.
//!
//! `TransparentCommand` is the Level 3 enforcement of ADR-030.
//! Where `Truth<'a>` restricts reading to observed fields,
//! `TransparentCommand` restricts construction to non-writing variants.
//! A handler returning `Vec<TransparentCommand>` provides a compile-time
//! witness that it cannot emit Kakoune-writing commands (A3 τ-transition).

use std::any::Any;
use std::time::Duration;

use crate::input::{InputEvent, KeyEvent};
use crate::protocol::Face;
use crate::session::{SessionCommand, SessionId};
use crate::state::DirtyFlags;
use crate::surface::{Surface, SurfaceId, SurfacePlacementRequest};
use crate::workspace::{Placement, WorkspaceCommand};

use super::PluginId;
use super::command::Command;
use super::io::StdinMode;
use super::setting::SettingValue;
use super::traits::KeyHandleResult;

/// A command guaranteed not to write to Kakoune.
///
/// Construction is restricted to non-writing `Command` variants.
/// `SendToKakoune`, `InsertText`, and `EditBuffer` have no constructor
/// on this type, making transparency a compile-time property.
pub struct TransparentCommand(Command);

impl std::fmt::Debug for TransparentCommand {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "TransparentCommand({})", self.0.variant_name())
    }
}

impl TransparentCommand {
    /// All variant names covered by this projection (sorted).
    pub const VARIANT_NAMES: &'static [&'static str] = &[
        "BindSurfaceSession",
        "CancelHttpRequest",
        "CancelTimer",
        "ClosePaneClient",
        "CloseProcessStdin",
        "ExposeVariable",
        "HttpRequest",
        "InjectInput",
        "KillProcess",
        "PasteClipboard",
        "PluginMessage",
        "ProjectionOff",
        "Quit",
        "RegisterSurface",
        "RegisterSurfaceRequested",
        "RegisterThemeTokens",
        "RequestRedraw",
        "ResizePty",
        "ScheduleTimer",
        "Session",
        "SetConfig",
        "SetSetting",
        "SetStructuralProjection",
        "SpawnPaneClient",
        "SpawnProcess",
        "StartProcessTask",
        "ToggleAdditiveProjection",
        "UnbindSurfaceSession",
        "UnregisterSurface",
        "UnregisterSurfaceKey",
        "Workspace",
        "WriteToProcess",
    ];

    // =========================================================================
    // Named constructors — one per transparent variant
    // =========================================================================

    pub fn request_redraw(flags: DirtyFlags) -> Self {
        Self(Command::RequestRedraw(flags))
    }

    pub fn schedule_timer(
        timer_id: u64,
        delay: Duration,
        target: PluginId,
        payload: Box<dyn Any + Send>,
    ) -> Self {
        Self(Command::ScheduleTimer {
            timer_id,
            delay,
            target,
            payload,
        })
    }

    pub fn cancel_timer(timer_id: u64) -> Self {
        Self(Command::CancelTimer { timer_id })
    }

    pub fn plugin_message(target: PluginId, payload: Box<dyn Any + Send>) -> Self {
        Self(Command::PluginMessage { target, payload })
    }

    pub fn set_config(key: String, value: String) -> Self {
        Self(Command::SetConfig { key, value })
    }

    pub fn set_setting(plugin_id: PluginId, key: String, value: SettingValue) -> Self {
        Self(Command::SetSetting {
            plugin_id,
            key,
            value,
        })
    }

    pub fn workspace(cmd: WorkspaceCommand) -> Self {
        Self(Command::Workspace(cmd))
    }

    pub fn register_surface(surface: Box<dyn Surface>, placement: Placement) -> Self {
        Self(Command::RegisterSurface { surface, placement })
    }

    pub fn register_surface_requested(
        surface: Box<dyn Surface>,
        placement: SurfacePlacementRequest,
    ) -> Self {
        Self(Command::RegisterSurfaceRequested { surface, placement })
    }

    pub fn unregister_surface(surface_id: SurfaceId) -> Self {
        Self(Command::UnregisterSurface { surface_id })
    }

    pub fn unregister_surface_key(surface_key: String) -> Self {
        Self(Command::UnregisterSurfaceKey { surface_key })
    }

    pub fn register_theme_tokens(tokens: Vec<(String, Face)>) -> Self {
        Self(Command::RegisterThemeTokens(tokens))
    }

    pub fn spawn_process(
        job_id: u64,
        program: String,
        args: Vec<String>,
        stdin_mode: StdinMode,
    ) -> Self {
        Self(Command::SpawnProcess {
            job_id,
            program,
            args,
            stdin_mode,
        })
    }

    pub fn session(cmd: SessionCommand) -> Self {
        Self(Command::Session(cmd))
    }

    pub fn write_to_process(job_id: u64, data: Vec<u8>) -> Self {
        Self(Command::WriteToProcess { job_id, data })
    }

    pub fn close_process_stdin(job_id: u64) -> Self {
        Self(Command::CloseProcessStdin { job_id })
    }

    pub fn kill_process(job_id: u64) -> Self {
        Self(Command::KillProcess { job_id })
    }

    pub fn resize_pty(job_id: u64, rows: u16, cols: u16) -> Self {
        Self(Command::ResizePty { job_id, rows, cols })
    }

    pub fn inject_input(event: InputEvent) -> Self {
        Self(Command::InjectInput(event))
    }

    pub fn quit() -> Self {
        Self(Command::Quit)
    }

    pub fn paste_clipboard() -> Self {
        Self(Command::PasteClipboard)
    }

    pub fn spawn_pane_client(pane_key: String, placement: Placement) -> Self {
        Self(Command::SpawnPaneClient {
            pane_key,
            placement,
        })
    }

    pub fn close_pane_client(pane_key: String) -> Self {
        Self(Command::ClosePaneClient { pane_key })
    }

    pub fn bind_surface_session(surface_id: SurfaceId, session_id: SessionId) -> Self {
        Self(Command::BindSurfaceSession {
            surface_id,
            session_id,
        })
    }

    pub fn unbind_surface_session(surface_id: SurfaceId) -> Self {
        Self(Command::UnbindSurfaceSession { surface_id })
    }

    pub fn start_process_task(task_name: String) -> Self {
        Self(Command::StartProcessTask { task_name })
    }

    pub fn expose_variable(name: String, value: crate::widget::types::Value) -> Self {
        Self(Command::ExposeVariable { name, value })
    }

    pub fn set_structural_projection(id: Option<crate::display::ProjectionId>) -> Self {
        Self(Command::SetStructuralProjection(id))
    }

    pub fn toggle_additive_projection(id: crate::display::ProjectionId) -> Self {
        Self(Command::ToggleAdditiveProjection(id))
    }

    pub fn http_request(job_id: u64, config: super::io::HttpRequestConfig) -> Self {
        Self(Command::HttpRequest { job_id, config })
    }

    pub fn cancel_http_request(job_id: u64) -> Self {
        Self(Command::CancelHttpRequest { job_id })
    }

    pub fn projection_off() -> Self {
        Self(Command::ProjectionOff)
    }

    // =========================================================================
    // Conversion
    // =========================================================================

    /// Unwrap into the underlying `Command`.
    pub fn into_command(self) -> Command {
        self.0
    }
}

impl From<TransparentCommand> for Command {
    fn from(tc: TransparentCommand) -> Self {
        tc.0
    }
}

/// Transparent variant of [`KeyHandleResult`].
///
/// Identical to `KeyHandleResult` but `Consumed` carries `Vec<TransparentCommand>`
/// instead of `Vec<Command>`, providing a compile-time transparency guarantee.
pub enum TransparentKeyResult {
    Consumed(Vec<TransparentCommand>),
    Transformed(KeyEvent),
    Passthrough,
}

impl From<TransparentKeyResult> for KeyHandleResult {
    fn from(r: TransparentKeyResult) -> Self {
        match r {
            TransparentKeyResult::Consumed(cmds) => {
                KeyHandleResult::Consumed(cmds.into_iter().map(Into::into).collect())
            }
            TransparentKeyResult::Transformed(k) => KeyHandleResult::Transformed(k),
            TransparentKeyResult::Passthrough => KeyHandleResult::Passthrough,
        }
    }
}
