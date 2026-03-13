mod backend;
mod input;

use std::io::Write;

use anyhow::Result;
use crossbeam_channel::unbounded;

use kasane_core::config::Config;
use kasane_core::input::InputEvent;
use kasane_core::layout::build_hit_map;
use kasane_core::plugin::{
    CommandResult, DeferredCommand, PluginId, PluginRegistry, execute_commands,
    extract_deferred_commands,
};
use kasane_core::protocol::KakouneRequest;
use kasane_core::render::view::view_cached;
use kasane_core::render::{CellGrid, RenderBackend, ViewCache, render_pipeline_cached};
use kasane_core::state::{AppState, DirtyFlags, Msg, tick_scroll_animation, update};

use backend::TuiBackend;
use input::convert_event;

enum Event {
    Kakoune(KakouneRequest),
    Input(InputEvent),
    KakouneDied,
    PluginTimer(PluginId, Box<dyn std::any::Any + Send>),
}

/// Handle deferred commands (timers, inter-plugin messages, config overrides).
#[allow(clippy::too_many_arguments)]
fn handle_deferred(
    deferred: Vec<DeferredCommand>,
    state: &mut AppState,
    registry: &mut PluginRegistry,
    kak_writer: &mut impl Write,
    backend: &mut TuiBackend,
    dirty: &mut DirtyFlags,
    tx: &crossbeam_channel::Sender<Event>,
) -> bool {
    for cmd in deferred {
        match cmd {
            DeferredCommand::PluginMessage { target, payload } => {
                let (flags, commands) = registry.deliver_message(&target, payload, state);
                *dirty |= flags;
                let (normal, nested_deferred) = extract_deferred_commands(commands);
                if matches!(
                    execute_commands(normal, kak_writer, &mut || backend.clipboard_get()),
                    CommandResult::Quit
                ) {
                    return true;
                }
                if handle_deferred(
                    nested_deferred,
                    state,
                    registry,
                    kak_writer,
                    backend,
                    dirty,
                    tx,
                ) {
                    return true;
                }
            }
            DeferredCommand::ScheduleTimer {
                delay,
                target,
                payload,
            } => {
                let timer_tx = tx.clone();
                std::thread::spawn(move || {
                    std::thread::sleep(delay);
                    let _ = timer_tx.send(Event::PluginTimer(target, payload));
                });
            }
            DeferredCommand::SetConfig { key, value } => {
                state.ui_options.insert(key, value);
                *dirty |= DirtyFlags::OPTIONS;
            }
        }
    }
    false
}

/// 単一イベントを処理する。終了が必要な場合 true を返す。
#[allow(clippy::too_many_arguments)]
fn process_event(
    event: Event,
    state: &mut AppState,
    registry: &mut PluginRegistry,
    grid: &mut CellGrid,
    scroll_amount: i32,
    kak_writer: &mut impl Write,
    backend: &mut TuiBackend,
    initial_resize_sent: &mut bool,
    dirty: &mut DirtyFlags,
    tx: &crossbeam_channel::Sender<Event>,
) -> bool {
    match event {
        Event::Kakoune(req) => {
            kasane_core::io::send_initial_resize(
                kak_writer,
                initial_resize_sent,
                state.rows,
                state.cols,
            );
            let (flags, commands) = update(state, Msg::Kakoune(req), registry, grid, scroll_amount);
            *dirty |= flags;
            let (normal, deferred) = extract_deferred_commands(commands);
            if matches!(
                execute_commands(normal, kak_writer, &mut || backend.clipboard_get()),
                CommandResult::Quit
            ) {
                return true;
            }
            handle_deferred(deferred, state, registry, kak_writer, backend, dirty, tx)
        }
        Event::Input(input_event) => {
            let (flags, commands) =
                update(state, Msg::from(input_event), registry, grid, scroll_amount);
            *dirty |= flags;
            if !*initial_resize_sent {
                return false;
            }
            let (normal, deferred) = extract_deferred_commands(commands);
            if matches!(
                execute_commands(normal, kak_writer, &mut || backend.clipboard_get()),
                CommandResult::Quit
            ) {
                return true;
            }
            handle_deferred(deferred, state, registry, kak_writer, backend, dirty, tx)
        }
        Event::PluginTimer(target, payload) => {
            let (flags, commands) = registry.deliver_message(&target, payload, state);
            *dirty |= flags;
            let (normal, deferred) = extract_deferred_commands(commands);
            if matches!(
                execute_commands(normal, kak_writer, &mut || backend.clipboard_get()),
                CommandResult::Quit
            ) {
                return true;
            }
            handle_deferred(deferred, state, registry, kak_writer, backend, dirty, tx)
        }
        Event::KakouneDied => true,
    }
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
/// `spawn_kakoune`: closure that spawns/connects to Kakoune and returns (reader, writer, child).
pub fn run_tui<R, W, C>(
    config: Config,
    spawn_kakoune: impl FnOnce() -> Result<(R, W, C)>,
    register_plugins: impl FnOnce(&mut PluginRegistry),
) -> Result<()>
where
    R: std::io::BufRead + Send + 'static,
    W: Write + Send + 'static,
    C: Send + 'static,
{
    install_panic_hook();

    let (kak_reader, mut kak_writer, _kak_child) = spawn_kakoune()?;

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

    // Plugin registry
    let mut registry = PluginRegistry::new();
    register_plugins(&mut registry);
    let init_commands = registry.init_all(&state);
    if matches!(
        execute_commands(init_commands, &mut kak_writer, &mut || backend
            .clipboard_get()),
        CommandResult::Quit
    ) {
        backend.cleanup();
        return Ok(());
    }

    // Cell grid
    let mut grid = CellGrid::new(cols, rows);
    let mut view_cache = ViewCache::new();

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
    let kak_tx = tx.clone();
    kasane_core::io::spawn_kak_reader(
        kak_reader,
        move |req| {
            let _ = kak_tx.send(Event::Kakoune(req));
        },
        {
            let died_tx = tx.clone();
            move || {
                let _ = died_tx.send(Event::KakouneDied);
            }
        },
    );

    // crossterm input reader thread
    spawn_input_thread(tx.clone());

    // Keep a sender for plugin timer events; drop will happen when loop breaks
    let timer_tx = tx.clone();

    // Drop the original sender so rx will close when reader threads exit
    drop(tx);

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
                tick_scroll_animation(&mut state, &mut kak_writer);
                continue;
            }
            Err(crossbeam_channel::RecvTimeoutError::Disconnected) => break,
        };

        let mut dirty = DirtyFlags::empty();

        // Process first event
        if process_event(
            first,
            &mut state,
            &mut registry,
            &mut grid,
            scroll_amount,
            &mut kak_writer,
            &mut backend,
            &mut initial_resize_sent,
            &mut dirty,
            &timer_tx,
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
                &mut grid,
                scroll_amount,
                &mut kak_writer,
                &mut backend,
                &mut initial_resize_sent,
                &mut dirty,
                &timer_tx,
            ) {
                quit = true;
                break;
            }
        }
        if quit {
            break;
        }

        if !dirty.is_empty() {
            registry.prepare_plugin_cache(dirty);
            backend.begin_frame()?;
            let result =
                render_pipeline_cached(&state, &registry, &mut grid, dirty, &mut view_cache);
            let diffs = grid.diff();
            backend.draw(&diffs)?;
            backend.show_cursor(result.cursor_x, result.cursor_y, result.cursor_style)?;
            backend.end_frame()?;
            backend.flush()?;
            grid.swap_with_dirty();
            state.lines_dirty.clear(); // consumed; prevent stale data next batch

            // Rebuild HitMap from cached view tree for plugin mouse routing
            let element = view_cached(&state, &registry, &mut view_cache);
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
