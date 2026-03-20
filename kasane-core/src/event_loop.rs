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
    AppliedWinnerDelta, BootstrapEffects, Command, CommandResult, DeferredCommand, IoEvent,
    PluginId, PluginRegistry, ProcessDispatcher, ProcessEvent, ReadyBatch, RuntimeBatch,
    RuntimeEffects, SessionReadyCommand, execute_commands, extract_deferred_commands,
    extract_redraw_flags,
};
use crate::scroll::ScrollPlan;
use crate::session::{SessionId, SessionManager, SessionSpec, SessionStateStore};
use crate::state::{AppState, DirtyFlags};
use crate::surface::buffer::KakouneBufferSurface;
use crate::surface::{SourcedSurfaceCommands, SurfaceEvent, SurfaceRegistry};
use crate::workspace::Workspace;

/// Structured result from processing a single event.
pub struct EventResult {
    pub flags: DirtyFlags,
    pub commands: Vec<Command>,
    pub scroll_plans: Vec<ScrollPlan>,
    pub surface_commands: Vec<SourcedSurfaceCommands>,
    pub command_source: Option<PluginId>,
}

impl EventResult {
    pub fn empty() -> Self {
        Self {
            flags: DirtyFlags::empty(),
            commands: vec![],
            scroll_plans: vec![],
            surface_commands: vec![],
            command_source: None,
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
    registry: &mut PluginRegistry,
    surface_registry: &mut SurfaceRegistry,
    state: &AppState,
) -> Vec<PluginId> {
    let mut disabled_plugins = Vec::new();
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
            disabled_plugins.push(surface_set.owner.clone());
            eprintln!(
                "disabling plugin {} after surface registration failure: {err:?}",
                surface_set.owner.0
            );
        } else {
            let mut bootstrap_dirty = DirtyFlags::empty();
            for (surface_id, request) in surface_registry.apply_initial_placements_with_total(
                &registered_ids,
                surface_set.legacy_workspace_request.as_ref(),
                &mut bootstrap_dirty,
                Some(Rect {
                    x: 0,
                    y: 0,
                    w: state.cols,
                    h: state.rows,
                }),
            ) {
                eprintln!(
                    "skipping unresolved initial placement for surface {surface_id:?}: {request:?}"
                );
            }
        }
    }
    disabled_plugins
}

pub fn register_builtin_surfaces(surface_registry: &mut SurfaceRegistry) {
    surface_registry
        .try_register(Box::new(KakouneBufferSurface::new()))
        .expect("failed to register built-in surface kasane.buffer");
    surface_registry
        .try_register(Box::new(crate::surface::status::StatusBarSurface::new()))
        .expect("failed to register built-in surface kasane.status");
}

pub fn rebuild_plugin_surface_registry(
    registry: &mut PluginRegistry,
    surface_registry: &mut SurfaceRegistry,
    state: &AppState,
) -> Vec<PluginId> {
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
    registry: &mut PluginRegistry,
    surface_registry: &mut SurfaceRegistry,
    state: &AppState,
    deltas: &[AppliedWinnerDelta],
) -> Vec<PluginId> {
    if deltas.is_empty() {
        return vec![];
    }

    for delta in deltas {
        if delta.old.is_some() {
            surface_registry.remove_owned_surfaces(&delta.id);
        }
    }

    let mut disabled_plugins = Vec::new();
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
            disabled_plugins.push(surface_set.owner.clone());
            eprintln!(
                "disabling plugin {} after surface registration failure: {err:?}",
                surface_set.owner.0
            );
            continue;
        }

        if !new_workspace_surfaces.is_empty() {
            let mut bootstrap_dirty = DirtyFlags::empty();
            for (surface_id, request) in surface_registry.apply_initial_placements_with_total(
                &new_workspace_surfaces,
                surface_set.legacy_workspace_request.as_ref(),
                &mut bootstrap_dirty,
                Some(Rect {
                    x: 0,
                    y: 0,
                    w: state.cols,
                    h: state.rows,
                }),
            ) {
                eprintln!(
                    "skipping unresolved initial placement for surface {surface_id:?}: {request:?}"
                );
            }
        }
    }

    prune_missing_workspace_surfaces(surface_registry);
    disabled_plugins
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

/// Synchronize session metadata from SessionManager into AppState.
pub fn sync_session_metadata<R, W, C>(
    session_manager: &SessionManager<R, W, C>,
    session_states: &SessionStateStore,
    state: &mut AppState,
) {
    state.session_descriptors = session_manager.enriched_session_descriptors(session_states, state);
    state.active_session_key = session_manager.active_session_key().map(str::to_owned);
}

/// Handle a Kakoune session death event.
///
/// Closes the session, removes its state, and restores the next active session
/// if needed. Returns `true` if the application should quit (no sessions remain).
pub fn handle_session_death<R, W, C>(
    session_id: SessionId,
    session_manager: &mut SessionManager<R, W, C>,
    session_states: &mut SessionStateStore,
    state: &mut AppState,
    dirty: &mut DirtyFlags,
    initial_resize_sent: &mut bool,
) -> bool {
    let was_active = session_manager.active_session_id() == Some(session_id);
    let _ = session_manager.close(session_id);
    *dirty |= DirtyFlags::SESSION;
    session_states.remove(session_id);
    if session_manager.is_empty() {
        return true;
    }
    if was_active {
        let restored = session_manager
            .active_session_id()
            .is_some_and(|active| session_states.restore_into(active, state));
        if !restored {
            state.reset_for_session_switch();
        }
        *dirty |= DirtyFlags::ALL;
        *initial_resize_sent = false;
    }
    sync_session_metadata(session_manager, session_states, state);
    false
}

/// Spawn a new managed session, returning the session ID and reader on success.
///
/// The reader is returned so the backend can wire it up to its specific event
/// channel. The activation logic (state restore, dirty flags) is handled here.
#[allow(clippy::too_many_arguments)]
pub fn spawn_session_core<R, W, C>(
    spec: &SessionSpec,
    activate: bool,
    session_manager: &mut SessionManager<R, W, C>,
    session_states: &mut SessionStateStore,
    state: &mut AppState,
    dirty: &mut DirtyFlags,
    initial_resize_sent: &mut bool,
    spawn_fn: fn(&SessionSpec) -> anyhow::Result<(R, W, C)>,
) -> Option<(SessionId, R)> {
    // Deduplicate the session key before spawning the process to avoid
    // orphaning a Kakoune process when insert() rejects a duplicate key.
    let spec = if session_manager.session_id_by_key(&spec.key).is_some() {
        let base = &spec.key;
        let mut deduped = None;
        for i in 2..=100 {
            let candidate = format!("{base}-{i}");
            if session_manager.session_id_by_key(&candidate).is_none() {
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
    let Ok(session_id) = session_manager.insert(spec, reader, writer, child) else {
        tracing::error!("failed to register spawned session");
        return None;
    };
    session_states.ensure_session(session_id, state);
    *dirty |= DirtyFlags::SESSION;
    let reader = session_manager
        .take_reader(session_id)
        .expect("spawned session reader missing");
    if activate {
        session_manager
            .sync_and_activate(session_states, session_id, state)
            .expect("spawned session must be activeable");
        if !session_states.restore_into(session_id, state) {
            state.reset_for_session_switch();
        }
        *dirty |= DirtyFlags::ALL;
        *initial_resize_sent = false;
        sync_session_metadata(session_manager, session_states, state);
    }
    sync_session_metadata(session_manager, session_states, state);
    Some((session_id, reader))
}

/// Close a managed session by key, or the active session when `key` is `None`.
///
/// Returns `true` when the application should exit because no sessions remain.
pub fn close_session_core<R, W, C>(
    key: Option<&str>,
    session_manager: &mut SessionManager<R, W, C>,
    session_states: &mut SessionStateStore,
    state: &mut AppState,
    dirty: &mut DirtyFlags,
    initial_resize_sent: &mut bool,
) -> bool {
    let target = key
        .and_then(|k| session_manager.session_id_by_key(k))
        .or_else(|| session_manager.active_session_id());
    let Some(target) = target else {
        return false;
    };
    let was_active = session_manager.active_session_id() == Some(target);
    let _ = session_manager.close(target);
    session_states.remove(target);
    *dirty |= DirtyFlags::SESSION;
    if session_manager.is_empty() {
        return true;
    }
    if was_active {
        let restored = session_manager
            .active_session_id()
            .is_some_and(|active| session_states.restore_into(active, state));
        if !restored {
            state.reset_for_session_switch();
        }
        *dirty |= DirtyFlags::ALL;
        *initial_resize_sent = false;
    }
    sync_session_metadata(session_manager, session_states, state);
    false
}

/// Switch to an existing managed session by key.
///
/// No-op if the key doesn't exist or is already active.
pub fn switch_session_core<R, W, C>(
    key: &str,
    session_manager: &mut SessionManager<R, W, C>,
    session_states: &mut SessionStateStore,
    state: &mut AppState,
    dirty: &mut DirtyFlags,
    initial_resize_sent: &mut bool,
) {
    let Some(target) = session_manager.session_id_by_key(key) else {
        return;
    };
    if session_manager.active_session_id() == Some(target) {
        return;
    }
    session_manager
        .sync_and_activate(session_states, target, state)
        .expect("switch target must be valid");
    if !session_states.restore_into(target, state) {
        state.reset_for_session_switch();
    }
    *dirty |= DirtyFlags::ALL | DirtyFlags::SESSION;
    *initial_resize_sent = false;
    sync_session_metadata(session_manager, session_states, state);
}

/// Rebuild the HitMap from the current view tree for plugin mouse routing.
pub fn rebuild_hit_map(
    state: &AppState,
    registry: &mut PluginRegistry,
    surface_registry: &SurfaceRegistry,
) {
    let root_area = Rect {
        x: 0,
        y: 0,
        w: state.cols,
        h: state.rows,
    };
    let element = surface_registry
        .compose_view_sections(state, registry, root_area)
        .into_element();
    let layout_result = crate::layout::flex::place(&element, root_area, state);
    let hit_map = crate::layout::build_hit_map(&element, &layout_result);
    registry.set_hit_map(hit_map);
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
}

/// Backend-owned access to the active session writer plus session lifecycle hooks.
pub trait SessionHost: SessionRuntime {
    fn active_writer(&mut self) -> &mut dyn Write;
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
    pub registry: &'a mut PluginRegistry,
    pub surface_registry: &'a mut SurfaceRegistry,
    pub clipboard_get: &'a mut dyn FnMut() -> Option<String>,
    pub dirty: &'a mut DirtyFlags,
    pub timer: &'a dyn TimerScheduler,
    pub session_host: &'a mut dyn SessionHost,
    pub initial_resize_sent: &'a mut bool,
    pub session_ready_gate: Option<&'a mut SessionReadyGate>,
    pub scroll_plan_sink: &'a mut dyn FnMut(ScrollPlan),
    pub process_dispatcher: &'a mut dyn ProcessDispatcher,
}

/// Maximum recursion depth for cascading deferred commands.
///
/// Prevents infinite loops when plugins produce deferred commands that trigger
/// further deferred commands (e.g., PluginMessage → PluginMessage chains).
const MAX_COMMAND_CASCADE_DEPTH: usize = 8;

/// Handle deferred commands (timers, inter-plugin messages, config overrides).
///
/// Returns `true` if a `Quit` command was encountered.
pub fn handle_deferred_commands(
    deferred: Vec<DeferredCommand>,
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
    let (normal, deferred) = extract_deferred_commands(commands);
    if matches!(
        execute_commands(normal, ctx.session_host.active_writer(), ctx.clipboard_get),
        CommandResult::Quit
    ) {
        return true;
    }
    handle_deferred_commands_inner(deferred, ctx, command_source_plugin, depth)
}

fn handle_deferred_commands_inner(
    deferred: Vec<DeferredCommand>,
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
        match cmd {
            DeferredCommand::PluginMessage { target, payload } => {
                let batch = ctx
                    .registry
                    .deliver_message_batch(&target, payload, ctx.state);
                if apply_runtime_batch(batch, ctx, Some(&target), depth + 1) {
                    return true;
                }
            }
            DeferredCommand::ScheduleTimer {
                delay,
                target,
                payload,
            } => {
                ctx.timer.schedule_timer(delay, target, payload);
            }
            DeferredCommand::SetConfig { key, value } => {
                crate::state::apply_set_config(ctx.state, ctx.dirty, &key, &value);
            }
            DeferredCommand::Workspace(ws_cmd) => {
                crate::workspace::dispatch_workspace_command_with_total(
                    ctx.surface_registry,
                    ws_cmd,
                    ctx.dirty,
                    Some(crate::layout::Rect {
                        x: 0,
                        y: 0,
                        w: ctx.state.cols,
                        h: ctx.state.rows,
                    }),
                );
            }
            DeferredCommand::RegisterThemeTokens(_tokens) => {
                // Theme token registration will be handled when Theme is
                // accessible from the event loop (Phase 1 completion).
            }
            DeferredCommand::SpawnProcess {
                job_id,
                program,
                args,
                stdin_mode,
            } => {
                if let Some(plugin_id) = command_source_plugin {
                    if ctx.registry.plugin_allows_process_spawn(plugin_id) {
                        ctx.process_dispatcher
                            .spawn(plugin_id, job_id, &program, &args, stdin_mode);
                    } else {
                        tracing::warn!(
                            plugin = plugin_id.0,
                            "SpawnProcess denied: process capability not granted"
                        );
                        let fail_event = IoEvent::Process(ProcessEvent::SpawnFailed {
                            job_id,
                            error: "process capability not granted".to_string(),
                        });
                        let batch =
                            ctx.registry
                                .deliver_io_event_batch(plugin_id, &fail_event, ctx.state);
                        if apply_runtime_batch(batch, ctx, Some(plugin_id), depth + 1) {
                            return true;
                        }
                    }
                }
            }
            DeferredCommand::WriteToProcess { job_id, data } => {
                if let Some(plugin_id) = command_source_plugin {
                    ctx.process_dispatcher.write(plugin_id, job_id, &data);
                }
            }
            DeferredCommand::CloseProcessStdin { job_id } => {
                if let Some(plugin_id) = command_source_plugin {
                    ctx.process_dispatcher.close_stdin(plugin_id, job_id);
                }
            }
            DeferredCommand::KillProcess { job_id } => {
                if let Some(plugin_id) = command_source_plugin {
                    ctx.process_dispatcher.kill(plugin_id, job_id);
                }
            }
            DeferredCommand::Session(cmd) => {
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
                            return true;
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
                    .notify_state_changed_batch(ctx.state, DirtyFlags::SESSION);
                if apply_runtime_batch_without_session_deferred(batch, ctx, None, depth + 1) {
                    return true;
                }
            }
        }
    }
    false
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

    let (normal, nested_deferred) = extract_deferred_commands(commands);
    if matches!(
        execute_commands(normal, ctx.session_host.active_writer(), ctx.clipboard_get),
        CommandResult::Quit
    ) {
        return true;
    }
    let nested_non_session: Vec<_> = nested_deferred
        .into_iter()
        .filter(|d| !matches!(d, DeferredCommand::Session(_)))
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

    let batch = ctx.registry.notify_active_session_ready_batch(ctx.state);
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
                        ctx.session_host.active_writer(),
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
                        ctx.session_host.active_writer(),
                        ctx.clipboard_get,
                    ),
                    CommandResult::Quit
                ) {
                    return true;
                }
            }
            SessionReadyCommand::PluginMessage { target, payload } => {
                let batch = ctx
                    .registry
                    .deliver_message_batch(&target, payload, ctx.state);
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

#[cfg(test)]
mod tests {
    use super::*;

    use crate::layout::SplitDirection;
    use crate::plugin::{
        AppliedWinnerDelta, Command, PluginBackend, PluginDescriptor, PluginId, PluginRank,
        PluginRevision, PluginSource, RuntimeEffects, StdinMode,
    };
    use crate::scroll::{ScrollAccumulationMode, ScrollCurve, ScrollPlan};
    use crate::surface::SurfaceId;
    use crate::test_support::TestSurfaceBuilder;
    use crate::workspace::Placement;

    struct TestPlugin {
        id: PluginId,
        allow_spawn: bool,
    }

    impl PluginBackend for TestPlugin {
        fn id(&self) -> PluginId {
            self.id.clone()
        }

        fn allows_process_spawn(&self) -> bool {
            self.allow_spawn
        }
    }

    struct RuntimeMessagePlugin;

    impl PluginBackend for RuntimeMessagePlugin {
        fn id(&self) -> PluginId {
            PluginId("runtime-message".to_string())
        }

        fn update_effects(&mut self, msg: &mut dyn Any, _state: &AppState) -> RuntimeEffects {
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
        let mut registry = PluginRegistry::new();
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
        let mut registry = PluginRegistry::new();
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
        let mut registry = PluginRegistry::new();
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

        let _ = registry.reload_plugin_batch(Box::new(ReplacementSurfacePlugin), &state);
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
    fn sourced_surface_commands_preserve_plugin_for_spawn_process() {
        let plugin_id = PluginId("surface-owner".to_string());
        let mut registry = PluginRegistry::new();
        registry.register_backend(Box::new(TestPlugin {
            id: plugin_id.clone(),
            allow_spawn: true,
        }));

        let mut state = AppState::default();
        let mut surface_registry = SurfaceRegistry::new();
        let mut dirty = DirtyFlags::empty();
        let timer = NoopTimer;
        let mut sessions = NoopSessionRuntime::default();
        let mut initial_resize_sent = false;
        let mut dispatcher = RecordingDispatcher::default();

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
                clipboard_get: &mut || None,
                dirty: &mut dirty,
                timer: &timer,
                session_host: &mut sessions,
                initial_resize_sent: &mut initial_resize_sent,
                session_ready_gate: None,
                scroll_plan_sink: &mut |_| {},
                process_dispatcher: &mut dispatcher,
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
        let mut registry = PluginRegistry::new();
        registry.register_backend(Box::new(RuntimeMessagePlugin));
        let mut surface_registry = SurfaceRegistry::new();
        let mut dirty = DirtyFlags::empty();
        let timer = NoopTimer;
        let mut sessions = NoopSessionRuntime::default();
        let mut initial_resize_sent = true;
        let mut dispatcher = RecordingDispatcher::default();
        let mut plans = Vec::new();

        let quit = handle_deferred_commands(
            vec![DeferredCommand::PluginMessage {
                target: PluginId("runtime-message".to_string()),
                payload: Box::new(11u32),
            }],
            &mut DeferredContext {
                state: &mut state,
                registry: &mut registry,
                surface_registry: &mut surface_registry,
                clipboard_get: &mut || None,
                dirty: &mut dirty,
                timer: &timer,
                session_host: &mut sessions,
                initial_resize_sent: &mut initial_resize_sent,
                session_ready_gate: None,
                scroll_plan_sink: &mut |plan| plans.push(plan),
                process_dispatcher: &mut dispatcher,
            },
            None,
        );

        assert!(!quit);
        assert!(dirty.contains(DirtyFlags::INFO));
        assert!(dirty.contains(DirtyFlags::STATUS));
        assert_eq!(plans.len(), 1);
        assert_eq!(plans[0].total_amount, 2);
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
        let mut registry = PluginRegistry::new();
        let mut surface_registry = SurfaceRegistry::new();
        let mut dirty = DirtyFlags::empty();
        let timer = NoopTimer;
        let mut sessions = RecordingSessionHost::default();
        let mut initial_resize_sent = true;
        let mut dispatcher = RecordingDispatcher::default();

        let quit = handle_deferred_commands(
            vec![DeferredCommand::Session(
                crate::session::SessionCommand::Spawn {
                    key: Some("work".to_string()),
                    session: Some("project".to_string()),
                    args: vec!["file.txt".to_string()],
                    activate: true,
                },
            )],
            &mut DeferredContext {
                state: &mut state,
                registry: &mut registry,
                surface_registry: &mut surface_registry,
                clipboard_get: &mut || None,
                dirty: &mut dirty,
                timer: &timer,
                session_host: &mut sessions,
                initial_resize_sent: &mut initial_resize_sent,
                session_ready_gate: None,
                scroll_plan_sink: &mut |_| {},
                process_dispatcher: &mut dispatcher,
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
        let mut registry = PluginRegistry::new();
        let mut surface_registry = SurfaceRegistry::new();
        let mut dirty = DirtyFlags::empty();
        let timer = NoopTimer;
        let mut sessions = RecordingSessionHost::default();
        let mut initial_resize_sent = false;
        let mut dispatcher = RecordingDispatcher::default();

        let quit = handle_deferred_commands(
            vec![DeferredCommand::Session(
                crate::session::SessionCommand::Close {
                    key: Some("work".to_string()),
                },
            )],
            &mut DeferredContext {
                state: &mut state,
                registry: &mut registry,
                surface_registry: &mut surface_registry,
                clipboard_get: &mut || None,
                dirty: &mut dirty,
                timer: &timer,
                session_host: &mut sessions,
                initial_resize_sent: &mut initial_resize_sent,
                session_ready_gate: None,
                scroll_plan_sink: &mut |_| {},
                process_dispatcher: &mut dispatcher,
            },
            None,
        );

        assert!(!quit);
        assert_eq!(sessions.closed, vec![Some("work".to_string())]);
    }

    #[test]
    fn deferred_session_close_returns_quit_when_host_requests_shutdown() {
        let mut state = AppState::default();
        let mut registry = PluginRegistry::new();
        let mut surface_registry = SurfaceRegistry::new();
        let mut dirty = DirtyFlags::empty();
        let timer = NoopTimer;
        let mut sessions = RecordingSessionHost {
            close_returns_quit: true,
            ..RecordingSessionHost::default()
        };
        let mut initial_resize_sent = false;
        let mut dispatcher = RecordingDispatcher::default();

        let quit = handle_deferred_commands(
            vec![DeferredCommand::Session(
                crate::session::SessionCommand::Close { key: None },
            )],
            &mut DeferredContext {
                state: &mut state,
                registry: &mut registry,
                surface_registry: &mut surface_registry,
                clipboard_get: &mut || None,
                dirty: &mut dirty,
                timer: &timer,
                session_host: &mut sessions,
                initial_resize_sent: &mut initial_resize_sent,
                session_ready_gate: None,
                scroll_plan_sink: &mut |_| {},
                process_dispatcher: &mut dispatcher,
            },
            None,
        );

        assert!(quit);
        assert_eq!(sessions.closed, vec![None]);
    }

    #[test]
    fn deferred_session_switch_is_routed() {
        let mut state = AppState::default();
        let mut registry = PluginRegistry::new();
        let mut surface_registry = SurfaceRegistry::new();
        let mut dirty = DirtyFlags::empty();
        let timer = NoopTimer;
        let mut sessions = RecordingSessionHost::default();
        let mut initial_resize_sent = false;
        let mut dispatcher = RecordingDispatcher::default();

        let quit = handle_deferred_commands(
            vec![DeferredCommand::Session(
                crate::session::SessionCommand::Switch {
                    key: "work".to_string(),
                },
            )],
            &mut DeferredContext {
                state: &mut state,
                registry: &mut registry,
                surface_registry: &mut surface_registry,
                clipboard_get: &mut || None,
                dirty: &mut dirty,
                timer: &timer,
                session_host: &mut sessions,
                initial_resize_sent: &mut initial_resize_sent,
                session_ready_gate: None,
                scroll_plan_sink: &mut |_| {},
                process_dispatcher: &mut dispatcher,
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
}
