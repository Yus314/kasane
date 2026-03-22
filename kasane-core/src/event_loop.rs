//! Backend-agnostic event loop helpers.
//!
//! Extracts the deferred command handling logic that is shared between
//! TUI and GUI backends.

use std::any::Any;
use std::io::Write;
use std::time::Duration;

use crate::input::InputEvent;
use crate::layout::Rect;
use crate::plugin::{
    AppView, AppliedWinnerDelta, BootstrapEffects, Command, CommandResult, IoEvent,
    PluginAuthorities, PluginDiagnostic, PluginDiagnosticOverlayState, PluginId, PluginRuntime,
    ProcessDispatcher, ProcessEvent, ReadyBatch, RuntimeBatch, RuntimeEffects, SessionReadyCommand,
    StdinMode, execute_commands, extract_redraw_flags, partition_commands,
};
use crate::scroll::ScrollPlan;
use crate::session::{SessionId, SessionManager, SessionSpec, SessionStateStore};
use crate::state::{AppState, DirtyFlags};
use crate::surface::buffer::KakouneBufferSurface;
use crate::surface::pane_map::PaneMap;
use crate::surface::{SourcedSurfaceCommands, SurfaceEvent, SurfaceId, SurfaceRegistry};
use crate::workspace::{Placement, Workspace};

/// Structured result from processing a single event.
pub struct EventResult {
    pub flags: DirtyFlags,
    pub commands: Vec<Command>,
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

/// Register plugin-owned surfaces in the surface registry.
///
/// Iterates over all plugin surface sets, registers each surface, and applies
/// initial placements. If registration fails for any surface in a set, all
/// previously-registered surfaces from that set are rolled back and the plugin
/// is removed from the registry.
pub fn setup_plugin_surfaces(
    registry: &mut PluginRuntime,
    surface_registry: &mut SurfaceRegistry,
    state: &AppState,
) -> Vec<PluginDiagnostic> {
    let mut diagnostics = Vec::new();
    for surface_set in registry.collect_plugin_surfaces() {
        let mut registered_ids = Vec::new();
        let mut registration_error = None;
        for surface in surface_set.surfaces {
            let surface_id = surface.id();
            match surface_registry.try_register_for_owner(surface, Some(surface_set.owner.clone()))
            {
                Ok(()) => registered_ids.push(surface_id),
                Err(err) => {
                    registration_error = Some(err);
                    break;
                }
            }
        }
        if let Some(err) = registration_error {
            for surface_id in registered_ids {
                surface_registry.remove(surface_id);
            }
            registry.unload_plugin(&surface_set.owner);
            diagnostics.push(PluginDiagnostic::surface_registration_failed(
                surface_set.owner.clone(),
                err.clone(),
            ));
        } else {
            apply_surface_initial_placements(
                surface_registry,
                &registered_ids,
                surface_set.legacy_workspace_request.as_ref(),
                state,
            );
        }
    }
    diagnostics
}

pub fn register_builtin_surfaces(surface_registry: &mut SurfaceRegistry) {
    surface_registry
        .try_register(Box::new(KakouneBufferSurface::new()))
        .expect("failed to register built-in surface kasane.buffer");
    surface_registry
        .try_register(Box::new(crate::surface::status::StatusBarSurface::new()))
        .expect("failed to register built-in surface kasane.status");
}

/// Apply initial surface placements and log any unresolved ones.
fn apply_surface_initial_placements(
    surface_registry: &mut SurfaceRegistry,
    surface_ids: &[SurfaceId],
    legacy_request: Option<&Placement>,
    state: &AppState,
) {
    let mut bootstrap_dirty = DirtyFlags::empty();
    for (surface_id, request) in surface_registry.apply_initial_placements_with_total(
        surface_ids,
        legacy_request,
        &mut bootstrap_dirty,
        Some(Rect {
            x: 0,
            y: 0,
            w: state.cols,
            h: state.rows,
        }),
    ) {
        tracing::warn!(
            "skipping unresolved initial placement for surface {surface_id:?}: {request:?}"
        );
    }
}

pub fn rebuild_plugin_surface_registry(
    registry: &mut PluginRuntime,
    surface_registry: &mut SurfaceRegistry,
    state: &AppState,
) -> Vec<PluginDiagnostic> {
    let workspace = std::mem::take(surface_registry.workspace_mut());
    let mut rebuilt = SurfaceRegistry::with_workspace(workspace);
    register_builtin_surfaces(&mut rebuilt);
    let disabled_plugins = setup_plugin_surfaces(registry, &mut rebuilt, state);
    prune_missing_workspace_surfaces(&mut rebuilt);
    *surface_registry = rebuilt;
    disabled_plugins
}

/// Reconcile plugin-owned surfaces for the specific set of changed winners.
///
/// Unchanged owners keep their surface instances and workspace placement.
/// Changed owners are removed first, then re-registered from the current
/// registry. Missing surfaces are pruned from the workspace afterwards.
pub fn reconcile_plugin_surfaces(
    registry: &mut PluginRuntime,
    surface_registry: &mut SurfaceRegistry,
    state: &AppState,
    deltas: &[AppliedWinnerDelta],
) -> Vec<PluginDiagnostic> {
    if deltas.is_empty() {
        return vec![];
    }

    for delta in deltas {
        if delta.old.is_some() {
            surface_registry.remove_owned_surfaces(&delta.id);
        }
    }

    let mut diagnostics = Vec::new();
    for delta in deltas {
        if delta.new.is_none() {
            continue;
        }
        let Some(surface_set) = registry.collect_plugin_surfaces_for_owner(&delta.id) else {
            continue;
        };

        let mut registered_ids = Vec::new();
        let mut new_workspace_surfaces = Vec::new();
        let mut registration_error = None;

        for surface in surface_set.surfaces {
            let surface_id = surface.id();
            let already_in_workspace = surface_registry.workspace_contains(surface_id);
            match surface_registry.try_register_for_owner(surface, Some(surface_set.owner.clone()))
            {
                Ok(()) => {
                    registered_ids.push(surface_id);
                    if !already_in_workspace {
                        new_workspace_surfaces.push(surface_id);
                    }
                }
                Err(err) => {
                    registration_error = Some(err);
                    break;
                }
            }
        }

        if let Some(err) = registration_error {
            for surface_id in registered_ids {
                surface_registry.remove(surface_id);
            }
            registry.unload_plugin(&surface_set.owner);
            diagnostics.push(PluginDiagnostic::surface_registration_failed(
                surface_set.owner.clone(),
                err.clone(),
            ));
            continue;
        }

        if !new_workspace_surfaces.is_empty() {
            apply_surface_initial_placements(
                surface_registry,
                &new_workspace_surfaces,
                surface_set.legacy_workspace_request.as_ref(),
                state,
            );
        }
    }

    prune_missing_workspace_surfaces(surface_registry);
    diagnostics
}

fn prune_missing_workspace_surfaces(surface_registry: &mut SurfaceRegistry) {
    let stale_ids: Vec<_> = surface_registry
        .workspace()
        .root()
        .collect_ids()
        .into_iter()
        .filter(|surface_id| surface_registry.get(*surface_id).is_none())
        .collect();

    if stale_ids.is_empty() {
        return;
    }

    for stale_id in stale_ids {
        let _ = surface_registry.workspace_mut().close(stale_id);
    }

    let still_stale = surface_registry
        .workspace()
        .root()
        .collect_ids()
        .into_iter()
        .any(|surface_id| surface_registry.get(surface_id).is_none());
    if still_stale {
        *surface_registry.workspace_mut() = Workspace::default();
    }
}

/// Send resize commands to all pane clients so each knows its allocated area.
///
/// Should be called after terminal resize, split creation/deletion, or divider drag.
pub fn send_pane_resizes(
    surface_registry: &SurfaceRegistry,
    pane_map: &mut PaneMap,
    session_host: &mut dyn SessionHost,
    total: Rect,
) {
    let rects = surface_registry.workspace().compute_rects(total);
    for (surface_id, rect) in &rects {
        if let Some(session_id) = pane_map.session_for_surface(*surface_id)
            // Skip sessions whose kak process hasn't finished initializing.
            // The initial Resize is sent when their first Kakoune event arrives.
            && !pane_map.has_pending_resize(session_id)
            // Only send when dimensions actually changed to avoid an infinite
            // Resize → Draw → dirty → Resize feedback loop.
            && pane_map.needs_resize(session_id, rect.h, rect.w)
            && let Some(writer) = session_host.writer_for_session(session_id)
        {
            crate::io::send_request(
                writer,
                &crate::protocol::KasaneRequest::Resize {
                    rows: rect.h,
                    cols: rect.w,
                },
            );
        }
    }
}

/// Synchronize session metadata from SessionManager into AppState.
pub fn sync_session_metadata<R, W, C>(
    session_manager: &SessionManager<R, W, C>,
    session_states: &SessionStateStore,
    state: &mut AppState,
) {
    state.session_descriptors = session_manager.enriched_session_descriptors(session_states, state);
    state.active_session_key = session_manager.active_session_key().map(str::to_owned);
}

/// Groups the five `&mut` parameters shared by session lifecycle functions.
pub struct SessionMutContext<'a, R, W, C> {
    pub session_manager: &'a mut SessionManager<R, W, C>,
    pub session_states: &'a mut SessionStateStore,
    pub state: &'a mut AppState,
    pub dirty: &'a mut DirtyFlags,
    pub initial_resize_sent: &'a mut bool,
}

/// Handle a Kakoune session death event.
///
/// Closes the session, removes its state, and restores the next active session
/// if needed. Returns `true` if the application should quit (no sessions remain).
pub fn handle_session_death<R, W, C>(
    session_id: SessionId,
    ctx: &mut SessionMutContext<'_, R, W, C>,
) -> bool {
    let was_active = ctx.session_manager.active_session_id() == Some(session_id);
    let _ = ctx.session_manager.close(session_id);
    *ctx.dirty |= DirtyFlags::SESSION;
    ctx.session_states.remove(session_id);
    if ctx.session_manager.is_empty() {
        return true;
    }
    if was_active {
        let restored = ctx
            .session_manager
            .active_session_id()
            .is_some_and(|active| ctx.session_states.restore_into(active, ctx.state));
        if !restored {
            ctx.state.reset_for_session_switch();
        }
        *ctx.dirty |= DirtyFlags::ALL;
        *ctx.initial_resize_sent = false;
    }
    sync_session_metadata(ctx.session_manager, ctx.session_states, ctx.state);
    false
}

/// Spawn a new managed session, returning the session ID and reader on success.
///
/// The reader is returned so the backend can wire it up to its specific event
/// channel. The activation logic (state restore, dirty flags) is handled here.
pub fn spawn_session_core<R, W, C>(
    spec: &SessionSpec,
    activate: bool,
    ctx: &mut SessionMutContext<'_, R, W, C>,
    spawn_fn: fn(&SessionSpec) -> anyhow::Result<(R, W, C)>,
) -> Option<(SessionId, R)> {
    // Deduplicate the session key before spawning the process to avoid
    // orphaning a Kakoune process when insert() rejects a duplicate key.
    let spec = if ctx.session_manager.session_id_by_key(&spec.key).is_some() {
        let base = &spec.key;
        let mut deduped = None;
        for i in 2..=100 {
            let candidate = format!("{base}-{i}");
            if ctx.session_manager.session_id_by_key(&candidate).is_none() {
                deduped = Some(SessionSpec::new(
                    candidate,
                    spec.session.clone(),
                    spec.args.clone(),
                ));
                break;
            }
        }
        match deduped {
            Some(s) => s,
            None => {
                tracing::error!(key = spec.key, "failed to find unique session key");
                return None;
            }
        }
    } else {
        spec.clone()
    };
    let Ok((reader, writer, child)) = spawn_fn(&spec) else {
        tracing::error!("failed to spawn session {}", spec.key);
        return None;
    };
    let Ok(session_id) = ctx.session_manager.insert(spec, reader, writer, child) else {
        tracing::error!("failed to register spawned session");
        return None;
    };
    ctx.session_states.ensure_session(session_id, ctx.state);
    *ctx.dirty |= DirtyFlags::SESSION;
    let reader = ctx
        .session_manager
        .take_reader(session_id)
        .expect("spawned session reader missing");
    if activate {
        ctx.session_manager
            .sync_and_activate(ctx.session_states, session_id, ctx.state)
            .expect("spawned session must be activeable");
        if !ctx.session_states.restore_into(session_id, ctx.state) {
            ctx.state.reset_for_session_switch();
        }
        *ctx.dirty |= DirtyFlags::ALL;
        *ctx.initial_resize_sent = false;
        sync_session_metadata(ctx.session_manager, ctx.session_states, ctx.state);
    }
    sync_session_metadata(ctx.session_manager, ctx.session_states, ctx.state);
    Some((session_id, reader))
}

/// Close a managed session by key, or the active session when `key` is `None`.
///
/// Returns `true` when the application should exit because no sessions remain.
pub fn close_session_core<R, W, C>(
    key: Option<&str>,
    ctx: &mut SessionMutContext<'_, R, W, C>,
) -> bool {
    let target = key
        .and_then(|k| ctx.session_manager.session_id_by_key(k))
        .or_else(|| ctx.session_manager.active_session_id());
    let Some(target) = target else {
        return false;
    };
    let was_active = ctx.session_manager.active_session_id() == Some(target);
    let _ = ctx.session_manager.close(target);
    ctx.session_states.remove(target);
    *ctx.dirty |= DirtyFlags::SESSION;
    if ctx.session_manager.is_empty() {
        return true;
    }
    if was_active {
        let restored = ctx
            .session_manager
            .active_session_id()
            .is_some_and(|active| ctx.session_states.restore_into(active, ctx.state));
        if !restored {
            ctx.state.reset_for_session_switch();
        }
        *ctx.dirty |= DirtyFlags::ALL;
        *ctx.initial_resize_sent = false;
    }
    sync_session_metadata(ctx.session_manager, ctx.session_states, ctx.state);
    false
}

/// Switch to an existing managed session by key.
///
/// No-op if the key doesn't exist or is already active.
pub fn switch_session_core<R, W, C>(key: &str, ctx: &mut SessionMutContext<'_, R, W, C>) {
    let Some(target) = ctx.session_manager.session_id_by_key(key) else {
        return;
    };
    if ctx.session_manager.active_session_id() == Some(target) {
        return;
    }
    ctx.session_manager
        .sync_and_activate(ctx.session_states, target, ctx.state)
        .expect("switch target must be valid");
    if !ctx.session_states.restore_into(target, ctx.state) {
        ctx.state.reset_for_session_switch();
    }
    *ctx.dirty |= DirtyFlags::ALL | DirtyFlags::SESSION;
    *ctx.initial_resize_sent = false;
    sync_session_metadata(ctx.session_manager, ctx.session_states, ctx.state);
}

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
    let query = surface_registry.workspace().query(Rect {
        x: 0,
        y: 0,
        w: state.cols,
        h: state.rows,
    });
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
        spec: SessionSpec,
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

#[derive(Debug, Clone, Default)]
pub struct SessionReadyGate {
    active_session_key: Option<String>,
    generation: u64,
    notified_generation: Option<u64>,
    initial_resize_sent: bool,
}

impl SessionReadyGate {
    pub fn sync_active_session(&mut self, key: Option<&str>) -> bool {
        let next = key.map(str::to_owned);
        if self.active_session_key == next {
            return false;
        }
        self.active_session_key = next;
        self.generation += 1;
        self.notified_generation = None;
        self.initial_resize_sent = false;
        true
    }

    pub fn mark_initial_resize_sent(&mut self) {
        self.initial_resize_sent = true;
    }

    pub fn clear_initial_resize(&mut self) {
        self.initial_resize_sent = false;
    }

    pub fn should_notify_ready(&self) -> bool {
        self.active_session_key.is_some()
            && self.initial_resize_sent
            && self.notified_generation != Some(self.generation)
    }

    pub fn mark_ready_notified(&mut self) {
        self.notified_generation = Some(self.generation);
    }

    pub fn rearm_ready_notification(&mut self) {
        self.notified_generation = None;
    }
}

/// Shared mutable context for deferred command handling.
///
/// Groups the many `&mut` parameters that `handle_deferred_commands` and
/// `handle_sourced_surface_commands` previously accepted individually.
pub struct DeferredContext<'a> {
    pub state: &'a mut AppState,
    pub registry: &'a mut PluginRuntime,
    pub surface_registry: &'a mut SurfaceRegistry,
    pub pane_map: &'a mut PaneMap,
    pub clipboard_get: &'a mut dyn FnMut() -> Option<String>,
    pub dirty: &'a mut DirtyFlags,
    pub timer: &'a dyn TimerScheduler,
    pub session_host: &'a mut dyn SessionHost,
    pub initial_resize_sent: &'a mut bool,
    pub session_ready_gate: Option<&'a mut SessionReadyGate>,
    pub scroll_plan_sink: &'a mut dyn FnMut(ScrollPlan),
    pub process_dispatcher: &'a mut dyn ProcessDispatcher,
    pub workspace_changed: &'a mut bool,
    /// Scroll quantum (lines per scroll event), needed for injected input re-dispatch.
    pub scroll_amount: i32,
}

/// Maximum recursion depth for cascading deferred commands.
///
/// Prevents infinite loops when plugins produce deferred commands that trigger
/// further deferred commands (e.g., PluginMessage → PluginMessage chains).
const MAX_COMMAND_CASCADE_DEPTH: usize = 8;

/// Maximum recursion depth for injected input events.
///
/// Prevents unbounded recursion when plugins inject keys that trigger
/// further injections (e.g., macro playback → key handler → inject another key).
const MAX_INJECT_DEPTH: usize = 10;

/// Resolve the writer for the focused pane, falling back to `active_writer()`.
///
/// This is a macro rather than a function because it needs split borrows on
/// `DeferredContext` fields (`surface_registry`, `pane_map`, `session_host`).
macro_rules! focused_writer {
    ($ctx:expr) => {{
        let focused_surface = $ctx.surface_registry.workspace().focused();
        let focused_session = $ctx.pane_map.session_for_surface(focused_surface);
        match focused_session {
            Some(sid) => match $ctx.session_host.writer_for_session(sid) {
                Some(w) => w,
                None => $ctx.session_host.active_writer(),
            },
            None => $ctx.session_host.active_writer(),
        }
    }};
}

/// Check that a command originates from a plugin with `DYNAMIC_SURFACE` authority.
///
/// Returns `Some(plugin_id)` when both the source plugin is present and the
/// authority is granted; logs a warning and returns `None` otherwise.
fn require_surface_authority<'a>(
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
fn dispatch_add_surface(
    ctx: &mut DeferredContext<'_>,
    surface_id: SurfaceId,
    placement: Placement,
) {
    crate::workspace::dispatch_workspace_command_with_total(
        ctx.surface_registry,
        crate::workspace::WorkspaceCommand::AddSurface {
            surface_id,
            placement,
        },
        ctx.dirty,
        Some(crate::layout::Rect {
            x: 0,
            y: 0,
            w: ctx.state.cols,
            h: ctx.state.rows,
        }),
    );
    *ctx.dirty |= DirtyFlags::ALL;
    *ctx.workspace_changed = true;
}

/// Deliver a `SpawnFailed` IO event to a plugin and cascade any resulting effects.
///
/// Returns `true` if the cascade produces a `Quit`.
fn deliver_spawn_failure(
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
    apply_runtime_batch(batch, ctx, Some(plugin_id), depth + 1)
}

/// Result of attempting to unregister a plugin-owned surface.
enum UnregisterResult {
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
fn try_unregister_owned_surface(
    surface_registry: &mut SurfaceRegistry,
    plugin_id: &PluginId,
    surface_id: SurfaceId,
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

/// Handle deferred commands (timers, inter-plugin messages, config overrides).
///
/// Returns `true` if a `Quit` command was encountered.
pub fn handle_deferred_commands(
    deferred: Vec<Command>,
    ctx: &mut DeferredContext<'_>,
    command_source_plugin: Option<&PluginId>,
) -> bool {
    handle_deferred_commands_inner(deferred, ctx, command_source_plugin, 0)
}

/// Execute a command batch, extracting host-owned scroll plans and cascading deferred effects.
pub fn handle_command_batch(
    commands: Vec<Command>,
    ctx: &mut DeferredContext<'_>,
    command_source_plugin: Option<&PluginId>,
) -> bool {
    handle_command_batch_inner(commands, ctx, command_source_plugin, 0)
}

fn handle_command_batch_inner(
    commands: Vec<Command>,
    ctx: &mut DeferredContext<'_>,
    command_source_plugin: Option<&PluginId>,
    depth: usize,
) -> bool {
    let (immediate, deferred) = partition_commands(commands);

    // Route commands to the focused pane's Kakoune client when in multi-pane mode.
    let writer = focused_writer!(ctx);
    if matches!(
        execute_commands(immediate, writer, ctx.clipboard_get),
        CommandResult::Quit
    ) {
        return true;
    }
    handle_deferred_commands_inner(deferred, ctx, command_source_plugin, depth)
}

fn handle_deferred_commands_inner(
    deferred: Vec<Command>,
    ctx: &mut DeferredContext<'_>,
    command_source_plugin: Option<&PluginId>,
    depth: usize,
) -> bool {
    if depth >= MAX_COMMAND_CASCADE_DEPTH {
        tracing::warn!(
            depth,
            "command cascade depth limit reached, dropping {} deferred commands",
            deferred.len()
        );
        return false;
    }

    for cmd in deferred {
        let quit = match &cmd {
            Command::PluginMessage { .. }
            | Command::ScheduleTimer { .. }
            | Command::SetConfig { .. } => {
                handle_inter_plugin_command(cmd, ctx, command_source_plugin, depth)
            }
            Command::RegisterSurface { .. }
            | Command::RegisterSurfaceRequested { .. }
            | Command::UnregisterSurface { .. }
            | Command::UnregisterSurfaceKey { .. } => {
                handle_surface_mgmt_command(cmd, ctx, command_source_plugin, depth)
            }
            Command::Workspace(_) | Command::RegisterThemeTokens(_) => {
                handle_workspace_command(cmd, ctx, command_source_plugin, depth)
            }
            Command::SpawnProcess { .. }
            | Command::WriteToProcess { .. }
            | Command::CloseProcessStdin { .. }
            | Command::KillProcess { .. }
            | Command::ResizePty { .. } => {
                handle_process_command(cmd, ctx, command_source_plugin, depth)
            }
            Command::SpawnPaneClient { .. }
            | Command::ClosePaneClient { .. }
            | Command::Session(_)
            | Command::InjectInput(_) => {
                handle_session_pane_command(cmd, ctx, command_source_plugin, depth)
            }
            // Immediate commands should not reach the deferred handler
            _ => unreachable!("immediate commands filtered by partition_commands"),
        };
        if quit == Some(true) {
            return true;
        }
    }
    false
}

/// Handle inter-plugin communication commands: messages, timers, and config overrides.
fn handle_inter_plugin_command(
    cmd: Command,
    ctx: &mut DeferredContext<'_>,
    command_source_plugin: Option<&PluginId>,
    depth: usize,
) -> Option<bool> {
    let _ = command_source_plugin;
    match cmd {
        Command::PluginMessage { target, payload } => {
            let batch =
                ctx.registry
                    .deliver_message_batch(&target, payload, &AppView::new(ctx.state));
            if apply_runtime_batch(batch, ctx, Some(&target), depth + 1) {
                return Some(true);
            }
        }
        Command::ScheduleTimer {
            delay,
            target,
            payload,
        } => {
            ctx.timer.schedule_timer(delay, target, payload);
        }
        Command::SetConfig { key, value } => {
            crate::state::apply_set_config(ctx.state, ctx.dirty, &key, &value);
        }
        _ => unreachable!(),
    }
    Some(false)
}

/// Handle dynamic surface registration and unregistration commands.
fn handle_surface_mgmt_command(
    cmd: Command,
    ctx: &mut DeferredContext<'_>,
    command_source_plugin: Option<&PluginId>,
    _depth: usize,
) -> Option<bool> {
    match cmd {
        Command::RegisterSurface { surface, placement } => {
            let Some(plugin_id) =
                require_surface_authority(ctx.registry, command_source_plugin, "RegisterSurface")
            else {
                return Some(false);
            };

            let surface_id = surface.id();
            match ctx
                .surface_registry
                .try_register_for_owner(surface, Some(plugin_id.clone()))
            {
                Ok(()) => {
                    dispatch_add_surface(ctx, surface_id, placement);
                }
                Err(err) => {
                    tracing::warn!(
                        plugin = plugin_id.0,
                        surface_id = surface_id.0,
                        "RegisterSurface ignored: {err:?}"
                    );
                }
            }
        }
        Command::RegisterSurfaceRequested { surface, placement } => {
            let Some(plugin_id) = require_surface_authority(
                ctx.registry,
                command_source_plugin,
                "RegisterSurfaceRequested",
            ) else {
                return Some(false);
            };

            let surface_id = surface.id();
            match ctx
                .surface_registry
                .try_register_for_owner(surface, Some(plugin_id.clone()))
            {
                Ok(()) => {
                    let Some(placement) =
                        ctx.surface_registry.resolve_placement_request(&placement)
                    else {
                        let _ = ctx.surface_registry.remove(surface_id);
                        tracing::warn!(
                            plugin = plugin_id.0,
                            surface_id = surface_id.0,
                            "RegisterSurfaceRequested ignored: unresolved placement request"
                        );
                        return Some(false);
                    };

                    dispatch_add_surface(ctx, surface_id, placement);
                }
                Err(err) => {
                    tracing::warn!(
                        plugin = plugin_id.0,
                        surface_id = surface_id.0,
                        "RegisterSurfaceRequested ignored: {err:?}"
                    );
                }
            }
        }
        Command::UnregisterSurface { surface_id } => {
            let Some(plugin_id) =
                require_surface_authority(ctx.registry, command_source_plugin, "UnregisterSurface")
            else {
                return Some(false);
            };

            match try_unregister_owned_surface(
                ctx.surface_registry,
                plugin_id,
                surface_id,
                ctx.dirty,
                ctx.workspace_changed,
            ) {
                UnregisterResult::Removed => {}
                UnregisterResult::OwnedByOther(owner) => {
                    tracing::warn!(
                        plugin = plugin_id.0,
                        owner = owner.0,
                        surface_id = surface_id.0,
                        "UnregisterSurface ignored: surface owned by another plugin"
                    );
                }
                UnregisterResult::NotFound => {
                    tracing::warn!(
                        plugin = plugin_id.0,
                        surface_id = surface_id.0,
                        "UnregisterSurface ignored: surface is not plugin-owned or missing"
                    );
                }
            }
        }
        Command::UnregisterSurfaceKey { surface_key } => {
            let Some(plugin_id) = require_surface_authority(
                ctx.registry,
                command_source_plugin,
                "UnregisterSurfaceKey",
            ) else {
                return Some(false);
            };

            let Some(surface_id) = ctx.surface_registry.surface_id_by_key(&surface_key) else {
                tracing::warn!(
                    plugin = plugin_id.0,
                    surface_key,
                    "UnregisterSurfaceKey ignored: unknown surface key"
                );
                return Some(false);
            };

            match try_unregister_owned_surface(
                ctx.surface_registry,
                plugin_id,
                surface_id,
                ctx.dirty,
                ctx.workspace_changed,
            ) {
                UnregisterResult::Removed => {}
                UnregisterResult::OwnedByOther(owner) => {
                    tracing::warn!(
                        plugin = plugin_id.0,
                        owner = owner.0,
                        surface_id = surface_id.0,
                        surface_key,
                        "UnregisterSurfaceKey ignored: surface owned by another plugin"
                    );
                }
                UnregisterResult::NotFound => {
                    tracing::warn!(
                        plugin = plugin_id.0,
                        surface_id = surface_id.0,
                        surface_key,
                        "UnregisterSurfaceKey ignored: surface is not plugin-owned or missing"
                    );
                }
            }
        }
        _ => unreachable!(),
    }
    Some(false)
}

/// Handle workspace layout and theme token commands.
fn handle_workspace_command(
    cmd: Command,
    ctx: &mut DeferredContext<'_>,
    _command_source_plugin: Option<&PluginId>,
    _depth: usize,
) -> Option<bool> {
    match cmd {
        Command::Workspace(ws_cmd) => {
            // Auto-register ClientBufferSurface for unknown surface IDs
            if let crate::workspace::WorkspaceCommand::AddSurface { surface_id, .. } = &ws_cmd
                && ctx.surface_registry.get(*surface_id).is_none()
            {
                let _ = ctx.surface_registry.try_register(Box::new(
                    crate::surface::buffer::ClientBufferSurface::new(*surface_id),
                ));
            }
            let mut workspace_dirty = DirtyFlags::empty();
            crate::workspace::dispatch_workspace_command_with_total(
                ctx.surface_registry,
                ws_cmd,
                &mut workspace_dirty,
                Some(crate::layout::Rect {
                    x: 0,
                    y: 0,
                    w: ctx.state.cols,
                    h: ctx.state.rows,
                }),
            );
            *ctx.dirty |= workspace_dirty;
            if !workspace_dirty.is_empty() {
                *ctx.workspace_changed = true;
            }
        }
        Command::RegisterThemeTokens(_tokens) => {
            // Theme token registration will be handled when Theme is
            // accessible from the event loop (Phase 1 completion).
        }
        _ => unreachable!(),
    }
    Some(false)
}

/// Handle process lifecycle commands: spawn, write, close stdin, kill, and PTY resize.
fn handle_process_command(
    cmd: Command,
    ctx: &mut DeferredContext<'_>,
    command_source_plugin: Option<&PluginId>,
    depth: usize,
) -> Option<bool> {
    match cmd {
        Command::SpawnProcess {
            job_id,
            program,
            args,
            stdin_mode,
        } => {
            if let Some(plugin_id) = command_source_plugin {
                // PTY mode requires PTY_PROCESS authority in addition to process spawn
                let pty_denied = matches!(stdin_mode, StdinMode::Pty { .. })
                    && !ctx
                        .registry
                        .plugin_has_authority(plugin_id, PluginAuthorities::PTY_PROCESS);
                if pty_denied {
                    tracing::warn!(
                        plugin = plugin_id.0.as_str(),
                        "SpawnProcess denied: PTY_PROCESS authority not granted"
                    );
                    if deliver_spawn_failure(
                        ctx,
                        plugin_id,
                        job_id,
                        "PTY_PROCESS authority not granted",
                        depth,
                    ) {
                        return Some(true);
                    }
                } else if ctx.registry.plugin_allows_process_spawn(plugin_id) {
                    ctx.process_dispatcher
                        .spawn(plugin_id, job_id, &program, &args, stdin_mode);
                } else {
                    tracing::warn!(
                        plugin = plugin_id.0,
                        "SpawnProcess denied: process capability not granted"
                    );
                    if deliver_spawn_failure(
                        ctx,
                        plugin_id,
                        job_id,
                        "process capability not granted",
                        depth,
                    ) {
                        return Some(true);
                    }
                }
            }
        }
        Command::WriteToProcess { job_id, data } => {
            if let Some(plugin_id) = command_source_plugin {
                ctx.process_dispatcher.write(plugin_id, job_id, &data);
            }
        }
        Command::CloseProcessStdin { job_id } => {
            if let Some(plugin_id) = command_source_plugin {
                ctx.process_dispatcher.close_stdin(plugin_id, job_id);
            }
        }
        Command::KillProcess { job_id } => {
            if let Some(plugin_id) = command_source_plugin {
                ctx.process_dispatcher.kill(plugin_id, job_id);
            }
        }
        Command::ResizePty { job_id, rows, cols } => {
            if let Some(plugin_id) = command_source_plugin {
                if !ctx
                    .registry
                    .plugin_has_authority(plugin_id, PluginAuthorities::PTY_PROCESS)
                {
                    tracing::warn!(
                        plugin = plugin_id.0.as_str(),
                        "ResizePty rejected: plugin lacks PTY_PROCESS authority"
                    );
                } else {
                    ctx.process_dispatcher
                        .resize_pty(plugin_id, job_id, rows, cols);
                }
            }
        }
        _ => unreachable!(),
    }
    Some(false)
}

/// Handle session lifecycle and pane management commands.
fn handle_session_pane_command(
    cmd: Command,
    ctx: &mut DeferredContext<'_>,
    _command_source_plugin: Option<&PluginId>,
    depth: usize,
) -> Option<bool> {
    match cmd {
        Command::SpawnPaneClient {
            surface_id,
            placement,
        } => {
            if let Some(server_name) = ctx.pane_map.server_session_name().map(str::to_owned) {
                let key = format!("pane-{}", surface_id.0);
                let spec = SessionSpec::new(
                    key.clone(),
                    Some(server_name.clone()),
                    vec!["-c".to_string(), server_name],
                );
                // Spawn session without activating (keep focus on current pane)
                ctx.session_host.spawn_session(
                    spec,
                    false,
                    ctx.state,
                    ctx.dirty,
                    ctx.initial_resize_sent,
                );

                // Bind surface -> session in PaneMap and defer initial resize
                if let Some(session_id) = ctx.session_host.session_id_by_key(&key) {
                    ctx.pane_map.bind(surface_id, session_id);
                    ctx.pane_map.mark_pending_resize(session_id);
                }

                // Register ClientBufferSurface
                let _ = ctx.surface_registry.try_register(Box::new(
                    crate::surface::buffer::ClientBufferSurface::new(surface_id),
                ));

                // Add to workspace
                dispatch_add_surface(ctx, surface_id, placement);
            } else {
                tracing::warn!("SpawnPaneClient ignored: no server session name available");
            }
        }
        Command::ClosePaneClient { surface_id } => {
            if let Some(_session_id) = ctx.pane_map.unbind_surface(surface_id) {
                // Close the Kakoune client session by key
                let key = format!("pane-{}", surface_id.0);
                ctx.session_host.close_session(
                    Some(&key),
                    ctx.state,
                    ctx.dirty,
                    ctx.initial_resize_sent,
                );
            }
            ctx.surface_registry.remove(surface_id);
            let _ = ctx.surface_registry.workspace_mut().close(surface_id);
            *ctx.dirty |= DirtyFlags::ALL;
            *ctx.workspace_changed = true;
        }
        Command::Session(cmd) => {
            match cmd {
                crate::session::SessionCommand::Spawn {
                    key,
                    session,
                    args,
                    activate,
                } => {
                    ctx.session_host.spawn_session(
                        SessionSpec::with_fallback_key(key, session, args),
                        activate,
                        ctx.state,
                        ctx.dirty,
                        ctx.initial_resize_sent,
                    );
                }
                crate::session::SessionCommand::Close { key } => {
                    if ctx.session_host.close_session(
                        key.as_deref(),
                        ctx.state,
                        ctx.dirty,
                        ctx.initial_resize_sent,
                    ) {
                        return Some(true);
                    }
                }
                crate::session::SessionCommand::Switch { key } => {
                    ctx.session_host.switch_session(
                        &key,
                        ctx.state,
                        ctx.dirty,
                        ctx.initial_resize_sent,
                    );
                }
            }
            // A session command may have set initial_resize_sent=false.
            // Send the resize immediately so the new session is unblocked
            // and subsequent input events are not suppressed.
            if !*ctx.initial_resize_sent {
                crate::io::send_initial_resize(
                    ctx.session_host.active_writer(),
                    ctx.initial_resize_sent,
                    ctx.state.rows,
                    ctx.state.cols,
                );
            }
            // Notify plugins of SESSION change so they update cached state
            // (e.g. session_count). Without this, plugins hold stale values
            // until the next Kakoune Draw triggers on_state_changed.
            let batch = ctx
                .registry
                .notify_state_changed_batch(&AppView::new(ctx.state), DirtyFlags::SESSION);
            if apply_runtime_batch_without_session_deferred(batch, ctx, None, depth + 1) {
                return Some(true);
            }
        }
        Command::InjectInput(input_event) => {
            if depth >= MAX_INJECT_DEPTH {
                tracing::warn!(
                    depth,
                    "inject input depth limit reached, dropping injected event"
                );
            } else {
                use crate::state::{Msg, update};

                let msg = Msg::from(input_event);
                let state = std::mem::take(ctx.state);
                let (returned_state, result) =
                    update(Box::new(state), msg, ctx.registry, ctx.scroll_amount);
                *ctx.state = *returned_state;
                *ctx.dirty |= result.flags;
                for plan in result.scroll_plans {
                    (ctx.scroll_plan_sink)(plan);
                }
                if !result.commands.is_empty()
                    && handle_command_batch_inner(
                        result.commands,
                        ctx,
                        result.source_plugin.as_ref(),
                        depth + 1,
                    )
                {
                    return Some(true);
                }
            }
        }
        _ => unreachable!(),
    }
    Some(false)
}

/// Execute grouped surface commands while preserving each surface owner's plugin identity.
///
/// Returns `true` if a `Quit` command was encountered.
pub fn handle_sourced_surface_commands(
    command_groups: Vec<SourcedSurfaceCommands>,
    ctx: &mut DeferredContext<'_>,
) -> bool {
    for entry in command_groups {
        if handle_command_batch(entry.commands, ctx, entry.source_plugin.as_ref()) {
            return true;
        }
    }
    false
}

pub fn apply_bootstrap_effects(effects: BootstrapEffects, dirty: &mut DirtyFlags) {
    *dirty |= effects.redraw;
}

fn apply_runtime_effects(
    mut effects: RuntimeEffects,
    ctx: &mut DeferredContext<'_>,
    command_source_plugin: Option<&PluginId>,
    depth: usize,
) -> bool {
    *ctx.dirty |= effects.redraw;
    *ctx.dirty |= extract_redraw_flags(&mut effects.commands);

    for plan in effects.scroll_plans {
        (ctx.scroll_plan_sink)(plan);
    }

    if effects.commands.is_empty() {
        return false;
    }
    handle_command_batch_inner(effects.commands, ctx, command_source_plugin, depth)
}

fn apply_runtime_batch(
    batch: RuntimeBatch,
    ctx: &mut DeferredContext<'_>,
    command_source_plugin: Option<&PluginId>,
    depth: usize,
) -> bool {
    apply_runtime_effects(batch.effects, ctx, command_source_plugin, depth)
}

fn apply_runtime_batch_without_session_deferred(
    batch: RuntimeBatch,
    ctx: &mut DeferredContext<'_>,
    command_source_plugin: Option<&PluginId>,
    depth: usize,
) -> bool {
    let mut effects = batch.effects;
    *ctx.dirty |= effects.redraw;
    *ctx.dirty |= extract_redraw_flags(&mut effects.commands);
    for plan in effects.scroll_plans {
        (ctx.scroll_plan_sink)(plan);
    }

    let commands = effects.commands;
    if commands.is_empty() {
        return false;
    }

    let (immediate, nested_deferred) = partition_commands(commands);
    if matches!(
        execute_commands(immediate, focused_writer!(ctx), ctx.clipboard_get),
        CommandResult::Quit
    ) {
        return true;
    }
    let nested_non_session: Vec<_> = nested_deferred
        .into_iter()
        .filter(|d| !matches!(d, Command::Session(_)))
        .collect();
    handle_deferred_commands_inner(nested_non_session, ctx, command_source_plugin, depth)
}

pub fn sync_session_ready_gate(gate: &mut SessionReadyGate, state: &AppState) -> bool {
    gate.sync_active_session(state.active_session_key.as_deref())
}

pub fn maybe_flush_active_session_ready(ctx: &mut DeferredContext<'_>) -> bool {
    let should_notify = ctx
        .session_ready_gate
        .as_deref_mut()
        .is_some_and(|gate| gate.should_notify_ready());
    if !should_notify {
        return false;
    }

    let batch = ctx
        .registry
        .notify_active_session_ready_batch(&AppView::new(ctx.state));
    let should_quit = apply_ready_batch(batch, ctx);
    if let Some(gate) = ctx.session_ready_gate.as_deref_mut() {
        gate.mark_ready_notified();
    }
    should_quit
}

pub fn apply_ready_batch(batch: ReadyBatch, ctx: &mut DeferredContext<'_>) -> bool {
    *ctx.dirty |= batch.effects.redraw;

    for command in batch.effects.commands {
        match command {
            SessionReadyCommand::SendToKakoune(request) => {
                if matches!(
                    execute_commands(
                        vec![Command::SendToKakoune(request)],
                        focused_writer!(ctx),
                        ctx.clipboard_get,
                    ),
                    CommandResult::Quit
                ) {
                    return true;
                }
            }
            SessionReadyCommand::Paste => {
                if matches!(
                    execute_commands(
                        vec![Command::Paste],
                        focused_writer!(ctx),
                        ctx.clipboard_get,
                    ),
                    CommandResult::Quit
                ) {
                    return true;
                }
            }
            SessionReadyCommand::PluginMessage { target, payload } => {
                let batch =
                    ctx.registry
                        .deliver_message_batch(&target, payload, &AppView::new(ctx.state));
                if apply_runtime_batch(batch, ctx, Some(&target), 0) {
                    return true;
                }
            }
        }
    }

    for plan in batch.effects.scroll_plans {
        (ctx.scroll_plan_sink)(plan);
    }

    false
}

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

#[cfg(test)]
mod tests {
    use super::*;

    use crate::layout::SplitDirection;
    use crate::plugin::{
        AppView, AppliedWinnerDelta, Command, PluginAuthorities, PluginBackend, PluginDescriptor,
        PluginId, PluginRank, PluginRevision, PluginSource, RuntimeEffects, StdinMode,
    };
    use crate::scroll::{ScrollAccumulationMode, ScrollCurve, ScrollPlan};
    use crate::surface::{SurfacePlacementRequest, SurfaceRegistrationError};
    use crate::test_support::TestSurfaceBuilder;

    struct TestPlugin {
        id: PluginId,
        allow_spawn: bool,
        authorities: PluginAuthorities,
    }

    impl PluginBackend for TestPlugin {
        fn id(&self) -> PluginId {
            self.id.clone()
        }

        fn allows_process_spawn(&self) -> bool {
            self.allow_spawn
        }

        fn authorities(&self) -> PluginAuthorities {
            self.authorities
        }
    }

    struct RuntimeMessagePlugin;

    impl PluginBackend for RuntimeMessagePlugin {
        fn id(&self) -> PluginId {
            PluginId("runtime-message".to_string())
        }

        fn update_effects(&mut self, msg: &mut dyn Any, _state: &AppView<'_>) -> RuntimeEffects {
            if msg.downcast_ref::<u32>() != Some(&11) {
                return RuntimeEffects::default();
            }
            RuntimeEffects {
                redraw: DirtyFlags::INFO,
                commands: vec![Command::RequestRedraw(DirtyFlags::STATUS)],
                scroll_plans: vec![ScrollPlan {
                    total_amount: 2,
                    line: 3,
                    column: 5,
                    frame_interval_ms: 12,
                    curve: ScrollCurve::Linear,
                    accumulation: ScrollAccumulationMode::Add,
                }],
            }
        }
    }

    struct NoopTimer;

    impl TimerScheduler for NoopTimer {
        fn schedule_timer(
            &self,
            _delay: Duration,
            _target: PluginId,
            _payload: Box<dyn Any + Send>,
        ) {
        }
    }

    #[derive(Default)]
    struct NoopSessionRuntime {
        writer: Vec<u8>,
    }

    impl SessionRuntime for NoopSessionRuntime {
        fn spawn_session(
            &mut self,
            _spec: SessionSpec,
            _activate: bool,
            _state: &mut AppState,
            _dirty: &mut DirtyFlags,
            _initial_resize_sent: &mut bool,
        ) {
        }

        fn close_session(
            &mut self,
            _key: Option<&str>,
            _state: &mut AppState,
            _dirty: &mut DirtyFlags,
            _initial_resize_sent: &mut bool,
        ) -> bool {
            false
        }

        fn switch_session(
            &mut self,
            _key: &str,
            _state: &mut AppState,
            _dirty: &mut DirtyFlags,
            _initial_resize_sent: &mut bool,
        ) {
        }
    }

    impl SessionHost for NoopSessionRuntime {
        fn active_writer(&mut self) -> &mut dyn Write {
            &mut self.writer
        }
    }

    #[derive(Default)]
    struct RecordingDispatcher {
        spawned: Vec<(PluginId, u64, String, Vec<String>, StdinMode)>,
    }

    impl ProcessDispatcher for RecordingDispatcher {
        fn spawn(
            &mut self,
            plugin_id: &PluginId,
            job_id: u64,
            program: &str,
            args: &[String],
            stdin_mode: StdinMode,
        ) {
            self.spawned.push((
                plugin_id.clone(),
                job_id,
                program.to_string(),
                args.to_vec(),
                stdin_mode,
            ));
        }

        fn write(&mut self, _plugin_id: &PluginId, _job_id: u64, _data: &[u8]) {}

        fn close_stdin(&mut self, _plugin_id: &PluginId, _job_id: u64) {}

        fn kill(&mut self, _plugin_id: &PluginId, _job_id: u64) {}

        fn resize_pty(&mut self, _plugin_id: &PluginId, _job_id: u64, _rows: u16, _cols: u16) {}

        fn remove_finished_job(&mut self, _plugin_id: &PluginId, _job_id: u64) {}
    }

    struct SurfacePlugin;

    impl PluginBackend for SurfacePlugin {
        fn id(&self) -> PluginId {
            PluginId("surface-plugin".to_string())
        }

        fn surfaces(&mut self) -> Vec<Box<dyn crate::surface::Surface>> {
            vec![TestSurfaceBuilder::new(SurfaceId(200)).build()]
        }

        fn workspace_request(&self) -> Option<Placement> {
            Some(Placement::SplitFocused {
                direction: SplitDirection::Vertical,
                ratio: 0.5,
            })
        }
    }

    struct ReplacementSurfacePlugin;

    impl PluginBackend for ReplacementSurfacePlugin {
        fn id(&self) -> PluginId {
            PluginId("surface-plugin".to_string())
        }

        fn surfaces(&mut self) -> Vec<Box<dyn crate::surface::Surface>> {
            vec![TestSurfaceBuilder::new(SurfaceId(200)).build()]
        }

        fn workspace_request(&self) -> Option<Placement> {
            Some(Placement::SplitFocused {
                direction: SplitDirection::Horizontal,
                ratio: 0.4,
            })
        }
    }

    struct InvalidSurfacePlugin;

    impl PluginBackend for InvalidSurfacePlugin {
        fn id(&self) -> PluginId {
            PluginId("invalid-surface-plugin".to_string())
        }

        fn surfaces(&mut self) -> Vec<Box<dyn crate::surface::Surface>> {
            vec![TestSurfaceBuilder::new(SurfaceId::BUFFER).build()]
        }
    }

    fn owner_delta(old: Option<&str>, new: Option<&str>) -> AppliedWinnerDelta {
        fn descriptor(id: &str, revision: &str) -> PluginDescriptor {
            PluginDescriptor {
                id: PluginId(id.to_string()),
                source: PluginSource::Host {
                    provider: "test".to_string(),
                },
                revision: PluginRevision(revision.to_string()),
                rank: PluginRank::HOST,
            }
        }

        AppliedWinnerDelta {
            id: PluginId("surface-plugin".to_string()),
            old: old.map(|rev| descriptor("surface-plugin", rev)),
            new: new.map(|rev| descriptor("surface-plugin", rev)),
        }
    }

    #[test]
    fn rebuild_plugin_surface_registry_removes_stale_plugin_surfaces() {
        let state = AppState::default();
        let mut registry = PluginRuntime::new();
        registry.register_backend(Box::new(SurfacePlugin));

        let mut surface_registry = SurfaceRegistry::new();
        register_builtin_surfaces(&mut surface_registry);
        setup_plugin_surfaces(&mut registry, &mut surface_registry, &state);

        assert!(surface_registry.get(SurfaceId(200)).is_some());
        assert!(
            surface_registry
                .workspace()
                .root()
                .collect_ids()
                .contains(&SurfaceId(200))
        );

        assert!(registry.unload_plugin(&PluginId("surface-plugin".to_string())));
        rebuild_plugin_surface_registry(&mut registry, &mut surface_registry, &state);

        assert!(surface_registry.get(SurfaceId(200)).is_none());
        assert!(
            !surface_registry
                .workspace()
                .root()
                .collect_ids()
                .contains(&SurfaceId(200))
        );
        assert!(surface_registry.get(SurfaceId::BUFFER).is_some());
        assert!(surface_registry.get(SurfaceId::STATUS).is_some());
    }

    #[test]
    fn reconcile_plugin_surfaces_removes_stale_plugin_surfaces() {
        let state = AppState::default();
        let mut registry = PluginRuntime::new();
        registry.register_backend(Box::new(SurfacePlugin));

        let mut surface_registry = SurfaceRegistry::new();
        register_builtin_surfaces(&mut surface_registry);
        let disabled_plugins = setup_plugin_surfaces(&mut registry, &mut surface_registry, &state);
        assert!(disabled_plugins.is_empty());

        assert!(surface_registry.get(SurfaceId(200)).is_some());
        assert!(
            surface_registry
                .workspace()
                .root()
                .collect_ids()
                .contains(&SurfaceId(200))
        );

        assert!(registry.unload_plugin(&PluginId("surface-plugin".to_string())));
        let disabled_plugins = reconcile_plugin_surfaces(
            &mut registry,
            &mut surface_registry,
            &state,
            &[owner_delta(Some("r0"), None)],
        );

        assert!(disabled_plugins.is_empty());
        assert!(surface_registry.get(SurfaceId(200)).is_none());
        assert!(
            !surface_registry
                .workspace()
                .root()
                .collect_ids()
                .contains(&SurfaceId(200))
        );
    }

    #[test]
    fn reconcile_plugin_surfaces_preserves_same_id_workspace_placement() {
        let state = AppState::default();
        let mut registry = PluginRuntime::new();
        registry.register_backend(Box::new(SurfacePlugin));

        let mut surface_registry = SurfaceRegistry::new();
        register_builtin_surfaces(&mut surface_registry);
        let disabled_plugins = setup_plugin_surfaces(&mut registry, &mut surface_registry, &state);
        assert!(disabled_plugins.is_empty());
        assert_eq!(
            surface_registry
                .workspace()
                .root()
                .collect_ids()
                .into_iter()
                .filter(|surface_id| *surface_id == SurfaceId(200))
                .count(),
            1
        );

        let _ =
            registry.reload_plugin_batch(Box::new(ReplacementSurfacePlugin), &AppView::new(&state));
        let disabled_plugins = reconcile_plugin_surfaces(
            &mut registry,
            &mut surface_registry,
            &state,
            &[owner_delta(Some("r1"), Some("r2"))],
        );

        assert!(disabled_plugins.is_empty());
        assert!(surface_registry.get(SurfaceId(200)).is_some());
        assert_eq!(
            surface_registry
                .workspace()
                .root()
                .collect_ids()
                .into_iter()
                .filter(|surface_id| *surface_id == SurfaceId(200))
                .count(),
            1
        );
    }

    #[test]
    fn setup_plugin_surfaces_returns_diagnostic_for_invalid_surface_contract() {
        let state = AppState::default();
        let mut registry = PluginRuntime::new();
        registry.register_backend(Box::new(InvalidSurfacePlugin));

        let mut surface_registry = SurfaceRegistry::new();
        register_builtin_surfaces(&mut surface_registry);
        let diagnostics = setup_plugin_surfaces(&mut registry, &mut surface_registry, &state);

        assert_eq!(diagnostics.len(), 1);
        assert_eq!(
            diagnostics[0].plugin_id(),
            Some(&PluginId("invalid-surface-plugin".to_string()))
        );
        assert!(matches!(
            diagnostics[0].kind,
            crate::plugin::PluginDiagnosticKind::SurfaceRegistrationFailed {
                reason: SurfaceRegistrationError::DuplicateSurfaceId { .. }
            }
        ));
        assert!(!registry.contains_plugin(&PluginId("invalid-surface-plugin".to_string())));
    }

    #[test]
    fn reconcile_plugin_surfaces_returns_diagnostic_for_invalid_replacement() {
        let state = AppState::default();
        let mut registry = PluginRuntime::new();
        registry.register_backend(Box::new(SurfacePlugin));

        let mut surface_registry = SurfaceRegistry::new();
        register_builtin_surfaces(&mut surface_registry);
        let diagnostics = setup_plugin_surfaces(&mut registry, &mut surface_registry, &state);
        assert!(diagnostics.is_empty());

        let _ = registry.reload_plugin_batch(Box::new(InvalidSurfacePlugin), &AppView::new(&state));
        let diagnostics = reconcile_plugin_surfaces(
            &mut registry,
            &mut surface_registry,
            &state,
            &[AppliedWinnerDelta {
                id: PluginId("invalid-surface-plugin".to_string()),
                old: None,
                new: Some(PluginDescriptor {
                    id: PluginId("invalid-surface-plugin".to_string()),
                    source: PluginSource::Host {
                        provider: "test".to_string(),
                    },
                    revision: PluginRevision("r1".to_string()),
                    rank: PluginRank::HOST,
                }),
            }],
        );

        assert_eq!(diagnostics.len(), 1);
        assert_eq!(
            diagnostics[0].plugin_id(),
            Some(&PluginId("invalid-surface-plugin".to_string()))
        );
        assert!(matches!(
            diagnostics[0].kind,
            crate::plugin::PluginDiagnosticKind::SurfaceRegistrationFailed {
                reason: SurfaceRegistrationError::DuplicateSurfaceId { .. }
            }
        ));
        assert!(!registry.contains_plugin(&PluginId("invalid-surface-plugin".to_string())));
    }

    #[test]
    fn sourced_surface_commands_preserve_plugin_for_spawn_process() {
        let plugin_id = PluginId("surface-owner".to_string());
        let mut registry = PluginRuntime::new();
        registry.register_backend(Box::new(TestPlugin {
            id: plugin_id.clone(),
            allow_spawn: true,
            authorities: PluginAuthorities::empty(),
        }));

        let mut state = AppState::default();
        let mut surface_registry = SurfaceRegistry::new();
        let mut pane_map = PaneMap::new();
        let mut dirty = DirtyFlags::empty();
        let timer = NoopTimer;
        let mut sessions = NoopSessionRuntime::default();
        let mut initial_resize_sent = false;
        let mut dispatcher = RecordingDispatcher::default();
        let mut workspace_changed = false;

        let quit = handle_sourced_surface_commands(
            vec![SourcedSurfaceCommands {
                source_plugin: Some(plugin_id.clone()),
                commands: vec![Command::SpawnProcess {
                    job_id: 42,
                    program: "fd".to_string(),
                    args: vec!["foo".to_string()],
                    stdin_mode: StdinMode::Null,
                }],
            }],
            &mut DeferredContext {
                state: &mut state,
                registry: &mut registry,
                surface_registry: &mut surface_registry,
                pane_map: &mut pane_map,
                clipboard_get: &mut || None,
                dirty: &mut dirty,
                timer: &timer,
                session_host: &mut sessions,
                initial_resize_sent: &mut initial_resize_sent,
                session_ready_gate: None,
                scroll_plan_sink: &mut |_| {},
                process_dispatcher: &mut dispatcher,
                workspace_changed: &mut workspace_changed,
                scroll_amount: 3,
            },
        );

        assert!(!quit);
        assert_eq!(dispatcher.spawned.len(), 1);
        assert_eq!(dispatcher.spawned[0].0, plugin_id);
        assert_eq!(dispatcher.spawned[0].1, 42);
        assert_eq!(dispatcher.spawned[0].2, "fd");
        assert_eq!(dispatcher.spawned[0].3, vec!["foo".to_string()]);
        assert_eq!(dispatcher.spawned[0].4, StdinMode::Null);
    }

    #[test]
    fn plugin_message_runtime_effects_update_dirty_and_enqueue_scroll_plans() {
        let mut state = AppState::default();
        let mut registry = PluginRuntime::new();
        registry.register_backend(Box::new(RuntimeMessagePlugin));
        let mut surface_registry = SurfaceRegistry::new();
        let mut pane_map = PaneMap::new();
        let mut dirty = DirtyFlags::empty();
        let timer = NoopTimer;
        let mut sessions = NoopSessionRuntime::default();
        let mut initial_resize_sent = true;
        let mut dispatcher = RecordingDispatcher::default();
        let mut plans = Vec::new();
        let mut workspace_changed = false;

        let quit = handle_deferred_commands(
            vec![Command::PluginMessage {
                target: PluginId("runtime-message".to_string()),
                payload: Box::new(11u32),
            }],
            &mut DeferredContext {
                state: &mut state,
                registry: &mut registry,
                surface_registry: &mut surface_registry,
                pane_map: &mut pane_map,
                clipboard_get: &mut || None,
                dirty: &mut dirty,
                timer: &timer,
                session_host: &mut sessions,
                initial_resize_sent: &mut initial_resize_sent,
                session_ready_gate: None,
                scroll_plan_sink: &mut |plan| plans.push(plan),
                process_dispatcher: &mut dispatcher,
                workspace_changed: &mut workspace_changed,
                scroll_amount: 3,
            },
            None,
        );

        assert!(!quit);
        assert!(dirty.contains(DirtyFlags::INFO));
        assert!(dirty.contains(DirtyFlags::STATUS));
        assert_eq!(plans.len(), 1);
        assert_eq!(plans[0].total_amount, 2);
    }

    #[test]
    fn register_surface_requires_dynamic_surface_authority() {
        let plugin_id = PluginId("surface-owner".to_string());
        let mut state = AppState::default();
        let mut registry = PluginRuntime::new();
        registry.register_backend(Box::new(TestPlugin {
            id: plugin_id.clone(),
            allow_spawn: false,
            authorities: PluginAuthorities::empty(),
        }));
        let mut surface_registry = SurfaceRegistry::new();
        register_builtin_surfaces(&mut surface_registry);
        let mut pane_map = PaneMap::new();
        let mut dirty = DirtyFlags::empty();
        let timer = NoopTimer;
        let mut sessions = NoopSessionRuntime::default();
        let mut initial_resize_sent = false;
        let mut dispatcher = RecordingDispatcher::default();
        let mut workspace_changed = false;

        let quit = handle_deferred_commands(
            vec![Command::RegisterSurface {
                surface: TestSurfaceBuilder::new(SurfaceId(300))
                    .key("dynamic.surface")
                    .build(),
                placement: Placement::SplitFocused {
                    direction: SplitDirection::Vertical,
                    ratio: 0.5,
                },
            }],
            &mut DeferredContext {
                state: &mut state,
                registry: &mut registry,
                surface_registry: &mut surface_registry,
                pane_map: &mut pane_map,
                clipboard_get: &mut || None,
                dirty: &mut dirty,
                timer: &timer,
                session_host: &mut sessions,
                initial_resize_sent: &mut initial_resize_sent,
                session_ready_gate: None,
                scroll_plan_sink: &mut |_| {},
                process_dispatcher: &mut dispatcher,
                workspace_changed: &mut workspace_changed,
                scroll_amount: 3,
            },
            Some(&plugin_id),
        );

        assert!(!quit);
        assert!(surface_registry.get(SurfaceId(300)).is_none());
        assert!(!surface_registry.workspace_contains(SurfaceId(300)));
    }

    #[test]
    fn register_surface_adds_plugin_owned_surface_to_workspace() {
        let plugin_id = PluginId("surface-owner".to_string());
        let mut state = AppState::default();
        let mut registry = PluginRuntime::new();
        registry.register_backend(Box::new(TestPlugin {
            id: plugin_id.clone(),
            allow_spawn: false,
            authorities: PluginAuthorities::DYNAMIC_SURFACE,
        }));
        let mut surface_registry = SurfaceRegistry::new();
        register_builtin_surfaces(&mut surface_registry);
        let mut pane_map = PaneMap::new();
        let mut dirty = DirtyFlags::empty();
        let timer = NoopTimer;
        let mut sessions = NoopSessionRuntime::default();
        let mut initial_resize_sent = false;
        let mut dispatcher = RecordingDispatcher::default();
        let mut workspace_changed = false;

        let quit = handle_deferred_commands(
            vec![Command::RegisterSurface {
                surface: TestSurfaceBuilder::new(SurfaceId(301))
                    .key("dynamic.surface.authorized")
                    .build(),
                placement: Placement::SplitFocused {
                    direction: SplitDirection::Vertical,
                    ratio: 0.5,
                },
            }],
            &mut DeferredContext {
                state: &mut state,
                registry: &mut registry,
                surface_registry: &mut surface_registry,
                pane_map: &mut pane_map,
                clipboard_get: &mut || None,
                dirty: &mut dirty,
                timer: &timer,
                session_host: &mut sessions,
                initial_resize_sent: &mut initial_resize_sent,
                session_ready_gate: None,
                scroll_plan_sink: &mut |_| {},
                process_dispatcher: &mut dispatcher,
                workspace_changed: &mut workspace_changed,
                scroll_amount: 3,
            },
            Some(&plugin_id),
        );

        assert!(!quit);
        assert!(surface_registry.get(SurfaceId(301)).is_some());
        assert_eq!(
            surface_registry.surface_owner_plugin(SurfaceId(301)),
            Some(&plugin_id)
        );
        assert!(surface_registry.workspace_contains(SurfaceId(301)));
        assert!(dirty.contains(DirtyFlags::ALL));
    }

    #[test]
    fn register_surface_requested_resolves_keyed_placement() {
        let plugin_id = PluginId("surface-owner".to_string());
        let mut state = AppState::default();
        let mut registry = PluginRuntime::new();
        registry.register_backend(Box::new(TestPlugin {
            id: plugin_id.clone(),
            allow_spawn: false,
            authorities: PluginAuthorities::DYNAMIC_SURFACE,
        }));
        let mut surface_registry = SurfaceRegistry::new();
        register_builtin_surfaces(&mut surface_registry);
        let mut pane_map = PaneMap::new();
        let mut dirty = DirtyFlags::empty();
        let timer = NoopTimer;
        let mut sessions = NoopSessionRuntime::default();
        let mut initial_resize_sent = false;
        let mut dispatcher = RecordingDispatcher::default();
        let mut workspace_changed = false;

        let quit = handle_deferred_commands(
            vec![Command::RegisterSurfaceRequested {
                surface: TestSurfaceBuilder::new(SurfaceId(304))
                    .key("dynamic.surface.requested")
                    .build(),
                placement: SurfacePlacementRequest::TabIn {
                    target_surface_key: "kasane.buffer".into(),
                },
            }],
            &mut DeferredContext {
                state: &mut state,
                registry: &mut registry,
                surface_registry: &mut surface_registry,
                pane_map: &mut pane_map,
                clipboard_get: &mut || None,
                dirty: &mut dirty,
                timer: &timer,
                session_host: &mut sessions,
                initial_resize_sent: &mut initial_resize_sent,
                session_ready_gate: None,
                scroll_plan_sink: &mut |_| {},
                process_dispatcher: &mut dispatcher,
                workspace_changed: &mut workspace_changed,
                scroll_amount: 3,
            },
            Some(&plugin_id),
        );

        assert!(!quit);
        assert!(surface_registry.get(SurfaceId(304)).is_some());
        assert_eq!(
            surface_registry.surface_owner_plugin(SurfaceId(304)),
            Some(&plugin_id)
        );
        assert!(surface_registry.workspace_contains(SurfaceId(304)));
        assert!(dirty.contains(DirtyFlags::ALL));
    }

    #[test]
    fn unregister_surface_rejects_non_owner_even_with_authority() {
        let owner_id = PluginId("surface-owner".to_string());
        let other_id = PluginId("other-owner".to_string());
        let mut state = AppState::default();
        let mut registry = PluginRuntime::new();
        registry.register_backend(Box::new(TestPlugin {
            id: owner_id.clone(),
            allow_spawn: false,
            authorities: PluginAuthorities::DYNAMIC_SURFACE,
        }));
        registry.register_backend(Box::new(TestPlugin {
            id: other_id.clone(),
            allow_spawn: false,
            authorities: PluginAuthorities::DYNAMIC_SURFACE,
        }));
        let mut surface_registry = SurfaceRegistry::new();
        register_builtin_surfaces(&mut surface_registry);
        surface_registry
            .try_register_for_owner(
                TestSurfaceBuilder::new(SurfaceId(302))
                    .key("dynamic.surface.owned")
                    .build(),
                Some(owner_id.clone()),
            )
            .unwrap();
        let mut bootstrap_dirty = DirtyFlags::empty();
        crate::workspace::dispatch_workspace_command_with_total(
            &mut surface_registry,
            crate::workspace::WorkspaceCommand::AddSurface {
                surface_id: SurfaceId(302),
                placement: Placement::SplitFocused {
                    direction: SplitDirection::Vertical,
                    ratio: 0.5,
                },
            },
            &mut bootstrap_dirty,
            Some(crate::layout::Rect {
                x: 0,
                y: 0,
                w: state.cols,
                h: state.rows,
            }),
        );

        let mut pane_map = PaneMap::new();
        let mut dirty = DirtyFlags::empty();
        let timer = NoopTimer;
        let mut sessions = NoopSessionRuntime::default();
        let mut initial_resize_sent = false;
        let mut dispatcher = RecordingDispatcher::default();
        let mut workspace_changed = false;

        let quit = handle_deferred_commands(
            vec![Command::UnregisterSurface {
                surface_id: SurfaceId(302),
            }],
            &mut DeferredContext {
                state: &mut state,
                registry: &mut registry,
                surface_registry: &mut surface_registry,
                pane_map: &mut pane_map,
                clipboard_get: &mut || None,
                dirty: &mut dirty,
                timer: &timer,
                session_host: &mut sessions,
                initial_resize_sent: &mut initial_resize_sent,
                session_ready_gate: None,
                scroll_plan_sink: &mut |_| {},
                process_dispatcher: &mut dispatcher,
                workspace_changed: &mut workspace_changed,
                scroll_amount: 3,
            },
            Some(&other_id),
        );

        assert!(!quit);
        assert!(surface_registry.get(SurfaceId(302)).is_some());
        assert!(surface_registry.workspace_contains(SurfaceId(302)));
    }

    #[test]
    fn unregister_surface_removes_owned_surface() {
        let plugin_id = PluginId("surface-owner".to_string());
        let mut state = AppState::default();
        let mut registry = PluginRuntime::new();
        registry.register_backend(Box::new(TestPlugin {
            id: plugin_id.clone(),
            allow_spawn: false,
            authorities: PluginAuthorities::DYNAMIC_SURFACE,
        }));
        let mut surface_registry = SurfaceRegistry::new();
        register_builtin_surfaces(&mut surface_registry);
        surface_registry
            .try_register_for_owner(
                TestSurfaceBuilder::new(SurfaceId(303))
                    .key("dynamic.surface.remove")
                    .build(),
                Some(plugin_id.clone()),
            )
            .unwrap();
        let mut bootstrap_dirty = DirtyFlags::empty();
        crate::workspace::dispatch_workspace_command_with_total(
            &mut surface_registry,
            crate::workspace::WorkspaceCommand::AddSurface {
                surface_id: SurfaceId(303),
                placement: Placement::SplitFocused {
                    direction: SplitDirection::Vertical,
                    ratio: 0.5,
                },
            },
            &mut bootstrap_dirty,
            Some(crate::layout::Rect {
                x: 0,
                y: 0,
                w: state.cols,
                h: state.rows,
            }),
        );

        let mut pane_map = PaneMap::new();
        let mut dirty = DirtyFlags::empty();
        let timer = NoopTimer;
        let mut sessions = NoopSessionRuntime::default();
        let mut initial_resize_sent = false;
        let mut dispatcher = RecordingDispatcher::default();
        let mut workspace_changed = false;

        let quit = handle_deferred_commands(
            vec![Command::UnregisterSurface {
                surface_id: SurfaceId(303),
            }],
            &mut DeferredContext {
                state: &mut state,
                registry: &mut registry,
                surface_registry: &mut surface_registry,
                pane_map: &mut pane_map,
                clipboard_get: &mut || None,
                dirty: &mut dirty,
                timer: &timer,
                session_host: &mut sessions,
                initial_resize_sent: &mut initial_resize_sent,
                session_ready_gate: None,
                scroll_plan_sink: &mut |_| {},
                process_dispatcher: &mut dispatcher,
                workspace_changed: &mut workspace_changed,
                scroll_amount: 3,
            },
            Some(&plugin_id),
        );

        assert!(!quit);
        assert!(surface_registry.get(SurfaceId(303)).is_none());
        assert!(!surface_registry.workspace_contains(SurfaceId(303)));
        assert!(dirty.contains(DirtyFlags::ALL));
    }

    #[test]
    fn unregister_surface_key_removes_owned_surface() {
        let plugin_id = PluginId("surface-owner".to_string());
        let mut state = AppState::default();
        let mut registry = PluginRuntime::new();
        registry.register_backend(Box::new(TestPlugin {
            id: plugin_id.clone(),
            allow_spawn: false,
            authorities: PluginAuthorities::DYNAMIC_SURFACE,
        }));
        let mut surface_registry = SurfaceRegistry::new();
        register_builtin_surfaces(&mut surface_registry);
        surface_registry
            .try_register_for_owner(
                TestSurfaceBuilder::new(SurfaceId(305))
                    .key("dynamic.surface.remove.by.key")
                    .build(),
                Some(plugin_id.clone()),
            )
            .unwrap();
        let mut bootstrap_dirty = DirtyFlags::empty();
        crate::workspace::dispatch_workspace_command_with_total(
            &mut surface_registry,
            crate::workspace::WorkspaceCommand::AddSurface {
                surface_id: SurfaceId(305),
                placement: Placement::SplitFocused {
                    direction: SplitDirection::Vertical,
                    ratio: 0.5,
                },
            },
            &mut bootstrap_dirty,
            Some(crate::layout::Rect {
                x: 0,
                y: 0,
                w: state.cols,
                h: state.rows,
            }),
        );

        let mut pane_map = PaneMap::new();
        let mut dirty = DirtyFlags::empty();
        let timer = NoopTimer;
        let mut sessions = NoopSessionRuntime::default();
        let mut initial_resize_sent = false;
        let mut dispatcher = RecordingDispatcher::default();
        let mut workspace_changed = false;

        let quit = handle_deferred_commands(
            vec![Command::UnregisterSurfaceKey {
                surface_key: "dynamic.surface.remove.by.key".into(),
            }],
            &mut DeferredContext {
                state: &mut state,
                registry: &mut registry,
                surface_registry: &mut surface_registry,
                pane_map: &mut pane_map,
                clipboard_get: &mut || None,
                dirty: &mut dirty,
                timer: &timer,
                session_host: &mut sessions,
                initial_resize_sent: &mut initial_resize_sent,
                session_ready_gate: None,
                scroll_plan_sink: &mut |_| {},
                process_dispatcher: &mut dispatcher,
                workspace_changed: &mut workspace_changed,
                scroll_amount: 3,
            },
            Some(&plugin_id),
        );

        assert!(!quit);
        assert!(surface_registry.get(SurfaceId(305)).is_none());
        assert!(!surface_registry.workspace_contains(SurfaceId(305)));
        assert!(dirty.contains(DirtyFlags::ALL));
    }

    #[derive(Default)]
    struct RecordingSessionHost {
        writer: Vec<u8>,
        spawned: Vec<(SessionSpec, bool)>,
        closed: Vec<Option<String>>,
        switched: Vec<String>,
        close_returns_quit: bool,
    }

    impl SessionRuntime for RecordingSessionHost {
        fn spawn_session(
            &mut self,
            spec: SessionSpec,
            activate: bool,
            _state: &mut AppState,
            _dirty: &mut DirtyFlags,
            _initial_resize_sent: &mut bool,
        ) {
            self.spawned.push((spec, activate));
        }

        fn close_session(
            &mut self,
            key: Option<&str>,
            _state: &mut AppState,
            _dirty: &mut DirtyFlags,
            _initial_resize_sent: &mut bool,
        ) -> bool {
            self.closed.push(key.map(ToOwned::to_owned));
            self.close_returns_quit
        }

        fn switch_session(
            &mut self,
            key: &str,
            _state: &mut AppState,
            _dirty: &mut DirtyFlags,
            _initial_resize_sent: &mut bool,
        ) {
            self.switched.push(key.to_owned());
        }
    }

    impl SessionHost for RecordingSessionHost {
        fn active_writer(&mut self) -> &mut dyn Write {
            &mut self.writer
        }
    }

    #[test]
    fn deferred_session_spawn_is_routed_to_session_host() {
        let mut state = AppState::default();
        let mut registry = PluginRuntime::new();
        let mut surface_registry = SurfaceRegistry::new();
        let mut pane_map = PaneMap::new();
        let mut dirty = DirtyFlags::empty();
        let timer = NoopTimer;
        let mut sessions = RecordingSessionHost::default();
        let mut initial_resize_sent = true;
        let mut dispatcher = RecordingDispatcher::default();
        let mut workspace_changed = false;

        let quit = handle_deferred_commands(
            vec![Command::Session(crate::session::SessionCommand::Spawn {
                key: Some("work".to_string()),
                session: Some("project".to_string()),
                args: vec!["file.txt".to_string()],
                activate: true,
            })],
            &mut DeferredContext {
                state: &mut state,
                registry: &mut registry,
                surface_registry: &mut surface_registry,
                pane_map: &mut pane_map,
                clipboard_get: &mut || None,
                dirty: &mut dirty,
                timer: &timer,
                session_host: &mut sessions,
                initial_resize_sent: &mut initial_resize_sent,
                session_ready_gate: None,
                scroll_plan_sink: &mut |_| {},
                process_dispatcher: &mut dispatcher,
                workspace_changed: &mut workspace_changed,
                scroll_amount: 3,
            },
            None,
        );

        assert!(!quit);
        assert_eq!(sessions.spawned.len(), 1);
        assert_eq!(sessions.spawned[0].0.key, "work");
        assert_eq!(sessions.spawned[0].0.session.as_deref(), Some("project"));
        assert_eq!(sessions.spawned[0].0.args, vec!["file.txt".to_string()]);
        assert!(sessions.spawned[0].1);
    }

    #[test]
    fn deferred_session_close_is_routed_to_session_host() {
        let mut state = AppState::default();
        let mut registry = PluginRuntime::new();
        let mut surface_registry = SurfaceRegistry::new();
        let mut pane_map = PaneMap::new();
        let mut dirty = DirtyFlags::empty();
        let timer = NoopTimer;
        let mut sessions = RecordingSessionHost::default();
        let mut initial_resize_sent = false;
        let mut dispatcher = RecordingDispatcher::default();
        let mut workspace_changed = false;

        let quit = handle_deferred_commands(
            vec![Command::Session(crate::session::SessionCommand::Close {
                key: Some("work".to_string()),
            })],
            &mut DeferredContext {
                state: &mut state,
                registry: &mut registry,
                surface_registry: &mut surface_registry,
                pane_map: &mut pane_map,
                clipboard_get: &mut || None,
                dirty: &mut dirty,
                timer: &timer,
                session_host: &mut sessions,
                initial_resize_sent: &mut initial_resize_sent,
                session_ready_gate: None,
                scroll_plan_sink: &mut |_| {},
                process_dispatcher: &mut dispatcher,
                workspace_changed: &mut workspace_changed,
                scroll_amount: 3,
            },
            None,
        );

        assert!(!quit);
        assert_eq!(sessions.closed, vec![Some("work".to_string())]);
    }

    #[test]
    fn deferred_session_close_returns_quit_when_host_requests_shutdown() {
        let mut state = AppState::default();
        let mut registry = PluginRuntime::new();
        let mut surface_registry = SurfaceRegistry::new();
        let mut pane_map = PaneMap::new();
        let mut dirty = DirtyFlags::empty();
        let timer = NoopTimer;
        let mut sessions = RecordingSessionHost {
            close_returns_quit: true,
            ..RecordingSessionHost::default()
        };
        let mut initial_resize_sent = false;
        let mut dispatcher = RecordingDispatcher::default();
        let mut workspace_changed = false;

        let quit = handle_deferred_commands(
            vec![Command::Session(crate::session::SessionCommand::Close {
                key: None,
            })],
            &mut DeferredContext {
                state: &mut state,
                registry: &mut registry,
                surface_registry: &mut surface_registry,
                pane_map: &mut pane_map,
                clipboard_get: &mut || None,
                dirty: &mut dirty,
                timer: &timer,
                session_host: &mut sessions,
                initial_resize_sent: &mut initial_resize_sent,
                session_ready_gate: None,
                scroll_plan_sink: &mut |_| {},
                process_dispatcher: &mut dispatcher,
                workspace_changed: &mut workspace_changed,
                scroll_amount: 3,
            },
            None,
        );

        assert!(quit);
        assert_eq!(sessions.closed, vec![None]);
    }

    #[test]
    fn deferred_session_switch_is_routed() {
        let mut state = AppState::default();
        let mut registry = PluginRuntime::new();
        let mut surface_registry = SurfaceRegistry::new();
        let mut pane_map = PaneMap::new();
        let mut dirty = DirtyFlags::empty();
        let timer = NoopTimer;
        let mut sessions = RecordingSessionHost::default();
        let mut initial_resize_sent = false;
        let mut dispatcher = RecordingDispatcher::default();
        let mut workspace_changed = false;

        let quit = handle_deferred_commands(
            vec![Command::Session(crate::session::SessionCommand::Switch {
                key: "work".to_string(),
            })],
            &mut DeferredContext {
                state: &mut state,
                registry: &mut registry,
                surface_registry: &mut surface_registry,
                pane_map: &mut pane_map,
                clipboard_get: &mut || None,
                dirty: &mut dirty,
                timer: &timer,
                session_host: &mut sessions,
                initial_resize_sent: &mut initial_resize_sent,
                session_ready_gate: None,
                scroll_plan_sink: &mut |_| {},
                process_dispatcher: &mut dispatcher,
                workspace_changed: &mut workspace_changed,
                scroll_amount: 3,
            },
            None,
        );

        assert!(!quit);
        assert_eq!(sessions.switched, vec!["work".to_string()]);
    }

    #[test]
    fn session_ready_gate_requires_active_session_and_initial_resize() {
        let mut gate = SessionReadyGate::default();
        assert!(!gate.should_notify_ready());

        gate.sync_active_session(Some("alpha"));
        assert!(!gate.should_notify_ready());

        gate.mark_initial_resize_sent();
        assert!(gate.should_notify_ready());
        gate.mark_ready_notified();
        assert!(!gate.should_notify_ready());
    }

    #[test]
    fn session_ready_gate_rearms_on_session_switch() {
        let mut gate = SessionReadyGate::default();
        gate.sync_active_session(Some("alpha"));
        gate.mark_initial_resize_sent();
        gate.mark_ready_notified();
        assert!(!gate.should_notify_ready());

        gate.sync_active_session(Some("beta"));
        assert!(!gate.should_notify_ready());
        gate.mark_initial_resize_sent();
        assert!(gate.should_notify_ready());
    }

    #[test]
    fn session_ready_gate_can_rearm_current_generation() {
        let mut gate = SessionReadyGate::default();
        gate.sync_active_session(Some("alpha"));
        gate.mark_initial_resize_sent();
        gate.mark_ready_notified();
        assert!(!gate.should_notify_ready());

        gate.rearm_ready_notification();
        assert!(gate.should_notify_ready());
    }

    #[test]
    fn pty_spawn_requires_pty_process_authority() {
        let plugin_id = PluginId("pty-plugin".to_string());
        let mut state = AppState::default();
        let mut registry = PluginRuntime::new();
        registry.register_backend(Box::new(TestPlugin {
            id: plugin_id.clone(),
            allow_spawn: true,
            authorities: PluginAuthorities::empty(), // no PTY_PROCESS authority
        }));
        let mut surface_registry = SurfaceRegistry::new();
        register_builtin_surfaces(&mut surface_registry);
        let mut pane_map = PaneMap::new();
        let mut dirty = DirtyFlags::empty();
        let timer = NoopTimer;
        let mut sessions = NoopSessionRuntime::default();
        let mut initial_resize_sent = true;
        let mut dispatcher = RecordingDispatcher::default();
        let mut workspace_changed = false;

        let quit = handle_deferred_commands(
            vec![Command::SpawnProcess {
                job_id: 1,
                program: "bash".to_string(),
                args: vec![],
                stdin_mode: StdinMode::Pty { rows: 24, cols: 80 },
            }],
            &mut DeferredContext {
                state: &mut state,
                registry: &mut registry,
                surface_registry: &mut surface_registry,
                pane_map: &mut pane_map,
                clipboard_get: &mut || None,
                dirty: &mut dirty,
                timer: &timer,
                session_host: &mut sessions,
                initial_resize_sent: &mut initial_resize_sent,
                session_ready_gate: None,
                scroll_plan_sink: &mut |_| {},
                process_dispatcher: &mut dispatcher,
                workspace_changed: &mut workspace_changed,
                scroll_amount: 3,
            },
            Some(&plugin_id),
        );

        assert!(!quit);
        // PTY spawn should be rejected — dispatcher should not receive the spawn
        assert!(dispatcher.spawned.is_empty());
    }

    #[test]
    fn pty_spawn_allowed_with_authority() {
        let plugin_id = PluginId("pty-plugin".to_string());
        let mut state = AppState::default();
        let mut registry = PluginRuntime::new();
        registry.register_backend(Box::new(TestPlugin {
            id: plugin_id.clone(),
            allow_spawn: true,
            authorities: PluginAuthorities::PTY_PROCESS,
        }));
        let mut surface_registry = SurfaceRegistry::new();
        register_builtin_surfaces(&mut surface_registry);
        let mut pane_map = PaneMap::new();
        let mut dirty = DirtyFlags::empty();
        let timer = NoopTimer;
        let mut sessions = NoopSessionRuntime::default();
        let mut initial_resize_sent = true;
        let mut dispatcher = RecordingDispatcher::default();
        let mut workspace_changed = false;

        let quit = handle_deferred_commands(
            vec![Command::SpawnProcess {
                job_id: 1,
                program: "bash".to_string(),
                args: vec![],
                stdin_mode: StdinMode::Pty { rows: 24, cols: 80 },
            }],
            &mut DeferredContext {
                state: &mut state,
                registry: &mut registry,
                surface_registry: &mut surface_registry,
                pane_map: &mut pane_map,
                clipboard_get: &mut || None,
                dirty: &mut dirty,
                timer: &timer,
                session_host: &mut sessions,
                initial_resize_sent: &mut initial_resize_sent,
                session_ready_gate: None,
                scroll_plan_sink: &mut |_| {},
                process_dispatcher: &mut dispatcher,
                workspace_changed: &mut workspace_changed,
                scroll_amount: 3,
            },
            Some(&plugin_id),
        );

        assert!(!quit);
        assert_eq!(dispatcher.spawned.len(), 1);
        assert_eq!(
            dispatcher.spawned[0].4,
            StdinMode::Pty { rows: 24, cols: 80 }
        );
    }

    #[test]
    fn piped_spawn_does_not_require_pty_authority() {
        let plugin_id = PluginId("piped-plugin".to_string());
        let mut state = AppState::default();
        let mut registry = PluginRuntime::new();
        registry.register_backend(Box::new(TestPlugin {
            id: plugin_id.clone(),
            allow_spawn: true,
            authorities: PluginAuthorities::empty(), // no PTY_PROCESS authority
        }));
        let mut surface_registry = SurfaceRegistry::new();
        register_builtin_surfaces(&mut surface_registry);
        let mut pane_map = PaneMap::new();
        let mut dirty = DirtyFlags::empty();
        let timer = NoopTimer;
        let mut sessions = NoopSessionRuntime::default();
        let mut initial_resize_sent = true;
        let mut dispatcher = RecordingDispatcher::default();
        let mut workspace_changed = false;

        let quit = handle_deferred_commands(
            vec![Command::SpawnProcess {
                job_id: 1,
                program: "echo".to_string(),
                args: vec!["test".to_string()],
                stdin_mode: StdinMode::Piped,
            }],
            &mut DeferredContext {
                state: &mut state,
                registry: &mut registry,
                surface_registry: &mut surface_registry,
                pane_map: &mut pane_map,
                clipboard_get: &mut || None,
                dirty: &mut dirty,
                timer: &timer,
                session_host: &mut sessions,
                initial_resize_sent: &mut initial_resize_sent,
                session_ready_gate: None,
                scroll_plan_sink: &mut |_| {},
                process_dispatcher: &mut dispatcher,
                workspace_changed: &mut workspace_changed,
                scroll_amount: 3,
            },
            Some(&plugin_id),
        );

        assert!(!quit);
        // Piped spawn should succeed without PTY_PROCESS authority
        assert_eq!(dispatcher.spawned.len(), 1);
        assert_eq!(dispatcher.spawned[0].4, StdinMode::Piped);
    }

    #[test]
    fn inject_input_dispatches_through_update() {
        use crate::input::{InputEvent, Key, KeyEvent, Modifiers};

        let mut state = AppState::default();
        let mut registry = PluginRuntime::new();
        let mut surface_registry = SurfaceRegistry::new();
        let mut pane_map = PaneMap::default();
        let mut dirty = DirtyFlags::empty();
        let timer = NoopTimer;
        let mut sessions = NoopSessionRuntime::default();
        let mut initial_resize_sent = false;
        let mut dispatcher = RecordingDispatcher::default();
        let mut workspace_changed = false;
        let mut scroll_plans = Vec::new();

        let quit = handle_deferred_commands(
            vec![Command::InjectInput(InputEvent::Key(KeyEvent {
                key: Key::Char('a'),
                modifiers: Modifiers::empty(),
            }))],
            &mut DeferredContext {
                state: &mut state,
                registry: &mut registry,
                surface_registry: &mut surface_registry,
                pane_map: &mut pane_map,
                clipboard_get: &mut || None,
                dirty: &mut dirty,
                timer: &timer,
                session_host: &mut sessions,
                initial_resize_sent: &mut initial_resize_sent,
                session_ready_gate: None,
                scroll_plan_sink: &mut |plan| scroll_plans.push(plan),
                process_dispatcher: &mut dispatcher,
                workspace_changed: &mut workspace_changed,
                scroll_amount: 3,
            },
            None,
        );

        // The injected key should have been processed through update()
        // which sends it to Kakoune via SendToKakoune (immediate command)
        assert!(!quit);
    }

    #[test]
    fn inject_input_respects_depth_limit() {
        use crate::input::{InputEvent, Key, KeyEvent, Modifiers};

        let mut state = AppState::default();
        let mut registry = PluginRuntime::new();
        let mut surface_registry = SurfaceRegistry::new();
        let mut pane_map = PaneMap::default();
        let mut dirty = DirtyFlags::empty();
        let timer = NoopTimer;
        let mut sessions = NoopSessionRuntime::default();
        let mut initial_resize_sent = false;
        let mut dispatcher = RecordingDispatcher::default();
        let mut workspace_changed = false;

        // Call at MAX depth — should be dropped
        let quit = handle_deferred_commands_inner(
            vec![Command::InjectInput(InputEvent::Key(KeyEvent {
                key: Key::Char('x'),
                modifiers: Modifiers::empty(),
            }))],
            &mut DeferredContext {
                state: &mut state,
                registry: &mut registry,
                surface_registry: &mut surface_registry,
                pane_map: &mut pane_map,
                clipboard_get: &mut || None,
                dirty: &mut dirty,
                timer: &timer,
                session_host: &mut sessions,
                initial_resize_sent: &mut initial_resize_sent,
                session_ready_gate: None,
                scroll_plan_sink: &mut |_| {},
                process_dispatcher: &mut dispatcher,
                workspace_changed: &mut workspace_changed,
                scroll_amount: 3,
            },
            None,
            MAX_INJECT_DEPTH, // at limit — should be dropped
        );

        assert!(!quit);
    }

    /// Plugin that responds to every PluginMessage by sending another
    /// PluginMessage to itself, creating an infinite cascade.
    struct CascadingMessagePlugin;

    impl PluginBackend for CascadingMessagePlugin {
        fn id(&self) -> PluginId {
            PluginId("cascading".to_string())
        }

        fn update_effects(&mut self, _msg: &mut dyn Any, _state: &AppView<'_>) -> RuntimeEffects {
            RuntimeEffects {
                redraw: DirtyFlags::empty(),
                commands: vec![Command::PluginMessage {
                    target: PluginId("cascading".to_string()),
                    payload: Box::new(()),
                }],
                scroll_plans: vec![],
            }
        }
    }

    #[test]
    fn command_cascade_terminates_at_depth_limit() {
        let mut state = AppState::default();
        let mut registry = PluginRuntime::new();
        registry.register_backend(Box::new(CascadingMessagePlugin));
        let mut surface_registry = SurfaceRegistry::new();
        let mut pane_map = PaneMap::default();
        let mut dirty = DirtyFlags::empty();
        let timer = NoopTimer;
        let mut sessions = NoopSessionRuntime::default();
        let mut initial_resize_sent = false;
        let mut dispatcher = RecordingDispatcher::default();
        let mut workspace_changed = false;

        // Seed a single PluginMessage — should cascade but terminate
        let quit = handle_deferred_commands(
            vec![Command::PluginMessage {
                target: PluginId("cascading".to_string()),
                payload: Box::new(()),
            }],
            &mut DeferredContext {
                state: &mut state,
                registry: &mut registry,
                surface_registry: &mut surface_registry,
                pane_map: &mut pane_map,
                clipboard_get: &mut || None,
                dirty: &mut dirty,
                timer: &timer,
                session_host: &mut sessions,
                initial_resize_sent: &mut initial_resize_sent,
                session_ready_gate: None,
                scroll_plan_sink: &mut |_| {},
                process_dispatcher: &mut dispatcher,
                workspace_changed: &mut workspace_changed,
                scroll_amount: 3,
            },
            None,
        );

        assert!(!quit);
        // The cascade should have been cut off at MAX_COMMAND_CASCADE_DEPTH.
        // The test's primary assertion is that it terminates without panic/hang.
    }

    /// Plugin that handles every key by injecting another key, creating
    /// an infinite injection cascade.
    struct CascadingInjectPlugin;

    impl PluginBackend for CascadingInjectPlugin {
        fn id(&self) -> PluginId {
            PluginId("cascading-inject".to_string())
        }

        fn capabilities(&self) -> crate::plugin::PluginCapabilities {
            crate::plugin::PluginCapabilities::INPUT_HANDLER
        }

        fn handle_key(
            &mut self,
            _key: &crate::input::KeyEvent,
            _state: &AppView<'_>,
        ) -> Option<Vec<Command>> {
            Some(vec![Command::InjectInput(crate::input::InputEvent::Key(
                crate::input::KeyEvent {
                    key: crate::input::Key::Char('z'),
                    modifiers: crate::input::Modifiers::empty(),
                },
            ))])
        }
    }

    #[test]
    fn inject_cascade_terminates_at_depth_limit() {
        let mut state = AppState::default();
        let mut registry = PluginRuntime::new();
        registry.register_backend(Box::new(CascadingInjectPlugin));
        let mut surface_registry = SurfaceRegistry::new();
        let mut pane_map = PaneMap::default();
        let mut dirty = DirtyFlags::empty();
        let timer = NoopTimer;
        let mut sessions = NoopSessionRuntime::default();
        let mut initial_resize_sent = false;
        let mut dispatcher = RecordingDispatcher::default();
        let mut workspace_changed = false;

        // Inject a key — the plugin will re-inject on every handle_key,
        // but the depth limit should cut it off.
        let quit = handle_deferred_commands(
            vec![Command::InjectInput(crate::input::InputEvent::Key(
                crate::input::KeyEvent {
                    key: crate::input::Key::Char('a'),
                    modifiers: crate::input::Modifiers::empty(),
                },
            ))],
            &mut DeferredContext {
                state: &mut state,
                registry: &mut registry,
                surface_registry: &mut surface_registry,
                pane_map: &mut pane_map,
                clipboard_get: &mut || None,
                dirty: &mut dirty,
                timer: &timer,
                session_host: &mut sessions,
                initial_resize_sent: &mut initial_resize_sent,
                session_ready_gate: None,
                scroll_plan_sink: &mut |_| {},
                process_dispatcher: &mut dispatcher,
                workspace_changed: &mut workspace_changed,
                scroll_amount: 3,
            },
            None,
        );

        assert!(!quit);
    }
}
