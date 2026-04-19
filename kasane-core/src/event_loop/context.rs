//! Deferred command context, constants, and helpers.

use crate::clipboard::SystemClipboard;
use crate::layout::Rect;
use crate::plugin::{
    AppView, HttpDispatcher, IoEvent, PluginAuthorities, PluginId, PluginRuntime,
    ProcessDispatcher, ProcessEvent,
};
use crate::scroll::ScrollPlan;
use crate::state::{AppState, DirtyFlags};
use crate::surface::SurfaceRegistry;
use crate::workspace::Placement;

use super::session::SessionReadyGate;
use super::{SessionHost, TimerScheduler};

/// Shared mutable context for deferred command handling.
///
/// Groups the many `&mut` parameters that `handle_deferred_commands` and
/// `handle_sourced_surface_commands` previously accepted individually.
pub struct DeferredContext<'a> {
    pub state: &'a mut AppState,
    pub registry: &'a mut PluginRuntime,
    pub surface_registry: &'a mut SurfaceRegistry,
    pub clipboard: &'a mut SystemClipboard,
    pub dirty: &'a mut DirtyFlags,
    pub timer: &'a dyn TimerScheduler,
    pub session_host: &'a mut dyn SessionHost,
    pub initial_resize_sent: &'a mut bool,
    pub session_ready_gate: Option<&'a mut SessionReadyGate>,
    pub scroll_plan_sink: &'a mut dyn FnMut(ScrollPlan),
    pub process_dispatcher: &'a mut dyn ProcessDispatcher,
    pub http_dispatcher: &'a mut dyn HttpDispatcher,
    pub workspace_changed: &'a mut bool,
    /// Scroll quantum (lines per scroll event), needed for injected input re-dispatch.
    pub scroll_amount: i32,
}

/// Maximum recursion depth for cascading deferred commands.
///
/// Prevents infinite loops when plugins produce deferred commands that trigger
/// further deferred commands (e.g., PluginMessage → PluginMessage chains).
pub(super) const MAX_COMMAND_CASCADE_DEPTH: usize = 8;

/// Maximum recursion depth for injected input events.
///
/// Prevents unbounded recursion when plugins inject keys that trigger
/// further injections (e.g., macro playback → key handler → inject another key).
pub(super) const MAX_INJECT_DEPTH: usize = 10;

/// Resolve the writer for the focused pane, falling back to `active_writer()`.
///
/// This is a macro rather than a function because it needs split borrows on
/// `DeferredContext` fields (`surface_registry`, `session_host`).
macro_rules! focused_writer {
    ($ctx:expr) => {{
        let focused_surface = $ctx.surface_registry.workspace().focused();
        let focused_session = $ctx.surface_registry.session_for_surface(focused_surface);
        match focused_session {
            Some(sid) => match $ctx.session_host.writer_for_session(sid) {
                Some(w) => w,
                None => $ctx.session_host.active_writer(),
            },
            None => $ctx.session_host.active_writer(),
        }
    }};
}
pub(super) use focused_writer;

/// Check that a command originates from a plugin with `DYNAMIC_SURFACE` authority.
///
/// Returns `Some(plugin_id)` when both the source plugin is present and the
/// authority is granted; logs a warning and returns `None` otherwise.
pub(super) fn require_surface_authority<'a>(
    registry: &PluginRuntime,
    command_source_plugin: Option<&'a PluginId>,
    command_name: &str,
) -> Option<&'a PluginId> {
    let Some(plugin_id) = command_source_plugin else {
        tracing::warn!("{command_name} ignored: missing command source plugin");
        return None;
    };
    if !registry.plugin_has_authority(plugin_id, PluginAuthorities::DYNAMIC_SURFACE) {
        tracing::warn!(
            plugin = plugin_id.0,
            "{command_name} denied: dynamic surface authority not granted"
        );
        return None;
    }
    Some(plugin_id)
}

/// Add a surface to the workspace and mark all dirty flags.
pub(super) fn dispatch_add_surface(
    ctx: &mut DeferredContext<'_>,
    surface_id: crate::surface::SurfaceId,
    placement: Placement,
) {
    crate::workspace::dispatch_workspace_command_with_total(
        ctx.surface_registry,
        crate::workspace::WorkspaceCommand::AddSurface {
            surface_id,
            placement,
        },
        ctx.dirty,
        Some(Rect {
            x: 0,
            y: 0,
            w: ctx.state.runtime.cols,
            h: ctx.state.runtime.rows,
        }),
    );
    *ctx.dirty |= DirtyFlags::ALL;
    *ctx.workspace_changed = true;
}

/// Deliver a `SpawnFailed` IO event to a plugin and cascade any resulting effects.
///
/// Returns `true` if the cascade produces a `Quit`.
pub(super) fn deliver_spawn_failure(
    ctx: &mut DeferredContext<'_>,
    plugin_id: &PluginId,
    job_id: u64,
    error_msg: &str,
    depth: usize,
) -> bool {
    let fail_event = IoEvent::Process(ProcessEvent::SpawnFailed {
        job_id,
        error: error_msg.to_string(),
    });
    let batch =
        ctx.registry
            .deliver_io_event_batch(plugin_id, &fail_event, &AppView::new(ctx.state));
    super::dispatch::apply_runtime_batch(batch, ctx, Some(plugin_id), depth + 1)
}

/// Result of attempting to unregister a plugin-owned surface.
pub(super) enum UnregisterResult {
    /// The surface was removed and the workspace entry closed.
    Removed,
    /// The surface is owned by a different plugin.
    OwnedByOther(PluginId),
    /// The surface has no owner or does not exist.
    NotFound,
}

/// Attempt to unregister a surface owned by `plugin_id`.
///
/// On success the surface is removed from the registry and its workspace
/// entry is closed, with dirty flags and workspace-changed set accordingly.
/// On failure the caller receives an [`UnregisterResult`] variant describing
/// why the operation was rejected so it can log context-specific warnings.
pub(super) fn try_unregister_owned_surface(
    surface_registry: &mut SurfaceRegistry,
    plugin_id: &PluginId,
    surface_id: crate::surface::SurfaceId,
    dirty: &mut DirtyFlags,
    workspace_changed: &mut bool,
) -> UnregisterResult {
    match surface_registry.surface_owner_plugin(surface_id) {
        Some(owner) if owner == plugin_id => {
            let _ = surface_registry.remove(surface_id);
            let _ = surface_registry.workspace_mut().close(surface_id);
            *dirty |= DirtyFlags::ALL;
            *workspace_changed = true;
            UnregisterResult::Removed
        }
        Some(owner) => UnregisterResult::OwnedByOther(owner.clone()),
        None => UnregisterResult::NotFound,
    }
}
