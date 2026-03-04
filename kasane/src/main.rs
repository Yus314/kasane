mod process;

use anyhow::Result;
use crossbeam_channel::unbounded;

use kasane_core::config::Config;
use kasane_core::input::{self, InputEvent};
use kasane_core::protocol::{CursorMode, KakouneRequest, KasaneRequest};
use kasane_core::render::{CellGrid, CursorStyle, RenderBackend, cursor_position, render_frame};
use kasane_core::state::AppState;
use kasane_tui::backend::TuiBackend;
use kasane_tui::input::convert_event;

enum Event {
    Kakoune(KakouneRequest),
    Input(InputEvent),
    KakouneDied,
}

fn main() -> Result<()> {
    // Parse CLI arguments
    let args: Vec<String> = std::env::args().skip(1).collect();
    let (session, kak_args) = parse_cli_args(&args);

    // Load config
    let config = Config::load();

    // Setup logging
    let _guard = setup_logging(&config);

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
    let (mut kak_reader, mut kak_writer, _kak_child) = if let Some(ref session) = session {
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
        ..AppState::default()
    };

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
    std::thread::spawn(move || {
        let mut buf = String::new();
        loop {
            match kak_reader.read_line(&mut buf) {
                Ok(0) => {
                    let _ = kak_tx.send(Event::KakouneDied);
                    return;
                }
                Ok(_) => {
                    let trimmed = buf.trim();
                    if trimmed.is_empty() {
                        continue;
                    }
                    let mut bytes = trimmed.as_bytes().to_vec();
                    match kasane_core::protocol::parse_request(&mut bytes) {
                        Ok(req) => {
                            if kak_tx.send(Event::Kakoune(req)).is_err() {
                                return;
                            }
                        }
                        Err(e) => {
                            tracing::warn!("failed to parse kak message: {e}");
                        }
                    }
                }
                Err(e) => {
                    tracing::error!("kak stdout read error: {e}");
                    let _ = kak_tx.send(Event::KakouneDied);
                    return;
                }
            }
        }
    });

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
    while let Ok(event) = rx.recv() {
        let mut needs_render = match event {
            Event::Kakoune(req) => {
                state.apply(req);
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
                true
            }
            Event::Input(input_event) => {
                handle_input(
                    &input_event,
                    &mut kak_writer,
                    &mut state,
                    &mut grid,
                    &mut backend,
                    scroll_amount,
                )?;
                true
            }
            Event::KakouneDied => break,
        };

        // Drain any pending events before rendering (batch processing)
        while let Ok(event) = rx.try_recv() {
            match event {
                Event::Kakoune(req) => {
                    state.apply(req);
                    needs_render = true;
                }
                Event::Input(input_event) => {
                    handle_input(
                        &input_event,
                        &mut kak_writer,
                        &mut state,
                        &mut grid,
                        &mut backend,
                        scroll_amount,
                    )?;
                    needs_render = true;
                }
                Event::KakouneDied => {
                    backend.cleanup();
                    return Ok(());
                }
            }
        }

        if needs_render {
            backend.begin_frame()?;
            render_frame(&state, &mut grid);
            backend.draw(&grid.diff())?;
            let (cx, cy) = cursor_position(&state, &grid);
            let cursor_style = state
                .ui_options
                .get("kasane_cursor_style")
                .map(|s| match s.as_str() {
                    "bar" => CursorStyle::Bar,
                    "underline" => CursorStyle::Underline,
                    _ => CursorStyle::Block,
                })
                .unwrap_or(if state.cursor_mode == CursorMode::Prompt {
                    CursorStyle::Bar
                } else {
                    CursorStyle::Block
                });
            backend.show_cursor(cx, cy, cursor_style)?;
            backend.end_frame()?;
            backend.flush()?;
            grid.swap();
        }
    }

    backend.cleanup();
    Ok(())
}

fn handle_input(
    event: &InputEvent,
    kak_writer: &mut process::KakouneWriter,
    state: &mut AppState,
    grid: &mut CellGrid,
    _backend: &mut TuiBackend,
    scroll_amount: i32,
) -> Result<()> {
    match event {
        InputEvent::Key(key_event) => {
            let kak_key = input::key_to_kakoune(key_event);
            let msg = KasaneRequest::Keys(vec![kak_key]).to_json();
            kak_writer.write_message(&msg)?;
        }
        InputEvent::Resize(cols, rows) => {
            state.cols = *cols;
            state.rows = *rows;
            grid.resize(*cols, *rows);
            grid.invalidate_all();
            let msg = KasaneRequest::Resize {
                rows: rows.saturating_sub(1),
                cols: *cols,
            }
            .to_json();
            kak_writer.write_message(&msg)?;
        }
        InputEvent::Mouse(mouse_event) => {
            if let Some(req) = input::mouse_to_kakoune(mouse_event, scroll_amount) {
                kak_writer.write_message(&req.to_json())?;
            }
        }
        InputEvent::FocusGained | InputEvent::FocusLost => {
            // Could be used for cursor style changes later
        }
    }
    Ok(())
}

fn parse_cli_args(args: &[String]) -> (Option<String>, Vec<String>) {
    let mut session = None;
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
            "--" => {
                pass_through = true;
            }
            _ => {
                kak_args.push(arg.clone());
            }
        }
    }

    (session, kak_args)
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
