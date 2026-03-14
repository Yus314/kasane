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
use kasane_core::render::view::surface_view_sections_cached;
use kasane_core::render::{
    CellGrid, CursorPatch, LayoutCache, MenuSelectionPatch, RenderBackend, StatusBarPatch,
    ViewCache, render_pipeline_surfaces_patched,
};
use kasane_core::state::{AppState, DirtyFlags, Msg, tick_scroll_animation, update};
use kasane_core::surface::SurfaceRegistry;
use kasane_core::surface::buffer::KakouneBufferSurface;

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
    surface_registry: &mut SurfaceRegistry,
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
                    surface_registry,
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
                kasane_core::state::apply_set_config(state, dirty, &key, &value);
            }
            DeferredCommand::Pane(_) => {
                // Pane commands will be handled in Phase 5a-1
            }
            DeferredCommand::Workspace(ws_cmd) => {
                dispatch_workspace_command(surface_registry, ws_cmd, dirty);
            }
            DeferredCommand::RegisterThemeTokens(_tokens) => {
                // Theme token registration will be handled when Theme is
                // accessible from the event loop (Phase 1 completion).
            }
        }
    }
    false
}

/// Dispatch a workspace command to the SurfaceRegistry.
fn dispatch_workspace_command(
    surface_registry: &mut SurfaceRegistry,
    cmd: kasane_core::workspace::WorkspaceCommand,
    dirty: &mut DirtyFlags,
) {
    use kasane_core::workspace::WorkspaceCommand;
    match cmd {
        WorkspaceCommand::AddSurface {
            surface_id,
            placement,
        } => {
            let ws = surface_registry.workspace_mut();
            match placement {
                kasane_core::workspace::Placement::SplitFocused { direction, ratio } => {
                    let focused = ws.focused();
                    ws.root_mut().split(focused, direction, ratio, surface_id);
                }
                kasane_core::workspace::Placement::SplitFrom {
                    target,
                    direction,
                    ratio,
                } => {
                    ws.root_mut().split(target, direction, ratio, surface_id);
                }
                _ => {} // Tab, Dock, Float — handled in future phases
            }
            *dirty |= DirtyFlags::ALL;
        }
        WorkspaceCommand::RemoveSurface(id) => {
            surface_registry.workspace_mut().close(id);
            *dirty |= DirtyFlags::ALL;
        }
        WorkspaceCommand::Focus(id) => {
            surface_registry.workspace_mut().focus(id);
            *dirty |= DirtyFlags::ALL;
        }
        WorkspaceCommand::FocusDirection(_) => {
            // Direction-based focus navigation — future phase
        }
        WorkspaceCommand::Swap(a, b) => {
            let _ = (a, b); // Swap — future phase
            *dirty |= DirtyFlags::ALL;
        }
        WorkspaceCommand::Resize { .. }
        | WorkspaceCommand::Float { .. }
        | WorkspaceCommand::Unfloat(_) => {
            // Future phases
        }
    }
}

/// 単一イベントを処理する。終了が必要な場合 true を返す。
#[allow(clippy::too_many_arguments)]
fn process_event(
    event: Event,
    state: &mut AppState,
    registry: &mut PluginRegistry,
    surface_registry: &mut SurfaceRegistry,
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
            let (flags, commands) = update(state, Msg::Kakoune(req), registry, scroll_amount);
            // Resize: update grid dimensions (update() only modifies state)
            if flags.contains(DirtyFlags::ALL) {
                grid.resize(state.cols, state.rows);
                grid.invalidate_all();
            }
            *dirty |= flags;
            let (normal, deferred) = extract_deferred_commands(commands);
            if matches!(
                execute_commands(normal, kak_writer, &mut || backend.clipboard_get()),
                CommandResult::Quit
            ) {
                return true;
            }
            handle_deferred(
                deferred,
                state,
                registry,
                surface_registry,
                kak_writer,
                backend,
                dirty,
                tx,
            )
        }
        Event::Input(input_event) => {
            let (flags, commands) = update(state, Msg::from(input_event), registry, scroll_amount);
            if flags.contains(DirtyFlags::ALL) {
                grid.resize(state.cols, state.rows);
                grid.invalidate_all();
            }
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
            handle_deferred(
                deferred,
                state,
                registry,
                surface_registry,
                kak_writer,
                backend,
                dirty,
                tx,
            )
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
            handle_deferred(
                deferred,
                state,
                registry,
                surface_registry,
                kak_writer,
                backend,
                dirty,
                tx,
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

    // Surface registry
    let mut surface_registry = SurfaceRegistry::new();
    surface_registry.register(Box::new(KakouneBufferSurface::new()));
    surface_registry.register(Box::new(
        kasane_core::surface::status::StatusBarSurface::new(),
    ));

    // Collect plugin-owned surfaces
    for surface in registry.collect_plugin_surfaces() {
        surface_registry.register(surface);
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
                if let Some(cmd) = tick_scroll_animation(&mut state)
                    && matches!(
                        execute_commands(vec![cmd], &mut kak_writer, &mut || backend
                            .clipboard_get()),
                        CommandResult::Quit
                    )
                {
                    break;
                }
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
            &mut surface_registry,
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
                &mut surface_registry,
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
