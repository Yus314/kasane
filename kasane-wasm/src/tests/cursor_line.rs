use super::*;

fn apply_cursor_line_state_change(
    plugin: &mut crate::WasmPlugin,
    state: &AppState,
    dirty: DirtyFlags,
) {
    let effects = plugin.on_state_changed_effects(&AppView::new(state), dirty);
    assert!(effects.redraw.is_empty());
    assert!(effects.commands.is_empty());
    assert!(effects.scroll_plans.is_empty());
}

#[test]
fn plugin_id() {
    let plugin = load_cursor_line_plugin();
    assert_eq!(plugin.id().0, "cursor_line");
}

#[test]
fn highlight_active_line() {
    let mut plugin = load_cursor_line_plugin();
    let mut state = AppState::default();
    state.observed.cursor_pos.line = 3;
    apply_cursor_line_state_change(&mut plugin, &state, DirtyFlags::BUFFER);

    assert!(plugin.has_unified_display());
    let directives = plugin.unified_display(&AppView::new(&state));
    assert_eq!(directives.len(), 1);
    match &directives[0] {
        DisplayDirective::StyleLine { line, face, .. } => {
            assert_eq!(*line, 3);
            assert_eq!(
                face.bg,
                Color::Rgb {
                    r: 40,
                    g: 40,
                    b: 50
                }
            );
        }
        other => panic!("expected StyleLine, got {:?}", other),
    }
}

#[test]
fn no_highlight_on_other_lines() {
    let mut plugin = load_cursor_line_plugin();
    let mut state = AppState::default();
    state.observed.cursor_pos.line = 3;
    apply_cursor_line_state_change(&mut plugin, &state, DirtyFlags::BUFFER);

    let directives = plugin.unified_display(&AppView::new(&state));
    // Should only contain line 3
    for d in &directives {
        match d {
            DisplayDirective::StyleLine { line, .. } => assert_eq!(*line, 3),
            other => panic!("expected StyleLine, got {:?}", other),
        }
    }
}

#[test]
fn tracks_cursor_movement() {
    let mut plugin = load_cursor_line_plugin();
    let mut state = AppState::default();

    state.observed.cursor_pos.line = 0;
    apply_cursor_line_state_change(&mut plugin, &state, DirtyFlags::BUFFER);
    let directives = plugin.unified_display(&AppView::new(&state));
    assert!(
        directives
            .iter()
            .any(|d| matches!(d, DisplayDirective::StyleLine { line: 0, .. }))
    );
    assert!(
        !directives
            .iter()
            .any(|d| matches!(d, DisplayDirective::StyleLine { line: 5, .. }))
    );

    state.observed.cursor_pos.line = 5;
    apply_cursor_line_state_change(&mut plugin, &state, DirtyFlags::BUFFER);
    let directives = plugin.unified_display(&AppView::new(&state));
    assert!(
        !directives
            .iter()
            .any(|d| matches!(d, DisplayDirective::StyleLine { line: 0, .. }))
    );
    assert!(
        directives
            .iter()
            .any(|d| matches!(d, DisplayDirective::StyleLine { line: 5, .. }))
    );
}

#[test]
fn state_hash_changes_on_line_change() {
    let mut plugin = load_cursor_line_plugin();
    let h1 = plugin.state_hash();

    let mut state = AppState::default();
    state.observed.cursor_pos.line = 10;
    apply_cursor_line_state_change(&mut plugin, &state, DirtyFlags::BUFFER);
    let h2 = plugin.state_hash();

    assert_ne!(h1, h2);
}

#[test]
fn typed_state_changed_effects_updates_state_hash() {
    let mut plugin = load_cursor_line_plugin();
    let h1 = plugin.state_hash();

    let mut state = AppState::default();
    state.observed.cursor_pos.line = 12;
    let effects = plugin.on_state_changed_effects(&AppView::new(&state), DirtyFlags::BUFFER);
    let h2 = plugin.state_hash();

    assert_eq!(effects.redraw, DirtyFlags::empty());
    assert!(effects.commands.is_empty());
    assert!(effects.scroll_plans.is_empty());
    assert_ne!(h1, h2);
}

#[test]
fn on_init_and_shutdown_do_not_panic() {
    let mut plugin = load_cursor_line_plugin();
    let state = AppState::default();
    let effects = plugin.on_init_effects(&AppView::new(&state));
    assert!(effects.redraw.is_empty());
    plugin.on_shutdown();
}

#[test]
fn contribute_returns_none() {
    let mut plugin = load_cursor_line_plugin();
    let state = AppState::default();
    apply_cursor_line_state_change(&mut plugin, &state, DirtyFlags::BUFFER);
    let ctx = default_contribute_ctx(&state);
    // cursor-line plugin has no slot contributions
    assert!(
        plugin
            .contribute_to(&SlotId::BUFFER_LEFT, &AppView::new(&state), &ctx)
            .is_none()
    );
}
