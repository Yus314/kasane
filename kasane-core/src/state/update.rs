use crate::input;
use crate::input::{DropEvent, InputEvent, KeyEvent, MouseEvent};
use crate::plugin::{
    AppView, Command, KeyDispatchResult, MouseHandleResult, PluginEffects, PluginId,
    TextInputHandleResult, extract_redraw_flags,
};
use crate::protocol::{KakouneRequest, KasaneRequest};
use crate::scroll::{LegacyScrollDispatch, ScrollPlan};

use super::{AppState, DirtyFlags, DragState};

/// Messages that drive the application state machine.
pub enum Msg {
    Kakoune(KakouneRequest),
    Key(KeyEvent),
    TextInput(String),
    Mouse(MouseEvent),
    ClipboardPaste,
    Drop(DropEvent),
    Resize { cols: u16, rows: u16 },
    FocusGained,
    FocusLost,
}

impl From<InputEvent> for Msg {
    fn from(event: InputEvent) -> Self {
        match event {
            InputEvent::Key(key) => Msg::Key(key),
            InputEvent::TextInput(text) => Msg::TextInput(text),
            InputEvent::Mouse(mouse) => Msg::Mouse(mouse),
            InputEvent::Paste(text) => Msg::TextInput(text),
            InputEvent::Drop(drop) => Msg::Drop(drop),
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

/// TEA-pure update: takes ownership of state and returns it alongside effects.
///
/// The implementation mutates `state` in place (through `DerefMut` on `Box`),
/// but the ownership-passing signature makes the data flow explicit and enables
/// future snapshot/rollback without changing callers.
pub fn update<E: PluginEffects>(
    mut state: Box<AppState>,
    msg: Msg,
    effects: &mut E,
    scroll_amount: i32,
) -> (Box<AppState>, UpdateResult) {
    let result = update_inner(&mut state, msg, effects, scroll_amount);
    (state, result)
}

/// Convenience wrapper: calls [`update()`] in-place on a `Box<AppState>`.
///
/// Useful for tests and call sites that hold a `Box<AppState>` but don't need
/// the ownership-passing signature.
pub fn update_in_place<E: PluginEffects>(
    state: &mut Box<AppState>,
    msg: Msg,
    effects: &mut E,
    scroll_amount: i32,
) -> UpdateResult {
    let s = std::mem::take(state);
    let (s, result) = update(s, msg, effects, scroll_amount);
    *state = s;
    result
}

fn update_inner<E: PluginEffects>(
    state: &mut AppState,
    msg: Msg,
    effects: &mut E,
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
            let mut scroll_plans = Vec::new();
            if !flags.is_empty() {
                let mut batch = effects.notify_state_changed(&AppView::new(state), flags);
                scroll_plans.append(&mut batch.effects.scroll_plans);
                commands.append(&mut batch.effects.commands);
                let effect_flags = batch.effects.redraw;
                let extra_flags = effect_flags | extract_redraw_flags(&mut commands);
                return UpdateResult {
                    flags: flags | extra_flags,
                    commands,
                    scroll_plans,
                    source_plugin: None,
                };
            }
            UpdateResult {
                flags,
                commands,
                scroll_plans,
                source_plugin: None,
            }
        }
        Msg::Key(key) => {
            // 1. Notify all plugins (observe only, cannot consume)
            effects.observe_key_all(&key, &AppView::new(state));

            // 2. Plugin key middleware chain.
            // PageUp/PageDown are handled by BuiltinInputPlugin (lowest priority).
            match effects.dispatch_key_middleware(&key, &AppView::new(state)) {
                KeyDispatchResult::Consumed {
                    source_plugin,
                    mut commands,
                } => {
                    let flags = extract_redraw_flags(&mut commands);
                    UpdateResult {
                        flags,
                        commands,
                        scroll_plans: vec![],
                        source_plugin: Some(source_plugin),
                    }
                }
                KeyDispatchResult::Passthrough(final_key) => {
                    // 3. Forward the final transformed key to Kakoune.
                    let kak_key = input::key_to_kakoune(&final_key);
                    let cmd = Command::SendToKakoune(KasaneRequest::Keys(vec![kak_key]));
                    UpdateResult {
                        flags: DirtyFlags::empty(),
                        commands: vec![cmd],
                        scroll_plans: vec![],
                        source_plugin: None,
                    }
                }
            }
        }
        Msg::TextInput(text) => {
            let app = AppView::new(state);
            effects.observe_text_input_all(&text, &app);

            match effects.dispatch_text_input_handler(&text, &app) {
                TextInputHandleResult::Handled {
                    source_plugin,
                    mut commands,
                } => {
                    let flags = extract_redraw_flags(&mut commands);
                    UpdateResult {
                        flags,
                        commands,
                        scroll_plans: vec![],
                        source_plugin: Some(source_plugin),
                    }
                }
                TextInputHandleResult::NotHandled => UpdateResult {
                    flags: DirtyFlags::empty(),
                    commands: vec![Command::InsertText(text)],
                    scroll_plans: vec![],
                    source_plugin: None,
                },
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
            effects.observe_mouse_all(&mouse, &AppView::new(state));

            // Plugin mouse handling: route click/press to plugins via hit test
            if let Some(id) = state.hit_map.test(mouse.column as u16, mouse.line as u16) {
                tracing::debug!(id = ?id, col = mouse.column, line = mouse.line, "hit_test matched");
                match effects.dispatch_mouse_handler(&mouse, id, &AppView::new(state)) {
                    MouseHandleResult::Handled {
                        source_plugin,
                        mut commands,
                    } => {
                        tracing::debug!(count = commands.len(), "handle_mouse returned commands");
                        let flags = extract_redraw_flags(&mut commands);
                        return UpdateResult {
                            flags,
                            commands,
                            scroll_plans: vec![],
                            source_plugin: Some(source_plugin),
                        };
                    }
                    MouseHandleResult::NotHandled => {
                        tracing::debug!(id = ?id, "no plugin handled mouse");
                    }
                }
            } else if matches!(mouse.kind, input::MouseEventKind::Press(_)) {
                tracing::debug!(col = mouse.column, line = mouse.line, kind = ?mouse.kind, "hit_test: no match");
            }

            // Temporarily take the hit_map to avoid split-borrow conflict
            // (dispatch_legacy_mouse_scroll needs &mut state and &HitMap simultaneously)
            let hit_map = std::mem::take(&mut state.hit_map);
            let scroll_result = crate::scroll::dispatch_legacy_mouse_scroll(
                state,
                &mouse,
                &hit_map,
                effects,
                scroll_amount,
            );
            state.hit_map = hit_map;
            match scroll_result {
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

            // Display unit dispatch (ρ₂'): when display transforms are active,
            // dispatch based on NavigationPolicy for the hit display unit.
            if let Some(unit) = state
                .display_unit_map
                .as_ref()
                .and_then(|dum| dum.hit_test(mouse.line, state.display_scroll_offset))
            {
                use crate::display::{
                    ActionResult, NavigationAction, NavigationPolicy, UnitSource,
                };
                let suppressed = UpdateResult {
                    flags: DirtyFlags::empty(),
                    commands: vec![],
                    scroll_plans: vec![],
                    source_plugin: None,
                };
                let policy = effects.resolve_navigation_policy(unit);
                match policy {
                    NavigationPolicy::Normal => {
                        // Fall through to mouse_to_kakoune
                    }
                    NavigationPolicy::Skip => {
                        return suppressed;
                    }
                    NavigationPolicy::Boundary { action } => {
                        // Only activate on press (not drag/move/scroll)
                        if matches!(mouse.kind, input::MouseEventKind::Press(_)) {
                            let result = effects.dispatch_navigation_action(unit, action.clone());
                            match result {
                                ActionResult::Handled => {
                                    return UpdateResult {
                                        flags: DirtyFlags::BUFFER_CONTENT,
                                        ..suppressed
                                    };
                                }
                                ActionResult::SendKeys(keys) => {
                                    return UpdateResult {
                                        commands: vec![Command::SendToKakoune(
                                            KasaneRequest::Keys(vec![keys]),
                                        )],
                                        ..suppressed
                                    };
                                }
                                ActionResult::Pass => {
                                    // Built-in fallback: fold toggle
                                    if let NavigationAction::ToggleFold = &action
                                        && let UnitSource::LineRange(ref range) = unit.source
                                    {
                                        state.fold_toggle_state.toggle(range);
                                        return UpdateResult {
                                            flags: DirtyFlags::BUFFER_CONTENT,
                                            ..suppressed
                                        };
                                    }
                                }
                            }
                        }
                        // Non-press events on Boundary, or unhandled actions: suppress
                        return suppressed;
                    }
                }
            }

            let cmds = if let Some(req) = input::mouse_to_kakoune(
                &mouse,
                scroll_amount,
                state.display_map.as_deref(),
                state.display_scroll_offset,
            ) {
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
        Msg::Drop(drop) => {
            // 1. Broadcast observation
            effects.observe_drop_all(&drop, &AppView::new(state));

            // 2. Hit-test + owner-based dispatch
            if let Some(id) = state.hit_map.test(drop.col, drop.row) {
                match effects.dispatch_drop_handler(&drop, id, &AppView::new(state)) {
                    MouseHandleResult::Handled {
                        source_plugin,
                        mut commands,
                    } => {
                        let flags = extract_redraw_flags(&mut commands);
                        return UpdateResult {
                            flags,
                            commands,
                            scroll_plans: vec![],
                            source_plugin: Some(source_plugin),
                        };
                    }
                    MouseHandleResult::NotHandled => {}
                }
            }

            // 3. Default: edit each dropped file
            let commands = drop
                .paths
                .iter()
                .map(|p| {
                    Command::kakoune_command(&format!("edit {}", input::kakoune_quote_path(p)))
                })
                .collect();

            UpdateResult {
                flags: DirtyFlags::empty(),
                commands,
                scroll_plans: vec![],
                source_plugin: None,
            }
        }
        Msg::ClipboardPaste => UpdateResult {
            flags: DirtyFlags::empty(),
            commands: vec![Command::PasteClipboard],
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
