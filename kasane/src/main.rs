mod process;

use anyhow::Result;
use crossbeam_channel::unbounded;

use kasane_core::config::Config;
use kasane_core::input::{self as core_input, InputEvent};
use kasane_core::layout::Rect;
use kasane_core::layout::flex;
use kasane_core::plugin::{Command, PluginRegistry};
use kasane_core::protocol::{KakouneRequest, KasaneRequest};
use kasane_core::render::paint;
use kasane_core::render::view;
use kasane_core::render::{
    CellGrid, RenderBackend, clear_block_cursor_face, cursor_position, cursor_style,
};
use kasane_core::state::{AppState, Msg, update};
use kasane_tui::backend::TuiBackend;
use kasane_tui::input::convert_event;

enum Event {
    Kakoune(KakouneRequest),
    Input(InputEvent),
    KakouneDied,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum UiMode {
    Tui,
    Gui,
}

fn main() -> Result<()> {
    // Parse CLI arguments
    let args: Vec<String> = std::env::args().skip(1).collect();
    let (session, ui_mode, kak_args) = parse_cli_args(&args);

    // Load config
    let config = Config::load();

    // Setup logging
    let _guard = setup_logging(&config);

    match ui_mode {
        UiMode::Tui => run_tui(config, session, kak_args),
        #[cfg(feature = "gui")]
        UiMode::Gui => {
            let session_clone = session.clone();
            let kak_args_clone = kak_args.clone();
            kasane_gui::run_gui(config, move || {
                if let Some(ref s) = session_clone {
                    process::connect_kakoune(s, &kak_args_clone)
                } else {
                    process::spawn_kakoune(&kak_args_clone)
                }
            })
        }
        #[cfg(not(feature = "gui"))]
        UiMode::Gui => {
            eprintln!("GUI support not compiled. Rebuild with: cargo build --features gui");
            std::process::exit(1);
        }
    }
}

fn run_tui(config: Config, session: Option<String>, kak_args: Vec<String>) -> Result<()> {
    // Install panic hook to restore terminal
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

    // Spawn Kakoune (split into reader + writer)
    let (kak_reader, mut kak_writer, _kak_child) = if let Some(ref session) = session {
        process::connect_kakoune(session, &kak_args)?
    } else {
        process::spawn_kakoune(&kak_args)?
    };

    // Initialize TUI backend
    let mut backend = TuiBackend::new()?;
    let (cols, rows) = backend.size();

    // Application state
    let mut state = AppState {
        cols,
        rows,
        shadow_enabled: config.ui.shadow,
        padding_char: config.ui.padding_char.clone(),
        menu_max_height: config.menu.max_height,
        menu_position: config.menu.menu_position(),
        search_dropdown: config.search.dropdown,
        status_at_top: config.ui.status_position() == kasane_core::config::StatusPosition::Top,
        smooth_scroll: config.scroll.smooth,
        ..AppState::default()
    };

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
    let input_tx = tx.clone();
    std::thread::spawn(move || {
        loop {
            match crossterm::event::read() {
                Ok(ct_event) => {
                    if let Some(event) = convert_event(ct_event)
                        && input_tx.send(Event::Input(event)).is_err()
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

        let event = match rx.recv_timeout(timeout) {
            Ok(e) => Some(e),
            Err(crossbeam_channel::RecvTimeoutError::Timeout) => None,
            Err(crossbeam_channel::RecvTimeoutError::Disconnected) => break,
        };

        // Handle scroll animation tick on timeout
        if event.is_none() {
            if let Some(ref mut anim) = state.scroll_animation {
                let step = anim.step.min(anim.remaining.abs()) * anim.remaining.signum();
                let req = KasaneRequest::Scroll {
                    amount: step,
                    line: anim.line,
                    column: anim.column,
                };
                kak_writer.write_message(&req.to_json())?;
                anim.remaining -= step;
                if anim.remaining == 0 {
                    state.scroll_animation = None;
                }
            }
            continue;
        }

        let msg = match event.unwrap() {
            Event::Kakoune(req) => {
                if !initial_resize_sent {
                    initial_resize_sent = true;
                    kak_writer.write_message(
                        &KasaneRequest::Resize {
                            rows: rows.saturating_sub(1),
                            cols,
                        }
                        .to_json(),
                    )?;
                }
                Msg::Kakoune(req)
            }
            Event::Input(input_event) => Msg::from(input_event),
            Event::KakouneDied => break,
        };

        let (flags, commands) = update(&mut state, msg, &mut registry, &mut grid, scroll_amount);
        let mut dirty = flags;
        if execute_commands(commands, &mut kak_writer, &mut backend)? {
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

        while batch_count < MAX_BATCH && std::time::Instant::now() < batch_deadline {
            let event = match rx.try_recv() {
                Ok(e) => e,
                Err(_) => break,
            };
            batch_count += 1;
            match event {
                Event::Kakoune(req) => {
                    let (flags, commands) = update(
                        &mut state,
                        Msg::Kakoune(req),
                        &mut registry,
                        &mut grid,
                        scroll_amount,
                    );
                    dirty |= flags;
                    if execute_commands(commands, &mut kak_writer, &mut backend)? {
                        backend.cleanup();
                        return Ok(());
                    }
                }
                Event::Input(input_event) => {
                    let msg = Msg::from(input_event);
                    let (flags, commands) =
                        update(&mut state, msg, &mut registry, &mut grid, scroll_amount);
                    dirty |= flags;
                    if execute_commands(commands, &mut kak_writer, &mut backend)? {
                        backend.cleanup();
                        return Ok(());
                    }
                }
                Event::KakouneDied => {
                    backend.cleanup();
                    return Ok(());
                }
            }
        }

        if !dirty.is_empty() {
            backend.begin_frame()?;

            // Declarative pipeline: view → layout → paint
            let element = view::view(&state, &registry);
            let root_area = Rect {
                x: 0,
                y: 0,
                w: state.cols,
                h: state.rows,
            };
            let layout_result = flex::place(&element, root_area, &state);
            grid.clear(&state.default_face);
            paint::paint(&element, &layout_result, &mut grid, &state);

            let cursor_style = cursor_style(&state);
            clear_block_cursor_face(&state, &mut grid, cursor_style);
            let diffs = grid.diff();
            backend.draw(&diffs)?;
            let (cx, cy) = cursor_position(&state, &grid);
            backend.show_cursor(cx, cy, cursor_style)?;
            backend.end_frame()?;
            backend.flush()?;
            grid.swap();
        }
    }

    backend.cleanup();
    Ok(())
}

/// Execute side-effect commands. Returns `true` if Quit was requested.
fn execute_commands(
    commands: Vec<Command>,
    kak_writer: &mut process::KakouneWriter,
    backend: &mut TuiBackend,
) -> Result<bool> {
    for cmd in commands {
        match cmd {
            Command::SendToKakoune(req) => {
                kak_writer.write_message(&req.to_json())?;
            }
            Command::Paste => {
                if let Some(text) = backend.clipboard_get() {
                    let keys = core_input::paste_text_to_keys(&text);
                    if !keys.is_empty() {
                        kak_writer
                            .write_message(&KasaneRequest::Keys(keys).to_json())?;
                    }
                }
            }
            Command::Quit => return Ok(true),
        }
    }
    Ok(false)
}

fn parse_cli_args(args: &[String]) -> (Option<String>, UiMode, Vec<String>) {
    let mut session = None;
    let mut ui_mode = UiMode::Tui;
    let mut kak_args = Vec::new();
    let mut iter = args.iter().peekable();
    let mut pass_through = false;

    while let Some(arg) = iter.next() {
        if pass_through {
            kak_args.push(arg.clone());
            continue;
        }
        match arg.as_str() {
            "-c" => {
                if let Some(s) = iter.next() {
                    session = Some(s.clone());
                }
            }
            "--ui" => {
                if let Some(mode) = iter.next() {
                    match mode.as_str() {
                        "gui" => ui_mode = UiMode::Gui,
                        "tui" => ui_mode = UiMode::Tui,
                        _ => {
                            eprintln!("unknown --ui mode: {mode}. Use 'tui' or 'gui'.");
                            std::process::exit(1);
                        }
                    }
                }
            }
            "--" => {
                pass_through = true;
            }
            _ => {
                kak_args.push(arg.clone());
            }
        }
    }

    (session, ui_mode, kak_args)
}

fn setup_logging(config: &Config) -> Option<tracing_appender::non_blocking::WorkerGuard> {
    let log_dir = if let Some(ref file) = config.log.file {
        std::path::PathBuf::from(file)
    } else if let Ok(state_home) = std::env::var("XDG_STATE_HOME") {
        std::path::PathBuf::from(state_home).join("kasane")
    } else if let Ok(home) = std::env::var("HOME") {
        std::path::PathBuf::from(home)
            .join(".local")
            .join("state")
            .join("kasane")
    } else {
        return None;
    };

    let _ = std::fs::create_dir_all(&log_dir);

    let file_appender = tracing_appender::rolling::daily(&log_dir, "kasane.log");
    let (non_blocking, guard) = tracing_appender::non_blocking(file_appender);

    let env_filter = std::env::var("KASANE_LOG").unwrap_or_else(|_| config.log.level.clone());

    let subscriber = tracing_subscriber::fmt()
        .with_writer(non_blocking)
        .with_env_filter(env_filter)
        .with_ansi(false)
        .finish();

    tracing::subscriber::set_global_default(subscriber).ok();

    Some(guard)
}
