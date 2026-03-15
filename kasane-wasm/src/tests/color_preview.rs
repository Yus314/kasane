use super::*;

fn load_color_preview_plugin() -> crate::WasmPlugin {
    let loader = WasmPluginLoader::new().expect("failed to create loader");
    let bytes = crate::load_wasm_fixture("color-preview.wasm").expect("failed to load fixture");
    loader
        .load(&bytes, &crate::WasiCapabilityConfig::default())
        .expect("failed to load plugin")
}

#[test]
fn plugin_id() {
    let plugin = load_color_preview_plugin();
    assert_eq!(plugin.id().0, "color_preview");
}

#[test]
fn detects_colors_in_line() {
    let mut plugin = load_color_preview_plugin();
    let state = make_state_with_lines(&["#ff0000"]);
    plugin.on_state_changed(&state, DirtyFlags::BUFFER);

    let ctx = default_annotate_ctx();
    let ann = plugin.annotate_line_with_ctx(0, &state, &ctx);
    assert!(ann.is_some());
    let ann = ann.unwrap();
    assert!(ann.left_gutter.is_some());
    assert!(ann.background.is_none());
}

#[test]
fn no_decoration_without_colors() {
    let mut plugin = load_color_preview_plugin();
    let state = make_state_with_lines(&["no colors here"]);
    plugin.on_state_changed(&state, DirtyFlags::BUFFER);

    let ctx = default_annotate_ctx();
    assert!(plugin.annotate_line_with_ctx(0, &state, &ctx).is_none());
}

#[test]
fn overlay_on_color_line() {
    let mut plugin = load_color_preview_plugin();
    let mut state = make_state_with_lines(&["#3498db"]);
    state.cursor_pos = kasane_core::protocol::Coord { line: 0, column: 0 };
    plugin.on_state_changed(&state, DirtyFlags::BUFFER);

    let ctx = default_overlay_ctx();
    let overlay = plugin.contribute_overlay_with_ctx(&state, &ctx);
    assert!(overlay.is_some());
}

#[test]
fn no_overlay_on_plain_line() {
    let mut plugin = load_color_preview_plugin();
    let mut state = make_state_with_lines(&["no colors here", "#ff0000"]);
    state.cursor_pos = kasane_core::protocol::Coord { line: 0, column: 0 };
    plugin.on_state_changed(&state, DirtyFlags::BUFFER);

    let ctx = default_overlay_ctx();
    assert!(plugin.contribute_overlay_with_ctx(&state, &ctx).is_none());
}

#[test]
fn state_hash_changes() {
    let mut plugin = load_color_preview_plugin();
    let h1 = plugin.state_hash();

    let state = make_state_with_lines(&["#aabbcc"]);
    plugin.on_state_changed(&state, DirtyFlags::BUFFER);
    let h2 = plugin.state_hash();

    assert_ne!(h1, h2);
}

#[test]
fn skips_non_buffer_dirty() {
    let mut plugin = load_color_preview_plugin();
    let h1 = plugin.state_hash();

    let state = make_state_with_lines(&["#aabbcc"]);
    plugin.on_state_changed(&state, DirtyFlags::STATUS);
    let h2 = plugin.state_hash();

    assert_eq!(h1, h2);
}

#[test]
fn handle_mouse_increments() {
    use kasane_core::element::InteractiveId;
    use kasane_core::input::{Modifiers, MouseButton, MouseEvent, MouseEventKind};

    let mut plugin = load_color_preview_plugin();
    let mut state = make_state_with_lines(&["#100000"]);
    state.cursor_pos = kasane_core::protocol::Coord { line: 0, column: 0 };
    plugin.on_state_changed(&state, DirtyFlags::BUFFER);

    // R up button: id = 2000 + 0*6 + 0 = 2000
    let event = MouseEvent {
        kind: MouseEventKind::Press(MouseButton::Left),
        line: 0,
        column: 0,
        modifiers: Modifiers::empty(),
    };
    let result = plugin.handle_mouse(&event, InteractiveId(2000), &state);
    assert!(result.is_some());
    let cmds = result.unwrap();
    assert_eq!(cmds.len(), 1);
    // Should be a SendToKakoune command
    match &cmds[0] {
        kasane_core::plugin::Command::SendToKakoune(
            kasane_core::protocol::KasaneRequest::Keys(keys),
        ) => {
            let joined: String = keys.join("");
            assert!(joined.contains("#110000"), "Expected #110000 in: {joined}");
        }
        _ => panic!("Expected SendToKakoune Keys"),
    }
}

#[test]
fn handle_mouse_consumes_release() {
    use kasane_core::element::InteractiveId;
    use kasane_core::input::{Modifiers, MouseButton, MouseEvent, MouseEventKind};

    let mut plugin = load_color_preview_plugin();
    let mut state = make_state_with_lines(&["#ff0000"]);
    state.cursor_pos = kasane_core::protocol::Coord { line: 0, column: 0 };
    plugin.on_state_changed(&state, DirtyFlags::BUFFER);

    let event = MouseEvent {
        kind: MouseEventKind::Release(MouseButton::Left),
        line: 0,
        column: 0,
        modifiers: Modifiers::empty(),
    };
    let result = plugin.handle_mouse(&event, InteractiveId(2000), &state);
    assert!(result.is_some());
    assert!(result.unwrap().is_empty());
}
