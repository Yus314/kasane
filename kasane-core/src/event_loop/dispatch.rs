//! Command dispatch and effects application.
//!
//! Routes deferred commands to domain-specific handlers and cascades
//! runtime effects back through the plugin system.

use crate::layout::Rect;
use crate::plugin::{
    AppView, Command, CommandResult, Effects, EffectsBatch, PluginAuthorities, PluginId, StdinMode,
    execute_commands, extract_redraw_flags, partition_commands,
};
use crate::session::SessionSpec;
use crate::state::{AppState, DirtyFlags, Msg, update};
use crate::surface::SourcedSurfaceCommands;

use super::context::{
    DeferredContext, MAX_COMMAND_CASCADE_DEPTH, MAX_INJECT_DEPTH, UnregisterResult,
    deliver_spawn_failure, dispatch_add_surface, focused_writer, require_surface_authority,
    try_unregister_owned_surface,
};
use super::session::apply_ready_batch;

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

    for command in immediate {
        match command {
            Command::PasteClipboard => {
                if let Some(text) = ctx.clipboard.get()
                    && dispatch_input_event(ctx, crate::input::InputEvent::Paste(text), depth)
                {
                    return true;
                }
            }
            other => {
                if matches!(
                    execute_commands(vec![other], focused_writer!(ctx), ctx.clipboard),
                    CommandResult::Quit
                ) {
                    return true;
                }
            }
        }
    }
    handle_deferred_commands_inner(deferred, ctx, command_source_plugin, depth)
}

pub(super) fn dispatch_input_event(
    ctx: &mut DeferredContext<'_>,
    input: crate::input::InputEvent,
    depth: usize,
) -> bool {
    if depth >= MAX_INJECT_DEPTH {
        tracing::warn!(
            depth,
            "dispatch_input_event depth limit reached, dropping event"
        );
        return false;
    }

    let input = super::normalize_input_for_state(input, ctx.state);
    let total = Rect {
        x: 0,
        y: 0,
        w: ctx.state.cols,
        h: ctx.state.rows,
    };

    if let Some(divider_dirty) =
        super::handle_workspace_divider_input(&input, ctx.surface_registry, total)
    {
        *ctx.dirty |= divider_dirty;
        if !divider_dirty.is_empty() {
            *ctx.workspace_changed = true;
            super::notify_workspace_observers(ctx.registry, ctx.surface_registry, ctx.state);
        }
        return false;
    }

    if let Some(surface_commands) =
        super::route_surface_key_input(&input, ctx.surface_registry, ctx.state, total)
    {
        return handle_sourced_surface_commands(vec![surface_commands], ctx);
    }

    if let Some(surface_commands) =
        super::route_surface_text_input(&input, ctx.surface_registry, ctx.state, total)
    {
        return handle_sourced_surface_commands(vec![surface_commands], ctx);
    }

    let surface_event = super::surface_event_from_input(&input);
    let state = std::mem::take(ctx.state);
    let (returned_state, result) = update(
        Box::new(state),
        Msg::from(input),
        ctx.registry,
        ctx.scroll_amount,
    );
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
        return true;
    }

    let surface_commands = surface_event
        .map(|event| {
            ctx.surface_registry
                .route_event_with_sources(event, ctx.state, total)
        })
        .unwrap_or_default();

    if !surface_commands.is_empty() && handle_sourced_surface_commands(surface_commands, ctx) {
        return true;
    }

    false
}

pub(super) fn handle_deferred_commands_inner(
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
            | Command::SetConfig { .. }
            | Command::SetSetting { .. } => {
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
            Command::StartProcessTask { .. } => {
                handle_start_process_task(cmd, ctx, command_source_plugin, depth)
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
        Command::SetSetting {
            plugin_id,
            key,
            value,
        } => {
            crate::state::apply_set_setting(ctx.state, ctx.dirty, &plugin_id, &key, value);
        }
        _ => unreachable!(),
    }
    Some(false)
}

/// Register a plugin-owned surface and dispatch layout addition on success.
fn register_surface_core(
    ctx: &mut DeferredContext<'_>,
    plugin_id: &PluginId,
    surface: Box<dyn crate::surface::Surface>,
    placement: crate::workspace::Placement,
    label: &str,
) {
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
                "{label} ignored: {err:?}"
            );
        }
    }
}

/// Unregister a plugin-owned surface and log the outcome.
fn unregister_and_log(
    ctx: &mut DeferredContext<'_>,
    plugin_id: &PluginId,
    surface_id: crate::surface::SurfaceId,
    surface_key: Option<&str>,
    label: &str,
) {
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
                surface_key = surface_key.unwrap_or(""),
                "{label} ignored: surface owned by another plugin"
            );
        }
        UnregisterResult::NotFound => {
            tracing::warn!(
                plugin = plugin_id.0,
                surface_id = surface_id.0,
                surface_key = surface_key.unwrap_or(""),
                "{label} ignored: surface is not plugin-owned or missing"
            );
        }
    }
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

            register_surface_core(ctx, plugin_id, surface, placement, "RegisterSurface");
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

            unregister_and_log(ctx, plugin_id, surface_id, None, "UnregisterSurface");
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

            unregister_and_log(
                ctx,
                plugin_id,
                surface_id,
                Some(&surface_key),
                "UnregisterSurfaceKey",
            );
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
        Command::RegisterThemeTokens(tokens) => {
            for (name, face) in tokens {
                let token = crate::element::StyleToken::new(name);
                if ctx.state.theme.get(&token).is_none() {
                    ctx.state.theme.set(token, face);
                }
            }
            *ctx.dirty |= DirtyFlags::OPTIONS;
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
    command_source_plugin: Option<&PluginId>,
    depth: usize,
) -> Option<bool> {
    match cmd {
        Command::SpawnPaneClient {
            pane_key,
            placement,
        } => {
            if let Some(plugin_id) = command_source_plugin
                && !ctx
                    .registry
                    .plugin_has_authority(plugin_id, PluginAuthorities::WORKSPACE)
            {
                tracing::warn!(
                    plugin = plugin_id.0.as_str(),
                    "SpawnPaneClient denied: WORKSPACE authority not granted"
                );
                return Some(false);
            }

            if let Some(server_name) = ctx
                .surface_registry
                .server_session_name()
                .map(str::to_owned)
            {
                let surface_id = ctx.surface_registry.workspace_mut().next_surface_id();
                let spec = SessionSpec::new(
                    pane_key.clone(),
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

                // Register ClientBufferSurface with pane_key (must exist before bind_session)
                let _ = ctx.surface_registry.try_register(Box::new(
                    crate::surface::buffer::ClientBufferSurface::with_key(surface_id, &pane_key),
                ));

                // Bind surface -> session and defer initial resize
                if let Some(session_id) = ctx.session_host.session_id_by_key(&pane_key) {
                    ctx.surface_registry.bind_session(surface_id, session_id);
                    ctx.surface_registry.mark_pending_resize(session_id);
                }

                // Add to workspace
                dispatch_add_surface(ctx, surface_id, placement);
            } else {
                tracing::warn!("SpawnPaneClient ignored: no server session name available");
            }
        }
        Command::ClosePaneClient { pane_key } => {
            if let Some(plugin_id) = command_source_plugin
                && !ctx
                    .registry
                    .plugin_has_authority(plugin_id, PluginAuthorities::WORKSPACE)
            {
                tracing::warn!(
                    plugin = plugin_id.0.as_str(),
                    "ClosePaneClient denied: WORKSPACE authority not granted"
                );
                return Some(false);
            }

            if let Some(surface_id) = ctx.surface_registry.surface_id_by_key(&pane_key) {
                if let Some(_session_id) =
                    ctx.surface_registry.unbind_session_by_surface(surface_id)
                {
                    ctx.session_host.close_session(
                        Some(&pane_key),
                        ctx.state,
                        ctx.dirty,
                        ctx.initial_resize_sent,
                    );
                }
                ctx.surface_registry.remove(surface_id);
                let _ = ctx.surface_registry.workspace_mut().close(surface_id);
                *ctx.dirty |= DirtyFlags::ALL;
                *ctx.workspace_changed = true;
            } else {
                tracing::warn!(pane_key, "ClosePaneClient ignored: unknown pane key");
            }
        }
        Command::BindSurfaceSession {
            surface_id,
            session_id,
        } => {
            ctx.surface_registry.bind_session(surface_id, session_id);
        }
        Command::UnbindSurfaceSession { surface_id } => {
            ctx.surface_registry.unbind_session_by_surface(surface_id);
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
            } else if dispatch_input_event(ctx, input_event, depth) {
                return Some(true);
            }
        }
        _ => unreachable!(),
    }
    Some(false)
}

/// Handle `StartProcessTask` command: look up the task spec, start the process.
fn handle_start_process_task(
    cmd: Command,
    ctx: &mut DeferredContext<'_>,
    command_source_plugin: Option<&PluginId>,
    depth: usize,
) -> Option<bool> {
    let Command::StartProcessTask { task_name } = cmd else {
        unreachable!();
    };
    let Some(plugin_id) = command_source_plugin else {
        tracing::warn!(task_name, "StartProcessTask ignored: no source plugin");
        return Some(false);
    };

    if !ctx.registry.plugin_allows_process_spawn(plugin_id) {
        tracing::warn!(
            plugin = plugin_id.0.as_str(),
            task_name,
            "StartProcessTask denied: process capability not granted"
        );
        return Some(false);
    }

    let spawn_commands = ctx.registry.start_process_task(plugin_id, &task_name);
    if spawn_commands.is_empty() {
        return Some(false);
    }

    // The spawn commands are process management commands that go through the
    // normal deferred dispatch (SpawnProcess, etc.).
    if handle_deferred_commands_inner(spawn_commands, ctx, command_source_plugin, depth + 1) {
        return Some(true);
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

pub fn apply_bootstrap_effects(effects: Effects, dirty: &mut DirtyFlags) {
    *dirty |= effects.redraw;
    // Bootstrap phase: only redraw is valid; commands/scroll_plans validated upstream.
}

fn apply_runtime_effects(
    mut effects: Effects,
    ctx: &mut DeferredContext<'_>,
    command_source_plugin: Option<&PluginId>,
    depth: usize,
) -> bool {
    effects.deduplicate_commutative();
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

pub(super) fn apply_runtime_batch(
    batch: EffectsBatch,
    ctx: &mut DeferredContext<'_>,
    command_source_plugin: Option<&PluginId>,
    depth: usize,
) -> bool {
    apply_runtime_effects(batch.effects, ctx, command_source_plugin, depth)
}

pub(super) fn apply_runtime_batch_without_session_deferred(
    batch: EffectsBatch,
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
        execute_commands(immediate, focused_writer!(ctx), ctx.clipboard),
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

pub fn sync_session_ready_gate(
    gate: &mut super::session::SessionReadyGate,
    state: &AppState,
) -> bool {
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
