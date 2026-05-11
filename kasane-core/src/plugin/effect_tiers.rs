//! Tier-typed effects ([ADR-044](../../docs/decisions.md#adr-044-handler--effect-tier-hierarchy)).
//!
//! Each tier names a re-entrance / performance budget at the handler return
//! boundary. The tier types lift to [`Effects`] before the type erasure that
//! happens in `HandlerTable`, so the runtime pipeline (dispatch, plugin
//! manager, event loop) is unchanged.
//!
//! * [`ObservationEffects`] — Tier 0. Read-only handlers (workspace
//!   observers, navigation-policy queries) that report observations
//!   without side effects.
//! * [`KakouneSideEffects`] — Tier 1. High-frequency handlers
//!   (`on_state_changed_effects`, `on_active_session_ready_effects`,
//!   `on_command_error_effects`, `on_init_effects`, `on_subscription`).
//!   Commands are scoped to Kakoune-side and host-local effects — no
//!   external-process / HTTP / session / workspace management.
//! * [`ProcessCapableEffects`] — Tier 2. User-driven handlers
//!   (`handle_key`, `handle_mouse`, `handle_drop`, `handle_text_input`,
//!   `on_io_event_effects`, `update_effects`). Full effect set, including
//!   process spawn and external I/O.
//!
//! ## Phase A-1 status
//!
//! This file lands the *foundation* of ADR-044: the tier-typed effect /
//! command projections are defined and `From<…>` lifts feed the unified
//! [`Effects`] / [`Command`] pipeline. **Handler signatures are not yet
//! tier-typed.** Until the handler-signature migration (Phase A-3) lands,
//! these types are opt-in for plugin authors and not enforced at the
//! handler return boundary. The runtime guards added in
//! [issue #100](https://github.com/Yus314/kasane/issues/100) /
//! [issue #101](https://github.com/Yus314/kasane/issues/101) remain in
//! effect.

use crate::scroll::ScrollPlan;
use crate::state::shadow_cursor::ShadowCursor;
use crate::state::{DirtyFlags, DragState};

use super::command::Command;
use super::effects::{Effects, StateUpdates};

// =========================================================================
// Tier-1 command projection: KakouneSideCommand
// =========================================================================

/// Commands admissible from a Tier-1 handler (`on_state_changed_effects`
/// and friends). The newtype wraps an inner [`Command`] whose variant is
/// guaranteed by construction to be *not* a process command (see
/// [`Command::is_process_command`]).
///
/// Plugin authors construct values via the named constructors below; each
/// constructor is restricted to a variant the projection admits. There is
/// no constructor for `SpawnProcess`, `HttpRequest`, `Session`, etc. —
/// those belong to [`ProcessCommand`].
///
/// The asymmetric projection — `KakouneSideCommand → Command` exists,
/// `Command → KakouneSideCommand` deliberately does not — is what powers
/// compile-time tier enforcement for input handlers (Phase A-3d):
///
/// ```compile_fail
/// // A raw `Command` (which might be SpawnProcess) cannot be coerced
/// // into a Tier-1 input handler's command list.
/// use kasane_core::plugin::{Command, KakouneSideCommand};
/// let c = Command::PasteClipboard;
/// let _: KakouneSideCommand = c.into();
/// ```
pub struct KakouneSideCommand(Command);

impl std::fmt::Debug for KakouneSideCommand {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "KakouneSideCommand({})", self.0.variant_name())
    }
}

impl KakouneSideCommand {
    // -- Kakoune-side writes (allowed; Tier-1 admits Kakoune writes; the
    // re-entrance concern is process spawn, not Kakoune commands) --

    pub fn send_to_kakoune(req: crate::protocol::KasaneRequest) -> Self {
        Self(Command::SendToKakoune(req))
    }

    pub fn insert_text(text: impl Into<String>) -> Self {
        Self(Command::InsertText(text.into()))
    }

    pub fn edit_buffer(edits: Vec<super::command::BufferEdit>) -> Self {
        Self(Command::EditBuffer { edits })
    }

    pub fn paste_clipboard() -> Self {
        Self(Command::PasteClipboard)
    }

    pub fn set_clipboard(text: impl Into<String>) -> Self {
        Self(Command::SetClipboard(text.into()))
    }

    // -- Host-local effects --

    pub fn request_redraw(flags: DirtyFlags) -> Self {
        Self(Command::RequestRedraw(flags))
    }

    pub fn dismiss_diagnostic_overlay() -> Self {
        Self(Command::DismissDiagnosticOverlay)
    }

    pub fn trigger_plugin_reload() -> Self {
        Self(Command::TriggerPluginReload)
    }

    pub fn quit() -> Self {
        Self(Command::Quit)
    }

    pub fn schedule_timer(
        timer_id: u64,
        delay: std::time::Duration,
        target: super::PluginId,
        payload: Box<dyn std::any::Any + Send>,
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

    pub fn plugin_message(target: super::PluginId, payload: Box<dyn std::any::Any + Send>) -> Self {
        Self(Command::PluginMessage { target, payload })
    }

    pub fn set_config(key: impl Into<String>, value: impl Into<String>) -> Self {
        Self(Command::SetConfig {
            key: key.into(),
            value: value.into(),
        })
    }

    pub fn set_setting(
        plugin_id: super::PluginId,
        key: impl Into<String>,
        value: super::setting::SettingValue,
    ) -> Self {
        Self(Command::SetSetting {
            plugin_id,
            key: key.into(),
            value,
        })
    }

    pub fn register_theme_tokens(tokens: Vec<(String, crate::protocol::Style)>) -> Self {
        Self(Command::RegisterThemeTokens(tokens))
    }

    pub fn register_surface(
        surface: Box<dyn crate::surface::Surface>,
        placement: crate::workspace::Placement,
    ) -> Self {
        Self(Command::RegisterSurface { surface, placement })
    }

    pub fn register_surface_requested(
        surface: Box<dyn crate::surface::Surface>,
        placement: crate::surface::SurfacePlacementRequest,
    ) -> Self {
        Self(Command::RegisterSurfaceRequested { surface, placement })
    }

    pub fn unregister_surface(surface_id: crate::surface::SurfaceId) -> Self {
        Self(Command::UnregisterSurface { surface_id })
    }

    pub fn unregister_surface_key(surface_key: impl Into<String>) -> Self {
        Self(Command::UnregisterSurfaceKey {
            surface_key: surface_key.into(),
        })
    }

    pub fn bind_surface_session(
        surface_id: crate::surface::SurfaceId,
        session_id: crate::session::SessionId,
    ) -> Self {
        Self(Command::BindSurfaceSession {
            surface_id,
            session_id,
        })
    }

    pub fn unbind_surface_session(surface_id: crate::surface::SurfaceId) -> Self {
        Self(Command::UnbindSurfaceSession { surface_id })
    }

    pub fn inject_input(event: crate::input::InputEvent) -> Self {
        Self(Command::InjectInput(event))
    }

    pub fn expose_variable(name: impl Into<String>, value: crate::widget::types::Value) -> Self {
        Self(Command::ExposeVariable {
            name: name.into(),
            value,
        })
    }

    pub fn set_structural_projection(id: Option<crate::display::ProjectionId>) -> Self {
        Self(Command::SetStructuralProjection(id))
    }

    pub fn toggle_additive_projection(id: crate::display::ProjectionId) -> Self {
        Self(Command::ToggleAdditiveProjection(id))
    }

    pub fn projection_off() -> Self {
        Self(Command::ProjectionOff)
    }

    /// Unwrap into the inner [`Command`].
    pub fn into_command(self) -> Command {
        self.0
    }

    /// Borrow the inner command for inspection.
    pub fn as_command(&self) -> &Command {
        &self.0
    }
}

impl From<KakouneSideCommand> for Command {
    fn from(c: KakouneSideCommand) -> Self {
        c.0
    }
}

// =========================================================================
// Tier-2 command projection: ProcessCommand
// =========================================================================

/// Commands admissible only from a Tier-2 handler (`handle_key`,
/// `on_io_event_effects`, `update_effects`, etc.). The host fills in
/// source attribution from the handler context, which is why these
/// commands are gated behind Tier 2 — they need a `PluginId` to reach
/// the dispatcher correctly.
pub struct ProcessCommand(Command);

impl std::fmt::Debug for ProcessCommand {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "ProcessCommand({})", self.0.variant_name())
    }
}

impl ProcessCommand {
    pub fn spawn_process(
        job_id: u64,
        program: impl Into<String>,
        args: Vec<String>,
        stdin_mode: super::io::StdinMode,
    ) -> Self {
        Self(Command::SpawnProcess {
            job_id,
            program: program.into(),
            args,
            stdin_mode,
        })
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

    pub fn http_request(job_id: u64, config: super::io::HttpRequestConfig) -> Self {
        Self(Command::HttpRequest { job_id, config })
    }

    pub fn cancel_http_request(job_id: u64) -> Self {
        Self(Command::CancelHttpRequest { job_id })
    }

    pub fn session(cmd: crate::session::SessionCommand) -> Self {
        Self(Command::Session(cmd))
    }

    pub fn spawn_pane_client(
        pane_key: impl Into<String>,
        placement: crate::workspace::Placement,
    ) -> Self {
        Self(Command::SpawnPaneClient {
            pane_key: pane_key.into(),
            placement,
        })
    }

    pub fn close_pane_client(pane_key: impl Into<String>) -> Self {
        Self(Command::ClosePaneClient {
            pane_key: pane_key.into(),
        })
    }

    pub fn start_process_task(task_name: impl Into<String>) -> Self {
        Self(Command::StartProcessTask {
            task_name: task_name.into(),
        })
    }

    pub fn workspace(cmd: crate::workspace::WorkspaceCommand) -> Self {
        Self(Command::Workspace(cmd))
    }

    /// Unwrap into the inner [`Command`].
    pub fn into_command(self) -> Command {
        self.0
    }

    /// Borrow the inner command for inspection.
    pub fn as_command(&self) -> &Command {
        &self.0
    }
}

impl From<ProcessCommand> for Command {
    fn from(c: ProcessCommand) -> Self {
        c.0
    }
}

// =========================================================================
// Tier 0 — ObservationEffects
// =========================================================================

/// Tier-0 effects: redraw flags, scroll plans, and typed state updates.
/// **No commands.** Returned by handlers that exist purely to observe
/// state and adjust output (e.g. workspace observers).
pub struct ObservationEffects {
    pub redraw: DirtyFlags,
    pub scroll_plans: Vec<ScrollPlan>,
    pub state_updates: StateUpdates,
}

impl Default for ObservationEffects {
    fn default() -> Self {
        Self {
            redraw: DirtyFlags::empty(),
            scroll_plans: Vec::new(),
            state_updates: StateUpdates::default(),
        }
    }
}

impl ObservationEffects {
    pub fn none() -> Self {
        Self::default()
    }

    pub fn redraw(flags: DirtyFlags) -> Self {
        Self {
            redraw: flags,
            ..Self::default()
        }
    }

    pub fn with_shadow_cursor(mut self, sc: Option<ShadowCursor>) -> Self {
        self.state_updates.shadow_cursor = Some(sc);
        self
    }

    pub fn with_drag(mut self, drag: DragState) -> Self {
        self.state_updates.drag = Some(drag);
        self
    }

    pub fn push_scroll(&mut self, plan: ScrollPlan) {
        self.scroll_plans.push(plan);
    }
}

impl From<ObservationEffects> for Effects {
    fn from(o: ObservationEffects) -> Self {
        Effects {
            redraw: o.redraw,
            commands: Vec::new(),
            scroll_plans: o.scroll_plans,
            state_updates: o.state_updates,
        }
    }
}

// =========================================================================
// Tier 1 — KakouneSideEffects
// =========================================================================

/// Tier-1 effects: observation + Kakoune-side / host-local commands.
/// **Excludes process / HTTP / session / pane / workspace commands** —
/// those require source attribution and are gated to [`ProcessCapableEffects`].
///
/// The asymmetric `From` impl (`KakouneSideEffects → Effects` exists, but
/// `Effects → KakouneSideEffects` deliberately does not) is what powers
/// the compile-time enforcement of
/// [ADR-044](../../../docs/decisions.md#adr-044-handler--effect-tier-hierarchy):
///
/// ```compile_fail
/// // An arbitrary `Effects` value cannot be coerced into Tier 1; it might
/// // carry a `ProcessCommand` which Tier 1 does not admit.
/// use kasane_core::plugin::{Effects, KakouneSideEffects};
/// let e = Effects::default();
/// let _: KakouneSideEffects = e.into();
/// ```
#[derive(Default)]
pub struct KakouneSideEffects {
    pub base: ObservationEffects,
    pub commands: Vec<KakouneSideCommand>,
}

impl KakouneSideEffects {
    pub fn none() -> Self {
        Self::default()
    }

    pub fn redraw(flags: DirtyFlags) -> Self {
        Self {
            base: ObservationEffects::redraw(flags),
            commands: Vec::new(),
        }
    }

    pub fn with(commands: Vec<KakouneSideCommand>) -> Self {
        Self {
            base: ObservationEffects::default(),
            commands,
        }
    }

    pub fn push(&mut self, cmd: KakouneSideCommand) {
        self.commands.push(cmd);
    }

    pub fn set_redraw(&mut self, flags: DirtyFlags) {
        self.base.redraw |= flags;
    }

    pub fn push_scroll(&mut self, plan: ScrollPlan) {
        self.base.scroll_plans.push(plan);
    }

    pub fn with_shadow_cursor(mut self, sc: Option<ShadowCursor>) -> Self {
        self.base.state_updates.shadow_cursor = Some(sc);
        self
    }

    pub fn with_drag(mut self, drag: DragState) -> Self {
        self.base.state_updates.drag = Some(drag);
        self
    }
}

impl From<KakouneSideEffects> for Effects {
    fn from(k: KakouneSideEffects) -> Self {
        Effects {
            redraw: k.base.redraw,
            commands: k.commands.into_iter().map(Into::into).collect(),
            scroll_plans: k.base.scroll_plans,
            state_updates: k.base.state_updates,
        }
    }
}

/// Tier-0 effects lift into Tier 1 trivially — observation is a subset.
/// Lets Tier-1 setters accept closures returning [`ObservationEffects`].
impl From<ObservationEffects> for KakouneSideEffects {
    fn from(o: ObservationEffects) -> Self {
        Self {
            base: o,
            commands: Vec::new(),
        }
    }
}

// =========================================================================
// Tier 2 — ProcessCapableEffects
// =========================================================================

/// Tier-2 effects: observation + Kakoune-side commands + process /
/// external I/O commands. Returned by user-driven and command-handler
/// contexts where source attribution is naturally available.
///
/// `From<KakouneSideEffects>` and `From<ObservationEffects>` lift
/// narrower tiers — Tier 2 admits the union. `From<Effects>` is
/// **deliberately absent**: `Effects` is the type-erased lowest common
/// denominator, and accepting it would let `on_io_event_tier2` /
/// `on_update_tier2` silently drop back to untyped returns, defeating
/// migration tracking:
///
/// ```compile_fail
/// use kasane_core::plugin::{Effects, ProcessCapableEffects};
/// let e = Effects::default();
/// let _: ProcessCapableEffects = e.into();
/// ```
#[derive(Default)]
pub struct ProcessCapableEffects {
    pub base: KakouneSideEffects,
    pub process_commands: Vec<ProcessCommand>,
}

impl ProcessCapableEffects {
    pub fn none() -> Self {
        Self::default()
    }

    pub fn redraw(flags: DirtyFlags) -> Self {
        Self {
            base: KakouneSideEffects::redraw(flags),
            process_commands: Vec::new(),
        }
    }

    pub fn push_side(&mut self, cmd: KakouneSideCommand) {
        self.base.push(cmd);
    }

    pub fn push_process(&mut self, cmd: ProcessCommand) {
        self.process_commands.push(cmd);
    }

    pub fn set_redraw(&mut self, flags: DirtyFlags) {
        self.base.set_redraw(flags);
    }

    pub fn with_shadow_cursor(mut self, sc: Option<ShadowCursor>) -> Self {
        self.base.base.state_updates.shadow_cursor = Some(sc);
        self
    }

    pub fn with_drag(mut self, drag: DragState) -> Self {
        self.base.base.state_updates.drag = Some(drag);
        self
    }
}

impl From<ProcessCapableEffects> for Effects {
    fn from(p: ProcessCapableEffects) -> Self {
        let mut commands: Vec<Command> = p.base.commands.into_iter().map(Into::into).collect();
        commands.extend(p.process_commands.into_iter().map(Into::into));
        Effects {
            redraw: p.base.base.redraw,
            commands,
            scroll_plans: p.base.base.scroll_plans,
            state_updates: p.base.base.state_updates,
        }
    }
}

/// Tier-0 and Tier-1 effects lift into Tier 2 — Tier 2 admits the union
/// of all command kinds, so narrower tiers fit by widening.
impl From<ObservationEffects> for ProcessCapableEffects {
    fn from(o: ObservationEffects) -> Self {
        Self {
            base: KakouneSideEffects::from(o),
            process_commands: Vec::new(),
        }
    }
}

impl From<KakouneSideEffects> for ProcessCapableEffects {
    fn from(k: KakouneSideEffects) -> Self {
        Self {
            base: k,
            process_commands: Vec::new(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn kakoune_side_command_lifts_to_command() {
        let cmd: Command = KakouneSideCommand::request_redraw(DirtyFlags::STATUS).into();
        assert!(matches!(cmd, Command::RequestRedraw(_)));
        assert!(!cmd.is_process_command());
    }

    #[test]
    fn process_command_lifts_to_command_and_classifies() {
        let cmd: Command = ProcessCommand::spawn_process(
            42,
            "fd".to_string(),
            vec!["needle".to_string()],
            super::super::io::StdinMode::Null,
        )
        .into();
        assert!(matches!(cmd, Command::SpawnProcess { job_id: 42, .. }));
        assert!(cmd.is_process_command());
    }

    #[test]
    fn observation_effects_default_is_empty() {
        let eff: Effects = ObservationEffects::default().into();
        assert!(eff.redraw.is_empty());
        assert!(eff.commands.is_empty());
        assert!(eff.scroll_plans.is_empty());
        assert!(eff.state_updates.is_empty());
    }

    #[test]
    fn observation_effects_redraw_lifts() {
        let eff: Effects = ObservationEffects::redraw(DirtyFlags::INFO).into();
        assert!(eff.redraw.contains(DirtyFlags::INFO));
        assert!(eff.commands.is_empty());
    }

    #[test]
    fn kakoune_side_effects_round_trip() {
        let mut k = KakouneSideEffects::redraw(DirtyFlags::STATUS);
        k.push(KakouneSideCommand::request_redraw(DirtyFlags::BUFFER));
        k.push(KakouneSideCommand::insert_text("hello"));
        let eff: Effects = k.into();
        assert!(eff.redraw.contains(DirtyFlags::STATUS));
        assert_eq!(eff.commands.len(), 2);
        assert!(matches!(eff.commands[0], Command::RequestRedraw(_)));
        assert!(matches!(eff.commands[1], Command::InsertText(_)));
        // None of the lifted commands classify as process commands.
        assert!(eff.commands.iter().all(|c| !c.is_process_command()));
    }

    #[test]
    fn process_capable_effects_preserves_command_order() {
        let mut p = ProcessCapableEffects::redraw(DirtyFlags::BUFFER);
        p.push_side(KakouneSideCommand::request_redraw(DirtyFlags::STATUS));
        p.push_process(ProcessCommand::spawn_process(
            7,
            "ls".to_string(),
            vec![],
            super::super::io::StdinMode::Null,
        ));
        let eff: Effects = p.into();
        assert_eq!(eff.commands.len(), 2);
        assert!(matches!(eff.commands[0], Command::RequestRedraw(_)));
        assert!(matches!(eff.commands[1], Command::SpawnProcess { .. }));
        assert!(eff.commands[1].is_process_command());
        assert!(!eff.commands[0].is_process_command());
    }
}
