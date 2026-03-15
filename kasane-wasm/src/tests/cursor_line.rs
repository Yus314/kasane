use super::*;

#[test]
fn plugin_id() {
    let plugin = load_cursor_line_plugin();
    assert_eq!(plugin.id().0, "cursor_line");
}

#[test]
fn highlight_active_line() {
    let mut plugin = load_cursor_line_plugin();
    let mut state = AppState::default();
    state.cursor_pos.line = 3;
    plugin.on_state_changed(&state, DirtyFlags::BUFFER);

    let ctx = default_annotate_ctx();
    let ann = plugin.annotate_line_with_ctx(3, &state, &ctx);
    assert!(ann.is_some());
    let ann = ann.unwrap();
    assert!(ann.background.is_some());
    let bg = ann.background.unwrap();
    assert_eq!(
        bg.face.bg,
        Color::Rgb {
            r: 40,
            g: 40,
            b: 50
        }
    );
}

#[test]
fn no_highlight_on_other_lines() {
    let mut plugin = load_cursor_line_plugin();
    let mut state = AppState::default();
    state.cursor_pos.line = 3;
    plugin.on_state_changed(&state, DirtyFlags::BUFFER);

    let ctx = default_annotate_ctx();
    assert!(plugin.annotate_line_with_ctx(0, &state, &ctx).is_none());
    assert!(plugin.annotate_line_with_ctx(2, &state, &ctx).is_none());
    assert!(plugin.annotate_line_with_ctx(4, &state, &ctx).is_none());
}

#[test]
fn tracks_cursor_movement() {
    let mut plugin = load_cursor_line_plugin();
    let mut state = AppState::default();
    let ctx = default_annotate_ctx();

    state.cursor_pos.line = 0;
    plugin.on_state_changed(&state, DirtyFlags::BUFFER);
    assert!(plugin.annotate_line_with_ctx(0, &state, &ctx).is_some());
    assert!(plugin.annotate_line_with_ctx(5, &state, &ctx).is_none());

    state.cursor_pos.line = 5;
    plugin.on_state_changed(&state, DirtyFlags::BUFFER);
    assert!(plugin.annotate_line_with_ctx(0, &state, &ctx).is_none());
    assert!(plugin.annotate_line_with_ctx(5, &state, &ctx).is_some());
}

#[test]
fn state_hash_changes_on_line_change() {
    let mut plugin = load_cursor_line_plugin();
    let h1 = plugin.state_hash();

    let mut state = AppState::default();
    state.cursor_pos.line = 10;
    plugin.on_state_changed(&state, DirtyFlags::BUFFER);
    let h2 = plugin.state_hash();

    assert_ne!(h1, h2);
}

#[test]
fn on_init_and_shutdown_do_not_panic() {
    let mut plugin = load_cursor_line_plugin();
    let state = AppState::default();
    let cmds = plugin.on_init(&state);
    assert!(cmds.is_empty());
    plugin.on_shutdown();
}

#[test]
fn contribute_returns_none() {
    let mut plugin = load_cursor_line_plugin();
    let state = AppState::default();
    plugin.on_state_changed(&state, DirtyFlags::BUFFER);
    let ctx = default_contribute_ctx(&state);
    // cursor-line plugin has no slot contributions
    assert!(
        plugin
            .contribute_to(&SlotId::BUFFER_LEFT, &state, &ctx)
            .is_none()
    );
}
