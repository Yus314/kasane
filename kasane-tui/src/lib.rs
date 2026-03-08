pub mod backend;
pub mod input;

use std::io::Write;

use anyhow::Result;
use crossbeam_channel::unbounded;

use kasane_core::config::Config;
use kasane_core::input::InputEvent;
use kasane_core::plugin::{CommandResult, PluginRegistry, execute_commands};
use kasane_core::protocol::{KakouneRequest, KasaneRequest};
use kasane_core::render::{CellGrid, RenderBackend, render_pipeline};
use kasane_core::state::{AppState, DirtyFlags, Msg, tick_scroll_animation, update};

use backend::TuiBackend;
use input::convert_event;

enum Event {
    Kakoune(KakouneRequest),
    Input(InputEvent),
    KakouneDied,
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
) -> bool {
    match event {
        Event::Kakoune(req) => {
            if !*initial_resize_sent {
                *initial_resize_sent = true;
                let resize = KasaneRequest::Resize {
                    rows: state.available_height(),
                    cols: state.cols,
                };
                let _ = writeln!(kak_writer, "{}", resize.to_json());
                let _ = kak_writer.flush();
            }
            let (flags, commands) = update(state, Msg::Kakoune(req), registry, grid, scroll_amount);
            *dirty |= flags;
            matches!(
                execute_commands(commands, kak_writer, &mut || backend.clipboard_get()),
                CommandResult::Quit
            )
        }
        Event::Input(input_event) => {
            let (flags, commands) =
                update(state, Msg::from(input_event), registry, grid, scroll_amount);
            *dirty |= flags;
            *initial_resize_sent
                && matches!(
                    execute_commands(commands, kak_writer, &mut || backend.clipboard_get()),
                    CommandResult::Quit
                )
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

    // Cell grid
    let mut grid = CellGrid::new(cols, rows);

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
            ) {
                quit = true;
                break;
            }
        }
        if quit {
            break;
        }

        if !dirty.is_empty() {
            backend.begin_frame()?;
            let result = render_pipeline(&state, &registry, &mut grid);
            let diffs = grid.diff();
            backend.draw(&diffs)?;
            backend.show_cursor(result.cursor_x, result.cursor_y, result.cursor_style)?;
            backend.end_frame()?;
            backend.flush()?;
            grid.swap();
        }
    }

    backend.cleanup();
    Ok(())
}
