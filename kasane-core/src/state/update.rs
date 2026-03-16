use crate::input;
use crate::input::{InputEvent, KeyEvent, MouseEvent};
use crate::plugin::{Command, PluginId, PluginRegistry, extract_redraw_flags};
use crate::protocol::{KakouneRequest, KasaneRequest};

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
///
/// The returned `Option<PluginId>` identifies the plugin that produced the commands
/// (when a plugin's `handle_key` / `handle_mouse` won the first-wins chain).
/// This is needed so that process-related deferred commands (`SpawnProcess`, etc.)
/// can be routed to the correct plugin by `handle_deferred_commands`.
pub fn update(
    state: &mut AppState,
    msg: Msg,
    registry: &mut PluginRegistry,
    scroll_amount: i32,
) -> (DirtyFlags, Vec<Command>, Option<PluginId>) {
    match msg {
        Msg::Kakoune(req) => {
            let req_kind = match &req {
                KakouneRequest::Draw { .. } => "Draw",
                KakouneRequest::DrawStatus { .. } => "DrawStatus",
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
            (flags | extra_flags, commands, None)
        }
        Msg::Key(key) => {
            // 1. Notify all plugins (observe only, cannot consume)
            for plugin in registry.plugins_mut() {
                plugin.observe_key(&key, state);
            }

            // 2. Plugin handle_key chain (first-wins)
            // PageUp/PageDown are handled by BuiltinInputPlugin (lowest priority).
            for plugin in registry.plugins_mut() {
                if let Some(mut commands) = plugin.handle_key(&key, state) {
                    let source = plugin.id();
                    let flags = extract_redraw_flags(&mut commands);
                    return (flags, commands, Some(source));
                }
            }

            // 3. Forward to Kakoune
            let kak_key = input::key_to_kakoune(&key);
            let cmd = Command::SendToKakoune(KasaneRequest::Keys(vec![kak_key]));
            (DirtyFlags::empty(), vec![cmd], None)
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
                        let source = plugin.id();
                        tracing::debug!(count = commands.len(), "handle_mouse returned commands");
                        let flags = extract_redraw_flags(&mut commands);
                        return (flags, commands, Some(source));
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
                if handle_info_scroll(state, &mouse, registry) {
                    return (DirtyFlags::INFO, vec![], None);
                }
                if let Some(scroll_req) = input::mouse_to_kakoune(&mouse, scroll_amount, None) {
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
                        None,
                    );
                }
            }

            // Check if mouse scroll targets an info popup
            if matches!(
                mouse.kind,
                input::MouseEventKind::ScrollUp | input::MouseEventKind::ScrollDown
            ) && handle_info_scroll(state, &mouse, registry)
            {
                return (DirtyFlags::INFO, vec![], None);
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
                return (DirtyFlags::empty(), vec![], None);
            }

            let cmds = if let Some(req) = input::mouse_to_kakoune(&mouse, scroll_amount, None) {
                vec![Command::SendToKakoune(req)]
            } else {
                vec![]
            };
            (DirtyFlags::empty(), cmds, None)
        }
        Msg::Paste => (DirtyFlags::empty(), vec![Command::Paste], None),
        Msg::Resize { cols, rows } => {
            state.cols = cols;
            state.rows = rows;
            let cmd = Command::SendToKakoune(KasaneRequest::Resize {
                rows: state.available_height(),
                cols,
            });
            (DirtyFlags::ALL, vec![cmd], None)
        }
        Msg::FocusGained => {
            state.focused = true;
            (DirtyFlags::ALL, vec![], None)
        }
        Msg::FocusLost => {
            state.focused = false;
            (DirtyFlags::ALL, vec![], None)
        }
    }
}

/// Check if a scroll event hits an info popup and adjust its scroll_offset.
/// Uses the HitMap from the previous frame to identify which info popup
/// the mouse is over, avoiding duplicated layout computation.
/// Returns true if the scroll was consumed by an info popup.
fn handle_info_scroll(
    state: &mut AppState,
    mouse: &input::MouseEvent,
    registry: &PluginRegistry,
) -> bool {
    use crate::element::InteractiveId;

    let (id, rect) = match registry.hit_test_with_rect(mouse.column as u16, mouse.line as u16) {
        Some(hit) => hit,
        None => return false,
    };

    // Check if the hit is on an info popup (InteractiveId in INFO_BASE range)
    if id.0 < InteractiveId::INFO_BASE {
        return false;
    }
    let index = (id.0 - InteractiveId::INFO_BASE) as usize;
    let info = match state.infos.get_mut(index) {
        Some(info) => info,
        None => return false,
    };

    // Compute content height for scroll bounds using the rect from HitMap
    let content_h = info
        .content
        .iter()
        .map(|line| crate::layout::word_wrap_line_height(line, rect.w.saturating_sub(4).max(1)))
        .sum::<u16>();
    let visible_h = rect.h.saturating_sub(2).max(1); // subtract borders

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
    true
}
