use std::io::Write;

use anyhow::Result;

use kasane_core::event_loop::{
    DeferredContext, TimerScheduler, handle_deferred_commands, handle_sourced_surface_commands,
    handle_workspace_divider_input,
};
use kasane_core::input::InputEvent;
use kasane_core::layout::Rect;
use kasane_core::plugin::{
    CommandResult, IoEvent, PluginId, PluginRegistry, ProcessDispatcher, ProcessEvent,
    ProcessEventSink, execute_commands, extract_deferred_commands, extract_redraw_flags,
};
use kasane_core::protocol::KakouneRequest;
use kasane_core::render::{CellGrid, RenderBackend};
use kasane_core::session::{SessionId, SessionManager, SessionSpec, SessionStateStore};
use kasane_core::state::{AppState, DirtyFlags, Msg, update};
use kasane_core::surface::{SurfaceEvent, SurfaceRegistry};

use crate::backend::TuiBackend;

pub(crate) enum Event {
    Kakoune(SessionId, KakouneRequest),
    Input(InputEvent),
    KakouneDied(SessionId),
    PluginTimer(PluginId, Box<dyn std::any::Any + Send>),
    ProcessOutput(PluginId, IoEvent),
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

pub(crate) fn surface_event_from_input(input: &InputEvent) -> Option<SurfaceEvent> {
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
        let Ok((reader, writer, child)) = (self.spawn_session)(&spec) else {
            tracing::error!("failed to spawn session {}", spec.key);
            return;
        };
        let Ok(session_id) = self.session_manager.insert(spec, reader, writer, child) else {
            tracing::error!("failed to register spawned session");
            return;
        };
        self.session_states.ensure_session(session_id, state);
        let reader = self
            .session_manager
            .take_reader(session_id)
            .expect("spawned session reader missing");
        spawn_session_reader(session_id, reader, self.tx.clone());
        if activate {
            self.session_manager
                .sync_and_activate(self.session_states, session_id, state)
                .expect("spawned session must be activeable");
            if !self.session_states.restore_into(session_id, state) {
                state.reset_for_session_switch();
            }
            *dirty |= DirtyFlags::ALL;
            *initial_resize_sent = false;
        }
    }

    fn close_session(
        &mut self,
        key: Option<&str>,
        state: &mut AppState,
        dirty: &mut DirtyFlags,
        initial_resize_sent: &mut bool,
    ) -> bool {
        let target = key
            .and_then(|k| self.session_manager.session_id_by_key(k))
            .or_else(|| self.session_manager.active_session_id());
        let Some(target) = target else {
            return false;
        };
        let was_active = self.session_manager.active_session_id() == Some(target);
        let _ = self.session_manager.close(target);
        self.session_states.remove(target);
        if self.session_manager.is_empty() {
            return true;
        }
        if was_active {
            let restored = self
                .session_manager
                .active_session_id()
                .is_some_and(|active| self.session_states.restore_into(active, state));
            if !restored {
                state.reset_for_session_switch();
            }
            *dirty |= DirtyFlags::ALL;
            *initial_resize_sent = false;
        }
        false
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

/// Process a single event, returning `true` if the application should quit.
#[allow(clippy::too_many_arguments)]
pub(crate) fn process_event<R, W, C>(
    event: Event,
    state: &mut AppState,
    registry: &mut PluginRegistry,
    surface_registry: &mut SurfaceRegistry,
    session_manager: &mut SessionManager<R, W, C>,
    session_states: &mut SessionStateStore,
    session_tx: &crossbeam_channel::Sender<Event>,
    spawn_session: fn(&SessionSpec) -> Result<(R, W, C)>,
    grid: &mut CellGrid,
    scroll_amount: i32,
    backend: &mut TuiBackend,
    initial_resize_sent: &mut bool,
    dirty: &mut DirtyFlags,
    timer: &TuiTimerScheduler,
    process_dispatcher: &mut dyn ProcessDispatcher,
) -> bool
where
    R: std::io::BufRead + Send + 'static,
    W: Write + Send + 'static,
    C: Send + 'static,
{
    let is_kakoune = matches!(&event, Event::Kakoune(..));
    let is_input = matches!(&event, Event::Input(_));

    let command_source_plugin;
    let (mut flags, commands, mut surface_command_groups) = match event {
        Event::Kakoune(session_id, req) => {
            if session_manager.active_session_id() != Some(session_id) {
                session_states.ensure_session(session_id, state).apply(req);
                return false;
            }
            kasane_core::io::send_initial_resize(
                session_manager
                    .active_writer_mut()
                    .expect("missing active session writer"),
                initial_resize_sent,
                state.rows,
                state.cols,
            );
            let (f, c, _source) = update(state, Msg::Kakoune(req), registry, scroll_amount);
            let surface_command_groups = if f.is_empty() {
                vec![]
            } else {
                surface_registry.on_state_changed_with_sources(state, f)
            };
            command_source_plugin = None;
            (f, c, surface_command_groups)
        }
        Event::Input(input_event) => {
            let total = Rect {
                x: 0,
                y: 0,
                w: state.cols,
                h: state.rows,
            };
            if let Some(divider_dirty) =
                handle_workspace_divider_input(&input_event, surface_registry, total)
            {
                command_source_plugin = None;
                (divider_dirty, vec![], vec![])
            } else {
                let surface_event = surface_event_from_input(&input_event);
                let (f, c, source) = update(state, Msg::from(input_event), registry, scroll_amount);
                command_source_plugin = source;
                let surface_command_groups = surface_event
                    .map(|event| surface_registry.route_event_with_sources(event, state, total))
                    .unwrap_or_default();
                (f, c, surface_command_groups)
            }
        }
        Event::PluginTimer(target, payload) => {
            command_source_plugin = None;
            let (flags, commands) = registry.deliver_message(&target, payload, state);
            (flags, commands, vec![])
        }
        Event::ProcessOutput(plugin_id, io_event) => {
            command_source_plugin = Some(plugin_id.clone());
            let (flags, commands) = registry.deliver_io_event(&plugin_id, &io_event, state);
            // Free per-plugin process count slot when a job finishes
            let IoEvent::Process(ref pe) = io_event;
            let finished_job = match pe {
                ProcessEvent::Exited { job_id, .. } | ProcessEvent::SpawnFailed { job_id, .. } => {
                    Some(*job_id)
                }
                _ => None,
            };
            if let Some(job_id) = finished_job {
                process_dispatcher.remove_finished_job(&plugin_id, job_id);
            }
            (flags, commands, vec![])
        }
        Event::KakouneDied(session_id) => {
            let was_active = session_manager.active_session_id() == Some(session_id);
            let _ = session_manager.close(session_id);
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
            return false;
        }
    };
    for entry in &mut surface_command_groups {
        flags |= extract_redraw_flags(&mut entry.commands);
    }

    if flags.contains(DirtyFlags::ALL) {
        grid.resize(state.cols, state.rows);
        grid.invalidate_all();
    }
    *dirty |= flags;

    // Suppress commands to Kakoune until initialization is complete.
    if is_input && !*initial_resize_sent {
        session_states.sync_active_from_manager(session_manager, state);
        return false;
    }
    // Kakoune events before initial resize: execute commands (they come from
    // init_all) but don't send anything to Kakoune yet (handled by the
    // send_initial_resize guard above).
    let _ = is_kakoune; // used only for the send_initial_resize call above

    let (normal, deferred) = extract_deferred_commands(commands);
    if matches!(
        execute_commands(
            normal,
            session_manager
                .active_writer_mut()
                .expect("missing active session writer"),
            &mut || backend.clipboard_get(),
        ),
        CommandResult::Quit
    ) {
        return true;
    }
    let should_quit = {
        let mut session_host = TuiSessionRuntime {
            session_manager,
            session_states,
            tx: session_tx.clone(),
            spawn_session,
        };
        let mut ctx = DeferredContext {
            state,
            registry,
            surface_registry,
            clipboard_get: &mut || backend.clipboard_get(),
            dirty,
            timer,
            session_host: &mut session_host,
            initial_resize_sent,
            process_dispatcher,
        };
        if handle_deferred_commands(deferred, &mut ctx, command_source_plugin.as_ref()) {
            return true;
        }
        handle_sourced_surface_commands(surface_command_groups, &mut ctx)
    };
    if !should_quit {
        session_states.sync_active_from_manager(session_manager, state);
    }
    should_quit
}
