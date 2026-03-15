mod backend;
mod input;

use std::io::Write;
use std::sync::Arc;

use anyhow::{Result, anyhow};
use crossbeam_channel::unbounded;

use kasane_core::config::Config;
use kasane_core::event_loop::{
    TimerScheduler, handle_deferred_commands, handle_sourced_surface_commands,
    handle_workspace_divider_input,
};
use kasane_core::input::InputEvent;
use kasane_core::layout::{Rect, build_hit_map};
use kasane_core::plugin::{
    CommandResult, IoEvent, PluginId, PluginRegistry, ProcessDispatcher, ProcessEvent,
    ProcessEventSink, execute_commands, extract_deferred_commands, extract_redraw_flags,
};
use kasane_core::protocol::KakouneRequest;
use kasane_core::render::view::surface_view_sections_cached;
use kasane_core::render::{
    CellGrid, CursorPatch, LayoutCache, MenuSelectionPatch, RenderBackend, StatusBarPatch,
    ViewCache, render_pipeline_surfaces_patched,
};
use kasane_core::session::{SessionId, SessionManager, SessionSpec, SessionStateStore};
use kasane_core::state::{AppState, DirtyFlags, Msg, tick_scroll_animation, update};
use kasane_core::surface::buffer::KakouneBufferSurface;
use kasane_core::surface::{SurfaceEvent, SurfaceRegistry};

use backend::TuiBackend;
use input::convert_event;

enum Event {
    Kakoune(SessionId, KakouneRequest),
    Input(InputEvent),
    KakouneDied(SessionId),
    PluginTimer(PluginId, Box<dyn std::any::Any + Send>),
    ProcessOutput(PluginId, IoEvent),
}

/// ProcessEventSink that injects process I/O events into the TUI event channel.
struct TuiProcessEventSink(crossbeam_channel::Sender<Event>);

impl ProcessEventSink for TuiProcessEventSink {
    fn send_process_output(&self, plugin_id: PluginId, event: IoEvent) {
        let _ = self.0.send(Event::ProcessOutput(plugin_id, event));
    }
}

/// TimerScheduler that injects timer events into the TUI event channel.
struct TuiTimerScheduler(crossbeam_channel::Sender<Event>);

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

fn surface_event_from_input(input: &InputEvent) -> Option<SurfaceEvent> {
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

fn spawn_session_reader<R>(session_id: SessionId, reader: R, tx: crossbeam_channel::Sender<Event>)
where
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

struct TuiSessionRuntime<'a, R, W, C>
where
    R: std::io::BufRead + Send + 'static,
    W: Write + Send + 'static,
    C: Send + 'static,
{
    session_manager: &'a mut SessionManager<R, W, C>,
    session_states: &'a mut SessionStateStore,
    tx: crossbeam_channel::Sender<Event>,
    spawn_session: fn(&SessionSpec) -> Result<(R, W, C)>,
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
fn process_event<R, W, C>(
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
    process_dispatcher: &mut dyn kasane_core::plugin::ProcessDispatcher,
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
    let mut session_host = TuiSessionRuntime {
        session_manager,
        session_states,
        tx: session_tx.clone(),
        spawn_session,
    };
    if handle_deferred_commands(
        deferred,
        state,
        registry,
        surface_registry,
        &mut || backend.clipboard_get(),
        dirty,
        timer,
        &mut session_host,
        initial_resize_sent,
        process_dispatcher,
        command_source_plugin.as_ref(),
    ) {
        return true;
    }

    let mut session_host = TuiSessionRuntime {
        session_manager,
        session_states,
        tx: session_tx.clone(),
        spawn_session,
    };
    let should_quit = handle_sourced_surface_commands(
        surface_command_groups,
        state,
        registry,
        surface_registry,
        &mut || backend.clipboard_get(),
        dirty,
        timer,
        &mut session_host,
        initial_resize_sent,
        process_dispatcher,
    );
    if !should_quit {
        session_states.sync_active_from_manager(session_manager, state);
    }
    should_quit
}

/// Install a panic hook that restores the terminal before printing the panic.
fn install_panic_hook() {
    let default_hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        let _ = crossterm::terminal::disable_raw_mode();
        let _ = crossterm::execute!(
            std::io::stdout(),
            crossterm::cursor::Show,
            crossterm::event::DisableFocusChange,
            crossterm::event::DisableMouseCapture,
            crossterm::terminal::LeaveAlternateScreen
        );
        default_hook(info);
    }));
}

/// Spawn a thread that reads crossterm events and sends them to the channel.
fn spawn_input_thread(tx: crossbeam_channel::Sender<Event>) {
    std::thread::spawn(move || {
        loop {
            match crossterm::event::read() {
                Ok(ct_event) => {
                    if let Some(event) = convert_event(ct_event)
                        && tx.send(Event::Input(event)).is_err()
                    {
                        return;
                    }
                }
                Err(e) => {
                    tracing::error!("crossterm read error: {e}");
                    return;
                }
            }
        }
    });
}

/// Run the TUI event loop.
///
/// `session_manager`: managed Kakoune sessions. V1 consumes the active session only.
/// `create_process_dispatcher`: factory that receives a `ProcessEventSink` and returns
///   a `ProcessDispatcher` for plugin-spawned processes.
pub fn run_tui<R, W, C>(
    config: Config,
    mut session_manager: SessionManager<R, W, C>,
    spawn_session: fn(&SessionSpec) -> Result<(R, W, C)>,
    register_plugins: impl FnOnce(&mut PluginRegistry),
    create_process_dispatcher: impl FnOnce(Arc<dyn ProcessEventSink>) -> Box<dyn ProcessDispatcher>,
) -> Result<()>
where
    R: std::io::BufRead + Send + 'static,
    W: Write + Send + 'static,
    C: Send + 'static,
{
    install_panic_hook();

    let active_session = session_manager
        .active_session_id()
        .ok_or_else(|| anyhow!("missing primary session id"))?;
    let kak_reader = session_manager
        .take_active_reader()
        .map_err(|err| anyhow!("failed to acquire primary session: {err:?}"))?;

    // Initialize TUI backend
    let mut backend = TuiBackend::new()?;
    let (cols, rows) = backend.size();

    // Application state
    let mut state = AppState {
        cols,
        rows,
        ..AppState::default()
    };
    state.apply_config(&config);
    let mut session_states = SessionStateStore::new();
    session_states.sync_from_active(active_session, &state);

    // Plugin registry
    let mut registry = PluginRegistry::new();
    register_plugins(&mut registry);

    // Surface registry
    let mut surface_registry = SurfaceRegistry::new();
    surface_registry
        .try_register(Box::new(KakouneBufferSurface::new()))
        .map_err(|err| anyhow!("failed to register built-in surface kasane.buffer: {err:?}"))?;
    surface_registry
        .try_register(Box::new(
            kasane_core::surface::status::StatusBarSurface::new(),
        ))
        .map_err(|err| anyhow!("failed to register built-in surface kasane.status: {err:?}"))?;

    // Collect plugin-owned surfaces before plugin init so invalid surface contracts
    // do not get a chance to produce side effects.
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
            registry.remove_plugin(&surface_set.owner);
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
                Some(kasane_core::layout::Rect {
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

    let init_commands = registry.init_all(&state);
    if matches!(
        execute_commands(
            init_commands,
            session_manager
                .active_writer_mut()
                .map_err(|err| anyhow!("failed to access primary session writer: {err:?}"))?,
            &mut || backend.clipboard_get(),
        ),
        CommandResult::Quit
    ) {
        backend.cleanup();
        return Ok(());
    }

    // Collect paint hooks from plugins
    let paint_hooks = registry.collect_paint_hooks();

    // Cell grid
    let mut grid = CellGrid::new(cols, rows);
    let mut view_cache = ViewCache::new();
    let mut layout_cache = LayoutCache::new();

    // Paint patches for fast-path rendering
    let status_patch = StatusBarPatch;
    let mut menu_patch = MenuSelectionPatch {
        prev_selected: None,
    };
    let mut cursor_patch = CursorPatch {
        prev_cursor_x: 0,
        prev_cursor_y: 0,
    };

    // NOTE: We do NOT send the initial resize here. Kakoune's JSON UI
    // registers its stdin FD watcher in EventMode::Urgent. During
    // initialization (before the Client sets the m_on_key callback),
    // urgent event processing may read stdin data into an internal
    // buffer. Without m_on_key, parse_requests() returns early and
    // the messages are silently accumulated but never processed —
    // until the next stdin read is triggered by user input.
    // Instead, we defer the resize to after receiving the first
    // Kakoune event, which guarantees initialization is complete.
    let mut initial_resize_sent = false;

    // Event channel
    let (tx, rx) = unbounded::<Event>();

    // Kakoune stdout reader thread
    spawn_session_reader(active_session, kak_reader, tx.clone());

    // crossterm input reader thread
    spawn_input_thread(tx.clone());

    // Timer scheduler for plugin timer events
    let timer = TuiTimerScheduler(tx.clone());

    // Process dispatcher for plugin-spawned processes
    let process_sink: Arc<dyn ProcessEventSink> = Arc::new(TuiProcessEventSink(tx.clone()));
    let mut process_dispatcher = create_process_dispatcher(process_sink);

    let scroll_amount = config.scroll.lines_per_scroll;

    // Main event loop
    loop {
        let timeout = if state.scroll_animation.is_some() {
            std::time::Duration::from_millis(16) // ~60fps for smooth scroll
        } else {
            std::time::Duration::from_secs(60) // effectively infinite
        };

        let first = match rx.recv_timeout(timeout) {
            Ok(e) => e,
            Err(crossbeam_channel::RecvTimeoutError::Timeout) => {
                if let Some(cmd) = tick_scroll_animation(&mut state)
                    && matches!(
                        execute_commands(
                            vec![cmd],
                            session_manager
                                .active_writer_mut()
                                .expect("missing active session writer"),
                            &mut || backend.clipboard_get(),
                        ),
                        CommandResult::Quit
                    )
                {
                    break;
                }
                session_states.sync_active_from_manager(&session_manager, &state);
                continue;
            }
            Err(crossbeam_channel::RecvTimeoutError::Disconnected) => break,
        };

        let mut dirty = DirtyFlags::empty();
        let _frame_span = tracing::debug_span!("frame").entered();

        // Process first event
        if process_event(
            first,
            &mut state,
            &mut registry,
            &mut surface_registry,
            &mut session_manager,
            &mut session_states,
            &tx,
            spawn_session,
            &mut grid,
            scroll_amount,
            &mut backend,
            &mut initial_resize_sent,
            &mut dirty,
            &timer,
            &mut *process_dispatcher,
        ) {
            break;
        }

        // Drain any pending events before rendering (batch processing).
        // Safety valve: stop batching after MAX_BATCH events or BATCH_DEADLINE_MS
        // to prevent render starvation during macro replay / rapid input.
        const MAX_BATCH: usize = 256;
        const BATCH_DEADLINE_MS: u64 = 16;
        let batch_deadline =
            std::time::Instant::now() + std::time::Duration::from_millis(BATCH_DEADLINE_MS);
        let mut batch_count = 0usize;
        let mut quit = false;

        while batch_count < MAX_BATCH && std::time::Instant::now() < batch_deadline {
            let Ok(event) = rx.try_recv() else { break };
            batch_count += 1;
            if process_event(
                event,
                &mut state,
                &mut registry,
                &mut surface_registry,
                &mut session_manager,
                &mut session_states,
                &tx,
                spawn_session,
                &mut grid,
                scroll_amount,
                &mut backend,
                &mut initial_resize_sent,
                &mut dirty,
                &timer,
                &mut *process_dispatcher,
            ) {
                quit = true;
                break;
            }
        }
        if quit {
            break;
        }

        if batch_count > 0 {
            tracing::debug!(batch_count, "event batch drained");
        }

        if !dirty.is_empty() {
            surface_registry.sync_ephemeral_surfaces(&state);
            registry.prepare_plugin_cache(dirty);
            backend.begin_frame()?;
            let patches: &[&dyn kasane_core::render::PaintPatch] =
                &[&status_patch, &menu_patch, &cursor_patch];
            let result = render_pipeline_surfaces_patched(
                &state,
                &registry,
                &surface_registry,
                &mut grid,
                dirty,
                &mut view_cache,
                &mut layout_cache,
                patches,
                &paint_hooks,
            );
            backend.draw_grid(&grid)?;
            backend.show_cursor(result.cursor_x, result.cursor_y, result.cursor_style)?;
            backend.end_frame()?;
            backend.flush()?;
            grid.swap_with_dirty();
            state.lines_dirty.clear(); // consumed; prevent stale data next batch

            // Update patch state for next frame
            cursor_patch.prev_cursor_x = result.cursor_x;
            cursor_patch.prev_cursor_y = result.cursor_y;
            menu_patch.prev_selected = state.menu.as_ref().and_then(|m| m.selected);

            // Rebuild HitMap from cached view tree for plugin mouse routing.
            // NOTE: Events within the same batch share the previous frame's HitMap.
            // This is an accepted tradeoff — the performance cost of mid-batch
            // HitMap rebuild outweighs the marginal correctness improvement
            // (at most 16ms of stale routing).
            let element =
                surface_view_sections_cached(&state, &registry, &surface_registry, &mut view_cache)
                    .into_element();
            let root_area = kasane_core::layout::Rect {
                x: 0,
                y: 0,
                w: state.cols,
                h: state.rows,
            };
            let layout_result = kasane_core::layout::flex::place(&element, root_area, &state);
            let hit_map = build_hit_map(&element, &layout_result);
            registry.set_hit_map(hit_map);
        }
    }

    registry.shutdown_all();
    backend.cleanup();
    Ok(())
}
