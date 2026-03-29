//! Backend-agnostic event loop helpers.
//!
//! Extracts the deferred command handling logic that is shared between
//! TUI and GUI backends.

mod context;
mod dispatch;
mod session;
mod surface;

#[cfg(test)]
mod tests;

use std::any::Any;
use std::io::Write;
use std::time::Duration;

use crate::input::InputEvent;
use crate::layout::Rect;
use crate::plugin::{PluginDiagnostic, PluginDiagnosticOverlayState, PluginId, PluginRuntime};
use crate::session::SessionId;
use crate::state::{AppState, DirtyFlags};
use crate::surface::{SurfaceEvent, SurfaceRegistry};

// ── Public re-exports — preserves existing import paths ─────────

pub use context::DeferredContext;
pub use dispatch::{
    apply_bootstrap_effects, handle_command_batch, handle_deferred_commands,
    handle_sourced_surface_commands, maybe_flush_active_session_ready, sync_session_ready_gate,
};
pub use session::{
    SessionMutContext, SessionReadyGate, SharedSessionRuntime, apply_ready_batch,
    close_session_core, handle_pane_death, restore_panes, send_pane_resizes, spawn_session_core,
    spawn_session_reader, switch_session_core, sync_session_metadata,
};
pub use surface::{
    rebuild_plugin_surface_registry, reconcile_plugin_surfaces, register_builtin_surfaces,
    setup_plugin_surfaces,
};

// ── EventResult ─────────────────────────────────────────────────

use crate::plugin::extract_redraw_flags;
use crate::scroll::ScrollPlan;
use crate::surface::SourcedSurfaceCommands;

/// Structured result from processing a single event.
pub struct EventResult {
    pub flags: DirtyFlags,
    pub commands: Vec<crate::plugin::Command>,
    pub scroll_plans: Vec<ScrollPlan>,
    pub surface_commands: Vec<SourcedSurfaceCommands>,
    pub command_source: Option<PluginId>,
    pub workspace_changed: bool,
}

impl EventResult {
    pub fn empty() -> Self {
        Self {
            flags: DirtyFlags::empty(),
            commands: vec![],
            scroll_plans: vec![],
            surface_commands: vec![],
            command_source: None,
            workspace_changed: false,
        }
    }

    /// Accumulate redraw flags from surface command groups.
    pub fn extract_surface_flags(&mut self) {
        for entry in &mut self.surface_commands {
            self.flags |= extract_redraw_flags(&mut entry.commands);
        }
    }
}

// ── Utility functions ───────────────────────────────────────────

/// Rebuild the HitMap from the current view tree for plugin mouse routing.
pub fn rebuild_hit_map(
    state: &mut AppState,
    registry: &PluginRuntime,
    surface_registry: &SurfaceRegistry,
) {
    let root_area = Rect {
        x: 0,
        y: 0,
        w: state.cols,
        h: state.rows,
    };
    let element = surface_registry
        .compose_view_sections(state, None, &registry.view(), root_area)
        .into_element();
    let layout_result = crate::layout::flex::place(&element, root_area, state);
    state.hit_map = crate::layout::build_hit_map(&element, &layout_result);
}

/// Notify workspace observers with a post-layout snapshot of the current workspace.
pub fn notify_workspace_observers(
    registry: &mut PluginRuntime,
    surface_registry: &SurfaceRegistry,
    state: &AppState,
) {
    use std::collections::HashMap;

    let total = Rect {
        x: 0,
        y: 0,
        w: state.cols,
        h: state.rows,
    };
    let surface_keys: HashMap<_, _> = surface_registry
        .workspace()
        .root()
        .collect_ids()
        .into_iter()
        .filter_map(|id| {
            surface_registry
                .descriptor(id)
                .map(|d| (id, d.surface_key.clone()))
        })
        .collect();
    let query = surface_registry
        .workspace()
        .query_with_keys(total, surface_keys);
    registry.notify_workspace_changed(&query);
}

/// Convert an input event into a surface event.
///
/// Shared between TUI and GUI backends for routing input through the surface system.
pub fn surface_event_from_input(input: &InputEvent) -> Option<SurfaceEvent> {
    match input {
        InputEvent::Key(key) => Some(SurfaceEvent::Key(key.clone())),
        InputEvent::Mouse(mouse) => Some(SurfaceEvent::Mouse(mouse.clone())),
        InputEvent::Resize(cols, rows) => Some(SurfaceEvent::Resize(Rect {
            x: 0,
            y: 0,
            w: *cols,
            h: *rows,
        })),
        InputEvent::FocusGained => Some(SurfaceEvent::FocusGained),
        InputEvent::FocusLost => Some(SurfaceEvent::FocusLost),
        InputEvent::Paste(_) => None,
    }
}

// ── EventSink ────────────��──────────────────────────────────────

/// Backend-agnostic event delivery.
///
/// Abstracts over TUI's `crossbeam_channel::Sender<Event>` and
/// GUI's `winit::event_loop::EventLoopProxy<GuiEvent>`.
pub trait EventSink: Clone + Send + 'static {
    fn send_kakoune(&self, session_id: SessionId, req: crate::protocol::KakouneRequest);
    fn send_died(&self, session_id: SessionId);
    fn send_timer(&self, target: PluginId, payload: Box<dyn Any + Send>);
    fn send_diagnostic_expire(&self, generation: u64);
}

// ── Generic schedulers ─────────────���────────────────────────────

/// Timer scheduler generic over [`EventSink`].
pub struct GenericTimerScheduler<E>(pub E);

impl<E: EventSink> TimerScheduler for GenericTimerScheduler<E> {
    fn schedule_timer(&self, delay: Duration, target: PluginId, payload: Box<dyn Any + Send>) {
        let sink = self.0.clone();
        std::thread::spawn(move || {
            std::thread::sleep(delay);
            sink.send_timer(target, payload);
        });
    }
}

/// Diagnostic overlay scheduler generic over [`EventSink`].
pub struct GenericDiagnosticScheduler<E>(pub E);

impl<E: EventSink> DiagnosticOverlayScheduler for GenericDiagnosticScheduler<E> {
    fn schedule_expiry(&self, delay: Duration, generation: u64) {
        let sink = self.0.clone();
        std::thread::spawn(move || {
            std::thread::sleep(delay);
            sink.send_diagnostic_expire(generation);
        });
    }
}

// ── Traits ────────────���─────────────────────────────���───────────

/// Backend-agnostic timer scheduling.
///
/// Implementations spawn a background thread that sleeps for `delay` and then
/// delivers the timer event through the backend's event system.
pub trait TimerScheduler {
    fn schedule_timer(&self, delay: Duration, target: PluginId, payload: Box<dyn Any + Send>);
}

/// Backend-owned session lifecycle hooks used by deferred commands.
pub trait SessionRuntime {
    /// Spawn a new managed session.
    fn spawn_session(
        &mut self,
        spec: crate::session::SessionSpec,
        activate: bool,
        state: &mut AppState,
        dirty: &mut DirtyFlags,
        initial_resize_sent: &mut bool,
    );

    /// Close a managed session by key, or the active session when `key` is `None`.
    ///
    /// Returns `true` when the application should exit because no session remains.
    fn close_session(
        &mut self,
        key: Option<&str>,
        state: &mut AppState,
        dirty: &mut DirtyFlags,
        initial_resize_sent: &mut bool,
    ) -> bool;

    /// Switch to an existing session by key.
    fn switch_session(
        &mut self,
        key: &str,
        state: &mut AppState,
        dirty: &mut DirtyFlags,
        initial_resize_sent: &mut bool,
    );

    /// Look up a session ID by its key name.
    fn session_id_by_key(&self, key: &str) -> Option<SessionId> {
        let _ = key;
        None
    }
}

/// Backend-owned access to the active session writer plus session lifecycle hooks.
pub trait SessionHost: SessionRuntime {
    fn active_writer(&mut self) -> &mut dyn Write;

    /// Get a writer for a specific session by ID.
    ///
    /// Used by multi-pane command routing to send commands to the
    /// correct Kakoune client. Returns `None` if the session doesn't exist.
    fn writer_for_session(&mut self, _session_id: SessionId) -> Option<&mut dyn Write> {
        None
    }
}

// ── Diagnostics ─────────────────────────────────────────────────

/// Consume an input event that targets a workspace split divider.
///
/// Divider drag is handled before normal input routing so divider presses do
/// not leak through to Kakoune or plugin mouse handlers.
pub fn handle_workspace_divider_input(
    input: &InputEvent,
    surface_registry: &mut SurfaceRegistry,
    total: Rect,
) -> Option<DirtyFlags> {
    match input {
        InputEvent::Mouse(mouse) => surface_registry.handle_workspace_divider_mouse(mouse, total),
        _ => None,
    }
}

/// Trait for scheduling diagnostic overlay expiry.
///
/// Implemented by TUI (crossbeam_channel::Sender) and GUI (EventLoopProxy)
/// to avoid duplicating the overlay scheduling logic.
pub trait DiagnosticOverlayScheduler {
    fn schedule_expiry(&self, delay: std::time::Duration, generation: u64);
}

/// Schedule a diagnostic overlay display with auto-dismiss.
///
/// Common logic shared by TUI and GUI backends.
pub fn schedule_diagnostic_overlay(
    scheduler: &impl DiagnosticOverlayScheduler,
    overlay: &mut PluginDiagnosticOverlayState,
    diagnostics: &[PluginDiagnostic],
) {
    let Some(generation) = overlay.record(diagnostics) else {
        return;
    };
    let Some(delay) = overlay.dismiss_after() else {
        return;
    };
    scheduler.schedule_expiry(delay, generation);
}

/// Synchronize all Salsa inputs for a render frame.
///
/// Shared sequence used by both TUI and GUI backends before rendering.
pub fn sync_salsa_for_render(
    db: &mut crate::salsa_db::KasaneDatabase,
    state: &AppState,
    registry: &PluginRuntime,
    handles: &crate::salsa_sync::SalsaInputHandles,
    contribution_cache: &mut crate::plugin::ContributionCache,
) {
    crate::salsa_sync::sync_inputs_from_state(db, state, handles);
    let view = registry.view();
    crate::salsa_sync::sync_display_directives(db, state, &view, handles);
    crate::salsa_sync::sync_plugin_contributions(db, state, &view, handles, contribution_cache);
}

/// Print a hint about reconnecting to a running Kakoune session.
///
/// Called from panic hooks in both TUI and GUI backends.
pub fn print_session_recovery_hint(session_name: Option<&str>) {
    eprintln!();
    eprintln!("Your Kakoune session is still running.");
    if let Some(name) = session_name {
        eprintln!("Reconnect with: kasane -c {name}");
    } else {
        eprintln!("List sessions with: kak -l");
        eprintln!("Reconnect with:     kasane -c <session_name>");
    }
}
