//! TUI event loop: polls crossterm and Kakoune, dispatches to core update/view/render.

use std::io::Write;

use anyhow::Result;

use kasane_core::clipboard::SystemClipboard;
use kasane_core::event_loop::{
    DeferredContext, EventResult, SessionReadyGate, TimerScheduler, apply_bootstrap_effects,
    apply_ready_batch, handle_command_batch, handle_sourced_surface_commands,
    handle_workspace_divider_input, maybe_flush_active_session_ready, normalize_input_for_state,
    notify_workspace_observers, reconcile_plugin_surfaces, route_surface_key_input,
    route_surface_text_input, surface_event_from_input, sync_session_ready_gate as sync_ready_gate,
};
use kasane_core::input::InputEvent;
use kasane_core::layout::Rect;
use kasane_core::plugin::{
    AppView, EffectsBatch, IoEvent, PluginDiagnostic, PluginDiagnosticOverlayState, PluginId,
    PluginManager, PluginRuntime, ProcessDispatcher, ProcessEvent, ProcessEventSink,
    extract_redraw_flags, report_plugin_diagnostics,
};
use kasane_core::protocol::KakouneRequest;
use kasane_core::render::CellGrid;
use kasane_core::scroll::ScrollRuntime;
use kasane_core::session::{SessionId, SessionManager, SessionSpec, SessionStateStore};
use kasane_core::state::{AppState, DirtyFlags, Msg, UpdateResult, update};
use kasane_core::surface::SurfaceRegistry;

use crate::backend::TuiBackend;
use kasane_core::event_loop::schedule_diagnostic_overlay;

pub(crate) enum Event {
    Kakoune(SessionId, KakouneRequest),
    Input(InputEvent),
    KakouneDied(SessionId),
    PluginTimer(PluginId, Box<dyn std::any::Any + Send>),
    ProcessOutput(PluginId, IoEvent),
    PluginReload,
    DiagnosticOverlayExpire(u64),
}

impl Event {
    /// Returns `true` if this is a `Kakoune(_, Draw { .. })` event.
    pub(crate) fn is_draw(&self) -> bool {
        matches!(self, Event::Kakoune(_, KakouneRequest::Draw { .. }))
    }

    /// Returns `true` if this is a `Kakoune(_, Refresh { .. })` event.
    pub(crate) fn is_refresh(&self) -> bool {
        matches!(self, Event::Kakoune(_, KakouneRequest::Refresh { .. }))
    }
}

/// ProcessEventSink that injects process I/O events into the TUI event channel.
pub(crate) struct TuiProcessEventSink(pub(crate) crossbeam_channel::Sender<Event>);

impl ProcessEventSink for TuiProcessEventSink {
    fn send_process_output(&self, plugin_id: PluginId, event: IoEvent) {
        let _ = self.0.send(Event::ProcessOutput(plugin_id, event));
    }
}

/// EventSink that injects events into the TUI crossbeam channel.
#[derive(Clone)]
pub(crate) struct TuiEventSink(pub(crate) crossbeam_channel::Sender<Event>);

impl kasane_core::event_loop::EventSink for TuiEventSink {
    fn send_kakoune(&self, session_id: SessionId, req: KakouneRequest) {
        let _ = self.0.send(Event::Kakoune(session_id, req));
    }
    fn send_died(&self, session_id: SessionId) {
        let _ = self.0.send(Event::KakouneDied(session_id));
    }
    fn send_timer(&self, target: PluginId, payload: Box<dyn std::any::Any + Send>) {
        let _ = self.0.send(Event::PluginTimer(target, payload));
    }
    fn send_diagnostic_expire(&self, generation: u64) {
        let _ = self.0.send(Event::DiagnosticOverlayExpire(generation));
    }
}

/// Grouped mutable context for `process_event`, reducing its parameter count.
pub(crate) struct EventProcessingContext<'a, R, W, C> {
    pub state: &'a mut Box<AppState>,
    pub registry: &'a mut PluginRuntime,
    pub surface_registry: &'a mut SurfaceRegistry,
    pub session_manager: &'a mut SessionManager<R, W, C>,
    pub session_states: &'a mut SessionStateStore,
    pub session_tx: &'a crossbeam_channel::Sender<Event>,
    pub spawn_session: fn(&SessionSpec) -> Result<(R, W, C)>,
    pub grid: &'a mut CellGrid,
    pub scroll_amount: i32,
    pub clipboard: &'a mut SystemClipboard,
    pub backend: &'a mut TuiBackend,
    pub initial_resize_sent: &'a mut bool,
    pub dirty: &'a mut DirtyFlags,
    pub timer: &'a dyn TimerScheduler,
    pub scroll_runtime: &'a mut ScrollRuntime,
    pub scroll_runtime_session: &'a mut Option<SessionId>,
    pub session_ready_gate: &'a mut SessionReadyGate,
    pub process_dispatcher: &'a mut dyn ProcessDispatcher,
    pub plugin_manager: &'a mut PluginManager,
    pub diagnostic_overlay: &'a mut PluginDiagnosticOverlayState,
}

struct PluginReloadOutcome {
    flags: DirtyFlags,
    should_quit: bool,
}

/// Process a single event, returning `true` if the application should quit.
pub(crate) fn process_event<R, W, C>(
    event: Event,
    ctx: &mut EventProcessingContext<'_, R, W, C>,
) -> bool
where
    R: std::io::BufRead + Send + 'static,
    W: Write + Send + 'static,
    C: Send + 'static,
{
    let is_input = matches!(&event, Event::Input(_));

    let result = match event {
        Event::Kakoune(session_id, req) => {
            if ctx.session_manager.active_session_id() != Some(session_id) {
                ctx.session_states
                    .ensure_session(session_id, ctx.state)
                    .apply(req);
                // Send the deferred initial Resize now that the kak process
                // has proven it's initialized (it sent its first event).
                if ctx.surface_registry.take_pending_resize(session_id)
                    && let Some(surface_id) = ctx.surface_registry.surface_for_session(session_id)
                {
                    let total = kasane_core::layout::Rect {
                        x: 0,
                        y: 0,
                        w: ctx.state.cols,
                        h: ctx.state.rows,
                    };
                    let rects = ctx.surface_registry.workspace().compute_rects(total);
                    if let Some(rect) = rects.get(&surface_id)
                        && let Ok(writer) = ctx.session_manager.writer_mut(session_id)
                    {
                        // Per-pane status bar occupies 1 row from each pane.
                        let rows = rect.h.saturating_sub(1);
                        kasane_core::io::send_request(
                            writer,
                            &kasane_core::protocol::KasaneRequest::Resize { rows, cols: rect.w },
                        );
                        ctx.surface_registry.record_resize(session_id, rows, rect.w);
                    }
                }
                // If the session is a visible pane, trigger a redraw
                if ctx
                    .surface_registry
                    .surface_for_session(session_id)
                    .is_some()
                {
                    *ctx.dirty |= DirtyFlags::ALL;
                }
                return false;
            }
            kasane_core::io::send_initial_resize(
                ctx.session_manager
                    .active_writer_mut()
                    .expect("missing active session writer"),
                ctx.initial_resize_sent,
                ctx.state.rows,
                ctx.state.cols,
            );
            sync_ready_gate(ctx.session_ready_gate, ctx.state);
            if *ctx.initial_resize_sent {
                ctx.session_ready_gate.mark_initial_resize_sent();
            }
            if flush_active_session_ready_if_needed(ctx) {
                return true;
            }
            let state = std::mem::take(ctx.state);
            let (
                state,
                UpdateResult {
                    flags,
                    commands,
                    scroll_plans,
                    source_plugin: _source,
                },
            ) = update(state, Msg::Kakoune(req), ctx.registry, ctx.scroll_amount);
            *ctx.state = state;
            let surface_commands = if flags.is_empty() {
                vec![]
            } else {
                ctx.surface_registry
                    .on_state_changed_with_sources(ctx.state, flags)
            };
            EventResult {
                flags,
                commands,
                scroll_plans,
                surface_commands,
                command_source: None,
                workspace_changed: false,
            }
        }
        Event::Input(ref input_event) => {
            tracing::trace!(?input_event, "process_event: Input");
            let input_event = if let Event::Input(ie) = event {
                normalize_input_for_state(ie, ctx.state)
            } else {
                unreachable!()
            };
            let total = Rect {
                x: 0,
                y: 0,
                w: ctx.state.cols,
                h: ctx.state.rows,
            };
            if let Some(divider_dirty) =
                handle_workspace_divider_input(&input_event, ctx.surface_registry, total)
            {
                EventResult {
                    flags: divider_dirty,
                    commands: vec![],
                    scroll_plans: vec![],
                    surface_commands: vec![],
                    command_source: None,
                    workspace_changed: !divider_dirty.is_empty(),
                }
            } else if let Some(surface_commands) =
                route_surface_key_input(&input_event, ctx.surface_registry, ctx.state, total)
            {
                EventResult {
                    flags: DirtyFlags::empty(),
                    commands: vec![],
                    scroll_plans: vec![],
                    surface_commands: vec![surface_commands],
                    command_source: None,
                    workspace_changed: false,
                }
            } else if let Some(surface_commands) =
                route_surface_text_input(&input_event, ctx.surface_registry, ctx.state, total)
            {
                EventResult {
                    flags: DirtyFlags::empty(),
                    commands: vec![],
                    scroll_plans: vec![],
                    surface_commands: vec![surface_commands],
                    command_source: None,
                    workspace_changed: false,
                }
            } else {
                let surface_event = surface_event_from_input(&input_event);
                let workspace_changed = matches!(input_event, InputEvent::Resize(..));
                let state = std::mem::take(ctx.state);
                let (
                    state,
                    UpdateResult {
                        flags,
                        commands,
                        scroll_plans,
                        source_plugin,
                    },
                ) = update(
                    state,
                    Msg::from(input_event),
                    ctx.registry,
                    ctx.scroll_amount,
                );
                *ctx.state = state;
                let surface_commands = surface_event
                    .map(|event| {
                        ctx.surface_registry
                            .route_event_with_sources(event, ctx.state, total)
                    })
                    .unwrap_or_default();
                EventResult {
                    flags,
                    commands,
                    scroll_plans,
                    surface_commands,
                    command_source: source_plugin,
                    workspace_changed,
                }
            }
        }
        Event::PluginTimer(target, payload) => event_result_from_runtime_batch(
            ctx.registry
                .deliver_message_batch(&target, payload, &AppView::new(ctx.state)),
            Some(target),
        ),
        Event::DiagnosticOverlayExpire(generation) => EventResult {
            flags: if ctx.diagnostic_overlay.dismiss(generation) {
                ctx.backend.invalidate();
                DirtyFlags::ALL
            } else {
                DirtyFlags::empty()
            },
            commands: vec![],
            scroll_plans: vec![],
            surface_commands: vec![],
            command_source: None,
            workspace_changed: false,
        },
        Event::ProcessOutput(plugin_id, io_event) => {
            let batch = ctx.registry.deliver_io_event_batch(
                &plugin_id,
                &io_event,
                &AppView::new(ctx.state),
            );
            // Free per-plugin process count slot when a job finishes
            let IoEvent::Process(ref pe) = io_event;
            let finished_job = match pe {
                ProcessEvent::Exited { job_id, .. } | ProcessEvent::SpawnFailed { job_id, .. } => {
                    Some(*job_id)
                }
                _ => None,
            };
            if let Some(job_id) = finished_job {
                ctx.process_dispatcher
                    .remove_finished_job(&plugin_id, job_id);
            }
            event_result_from_runtime_batch(batch, Some(plugin_id))
        }
        Event::PluginReload => {
            match handle_plugin_reload(ctx) {
                Ok(outcome) => {
                    if outcome.should_quit {
                        return true;
                    }
                    return process_event_result(
                        EventResult {
                            flags: outcome.flags,
                            commands: vec![],
                            scroll_plans: vec![],
                            surface_commands: vec![],
                            command_source: None,
                            workspace_changed: false,
                        },
                        false,
                        ctx,
                    );
                }
                Err(err) => {
                    tracing::error!("failed to hot-reload plugins: {err}");
                }
            }
            EventResult {
                flags: DirtyFlags::all(),
                commands: vec![],
                scroll_plans: vec![],
                surface_commands: vec![],
                command_source: None,
                workspace_changed: false,
            }
        }
        Event::KakouneDied(session_id) => {
            let mut session_ctx = kasane_core::event_loop::SessionMutContext {
                session_manager: ctx.session_manager,
                session_states: ctx.session_states,
                state: ctx.state,
                dirty: ctx.dirty,
                initial_resize_sent: ctx.initial_resize_sent,
            };
            if kasane_core::event_loop::handle_pane_death(
                session_id,
                ctx.surface_registry,
                &mut session_ctx,
            ) {
                return true;
            }
            // handle_pane_death may have reset initial_resize_sent when
            // switching to the next active session.  Send the resize now so
            // the new session is unblocked.
            if !*ctx.initial_resize_sent {
                kasane_core::io::send_initial_resize(
                    ctx.session_manager
                        .active_writer_mut()
                        .expect("missing active session writer"),
                    ctx.initial_resize_sent,
                    ctx.state.rows,
                    ctx.state.cols,
                );
                sync_ready_gate(ctx.session_ready_gate, ctx.state);
                if *ctx.initial_resize_sent {
                    ctx.session_ready_gate.mark_initial_resize_sent();
                }
                if flush_active_session_ready_if_needed(ctx) {
                    return true;
                }
            }
            notify_workspace_observers(ctx.registry, ctx.surface_registry, ctx.state);
            let batch = ctx
                .registry
                .notify_state_changed_batch(&AppView::new(ctx.state), DirtyFlags::SESSION);
            let result = event_result_from_runtime_batch(batch, None);
            return process_event_result(result, false, ctx);
        }
    };

    process_event_result(result, is_input, ctx)
}

fn handle_plugin_reload<R, W, C>(
    ctx: &mut EventProcessingContext<'_, R, W, C>,
) -> Result<PluginReloadOutcome>
where
    R: std::io::BufRead + Send + 'static,
    W: Write + Send + 'static,
    C: Send + 'static,
{
    let reload = ctx.plugin_manager.reload(
        ctx.registry,
        &AppView::new(ctx.state),
        |result, registry| {
            reconcile_reloaded_plugin_resources(
                registry,
                ctx.surface_registry,
                ctx.state,
                result.deltas.as_slice(),
            )
        },
    )?;
    reload.apply_settings(ctx.state);
    report_plugin_diagnostics(&reload.diagnostics);
    schedule_diagnostic_overlay(
        &kasane_core::event_loop::GenericDiagnosticScheduler(TuiEventSink(ctx.session_tx.clone())),
        ctx.diagnostic_overlay,
        &reload.diagnostics,
    );

    let ready_targets = reload.ready_targets().cloned().collect::<Vec<_>>();
    let mut flags = DirtyFlags::all();
    apply_bootstrap_effects(reload.bootstrap, &mut flags);
    sync_ready_gate(ctx.session_ready_gate, ctx.state);
    if !reload.deltas.is_empty() {
        notify_workspace_observers(ctx.registry, ctx.surface_registry, ctx.state);
    }
    let should_quit = *ctx.initial_resize_sent && flush_reloaded_plugins_ready(ctx, &ready_targets);
    tracing::info!("hot-reloaded plugins");

    Ok(PluginReloadOutcome { flags, should_quit })
}

fn reconcile_reloaded_plugin_resources(
    registry: &mut PluginRuntime,
    surface_registry: &mut SurfaceRegistry,
    state: &AppState,
    deltas: &[kasane_core::plugin::AppliedWinnerDelta],
) -> Vec<PluginDiagnostic> {
    if deltas.is_empty() {
        return vec![];
    }

    reconcile_plugin_surfaces(registry, surface_registry, state, deltas)
}

fn event_result_from_runtime_batch(
    mut batch: EffectsBatch,
    command_source: Option<PluginId>,
) -> EventResult {
    let mut commands = std::mem::take(&mut batch.effects.commands);
    let flags = batch.effects.redraw | extract_redraw_flags(&mut commands);
    EventResult {
        flags,
        commands,
        scroll_plans: batch.effects.scroll_plans,
        surface_commands: vec![],
        command_source,
        workspace_changed: false,
    }
}

/// Apply an `EventResult` to the shared context: accumulate dirty flags,
/// execute commands, handle deferred commands and surface commands.
///
/// Returns `true` if the application should quit.
fn process_event_result<R, W, C>(
    mut result: EventResult,
    is_input: bool,
    ctx: &mut EventProcessingContext<'_, R, W, C>,
) -> bool
where
    R: std::io::BufRead + Send + 'static,
    W: Write + Send + 'static,
    C: Send + 'static,
{
    result.extract_surface_flags();

    if result.flags.contains(DirtyFlags::ALL) {
        ctx.grid.resize(ctx.state.cols, ctx.state.rows);
        ctx.backend.invalidate();
    }
    *ctx.dirty |= result.flags;
    let active_session = ctx.session_manager.active_session_id();
    if *ctx.scroll_runtime_session != active_session {
        ctx.scroll_runtime.advance_generation();
        ctx.scroll_runtime.suspend();
        *ctx.scroll_runtime_session = active_session;
    }
    ctx.scroll_runtime
        .set_initial_resize_complete(*ctx.initial_resize_sent);

    // Suppress commands to Kakoune until initialization is complete.
    if is_input && !*ctx.initial_resize_sent {
        ctx.session_states
            .sync_active_from_manager(ctx.session_manager, ctx.state);
        return false;
    }

    for plan in result.scroll_plans {
        ctx.scroll_runtime.enqueue(plan);
    }

    let should_quit = {
        with_deferred_context(ctx, |deferred_ctx| {
            if handle_command_batch(
                result.commands,
                deferred_ctx,
                result.command_source.as_ref(),
            ) {
                return true;
            }
            handle_sourced_surface_commands(result.surface_commands, deferred_ctx)
        })
    };
    if !should_quit {
        if result.workspace_changed {
            notify_workspace_observers(ctx.registry, ctx.surface_registry, ctx.state);
            // Layout save is handled in with_deferred_context (command-driven
            // changes only).  The workspace_changed flag here can also be set by
            // InputEvent::Resize which is NOT a structural change; saving on
            // resize would overwrite the layout with a degraded workspace when a
            // pane has died via :q.
        }
        sync_ready_gate(ctx.session_ready_gate, ctx.state);
        if !*ctx.initial_resize_sent {
            ctx.session_ready_gate.clear_initial_resize();
        }
        ctx.session_states
            .sync_active_from_manager(ctx.session_manager, ctx.state);
    }
    should_quit
}

fn flush_reloaded_plugins_ready<R, W, C>(
    ctx: &mut EventProcessingContext<'_, R, W, C>,
    reloaded_plugins: &[PluginId],
) -> bool
where
    R: std::io::BufRead + Send + 'static,
    W: Write + Send + 'static,
    C: Send + 'static,
{
    with_deferred_context(ctx, |deferred_ctx| {
        for plugin_id in reloaded_plugins {
            let batch = deferred_ctx
                .registry
                .notify_plugin_active_session_ready_batch(
                    plugin_id,
                    &AppView::new(deferred_ctx.state),
                );
            if apply_ready_batch(batch, deferred_ctx) {
                return true;
            }
        }
        false
    })
}

fn flush_active_session_ready_if_needed<R, W, C>(
    ctx: &mut EventProcessingContext<'_, R, W, C>,
) -> bool
where
    R: std::io::BufRead + Send + 'static,
    W: Write + Send + 'static,
    C: Send + 'static,
{
    with_deferred_context(ctx, maybe_flush_active_session_ready)
}

fn with_deferred_context<R, W, C, T>(
    ctx: &mut EventProcessingContext<'_, R, W, C>,
    f: impl FnOnce(&mut DeferredContext<'_>) -> T,
) -> T
where
    R: std::io::BufRead + Send + 'static,
    W: Write + Send + 'static,
    C: Send + 'static,
{
    let mut session_host = kasane_core::event_loop::SharedSessionRuntime {
        session_manager: ctx.session_manager,
        session_states: ctx.session_states,
        sink: TuiEventSink(ctx.session_tx.clone()),
        spawn_session: ctx.spawn_session,
    };
    let scroll_runtime = &mut *ctx.scroll_runtime;
    let mut workspace_changed = false;
    let result = {
        let mut deferred_ctx = DeferredContext {
            state: ctx.state,
            registry: ctx.registry,
            surface_registry: ctx.surface_registry,
            clipboard: ctx.clipboard,
            dirty: ctx.dirty,
            timer: ctx.timer,
            session_host: &mut session_host,
            initial_resize_sent: ctx.initial_resize_sent,
            session_ready_gate: Some(&mut *ctx.session_ready_gate),
            scroll_plan_sink: &mut |plan| scroll_runtime.enqueue(plan),
            process_dispatcher: ctx.process_dispatcher,
            workspace_changed: &mut workspace_changed,
            scroll_amount: ctx.scroll_amount,
        };
        f(&mut deferred_ctx)
    };
    if workspace_changed {
        notify_workspace_observers(ctx.registry, ctx.surface_registry, ctx.state);
        // Save layout on structural changes
        if let Some(server_name) = ctx.surface_registry.server_session_name() {
            kasane_core::workspace::persist::save_layout(
                server_name,
                ctx.surface_registry.workspace(),
                ctx.surface_registry,
                ctx.session_states,
                ctx.state,
                ctx.session_manager.active_session_id(),
            );
        }
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use kasane_core::event_loop::{register_builtin_surfaces, setup_plugin_surfaces};
    use kasane_core::layout::SplitDirection;
    use kasane_core::plugin::{
        AppView, PluginBackend, PluginCapabilities, PluginDescriptor, PluginRank, PluginRevision,
        PluginSource,
    };
    use kasane_core::surface::{Surface, SurfaceId};
    use kasane_core::test_support::TestSurfaceBuilder;
    use kasane_core::workspace::Placement;

    struct ReloadResourcePlugin {
        surface_id: SurfaceId,
    }

    impl PluginBackend for ReloadResourcePlugin {
        fn id(&self) -> PluginId {
            PluginId("reload-owner".to_string())
        }

        fn capabilities(&self) -> PluginCapabilities {
            PluginCapabilities::SURFACE_PROVIDER
        }

        fn surfaces(&mut self) -> Vec<Box<dyn Surface>> {
            vec![TestSurfaceBuilder::new(self.surface_id).build()]
        }

        fn workspace_request(&self) -> Option<Placement> {
            Some(Placement::SplitFocused {
                direction: SplitDirection::Vertical,
                ratio: 0.4,
            })
        }
    }

    fn owner_delta(
        old: Option<&str>,
        new: Option<&str>,
    ) -> kasane_core::plugin::AppliedWinnerDelta {
        fn descriptor(revision: &str) -> PluginDescriptor {
            PluginDescriptor {
                id: PluginId("reload-owner".to_string()),
                source: PluginSource::Host {
                    provider: "test".to_string(),
                },
                revision: PluginRevision(revision.to_string()),
                rank: PluginRank::HOST,
            }
        }

        kasane_core::plugin::AppliedWinnerDelta {
            id: PluginId("reload-owner".to_string()),
            old: old.map(descriptor),
            new: new.map(descriptor),
        }
    }

    #[test]
    fn reconcile_reloaded_plugin_resources_replaces_owner_surfaces() {
        let state = AppState::default();
        let mut registry = PluginRuntime::new();
        registry.register_backend(Box::new(ReloadResourcePlugin {
            surface_id: SurfaceId(200),
        }));

        let mut surface_registry = SurfaceRegistry::new();
        register_builtin_surfaces(&mut surface_registry);
        let disabled = setup_plugin_surfaces(&mut registry, &mut surface_registry, &state);
        assert!(disabled.is_empty());

        let _ = registry.reload_plugin_batch(
            Box::new(ReloadResourcePlugin {
                surface_id: SurfaceId(200),
            }),
            &AppView::new(&state),
        );

        let disabled = reconcile_reloaded_plugin_resources(
            &mut registry,
            &mut surface_registry,
            &state,
            &[owner_delta(Some("r1"), Some("r2"))],
        );

        assert!(disabled.is_empty());
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
    fn reconcile_reloaded_plugin_resources_removes_owner_surfaces() {
        let state = AppState::default();
        let mut registry = PluginRuntime::new();
        registry.register_backend(Box::new(ReloadResourcePlugin {
            surface_id: SurfaceId(200),
        }));

        let mut surface_registry = SurfaceRegistry::new();
        register_builtin_surfaces(&mut surface_registry);
        let disabled = setup_plugin_surfaces(&mut registry, &mut surface_registry, &state);
        assert!(disabled.is_empty());

        assert!(registry.unload_plugin(&PluginId("reload-owner".to_string())));

        let disabled = reconcile_reloaded_plugin_resources(
            &mut registry,
            &mut surface_registry,
            &state,
            &[owner_delta(Some("r1"), None)],
        );

        assert!(disabled.is_empty());
        assert!(surface_registry.get(SurfaceId(200)).is_none());
        assert!(!surface_registry.workspace_contains(SurfaceId(200)));
    }
}
