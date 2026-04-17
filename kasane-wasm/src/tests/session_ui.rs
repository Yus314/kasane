use super::*;
use kasane_core::input::{Key, KeyEvent, KeyResponse, Modifiers};
use kasane_core::plugin::{Command, PluginBackend};
use kasane_core::session::{SessionCommand, SessionDescriptor};

fn apply_session_state_change(plugin: &mut crate::WasmPlugin, state: &AppState, dirty: DirtyFlags) {
    let effects = plugin.on_state_changed_effects(&AppView::new(state), dirty);
    assert!(effects.redraw.is_empty());
    assert!(effects.commands.is_empty());
    assert!(effects.scroll_plans.is_empty());
}

/// Dispatch a key through the key map path (like the registry does).
/// Refreshes group active flags, matches the key, and calls invoke_action.
/// Returns `Some(commands)` if consumed, `None` if passed.
fn dispatch_key(
    plugin: &mut crate::WasmPlugin,
    key: &KeyEvent,
    state: &AppState,
) -> Option<Vec<Command>> {
    let app = AppView::new(state);

    // Refresh group active flags
    plugin.refresh_key_groups(&app);

    // Try to match key in compiled key map
    if let Some(map) = plugin.compiled_key_map() {
        if let Some(action_id) = map.match_key(key) {
            let response = plugin.invoke_action(action_id, key, &app);
            return match response {
                KeyResponse::Pass => None,
                KeyResponse::Consume => Some(vec![]),
                KeyResponse::ConsumeRedraw => Some(vec![]),
                KeyResponse::ConsumeWith(cmds) => Some(cmds),
            };
        }
    }

    // Fall back to legacy handle_key
    plugin.handle_key(key, &app)
}

fn ctrl_t_event() -> KeyEvent {
    KeyEvent {
        key: Key::Char('t'),
        modifiers: Modifiers::CTRL,
    }
}

fn key_event(key: Key) -> KeyEvent {
    KeyEvent {
        key,
        modifiers: Modifiers::empty(),
    }
}

fn char_event(c: char) -> KeyEvent {
    KeyEvent {
        key: Key::Char(c),
        modifiers: Modifiers::empty(),
    }
}

fn state_with_sessions(count: usize) -> AppState {
    let mut state = AppState::default();
    state.runtime.cols = 80;
    state.runtime.rows = 24;
    state.session.session_descriptors = (0..count)
        .map(|i| SessionDescriptor {
            key: format!("session{i}"),
            session_name: None,
            buffer_name: None,
            mode_line: None,
        })
        .collect();
    state.session.active_session_key = state
        .session
        .session_descriptors
        .first()
        .map(|d| d.key.clone());
    state
}

#[test]
fn plugin_id() {
    let plugin = load_session_ui_plugin();
    assert_eq!(plugin.id().0, "session_ui");
}

#[test]
fn status_right_hidden_single_session() {
    let mut plugin = load_session_ui_plugin();
    let state = state_with_sessions(1);
    apply_session_state_change(&mut plugin, &state, DirtyFlags::SESSION);

    let ctx = default_contribute_ctx(&state);
    let result = plugin.contribute_to(&SlotId::STATUS_RIGHT, &AppView::new(&state), &ctx);
    assert!(result.is_none());
}

#[test]
fn status_right_shown_multiple_sessions() {
    let mut plugin = load_session_ui_plugin();
    let state = state_with_sessions(3);
    apply_session_state_change(&mut plugin, &state, DirtyFlags::SESSION);

    let ctx = default_contribute_ctx(&state);
    let result = plugin.contribute_to(&SlotId::STATUS_RIGHT, &AppView::new(&state), &ctx);
    assert!(result.is_some());
}

#[test]
fn ctrl_t_opens_switcher() {
    let mut plugin = load_session_ui_plugin();
    let state = state_with_sessions(2);
    apply_session_state_change(&mut plugin, &state, DirtyFlags::SESSION);

    let result = dispatch_key(&mut plugin, &ctrl_t_event(), &state);
    assert!(result.is_some());
}

#[test]
fn overlay_present_when_open() {
    let mut plugin = load_session_ui_plugin();
    let state = state_with_sessions(2);
    apply_session_state_change(&mut plugin, &state, DirtyFlags::SESSION);

    // Open switcher
    dispatch_key(&mut plugin, &ctrl_t_event(), &state);

    // Overlay should be present
    let ctx = default_overlay_ctx();
    let overlay = plugin.contribute_overlay_with_ctx(&AppView::new(&state), &ctx);
    assert!(overlay.is_some());
}

#[test]
fn overlay_absent_when_closed() {
    let mut plugin = load_session_ui_plugin();
    let state = state_with_sessions(2);
    apply_session_state_change(&mut plugin, &state, DirtyFlags::SESSION);

    // Before opening, no overlay
    let ctx = default_overlay_ctx();
    let overlay = plugin.contribute_overlay_with_ctx(&AppView::new(&state), &ctx);
    assert!(overlay.is_none());
}

#[test]
fn enter_issues_switch_command() {
    let mut plugin = load_session_ui_plugin();
    let state = state_with_sessions(3);
    apply_session_state_change(&mut plugin, &state, DirtyFlags::SESSION);

    // Open switcher
    dispatch_key(&mut plugin, &ctrl_t_event(), &state);
    // Navigate down
    dispatch_key(&mut plugin, &key_event(Key::Down), &state);
    // Select
    let result = dispatch_key(&mut plugin, &key_event(Key::Enter), &state);
    assert!(result.is_some());
    let cmds = result.unwrap();
    let has_switch = cmds
        .iter()
        .any(|c| matches!(c, Command::Session(SessionCommand::Switch { .. })));
    assert!(has_switch, "Enter should issue SwitchSession command");
}

#[test]
fn escape_closes_switcher() {
    let mut plugin = load_session_ui_plugin();
    let state = state_with_sessions(2);
    apply_session_state_change(&mut plugin, &state, DirtyFlags::SESSION);

    // Open
    dispatch_key(&mut plugin, &ctrl_t_event(), &state);
    // Close
    dispatch_key(&mut plugin, &key_event(Key::Escape), &state);

    // Overlay should be gone
    let ctx = default_overlay_ctx();
    let overlay = plugin.contribute_overlay_with_ctx(&AppView::new(&state), &ctx);
    assert!(overlay.is_none());
}

#[test]
fn enriched_descriptor_fields() {
    let mut plugin = load_session_ui_plugin();
    let mut state = AppState::default();
    state.runtime.cols = 80;
    state.runtime.rows = 24;
    state.session.session_descriptors = vec![
        SessionDescriptor {
            key: "work".into(),
            session_name: Some("project".into()),
            buffer_name: Some("main.rs".into()),
            mode_line: Some("normal".into()),
        },
        SessionDescriptor {
            key: "play".into(),
            session_name: None,
            buffer_name: Some("test.rs".into()),
            mode_line: Some("insert".into()),
        },
    ];
    state.session.active_session_key = Some("work".into());
    apply_session_state_change(&mut plugin, &state, DirtyFlags::SESSION);

    // Open switcher — the overlay should contain elements for both sessions
    dispatch_key(&mut plugin, &ctrl_t_event(), &state);
    let ctx = default_overlay_ctx();
    let overlay = plugin.contribute_overlay_with_ctx(&AppView::new(&state), &ctx);
    assert!(
        overlay.is_some(),
        "overlay should be present with enriched descriptors"
    );
}

#[test]
fn d_closes_selected_session() {
    let mut plugin = load_session_ui_plugin();
    let state = state_with_sessions(3);
    apply_session_state_change(&mut plugin, &state, DirtyFlags::SESSION);

    // Open switcher
    dispatch_key(&mut plugin, &ctrl_t_event(), &state);
    // Press 'd' to close selected session
    let result = dispatch_key(&mut plugin, &char_event('d'), &state);
    assert!(result.is_some());
    let cmds = result.unwrap();
    let has_close = cmds
        .iter()
        .any(|c| matches!(c, Command::Session(SessionCommand::Close { .. })));
    assert!(has_close, "'d' should issue CloseSession command");
}

#[test]
fn d_does_not_close_last_session() {
    let mut plugin = load_session_ui_plugin();
    let state = state_with_sessions(1);
    apply_session_state_change(&mut plugin, &state, DirtyFlags::SESSION);

    // Open switcher
    dispatch_key(&mut plugin, &ctrl_t_event(), &state);
    // Press 'd' — should NOT close the last session
    let result = dispatch_key(&mut plugin, &char_event('d'), &state);
    assert!(result.is_some());
    let cmds = result.unwrap();
    let has_close = cmds
        .iter()
        .any(|c| matches!(c, Command::Session(SessionCommand::Close { .. })));
    assert!(!has_close, "'d' should not close the last session");
}

#[test]
fn n_spawns_new_session() {
    let mut plugin = load_session_ui_plugin();
    let state = state_with_sessions(1);
    apply_session_state_change(&mut plugin, &state, DirtyFlags::SESSION);

    // Open switcher
    dispatch_key(&mut plugin, &ctrl_t_event(), &state);
    // Press 'n' to create new session
    let result = dispatch_key(&mut plugin, &char_event('n'), &state);
    assert!(result.is_some());
    let cmds = result.unwrap();
    let has_spawn = cmds
        .iter()
        .any(|c| matches!(c, Command::Session(SessionCommand::Spawn { .. })));
    assert!(has_spawn, "'n' should issue SpawnSession command");
}

#[test]
fn n_closes_switcher() {
    let mut plugin = load_session_ui_plugin();
    let state = state_with_sessions(1);
    apply_session_state_change(&mut plugin, &state, DirtyFlags::SESSION);

    // Open switcher
    dispatch_key(&mut plugin, &ctrl_t_event(), &state);
    // Press 'n'
    dispatch_key(&mut plugin, &char_event('n'), &state);

    // Switcher should be closed
    let ctx = default_overlay_ctx();
    let overlay = plugin.contribute_overlay_with_ctx(&AppView::new(&state), &ctx);
    assert!(overlay.is_none(), "switcher should close after 'n'");
}
