use crate::input;
use crate::input::{DropEvent, InputEvent, KeyEvent, MouseEvent};
use crate::plugin::{
    AppView, Command, KeyDispatchResult, KeyPreDispatchResult, MouseHandleResult,
    MousePreDispatchResult, PluginEffects, PluginId, TextInputHandleResult,
    TextInputPreDispatchResult, extract_drag_state_update, extract_redraw_flags,
    extract_shadow_cursor_update,
};
use crate::protocol::{KakouneRequest, KasaneRequest};
use crate::scroll::{LegacyScrollDispatch, ScrollPlan};

use super::shadow_cursor::{ShadowCursor, ShadowPhase};
use super::{AppState, DirtyFlags};

/// Messages that drive the application state machine.
pub enum Msg {
    Kakoune(KakouneRequest),
    Key(KeyEvent),
    TextInput(String),
    Mouse(MouseEvent),
    /// Explicit clipboard paste request (not from bracketed paste payload).
    ///
    /// No `InputEvent` variant maps to this — it is constructed directly
    /// when a keybinding or command explicitly requests a clipboard paste.
    /// Bracketed paste payloads arrive as `InputEvent::Paste(text)` and
    /// are mapped to `Msg::TextInput(text)` instead.
    ClipboardPaste,
    Drop(DropEvent),
    Resize {
        cols: u16,
        rows: u16,
    },
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

impl Default for UpdateResult {
    fn default() -> Self {
        Self {
            flags: DirtyFlags::empty(),
            commands: vec![],
            scroll_plans: vec![],
            source_plugin: None,
        }
    }
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
                if let Some(sc) = extract_shadow_cursor_update(&mut commands) {
                    state.runtime.shadow_cursor = sc;
                }
                let effect_flags = batch.effects.redraw;
                let extra_flags = effect_flags | extract_redraw_flags(&mut commands);
                return UpdateResult {
                    flags: flags | extra_flags,
                    commands,
                    scroll_plans,
                    ..Default::default()
                };
            }
            UpdateResult {
                flags,
                commands,
                scroll_plans,
                ..Default::default()
            }
        }
        Msg::Key(key) => {
            // Pre-dispatch: plugins with KEY_PRE_DISPATCH capability
            // (e.g., BuiltinShadowCursorPlugin) intercept keys before middleware.
            match effects.dispatch_key_pre_dispatch(&key, &AppView::new(state)) {
                KeyPreDispatchResult::Consumed {
                    flags,
                    mut commands,
                } => {
                    if let Some(sc) = extract_shadow_cursor_update(&mut commands) {
                        state.runtime.shadow_cursor = sc;
                    }
                    let extra_flags = extract_redraw_flags(&mut commands);
                    return UpdateResult {
                        flags: flags | extra_flags,
                        commands,
                        ..Default::default()
                    };
                }
                KeyPreDispatchResult::Pass { mut commands } => {
                    if let Some(sc) = extract_shadow_cursor_update(&mut commands) {
                        state.runtime.shadow_cursor = sc;
                    }
                    // Fall through to normal key handling
                }
            }

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
                        source_plugin: Some(source_plugin),
                        ..Default::default()
                    }
                }
                KeyDispatchResult::Passthrough(final_key) => {
                    // 3. Forward the final transformed key to Kakoune.
                    let kak_key = input::key_to_kakoune(&final_key);
                    let cmd = Command::SendToKakoune(KasaneRequest::Keys(vec![kak_key]));
                    UpdateResult {
                        commands: vec![cmd],
                        ..Default::default()
                    }
                }
            }
        }
        Msg::TextInput(text) => {
            // Pre-dispatch: plugins with KEY_PRE_DISPATCH capability
            // (e.g., BuiltinShadowCursorPlugin) intercept text input during editing.
            match effects.dispatch_text_input_pre_dispatch(&text, &AppView::new(state)) {
                TextInputPreDispatchResult::Consumed {
                    flags,
                    mut commands,
                } => {
                    if let Some(sc) = extract_shadow_cursor_update(&mut commands) {
                        state.runtime.shadow_cursor = sc;
                    }
                    let extra_flags = extract_redraw_flags(&mut commands);
                    return UpdateResult {
                        flags: flags | extra_flags,
                        commands,
                        ..Default::default()
                    };
                }
                TextInputPreDispatchResult::Pass => {}
            }

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
                        source_plugin: Some(source_plugin),
                        ..Default::default()
                    }
                }
                TextInputHandleResult::NotHandled => UpdateResult {
                    commands: vec![Command::InsertText(text)],
                    ..Default::default()
                },
            }
        }
        Msg::Mouse(mouse) => {
            // Pre-dispatch: plugins with MOUSE_PRE_DISPATCH capability
            // (e.g., BuiltinShadowCursorPlugin) intercept mouse before observation.
            match effects.dispatch_mouse_pre_dispatch(&mouse, &AppView::new(state)) {
                MousePreDispatchResult::Consumed {
                    flags,
                    mut commands,
                } => {
                    if let Some(sc) = extract_shadow_cursor_update(&mut commands) {
                        state.runtime.shadow_cursor = sc;
                    }
                    if let Some(drag) = extract_drag_state_update(&mut commands) {
                        state.runtime.drag = drag;
                    }
                    let extra_flags = extract_redraw_flags(&mut commands);
                    return UpdateResult {
                        flags: flags | extra_flags,
                        commands,
                        ..Default::default()
                    };
                }
                MousePreDispatchResult::Pass { mut commands } => {
                    if let Some(sc) = extract_shadow_cursor_update(&mut commands) {
                        state.runtime.shadow_cursor = sc;
                    }
                    if let Some(drag) = extract_drag_state_update(&mut commands) {
                        state.runtime.drag = drag;
                    }
                }
            }

            // Notify all plugins (observe only, independent of hit test)
            effects.observe_mouse_all(&mouse, &AppView::new(state));

            // Plugin mouse handling: route click/press to plugins via hit test
            if let Some(id) = state
                .runtime
                .hit_map
                .test(mouse.column as u16, mouse.line as u16)
            {
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
                            source_plugin: Some(source_plugin),
                            ..Default::default()
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
            let hit_map = std::mem::take(&mut state.runtime.hit_map);
            let scroll_result = crate::scroll::dispatch_legacy_mouse_scroll(
                state,
                &mouse,
                &hit_map,
                effects,
                scroll_amount,
            );
            state.runtime.hit_map = hit_map;
            match scroll_result {
                LegacyScrollDispatch::ConsumedInfo => {
                    return UpdateResult {
                        flags: DirtyFlags::INFO,
                        ..Default::default()
                    };
                }
                LegacyScrollDispatch::Requests(requests) => {
                    let commands = requests.into_iter().map(Command::SendToKakoune).collect();
                    return UpdateResult {
                        commands,
                        ..Default::default()
                    };
                }
                LegacyScrollDispatch::Plan(plan) => {
                    return UpdateResult {
                        scroll_plans: vec![plan],
                        ..Default::default()
                    };
                }
                LegacyScrollDispatch::NotHandled => {}
            }

            // Display unit dispatch (ρ₂'): when display transforms are active,
            // dispatch based on NavigationPolicy for the hit display unit.
            if let Some(result) = dispatch_display_unit_mouse(state, effects, &mouse) {
                return result;
            }

            let cmds = effects
                .dispatch_mouse_fallback(&mouse, scroll_amount, &AppView::new(state))
                .unwrap_or_default();
            UpdateResult {
                commands: cmds,
                ..Default::default()
            }
        }
        Msg::Drop(drop) => {
            // 1. Broadcast observation
            effects.observe_drop_all(&drop, &AppView::new(state));

            // 2. Hit-test + owner-based dispatch
            if let Some(id) = state.runtime.hit_map.test(drop.col, drop.row) {
                match effects.dispatch_drop_handler(&drop, id, &AppView::new(state)) {
                    MouseHandleResult::Handled {
                        source_plugin,
                        mut commands,
                    } => {
                        let flags = extract_redraw_flags(&mut commands);
                        return UpdateResult {
                            flags,
                            commands,
                            source_plugin: Some(source_plugin),
                            ..Default::default()
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
                commands,
                ..Default::default()
            }
        }
        Msg::ClipboardPaste => UpdateResult {
            commands: vec![Command::PasteClipboard],
            ..Default::default()
        },
        Msg::Resize { cols, rows } => {
            state.runtime.cols = cols;
            state.runtime.rows = rows;
            let cmd = Command::SendToKakoune(KasaneRequest::Resize {
                rows: state.available_height(),
                cols,
            });
            UpdateResult {
                flags: DirtyFlags::ALL,
                commands: vec![cmd],
                ..Default::default()
            }
        }
        Msg::FocusGained => {
            state.runtime.focused = true;
            UpdateResult {
                flags: DirtyFlags::ALL,
                ..Default::default()
            }
        }
        Msg::FocusLost => {
            state.runtime.focused = false;
            UpdateResult {
                flags: DirtyFlags::ALL,
                ..Default::default()
            }
        }
    }
}

/// Dispatch mouse event through display unit navigation policy.
/// Returns `Some(result)` if handled, `None` to fall through to mouse_to_kakoune.
fn dispatch_display_unit_mouse<E: PluginEffects>(
    state: &mut AppState,
    effects: &mut E,
    mouse: &MouseEvent,
) -> Option<UpdateResult> {
    use crate::display::{ActionResult, NavigationAction, NavigationPolicy, UnitSource};

    let unit = state
        .runtime
        .display_unit_map
        .as_ref()
        .and_then(|dum| dum.hit_test(mouse.line, state.runtime.display_scroll_offset))?;

    let policy = effects.resolve_navigation_policy(unit);
    match policy {
        NavigationPolicy::Normal => None,
        NavigationPolicy::Skip => Some(UpdateResult::default()),
        NavigationPolicy::Boundary { action } => {
            if matches!(mouse.kind, input::MouseEventKind::Press(_)) {
                let result = effects.dispatch_navigation_action(unit, action.clone());
                match result {
                    ActionResult::Handled => {
                        return Some(UpdateResult {
                            flags: DirtyFlags::BUFFER_CONTENT,
                            ..Default::default()
                        });
                    }
                    ActionResult::SendKeys(keys) => {
                        return Some(UpdateResult {
                            commands: vec![Command::SendToKakoune(KasaneRequest::Keys(vec![keys]))],
                            ..Default::default()
                        });
                    }
                    ActionResult::ToggleFold(range) => {
                        // Per-projection fold state scoping
                        if let Some(active_id) = state.config.projection_policy.active_structural()
                        {
                            state
                                .config
                                .projection_policy
                                .fold_state_for_mut(&active_id.clone())
                                .toggle(&range);
                        } else {
                            state.config.fold_toggle_state.toggle(&range);
                        }
                        return Some(UpdateResult {
                            flags: DirtyFlags::BUFFER_CONTENT,
                            ..Default::default()
                        });
                    }
                    ActionResult::Pass => {
                        // Built-in fallback: shadow cursor activation
                        if let NavigationAction::ActivateShadowCursor = &action
                            && let UnitSource::ProjectedLine { anchor, spans: _ } = &unit.source
                        {
                            let owner = crate::plugin::PluginId(String::new());
                            state.runtime.shadow_cursor = Some(ShadowCursor {
                                display_line: unit.display_line,
                                span_index: 0,
                                phase: ShadowPhase::Navigating,
                                owner_plugin: owner,
                            });
                            let _ = anchor;
                            return Some(UpdateResult {
                                flags: DirtyFlags::BUFFER_CONTENT,
                                ..Default::default()
                            });
                        }
                    }
                }
            }
            // Non-press events on Boundary, or unhandled actions: suppress
            Some(UpdateResult::default())
        }
    }
}
