use crate::input;
use crate::input::{InputEvent, Key, KeyEvent, MouseEvent};
use crate::plugin::{Command, PluginRegistry, extract_redraw_flags};
use crate::protocol::{KakouneRequest, KasaneRequest};
use crate::render::CellGrid;

use super::{AppState, DirtyFlags, DragState, MouseButton, ScrollAnimation};

/// Messages that drive the application state machine.
pub enum Msg {
    Kakoune(KakouneRequest),
    Key(KeyEvent),
    Mouse(MouseEvent),
    Paste,
    Resize { cols: u16, rows: u16 },
    FocusGained,
    FocusLost,
}

impl From<InputEvent> for Msg {
    fn from(event: InputEvent) -> Self {
        match event {
            InputEvent::Key(key) => Msg::Key(key),
            InputEvent::Mouse(mouse) => Msg::Mouse(mouse),
            InputEvent::Paste(_) => Msg::Paste,
            InputEvent::Resize(cols, rows) => Msg::Resize { cols, rows },
            InputEvent::FocusGained => Msg::FocusGained,
            InputEvent::FocusLost => Msg::FocusLost,
        }
    }
}

/// Process a message, updating state and returning dirty flags + side-effect commands.
pub fn update(
    state: &mut AppState,
    msg: Msg,
    registry: &mut PluginRegistry,
    grid: &mut CellGrid,
    scroll_amount: i32,
) -> (DirtyFlags, Vec<Command>) {
    match msg {
        Msg::Kakoune(req) => {
            let req_kind = match &req {
                KakouneRequest::Draw { .. } => "Draw",
                KakouneRequest::DrawStatus { .. } => "DrawStatus",
                KakouneRequest::SetCursor { .. } => "SetCursor",
                _ => "",
            };
            if !req_kind.is_empty() {
                tracing::debug!(kind = req_kind, "incoming Kakoune request");
            }
            let flags = state.apply(req);
            let mut commands = Vec::new();
            if !flags.is_empty() {
                for plugin in registry.plugins_mut() {
                    commands.extend(plugin.on_state_changed(state, flags));
                }
            }
            let extra_flags = extract_redraw_flags(&mut commands);
            (flags | extra_flags, commands)
        }
        Msg::Key(key) => {
            // 1. Notify all plugins (observe only, cannot consume)
            for plugin in registry.plugins_mut() {
                plugin.observe_key(&key, state);
            }

            // 2. Plugin handle_key chain (first-wins)
            for plugin in registry.plugins_mut() {
                if let Some(mut commands) = plugin.handle_key(&key, state) {
                    let flags = extract_redraw_flags(&mut commands);
                    return (flags, commands);
                }
            }

            // 3. Built-in PageUp/PageDown (plugins can override via handle_key above)
            if key.modifiers.is_empty() {
                match key.key {
                    Key::PageUp => {
                        let cmd = Command::SendToKakoune(KasaneRequest::Scroll {
                            amount: -(state.available_height() as i32),
                            line: state.cursor_pos.line as u32,
                            column: state.cursor_pos.column as u32,
                        });
                        return (DirtyFlags::empty(), vec![cmd]);
                    }
                    Key::PageDown => {
                        let cmd = Command::SendToKakoune(KasaneRequest::Scroll {
                            amount: state.available_height() as i32,
                            line: state.cursor_pos.line as u32,
                            column: state.cursor_pos.column as u32,
                        });
                        return (DirtyFlags::empty(), vec![cmd]);
                    }
                    _ => {}
                }
            }

            // 4. Forward to Kakoune
            let kak_key = input::key_to_kakoune(&key);
            let cmd = Command::SendToKakoune(KasaneRequest::Keys(vec![kak_key]));
            (DirtyFlags::empty(), vec![cmd])
        }
        Msg::Mouse(mouse) => {
            // Update drag state
            match mouse.kind {
                input::MouseEventKind::Press(button) => {
                    state.drag = DragState::Active {
                        button,
                        start_line: mouse.line,
                        start_column: mouse.column,
                    };
                }
                input::MouseEventKind::Release(_) => {
                    state.drag = DragState::None;
                }
                _ => {}
            }

            // Notify all plugins (observe only, independent of hit test)
            for plugin in registry.plugins_mut() {
                plugin.observe_mouse(&mouse, state);
            }

            // Plugin mouse handling: route click/press to plugins via hit test
            if let Some(id) = registry.hit_test(mouse.column as u16, mouse.line as u16) {
                tracing::debug!(id = ?id, col = mouse.column, line = mouse.line, "hit_test matched");
                for plugin in registry.plugins_mut() {
                    if let Some(mut commands) = plugin.handle_mouse(&mouse, id, state) {
                        tracing::debug!(count = commands.len(), "handle_mouse returned commands");
                        let flags = extract_redraw_flags(&mut commands);
                        return (flags, commands);
                    }
                }
                tracing::debug!(id = ?id, "no plugin handled mouse");
            } else if matches!(mouse.kind, input::MouseEventKind::Press(_)) {
                tracing::debug!(col = mouse.column, line = mouse.line, kind = ?mouse.kind, "hit_test: no match");
            }

            // Selection-during-scroll: when dragging with left button and scrolling,
            // send scroll + mouse_move to extend selection (R-046)
            if let DragState::Active {
                button: MouseButton::Left,
                ..
            } = &state.drag
                && matches!(
                    mouse.kind,
                    input::MouseEventKind::ScrollUp | input::MouseEventKind::ScrollDown
                )
            {
                // Check info scroll first
                if handle_info_scroll(state, &mouse) {
                    return (DirtyFlags::INFO, vec![]);
                }
                if let Some(scroll_req) = input::mouse_to_kakoune(&mouse, scroll_amount) {
                    let edge_line = match mouse.kind {
                        input::MouseEventKind::ScrollUp => 0,
                        _ => state.rows.saturating_sub(2) as u32,
                    };
                    let move_req = KasaneRequest::MouseMove {
                        line: edge_line,
                        column: mouse.column,
                    };
                    return (
                        DirtyFlags::empty(),
                        vec![
                            Command::SendToKakoune(scroll_req),
                            Command::SendToKakoune(move_req),
                        ],
                    );
                }
            }

            // Check if mouse scroll targets an info popup
            if matches!(
                mouse.kind,
                input::MouseEventKind::ScrollUp | input::MouseEventKind::ScrollDown
            ) && handle_info_scroll(state, &mouse)
            {
                return (DirtyFlags::INFO, vec![]);
            }

            // Smooth scrolling: set up animation instead of immediate scroll
            if state.smooth_scroll
                && matches!(
                    mouse.kind,
                    input::MouseEventKind::ScrollUp | input::MouseEventKind::ScrollDown
                )
            {
                let amount = match mouse.kind {
                    input::MouseEventKind::ScrollUp => -scroll_amount,
                    _ => scroll_amount,
                };
                if let Some(ref mut anim) = state.scroll_animation {
                    // Accumulate into existing animation
                    anim.remaining += amount;
                    anim.line = mouse.line;
                    anim.column = mouse.column;
                } else {
                    state.scroll_animation = Some(ScrollAnimation {
                        remaining: amount,
                        step: 1,
                        line: mouse.line,
                        column: mouse.column,
                    });
                }
                return (DirtyFlags::empty(), vec![]);
            }

            let cmds = if let Some(req) = input::mouse_to_kakoune(&mouse, scroll_amount) {
                vec![Command::SendToKakoune(req)]
            } else {
                vec![]
            };
            (DirtyFlags::empty(), cmds)
        }
        Msg::Paste => (DirtyFlags::empty(), vec![Command::Paste]),
        Msg::Resize { cols, rows } => {
            state.cols = cols;
            state.rows = rows;
            grid.resize(cols, rows);
            grid.invalidate_all();
            let cmd = Command::SendToKakoune(KasaneRequest::Resize {
                rows: state.available_height(),
                cols,
            });
            (DirtyFlags::ALL, vec![cmd])
        }
        Msg::FocusGained => {
            state.focused = true;
            (DirtyFlags::ALL, vec![])
        }
        Msg::FocusLost => {
            state.focused = false;
            (DirtyFlags::ALL, vec![])
        }
    }
}

/// Check if a scroll event hits an info popup and adjust its scroll_offset.
/// Returns true if the scroll was consumed by an info popup.
fn handle_info_scroll(state: &mut AppState, mouse: &input::MouseEvent) -> bool {
    let screen_h = state.available_height();
    let mut avoid: Vec<crate::layout::Rect> = Vec::new();
    if let Some(mr) = crate::layout::get_menu_rect(state) {
        avoid.push(mr);
    }

    for info in state.infos.iter_mut().rev() {
        let win = crate::layout::layout_info(
            &info.title,
            &info.content,
            &info.anchor,
            info.style,
            state.cols,
            screen_h,
            &avoid,
        );
        if win.width == 0 || win.height == 0 {
            continue;
        }

        let mx = mouse.column as u16;
        let my = mouse.line as u16;
        if mx >= win.x && mx < win.x + win.width && my >= win.y && my < win.y + win.height {
            // Compute content height for scroll bounds
            let content_h = info
                .content
                .iter()
                .map(|line| {
                    crate::layout::word_wrap_line_height(line, win.width.saturating_sub(4).max(1))
                })
                .sum::<u16>();
            let visible_h = win.height.saturating_sub(2).max(1); // subtract borders

            match mouse.kind {
                input::MouseEventKind::ScrollUp => {
                    info.scroll_offset = info.scroll_offset.saturating_sub(3);
                }
                input::MouseEventKind::ScrollDown => {
                    let max_offset = content_h.saturating_sub(visible_h);
                    info.scroll_offset = (info.scroll_offset + 3).min(max_offset);
                }
                _ => {}
            }
            return true;
        }
    }
    false
}
