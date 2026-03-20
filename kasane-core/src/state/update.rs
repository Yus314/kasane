use crate::input;
use crate::input::{InputEvent, KeyEvent, MouseEvent};
use crate::plugin::{Command, PluginId, PluginRegistry, extract_redraw_flags};
use crate::protocol::{KakouneRequest, KasaneRequest};
use crate::scroll::{LegacyScrollDispatch, ScrollPlan};

use super::{AppState, DirtyFlags, DragState};

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
pub struct UpdateResult {
    pub flags: DirtyFlags,
    pub commands: Vec<Command>,
    pub scroll_plans: Vec<ScrollPlan>,
    pub source_plugin: Option<PluginId>,
}

pub fn update(
    state: &mut AppState,
    msg: Msg,
    registry: &mut PluginRegistry,
    scroll_amount: i32,
) -> UpdateResult {
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
            UpdateResult {
                flags: flags | extra_flags,
                commands,
                scroll_plans: vec![],
                source_plugin: None,
            }
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
                    return UpdateResult {
                        flags,
                        commands,
                        scroll_plans: vec![],
                        source_plugin: Some(source),
                    };
                }
            }

            // 3. Forward to Kakoune
            let kak_key = input::key_to_kakoune(&key);
            let cmd = Command::SendToKakoune(KasaneRequest::Keys(vec![kak_key]));
            UpdateResult {
                flags: DirtyFlags::empty(),
                commands: vec![cmd],
                scroll_plans: vec![],
                source_plugin: None,
            }
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
                        return UpdateResult {
                            flags,
                            commands,
                            scroll_plans: vec![],
                            source_plugin: Some(source),
                        };
                    }
                }
                tracing::debug!(id = ?id, "no plugin handled mouse");
            } else if matches!(mouse.kind, input::MouseEventKind::Press(_)) {
                tracing::debug!(col = mouse.column, line = mouse.line, kind = ?mouse.kind, "hit_test: no match");
            }

            match crate::scroll::dispatch_legacy_mouse_scroll(
                state,
                &mouse,
                registry,
                scroll_amount,
            ) {
                LegacyScrollDispatch::ConsumedInfo => {
                    return UpdateResult {
                        flags: DirtyFlags::INFO,
                        commands: vec![],
                        scroll_plans: vec![],
                        source_plugin: None,
                    };
                }
                LegacyScrollDispatch::Requests(requests) => {
                    let commands = requests.into_iter().map(Command::SendToKakoune).collect();
                    return UpdateResult {
                        flags: DirtyFlags::empty(),
                        commands,
                        scroll_plans: vec![],
                        source_plugin: None,
                    };
                }
                LegacyScrollDispatch::Plan(plan) => {
                    return UpdateResult {
                        flags: DirtyFlags::empty(),
                        commands: vec![],
                        scroll_plans: vec![plan],
                        source_plugin: None,
                    };
                }
                LegacyScrollDispatch::NotHandled => {}
            }

            let cmds = if let Some(req) = input::mouse_to_kakoune(&mouse, scroll_amount, None) {
                vec![Command::SendToKakoune(req)]
            } else {
                vec![]
            };
            UpdateResult {
                flags: DirtyFlags::empty(),
                commands: cmds,
                scroll_plans: vec![],
                source_plugin: None,
            }
        }
        Msg::Paste => UpdateResult {
            flags: DirtyFlags::empty(),
            commands: vec![Command::Paste],
            scroll_plans: vec![],
            source_plugin: None,
        },
        Msg::Resize { cols, rows } => {
            state.cols = cols;
            state.rows = rows;
            let cmd = Command::SendToKakoune(KasaneRequest::Resize {
                rows: state.available_height(),
                cols,
            });
            UpdateResult {
                flags: DirtyFlags::ALL,
                commands: vec![cmd],
                scroll_plans: vec![],
                source_plugin: None,
            }
        }
        Msg::FocusGained => {
            state.focused = true;
            UpdateResult {
                flags: DirtyFlags::ALL,
                commands: vec![],
                scroll_plans: vec![],
                source_plugin: None,
            }
        }
        Msg::FocusLost => {
            state.focused = false;
            UpdateResult {
                flags: DirtyFlags::ALL,
                commands: vec![],
                scroll_plans: vec![],
                source_plugin: None,
            }
        }
    }
}
