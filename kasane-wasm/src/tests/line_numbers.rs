use super::*;

#[test]
fn plugin_id() {
    let plugin = load_line_numbers_plugin();
    assert_eq!(plugin.id().0, "wasm_line_numbers");
}

#[test]
fn contribute_buffer_left() {
    let mut plugin = load_line_numbers_plugin();
    let mut state = AppState::default();
    state.lines = vec![vec![], vec![], vec![]]; // 3 lines
    plugin.on_state_changed(&state, DirtyFlags::BUFFER);

    let ctx = default_contribute_ctx(&state);
    let contrib = plugin.contribute_to(&SlotId::BUFFER_LEFT, &state, &ctx);
    assert!(contrib.is_some());

    // Should be a column with 3 children
    match contrib.unwrap().element {
        Element::Flex {
            direction: Direction::Column,
            children,
            ..
        } => {
            assert_eq!(children.len(), 3);
            // Check first child is " 1 "
            match &children[0].element {
                Element::Text(s, _) => assert_eq!(s, " 1 "),
                other => panic!("expected Text, got {other:?}"),
            }
            // Check last child is " 3 "
            match &children[2].element {
                Element::Text(s, _) => assert_eq!(s, " 3 "),
                other => panic!("expected Text, got {other:?}"),
            }
        }
        other => panic!("expected Column Flex, got {other:?}"),
    }
}

#[test]
fn no_contribution_for_other_slots() {
    let mut plugin = load_line_numbers_plugin();
    let mut state = AppState::default();
    state.lines = vec![vec![]];
    plugin.on_state_changed(&state, DirtyFlags::BUFFER);

    let ctx = default_contribute_ctx(&state);
    assert!(
        plugin
            .contribute_to(&SlotId::BUFFER_RIGHT, &state, &ctx)
            .is_none()
    );
    assert!(
        plugin
            .contribute_to(&SlotId::STATUS_LEFT, &state, &ctx)
            .is_none()
    );
}

#[test]
fn empty_buffer_returns_none() {
    let plugin = load_line_numbers_plugin();
    let state = AppState::default();
    let ctx = default_contribute_ctx(&state);
    // default lines is empty
    assert!(
        plugin
            .contribute_to(&SlotId::BUFFER_LEFT, &state, &ctx)
            .is_none()
    );
}

#[test]
fn state_hash_changes_with_line_count() {
    let mut plugin = load_line_numbers_plugin();
    let h1 = plugin.state_hash();

    let mut state = AppState::default();
    state.lines = vec![vec![], vec![]];
    plugin.on_state_changed(&state, DirtyFlags::BUFFER);
    let h2 = plugin.state_hash();

    assert_ne!(h1, h2);
}

#[test]
fn contribute_deps() {
    let plugin = load_line_numbers_plugin();
    // BufferLeft depends on BUFFER
    let deps = plugin.contribute_deps(&SlotId::BUFFER_LEFT);
    assert!(deps.intersects(DirtyFlags::BUFFER));
}

#[test]
fn width_adapts_to_line_count() {
    let mut plugin = load_line_numbers_plugin();
    let mut state = AppState::default();
    // 100 lines → 3-digit width
    state.lines = (0..100).map(|_| vec![]).collect();
    plugin.on_state_changed(&state, DirtyFlags::BUFFER);

    let ctx = default_contribute_ctx(&state);
    let contrib = plugin
        .contribute_to(&SlotId::BUFFER_LEFT, &state, &ctx)
        .unwrap();
    match contrib.element {
        Element::Flex {
            direction: Direction::Column,
            children,
            ..
        } => {
            assert_eq!(children.len(), 100);
            // First line: "  1 " (3 digits padded)
            match &children[0].element {
                Element::Text(s, _) => assert_eq!(s, "  1 "),
                other => panic!("expected Text, got {other:?}"),
            }
            // Line 100: "100 "
            match &children[99].element {
                Element::Text(s, _) => assert_eq!(s, "100 "),
                other => panic!("expected Text, got {other:?}"),
            }
        }
        other => panic!("expected Column Flex, got {other:?}"),
    }
}
