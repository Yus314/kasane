use std::io::Write;

use anyhow::Result;

use kasane_core::event_loop::{
    DeferredContext, EventResult, TimerScheduler, handle_deferred_commands,
    handle_sourced_surface_commands, handle_workspace_divider_input, surface_event_from_input,
};
use kasane_core::input::InputEvent;
use kasane_core::layout::Rect;
use kasane_core::plugin::{
    CommandResult, IoEvent, PluginId, PluginRegistry, ProcessDispatcher, ProcessEvent,
    ProcessEventSink, execute_commands, extract_deferred_commands,
};
use kasane_core::protocol::KakouneRequest;
use kasane_core::render::{CellGrid, RenderBackend};
use kasane_core::session::{SessionId, SessionManager, SessionSpec, SessionStateStore};
use kasane_core::state::{AppState, DirtyFlags, Msg, update};
use kasane_core::surface::SurfaceRegistry;

use crate::backend::TuiBackend;

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
    pub process_dispatcher: &'a mut dyn ProcessDispatcher,
    pub plugin_reloader: &'a Option<crate::PluginReloader>,
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
            let (f, c, _source) = update(
                ctx.state,
                Msg::Kakoune(req),
                ctx.registry,
                ctx.scroll_amount,
            );
            let surface_commands = if f.is_empty() {
                vec![]
            } else {
                ctx.surface_registry
                    .on_state_changed_with_sources(ctx.state, f)
            };
            EventResult {
                flags: f,
                commands: c,
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
                    surface_commands: vec![],
                    command_source: None,
                }
            } else {
                let surface_event = surface_event_from_input(&input_event);
                let (f, c, source) = update(
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
                    flags: f,
                    commands: c,
                    surface_commands,
                    command_source: source,
                }
            }
        }
        Event::PluginTimer(target, payload) => {
            let (flags, commands) = ctx.registry.deliver_message(&target, payload, ctx.state);
            EventResult {
                flags,
                commands,
                surface_commands: vec![],
                command_source: None,
            }
        }
        Event::ProcessOutput(plugin_id, io_event) => {
            let (flags, commands) = ctx
                .registry
                .deliver_io_event(&plugin_id, &io_event, ctx.state);
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
            EventResult {
                flags,
                commands,
                surface_commands: vec![],
                command_source: Some(plugin_id),
            }
        }
        Event::PluginReload => {
            // Reload plugins from disk — triggered by `.reload` sentinel
            if let Some(reloader) = ctx.plugin_reloader.as_ref() {
                reloader(ctx.registry, ctx.state);
                tracing::info!("hot-reloaded plugins");
            }
            EventResult {
                flags: DirtyFlags::all(),
                commands: vec![],
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
            }
            // Notify plugins of session change so cached state is updated.
            for plugin in ctx.registry.plugins_mut() {
                plugin.on_state_changed(ctx.state, DirtyFlags::SESSION);
            }
            return false;
        }
    };

    process_event_result(result, is_input, ctx)
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

    // Suppress commands to Kakoune until initialization is complete.
    if is_input && !*ctx.initial_resize_sent {
        ctx.session_states
            .sync_active_from_manager(ctx.session_manager, ctx.state);
        return false;
    }

    let (normal, deferred) = extract_deferred_commands(result.commands);
    if matches!(
        execute_commands(
            normal,
            ctx.session_manager
                .active_writer_mut()
                .expect("missing active session writer"),
            &mut || ctx.backend.clipboard_get(),
        ),
        CommandResult::Quit
    ) {
        return true;
    }
    let should_quit = {
        let mut session_host = TuiSessionRuntime {
            session_manager: ctx.session_manager,
            session_states: ctx.session_states,
            tx: ctx.session_tx.clone(),
            spawn_session: ctx.spawn_session,
        };
        let mut deferred_ctx = DeferredContext {
            state: ctx.state,
            registry: ctx.registry,
            surface_registry: ctx.surface_registry,
            clipboard_get: &mut || ctx.backend.clipboard_get(),
            dirty: ctx.dirty,
            timer: ctx.timer,
            session_host: &mut session_host,
            initial_resize_sent: ctx.initial_resize_sent,
            process_dispatcher: ctx.process_dispatcher,
        };
        if handle_deferred_commands(deferred, &mut deferred_ctx, result.command_source.as_ref()) {
            return true;
        }
        handle_sourced_surface_commands(result.surface_commands, &mut deferred_ctx)
    };
    if !should_quit {
        ctx.session_states
            .sync_active_from_manager(ctx.session_manager, ctx.state);
    }
    should_quit
}
