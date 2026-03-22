mod backend;
mod diagnostics_overlay;
mod event_handler;
mod input;
mod paint_hooks;
pub mod sgr;

use std::io::Write;
use std::sync::Arc;
use std::sync::OnceLock;

use anyhow::{Result, anyhow};
use crossbeam_channel::unbounded;

/// Global session name for panic hook reconnect message.
static SESSION_NAME: OnceLock<String> = OnceLock::new();

use kasane_core::config::Config;
use kasane_core::event_loop::{
    SessionReadyGate, apply_bootstrap_effects, register_builtin_surfaces,
    sync_session_ready_gate as sync_ready_gate,
};
use kasane_core::plugin::{
    AppView, CommandResult, PluginDiagnosticOverlayState, PluginManager, PluginRuntime,
    ProcessDispatcher, ProcessEventSink, execute_commands, report_plugin_diagnostics,
};
use kasane_core::render::render_pipeline_cached;
use kasane_core::render::{CellGrid, RenderBackend};
use kasane_core::salsa_db::KasaneDatabase;
use kasane_core::salsa_sync::{
    SalsaInputHandles, sync_display_directives, sync_inputs_from_state, sync_plugin_contributions,
};
use kasane_core::scroll::ScrollRuntime;
use kasane_core::session::{SessionManager, SessionSpec, SessionStateStore};
use kasane_core::state::{AppState, DirtyFlags};
use kasane_core::surface::SurfaceRegistry;
use kasane_core::surface::pane_map::PaneStates;

use backend::TuiBackend;
use diagnostics_overlay::paint_diagnostic_overlay;
use event_handler::{
    Event, EventProcessingContext, TuiProcessEventSink, TuiSessionRuntime, TuiTimerScheduler,
    process_event, spawn_session_reader,
};
use input::convert_event;
use paint_hooks::PaintHookState;

/// Install a panic hook that restores the terminal and shows reconnect info.
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
        kasane_core::event_loop::print_session_recovery_hint(
            SESSION_NAME.get().map(|s| s.as_str()),
        );
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
    create_process_dispatcher: impl FnOnce(Arc<dyn ProcessEventSink>) -> Box<dyn ProcessDispatcher>,
    mut plugin_manager: PluginManager,
) -> Result<()>
where
    R: std::io::BufRead + Send + 'static,
    W: Write + Send + 'static,
    C: Send + 'static,
{
    install_panic_hook();

    let (tx, rx) = unbounded::<Event>();

    // Store session name for panic hook reconnect message
    if let Some(spec) = session_manager.active_spec()
        && let Some(ref name) = spec.session
    {
        let _ = SESSION_NAME.set(name.clone());
    }

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
    let mut state = Box::new(AppState {
        cols,
        rows,
        ..AppState::default()
    });
    state.apply_config(&config);
    let mut session_states = SessionStateStore::new();
    session_states.sync_from_active(active_session, &state);
    kasane_core::event_loop::sync_session_metadata(&session_manager, &session_states, &mut state);

    // Plugin registry
    let mut registry = PluginRuntime::new();
    // Surface registry
    let mut surface_registry = SurfaceRegistry::new();
    register_builtin_surfaces(&mut surface_registry);
    // Bind initial session to the primary buffer surface
    surface_registry.bind_session(kasane_core::surface::SurfaceId::BUFFER, active_session);
    if let Some(spec) = session_manager.active_spec()
        && let Some(ref name) = spec.session
    {
        surface_registry.set_server_session_name(name.clone());
    }
    let mut diagnostic_overlay = PluginDiagnosticOverlayState::default();

    // Collect plugin-owned surfaces before plugin init so invalid surface contracts
    // do not get a chance to produce side effects.
    let initial_plugins = plugin_manager.initialize(&mut registry, |_, registry| {
        kasane_core::event_loop::setup_plugin_surfaces(registry, &mut surface_registry, &state)
    })?;
    report_plugin_diagnostics(&initial_plugins.diagnostics);
    kasane_core::event_loop::schedule_diagnostic_overlay(
        &TuiDiagnosticScheduler(tx.clone()),
        &mut diagnostic_overlay,
        &initial_plugins.diagnostics,
    );

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

    // Kakoune stdout reader thread
    spawn_session_reader(active_session, kak_reader, tx.clone());

    // crossterm input reader thread
    spawn_input_thread(tx.clone());

    // Plugin hot-reload sentinel watcher thread
    {
        let plugins_dir = config.plugins.plugins_dir();
        let reload_sentinel = plugins_dir.join(".reload");
        let reload_tx = tx.clone();
        std::thread::spawn(move || {
            let mut last_modified = reload_sentinel.metadata().and_then(|m| m.modified()).ok();
            loop {
                std::thread::sleep(std::time::Duration::from_millis(500));
                let current = reload_sentinel.metadata().and_then(|m| m.modified()).ok();
                if current != last_modified && current.is_some() {
                    last_modified = current;
                    if reload_tx.send(Event::PluginReload).is_err() {
                        return;
                    }
                }
            }
        });
    }

    // Timer scheduler for plugin timer events
    let timer = TuiTimerScheduler(tx.clone());
    let mut scroll_runtime = ScrollRuntime::default();
    let mut scroll_runtime_session = session_manager.active_session_id();

    // Process dispatcher for plugin-spawned processes
    let process_sink: Arc<dyn ProcessEventSink> = Arc::new(TuiProcessEventSink(tx.clone()));
    let mut process_dispatcher = create_process_dispatcher(process_sink);

    let init_batch = registry.init_all_batch(&AppView::new(&state));
    let mut session_ready_gate = SessionReadyGate::default();
    let mut bootstrap_dirty = DirtyFlags::empty();
    sync_ready_gate(&mut session_ready_gate, &state);
    apply_bootstrap_effects(init_batch.effects, &mut bootstrap_dirty);
    kasane_core::event_loop::notify_workspace_observers(&mut registry, &surface_registry, &state);

    // Collect paint hooks from plugins
    let mut paint_hooks = PaintHookState::from_registry(&registry);

    // Salsa database
    let (mut salsa_db, salsa_handles) = {
        let mut db = KasaneDatabase::default();
        let handles = SalsaInputHandles::new(&mut db);
        sync_inputs_from_state(&mut db, &state, &handles);
        (db, handles)
    };

    // Cell grid
    let mut grid = CellGrid::new(cols, rows);

    let scroll_amount = config.scroll.lines_per_scroll;

    // Main event loop
    loop {
        let timeout = scroll_runtime
            .active_frame_interval()
            .unwrap_or_else(|| std::time::Duration::from_secs(60));

        let first = match rx.recv_timeout(timeout) {
            Ok(e) => e,
            Err(crossbeam_channel::RecvTimeoutError::Timeout) => {
                scroll_runtime.set_initial_resize_complete(initial_resize_sent);
                if let Some(resolved) = scroll_runtime.tick() {
                    let focused_surface = surface_registry.workspace().focused();
                    let focused_sid = surface_registry.session_for_surface(focused_surface);
                    let writer =
                        match focused_sid.and_then(|sid| session_manager.writer_mut(sid).ok()) {
                            Some(w) => w,
                            None => session_manager
                                .active_writer_mut()
                                .expect("missing active session writer"),
                        };
                    if matches!(
                        execute_commands(
                            vec![kasane_core::plugin::Command::SendToKakoune(
                                resolved.to_kakoune_request(),
                            )],
                            writer,
                            &mut || backend.clipboard_get(),
                        ),
                        CommandResult::Quit
                    ) {
                        break;
                    }
                }
                session_states.sync_active_from_manager(&session_manager, &state);
                continue;
            }
            Err(crossbeam_channel::RecvTimeoutError::Disconnected) => break,
        };

        let mut dirty = bootstrap_dirty;
        bootstrap_dirty = DirtyFlags::empty();
        let _frame_span = tracing::debug_span!("frame").entered();

        // FIFO metrics: count message types in the batch for diagnostics
        let mut fifo_draw_count: u32 = if first.is_draw() { 1 } else { 0 };
        let mut fifo_refresh_count: u32 = if first.is_refresh() { 1 } else { 0 };
        let batch_start = std::time::Instant::now();
        let queue_depth = rx.len();

        let (batch_count, quit) = {
            let mut ctx = EventProcessingContext {
                state: &mut state,
                registry: &mut registry,
                surface_registry: &mut surface_registry,
                session_manager: &mut session_manager,
                session_states: &mut session_states,
                session_tx: &tx,
                spawn_session,
                grid: &mut grid,
                scroll_amount,
                backend: &mut backend,
                initial_resize_sent: &mut initial_resize_sent,
                dirty: &mut dirty,
                timer: &timer,
                scroll_runtime: &mut scroll_runtime,
                scroll_runtime_session: &mut scroll_runtime_session,
                session_ready_gate: &mut session_ready_gate,
                process_dispatcher: &mut *process_dispatcher,
                plugin_manager: &mut plugin_manager,
                paint_hooks: &mut paint_hooks,
                diagnostic_overlay: &mut diagnostic_overlay,
            };

            // Process first event
            if process_event(first, &mut ctx) {
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
                fifo_draw_count += u32::from(event.is_draw());
                fifo_refresh_count += u32::from(event.is_refresh());
                if process_event(event, &mut ctx) {
                    quit = true;
                    break;
                }
            }
            (batch_count, quit)
        };

        if quit {
            break;
        }

        // Emit FIFO diagnostics when the batch contained multiple draw messages
        // (indicates rapid Kakoune updates, e.g. FIFO buffer streaming).
        if fifo_draw_count > 1 || fifo_refresh_count > 0 {
            let batch_ms = batch_start.elapsed().as_secs_f64() * 1000.0;
            tracing::debug!(
                draw_count = fifo_draw_count,
                refresh_count = fifo_refresh_count,
                batch_count,
                queue_depth,
                batch_ms = format_args!("{batch_ms:.2}"),
                "fifo_metrics"
            );
        } else if batch_count > 0 {
            tracing::debug!(batch_count, "event batch drained");
        }

        // Send resize commands to pane clients when layout may have changed
        if !dirty.is_empty() && surface_registry.is_multi_pane() {
            let total = kasane_core::layout::Rect {
                x: 0,
                y: 0,
                w: state.cols,
                h: state.rows,
            };
            let mut session_host = TuiSessionRuntime {
                session_manager: &mut session_manager,
                session_states: &mut session_states,
                tx: tx.clone(),
                spawn_session,
            };
            kasane_core::event_loop::send_pane_resizes(
                &mut surface_registry,
                &mut session_host,
                total,
            );
        }

        if !dirty.is_empty() {
            let render_start = std::time::Instant::now();

            surface_registry.sync_ephemeral_surfaces(&state);
            registry.prepare_plugin_cache(dirty);

            // Sync Salsa inputs from updated state
            sync_inputs_from_state(&mut salsa_db, &state, &salsa_handles);
            let view = registry.view();
            sync_display_directives(&mut salsa_db, &state, &view, &salsa_handles);
            sync_plugin_contributions(&mut salsa_db, &state, &view, &salsa_handles);

            backend.begin_frame()?;

            let pane_states_val;
            let pane_states_opt = if surface_registry.is_multi_pane() {
                pane_states_val = PaneStates::from_registry(
                    &surface_registry,
                    &session_states,
                    &state,
                    session_manager.active_session_id(),
                );
                Some(&pane_states_val)
            } else {
                None
            };

            let result = render_pipeline_cached(
                &salsa_db,
                &salsa_handles,
                &state,
                &view,
                &mut grid,
                dirty,
                paint_hooks.hooks(),
                Some(&surface_registry),
                pane_states_opt,
            );
            if diagnostic_overlay.is_active() {
                paint_diagnostic_overlay(&diagnostic_overlay, &mut grid);
            }
            backend.draw_grid(&grid)?;
            backend.show_cursor(result.cursor_x, result.cursor_y, result.cursor_style)?;
            backend.end_frame()?;
            backend.flush()?;
            grid.swap_with_dirty();
            state.lines_dirty.clear(); // consumed; prevent stale data next batch

            let frame_ms = render_start.elapsed().as_secs_f64() * 1000.0;
            if fifo_draw_count > 0 {
                tracing::debug!(
                    frame_ms = format_args!("{frame_ms:.2}"),
                    dirty = ?dirty,
                    "fifo_frame"
                );
            }

            // Rebuild HitMap from cached view tree for plugin mouse routing.
            // NOTE: Events within the same batch share the previous frame's HitMap.
            // This is an accepted tradeoff — the performance cost of mid-batch
            // HitMap rebuild outweighs the marginal correctness improvement
            // (at most 16ms of stale routing).
            kasane_core::event_loop::rebuild_hit_map(&mut state, &registry, &surface_registry);
        }
    }

    registry.shutdown_all();
    backend.cleanup();
    Ok(())
}

/// Newtype wrapper for crossbeam Sender to implement `DiagnosticOverlayScheduler`.
pub(crate) struct TuiDiagnosticScheduler(pub(crate) crossbeam_channel::Sender<Event>);

impl kasane_core::event_loop::DiagnosticOverlayScheduler for TuiDiagnosticScheduler {
    fn schedule_expiry(&self, delay: std::time::Duration, generation: u64) {
        let tx = self.0.clone();
        std::thread::spawn(move || {
            std::thread::sleep(delay);
            let _ = tx.send(Event::DiagnosticOverlayExpire(generation));
        });
    }
}
