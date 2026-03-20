//! TUI event loop: polls crossterm and Kakoune, dispatches to core update/view/render.

use std::collections::BTreeMap;
use std::io::Write;

use anyhow::Result;

use kasane_core::event_loop::{
    DeferredContext, EventResult, SessionReadyGate, TimerScheduler, apply_bootstrap_effects,
    apply_ready_batch, handle_command_batch, handle_sourced_surface_commands,
    handle_workspace_divider_input, maybe_flush_active_session_ready, reconcile_plugin_surfaces,
    surface_event_from_input, sync_session_ready_gate as sync_ready_gate,
};
use kasane_core::input::InputEvent;
use kasane_core::layout::Rect;
use kasane_core::plugin::{
    IoEvent, PaintHook, PluginDiagnostic, PluginId, PluginManager, PluginRegistry,
    ProcessDispatcher, ProcessEvent, ProcessEventSink, RuntimeBatch, extract_redraw_flags,
};
use kasane_core::protocol::KakouneRequest;
use kasane_core::render::{CellGrid, RenderBackend};
use kasane_core::scroll::ScrollRuntime;
use kasane_core::session::{SessionId, SessionManager, SessionSpec, SessionStateStore};
use kasane_core::state::{AppState, DirtyFlags, Msg, UpdateResult, update};
use kasane_core::surface::SurfaceRegistry;

use crate::backend::TuiBackend;

#[derive(Default)]
pub(crate) struct PaintHookState {
    hooks: Vec<Box<dyn PaintHook>>,
    owner_ranges: BTreeMap<PluginId, std::ops::Range<usize>>,
}

impl PaintHookState {
    pub(crate) fn from_registry(registry: &PluginRegistry) -> Self {
        let mut state = Self::default();
        state.rebuild_from_grouped(
            registry,
            registry
                .paint_hook_owners_in_order()
                .into_iter()
                .map(|owner| {
                    (
                        owner.clone(),
                        registry.collect_paint_hooks_for_owner(&owner),
                    )
                })
                .collect(),
        );
        state
    }

    pub(crate) fn hooks(&self) -> &[Box<dyn PaintHook>] {
        &self.hooks
    }

    pub(crate) fn reconcile(
        &mut self,
        registry: &PluginRegistry,
        deltas: &[kasane_core::plugin::AppliedWinnerDelta],
        diagnostics: &[PluginDiagnostic],
    ) {
        if deltas.is_empty() && diagnostics.is_empty() {
            return;
        }

        let mut grouped = self.take_grouped();
        let mut changed_owners = BTreeMap::<PluginId, ()>::new();
        for delta in deltas {
            changed_owners.insert(delta.id.clone(), ());
        }
        for diagnostic in diagnostics {
            changed_owners.insert(diagnostic.plugin_id.clone(), ());
        }

        for plugin_id in changed_owners.keys() {
            grouped.remove(plugin_id);
        }
        for plugin_id in changed_owners.keys() {
            if diagnostics.iter().any(|d| d.plugin_id == *plugin_id)
                || !registry.contains_plugin(plugin_id)
            {
                continue;
            }
            let hooks = registry.collect_paint_hooks_for_owner(plugin_id);
            if !hooks.is_empty() {
                grouped.insert(plugin_id.clone(), hooks);
            }
        }

        self.rebuild_from_grouped(registry, grouped);
    }

    fn take_grouped(&mut self) -> BTreeMap<PluginId, Vec<Box<dyn PaintHook>>> {
        let old_hooks = std::mem::take(&mut self.hooks);
        let old_ranges = std::mem::take(&mut self.owner_ranges);
        let mut entries: Vec<_> = old_ranges.into_iter().collect();
        entries.sort_by_key(|(_, range)| range.start);
        let mut hooks_iter = old_hooks.into_iter();
        let mut grouped = BTreeMap::new();
        for (owner, range) in entries {
            let len = range.end.saturating_sub(range.start);
            grouped.insert(owner, hooks_iter.by_ref().take(len).collect());
        }
        grouped
    }

    fn rebuild_from_grouped(
        &mut self,
        registry: &PluginRegistry,
        mut grouped: BTreeMap<PluginId, Vec<Box<dyn PaintHook>>>,
    ) {
        let mut hooks = Vec::new();
        let mut owner_ranges = BTreeMap::new();
        for owner in registry.paint_hook_owners_in_order() {
            let Some(owner_hooks) = grouped.remove(&owner) else {
                continue;
            };
            let start = hooks.len();
            hooks.extend(owner_hooks);
            owner_ranges.insert(owner, start..hooks.len());
        }
        self.hooks = hooks;
        self.owner_ranges = owner_ranges;
    }
}

pub(crate) enum Event {
    Kakoune(SessionId, KakouneRequest),
    Input(InputEvent),
    KakouneDied(SessionId),
    PluginTimer(PluginId, Box<dyn std::any::Any + Send>),
    ProcessOutput(PluginId, IoEvent),
    PluginReload,
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

/// TimerScheduler that injects timer events into the TUI event channel.
pub(crate) struct TuiTimerScheduler(pub(crate) crossbeam_channel::Sender<Event>);

impl TimerScheduler for TuiTimerScheduler {
    fn schedule_timer(
        &self,
        delay: std::time::Duration,
        target: PluginId,
        payload: Box<dyn std::any::Any + Send>,
    ) {
        let tx = self.0.clone();
        std::thread::spawn(move || {
            std::thread::sleep(delay);
            let _ = tx.send(Event::PluginTimer(target, payload));
        });
    }
}

pub(crate) fn spawn_session_reader<R>(
    session_id: SessionId,
    reader: R,
    tx: crossbeam_channel::Sender<Event>,
) where
    R: std::io::BufRead + Send + 'static,
{
    let died_tx = tx.clone();
    kasane_core::io::spawn_kak_reader(
        reader,
        move |req| {
            let _ = tx.send(Event::Kakoune(session_id, req));
        },
        move || {
            let _ = died_tx.send(Event::KakouneDied(session_id));
        },
    );
}

pub(crate) struct TuiSessionRuntime<'a, R, W, C>
where
    R: std::io::BufRead + Send + 'static,
    W: Write + Send + 'static,
    C: Send + 'static,
{
    pub(crate) session_manager: &'a mut SessionManager<R, W, C>,
    pub(crate) session_states: &'a mut SessionStateStore,
    pub(crate) tx: crossbeam_channel::Sender<Event>,
    pub(crate) spawn_session: fn(&SessionSpec) -> Result<(R, W, C)>,
}

impl<'a, R, W, C> kasane_core::event_loop::SessionRuntime for TuiSessionRuntime<'a, R, W, C>
where
    R: std::io::BufRead + Send + 'static,
    W: Write + Send + 'static,
    C: Send + 'static,
{
    fn spawn_session(
        &mut self,
        spec: SessionSpec,
        activate: bool,
        state: &mut AppState,
        dirty: &mut DirtyFlags,
        initial_resize_sent: &mut bool,
    ) {
        if let Some((session_id, reader)) = kasane_core::event_loop::spawn_session_core(
            &spec,
            activate,
            self.session_manager,
            self.session_states,
            state,
            dirty,
            initial_resize_sent,
            self.spawn_session,
        ) {
            spawn_session_reader(session_id, reader, self.tx.clone());
        }
    }

    fn close_session(
        &mut self,
        key: Option<&str>,
        state: &mut AppState,
        dirty: &mut DirtyFlags,
        initial_resize_sent: &mut bool,
    ) -> bool {
        kasane_core::event_loop::close_session_core(
            key,
            self.session_manager,
            self.session_states,
            state,
            dirty,
            initial_resize_sent,
        )
    }

    fn switch_session(
        &mut self,
        key: &str,
        state: &mut AppState,
        dirty: &mut DirtyFlags,
        initial_resize_sent: &mut bool,
    ) {
        kasane_core::event_loop::switch_session_core(
            key,
            self.session_manager,
            self.session_states,
            state,
            dirty,
            initial_resize_sent,
        );
    }
}

impl<'a, R, W, C> kasane_core::event_loop::SessionHost for TuiSessionRuntime<'a, R, W, C>
where
    R: std::io::BufRead + Send + 'static,
    W: Write + Send + 'static,
    C: Send + 'static,
{
    fn active_writer(&mut self) -> &mut dyn Write {
        self.session_manager
            .active_writer_mut()
            .expect("missing active session writer")
    }
}

/// Grouped mutable context for `process_event`, reducing its parameter count.
pub(crate) struct EventProcessingContext<'a, R, W, C> {
    pub state: &'a mut AppState,
    pub registry: &'a mut PluginRegistry,
    pub surface_registry: &'a mut SurfaceRegistry,
    pub session_manager: &'a mut SessionManager<R, W, C>,
    pub session_states: &'a mut SessionStateStore,
    pub session_tx: &'a crossbeam_channel::Sender<Event>,
    pub spawn_session: fn(&SessionSpec) -> Result<(R, W, C)>,
    pub grid: &'a mut CellGrid,
    pub scroll_amount: i32,
    pub backend: &'a mut TuiBackend,
    pub initial_resize_sent: &'a mut bool,
    pub dirty: &'a mut DirtyFlags,
    pub timer: &'a TuiTimerScheduler,
    pub scroll_runtime: &'a mut ScrollRuntime,
    pub scroll_runtime_session: &'a mut Option<SessionId>,
    pub session_ready_gate: &'a mut SessionReadyGate,
    pub process_dispatcher: &'a mut dyn ProcessDispatcher,
    pub plugin_manager: &'a mut PluginManager,
    pub paint_hooks: &'a mut PaintHookState,
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
            let UpdateResult {
                flags,
                commands,
                scroll_plans,
                source_plugin: _source,
            } = update(
                ctx.state,
                Msg::Kakoune(req),
                ctx.registry,
                ctx.scroll_amount,
            );
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
            }
        }
        Event::Input(ref input_event) => {
            tracing::trace!(?input_event, "process_event: Input");
            let input_event = if let Event::Input(ie) = event {
                ie
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
                }
            } else {
                let surface_event = surface_event_from_input(&input_event);
                let UpdateResult {
                    flags,
                    commands,
                    scroll_plans,
                    source_plugin,
                } = update(
                    ctx.state,
                    Msg::from(input_event),
                    ctx.registry,
                    ctx.scroll_amount,
                );
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
                }
            }
        }
        Event::PluginTimer(target, payload) => event_result_from_runtime_batch(
            ctx.registry
                .deliver_message_batch(&target, payload, ctx.state),
            Some(target),
        ),
        Event::ProcessOutput(plugin_id, io_event) => {
            let batch = ctx
                .registry
                .deliver_io_event_batch(&plugin_id, &io_event, ctx.state);
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
            }
        }
        Event::KakouneDied(session_id) => {
            if kasane_core::event_loop::handle_session_death(
                session_id,
                ctx.session_manager,
                ctx.session_states,
                ctx.state,
                ctx.dirty,
                ctx.initial_resize_sent,
            ) {
                return true;
            }
            // handle_session_death may have reset initial_resize_sent when
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
            let batch = ctx
                .registry
                .notify_state_changed_batch(ctx.state, DirtyFlags::SESSION);
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
    let reload = ctx
        .plugin_manager
        .reload(ctx.registry, ctx.state, |result, registry| {
            reconcile_reloaded_plugin_resources(
                registry,
                ctx.surface_registry,
                ctx.state,
                ctx.paint_hooks,
                result.deltas.as_slice(),
            )
        })?;

    let mut flags = DirtyFlags::all();
    apply_bootstrap_effects(reload.bootstrap, &mut flags);
    sync_ready_gate(ctx.session_ready_gate, ctx.state);

    let ready_targets = reload.ready_targets().cloned().collect::<Vec<_>>();
    let should_quit = *ctx.initial_resize_sent && flush_reloaded_plugins_ready(ctx, &ready_targets);
    tracing::info!("hot-reloaded plugins");

    Ok(PluginReloadOutcome { flags, should_quit })
}

fn reconcile_reloaded_plugin_resources(
    registry: &mut PluginRegistry,
    surface_registry: &mut SurfaceRegistry,
    state: &AppState,
    paint_hooks: &mut PaintHookState,
    deltas: &[kasane_core::plugin::AppliedWinnerDelta],
) -> Vec<PluginDiagnostic> {
    if deltas.is_empty() {
        return vec![];
    }

    let diagnostics = reconcile_plugin_surfaces(registry, surface_registry, state, deltas);
    paint_hooks.reconcile(registry, deltas, &diagnostics);
    diagnostics
}

fn event_result_from_runtime_batch(
    mut batch: RuntimeBatch,
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
        ctx.grid.invalidate_all();
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
                .notify_plugin_active_session_ready_batch(plugin_id, deferred_ctx.state);
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
    let mut session_host = TuiSessionRuntime {
        session_manager: ctx.session_manager,
        session_states: ctx.session_states,
        tx: ctx.session_tx.clone(),
        spawn_session: ctx.spawn_session,
    };
    let scroll_runtime = &mut *ctx.scroll_runtime;
    let mut deferred_ctx = DeferredContext {
        state: ctx.state,
        registry: ctx.registry,
        surface_registry: ctx.surface_registry,
        clipboard_get: &mut || ctx.backend.clipboard_get(),
        dirty: ctx.dirty,
        timer: ctx.timer,
        session_host: &mut session_host,
        initial_resize_sent: ctx.initial_resize_sent,
        session_ready_gate: Some(&mut *ctx.session_ready_gate),
        scroll_plan_sink: &mut |plan| scroll_runtime.enqueue(plan),
        process_dispatcher: ctx.process_dispatcher,
    };
    f(&mut deferred_ctx)
}

#[cfg(test)]
mod tests {
    use super::*;
    use kasane_core::event_loop::{register_builtin_surfaces, setup_plugin_surfaces};
    use kasane_core::layout::SplitDirection;
    use kasane_core::plugin::{
        PaintHook, PluginBackend, PluginCapabilities, PluginDescriptor, PluginRank, PluginRevision,
        PluginSource,
    };
    use kasane_core::surface::{Surface, SurfaceId};
    use kasane_core::test_support::TestSurfaceBuilder;
    use kasane_core::workspace::Placement;

    struct TestPaintHook {
        id: &'static str,
    }

    impl PaintHook for TestPaintHook {
        fn id(&self) -> &str {
            self.id
        }

        fn deps(&self) -> DirtyFlags {
            DirtyFlags::ALL
        }

        fn apply(
            &self,
            _grid: &mut kasane_core::render::CellGrid,
            _region: &kasane_core::layout::Rect,
            _state: &AppState,
        ) {
        }
    }

    struct ReloadResourcePlugin {
        surface_id: SurfaceId,
        hook_id: &'static str,
    }

    impl PluginBackend for ReloadResourcePlugin {
        fn id(&self) -> PluginId {
            PluginId("reload-owner".to_string())
        }

        fn capabilities(&self) -> PluginCapabilities {
            PluginCapabilities::PAINT_HOOK
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

        fn paint_hooks(&self) -> Vec<Box<dyn PaintHook>> {
            vec![Box::new(TestPaintHook { id: self.hook_id })]
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
    fn reconcile_reloaded_plugin_resources_replaces_owner_surfaces_and_hooks() {
        let state = AppState::default();
        let mut registry = PluginRegistry::new();
        registry.register_backend(Box::new(ReloadResourcePlugin {
            surface_id: SurfaceId(200),
            hook_id: "hook-a",
        }));

        let mut surface_registry = SurfaceRegistry::new();
        register_builtin_surfaces(&mut surface_registry);
        let disabled = setup_plugin_surfaces(&mut registry, &mut surface_registry, &state);
        assert!(disabled.is_empty());

        let mut paint_hooks = PaintHookState::from_registry(&registry);
        assert_eq!(paint_hooks.hooks().len(), 1);
        assert_eq!(paint_hooks.hooks()[0].id(), "hook-a");

        let _ = registry.reload_plugin_batch(
            Box::new(ReloadResourcePlugin {
                surface_id: SurfaceId(200),
                hook_id: "hook-b",
            }),
            &state,
        );

        let disabled = reconcile_reloaded_plugin_resources(
            &mut registry,
            &mut surface_registry,
            &state,
            &mut paint_hooks,
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
        assert_eq!(paint_hooks.hooks().len(), 1);
        assert_eq!(paint_hooks.hooks()[0].id(), "hook-b");
    }

    #[test]
    fn reconcile_reloaded_plugin_resources_removes_owner_surfaces_and_hooks() {
        let state = AppState::default();
        let mut registry = PluginRegistry::new();
        registry.register_backend(Box::new(ReloadResourcePlugin {
            surface_id: SurfaceId(200),
            hook_id: "hook-a",
        }));

        let mut surface_registry = SurfaceRegistry::new();
        register_builtin_surfaces(&mut surface_registry);
        let disabled = setup_plugin_surfaces(&mut registry, &mut surface_registry, &state);
        assert!(disabled.is_empty());

        let mut paint_hooks = PaintHookState::from_registry(&registry);
        assert_eq!(paint_hooks.hooks().len(), 1);
        assert!(registry.unload_plugin(&PluginId("reload-owner".to_string())));

        let disabled = reconcile_reloaded_plugin_resources(
            &mut registry,
            &mut surface_registry,
            &state,
            &mut paint_hooks,
            &[owner_delta(Some("r1"), None)],
        );

        assert!(disabled.is_empty());
        assert!(surface_registry.get(SurfaceId(200)).is_none());
        assert!(!surface_registry.workspace_contains(SurfaceId(200)));
        assert!(paint_hooks.hooks().is_empty());
    }
}
