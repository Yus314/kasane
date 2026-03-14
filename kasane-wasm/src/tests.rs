use std::path::PathBuf;

use kasane_core::config::PluginsConfig;
use kasane_core::element::{Direction, Element};
use kasane_core::plugin::{
    AnnotateContext, ContributeContext, OverlayContext, Plugin, PluginRegistry, SlotId,
};
use kasane_core::protocol::Color;
use kasane_core::state::{AppState, DirtyFlags};

use crate::WasmPluginLoader;

fn load_cursor_line_plugin() -> crate::WasmPlugin {
    let loader = WasmPluginLoader::new().expect("failed to create loader");
    let bytes = crate::load_wasm_fixture("cursor-line.wasm").expect("failed to load fixture");
    loader.load(&bytes).expect("failed to load plugin")
}

fn load_line_numbers_plugin() -> crate::WasmPlugin {
    let loader = WasmPluginLoader::new().expect("failed to create loader");
    let bytes = crate::load_wasm_fixture("line-numbers.wasm").expect("failed to load fixture");
    loader.load(&bytes).expect("failed to load plugin")
}

fn default_annotate_ctx() -> AnnotateContext {
    AnnotateContext {
        line_width: 80,
        gutter_width: 0,
    }
}

fn default_contribute_ctx(state: &AppState) -> ContributeContext {
    ContributeContext::new(state, None)
}

fn default_overlay_ctx() -> OverlayContext {
    OverlayContext {
        screen_cols: 80,
        screen_rows: 24,
        menu_rect: None,
        existing_overlays: vec![],
    }
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

// --- cursor-line contribute tests ---

#[test]
fn cursor_line_contribute_returns_none() {
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

// --- line-numbers plugin tests ---

#[test]
fn line_numbers_plugin_id() {
    let plugin = load_line_numbers_plugin();
    assert_eq!(plugin.id().0, "wasm_line_numbers");
}

#[test]
fn line_numbers_contribute_buffer_left() {
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
fn line_numbers_no_contribution_for_other_slots() {
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
fn line_numbers_empty_buffer_returns_none() {
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
fn line_numbers_state_hash_changes_with_line_count() {
    let mut plugin = load_line_numbers_plugin();
    let h1 = plugin.state_hash();

    let mut state = AppState::default();
    state.lines = vec![vec![], vec![]];
    plugin.on_state_changed(&state, DirtyFlags::BUFFER);
    let h2 = plugin.state_hash();

    assert_ne!(h1, h2);
}

#[test]
fn line_numbers_contribute_deps() {
    let plugin = load_line_numbers_plugin();
    // BufferLeft depends on BUFFER
    let deps = plugin.contribute_deps(&SlotId::BUFFER_LEFT);
    assert!(deps.intersects(DirtyFlags::BUFFER));
}

#[test]
fn line_numbers_width_adapts_to_line_count() {
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

// --- discover_and_register tests ---

#[test]
fn discover_loads_fixtures_directory() {
    let config = PluginsConfig {
        auto_discover: true,
        path: Some(
            PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                .join("fixtures")
                .to_string_lossy()
                .into_owned(),
        ),
        disabled: vec![],
    };
    let mut registry = PluginRegistry::new();
    crate::discover_and_register(&config, &mut registry);

    // Should have loaded both cursor-line.wasm and line-numbers.wasm
    assert!(registry.plugin_count() >= 2, "expected at least 2 plugins");
}

#[test]
fn discover_skips_disabled_plugins() {
    let config = PluginsConfig {
        auto_discover: true,
        path: Some(
            PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                .join("fixtures")
                .to_string_lossy()
                .into_owned(),
        ),
        disabled: vec!["cursor_line".to_string()],
    };
    let mut registry = PluginRegistry::new();
    crate::discover_and_register(&config, &mut registry);

    // cursor-line skipped, line-numbers + color-preview + sel-badge loaded
    assert_eq!(registry.plugin_count(), 3);
}

#[test]
fn discover_does_nothing_when_disabled() {
    let config = PluginsConfig {
        auto_discover: false,
        path: Some(
            PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                .join("fixtures")
                .to_string_lossy()
                .into_owned(),
        ),
        disabled: vec![],
    };
    let mut registry = PluginRegistry::new();
    crate::discover_and_register(&config, &mut registry);

    assert_eq!(registry.plugin_count(), 0);
}

#[test]
fn discover_handles_missing_directory() {
    let config = PluginsConfig {
        auto_discover: true,
        path: Some("/nonexistent/path/to/plugins".to_string()),
        disabled: vec![],
    };
    let mut registry = PluginRegistry::new();
    // Should not panic, just silently skip
    crate::discover_and_register(&config, &mut registry);
    assert_eq!(registry.plugin_count(), 0);
}

// --- color-preview plugin tests ---

fn load_color_preview_plugin() -> crate::WasmPlugin {
    let loader = WasmPluginLoader::new().expect("failed to create loader");
    let bytes = crate::load_wasm_fixture("color-preview.wasm").expect("failed to load fixture");
    loader.load(&bytes).expect("failed to load plugin")
}

fn make_state_with_lines(lines: &[&str]) -> AppState {
    use kasane_core::protocol::{Atom, Face};
    let mut state = AppState::default();
    state.lines = lines
        .iter()
        .map(|s| {
            vec![Atom {
                face: Face::default(),
                contents: (*s).into(),
            }]
        })
        .collect();
    state.lines_dirty = vec![true; lines.len()];
    state
}

#[test]
fn color_preview_plugin_id() {
    let plugin = load_color_preview_plugin();
    assert_eq!(plugin.id().0, "color_preview");
}

#[test]
fn color_preview_detects_colors_in_line() {
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
fn color_preview_no_decoration_without_colors() {
    let mut plugin = load_color_preview_plugin();
    let state = make_state_with_lines(&["no colors here"]);
    plugin.on_state_changed(&state, DirtyFlags::BUFFER);

    let ctx = default_annotate_ctx();
    assert!(plugin.annotate_line_with_ctx(0, &state, &ctx).is_none());
}

#[test]
fn color_preview_overlay_on_color_line() {
    let mut plugin = load_color_preview_plugin();
    let mut state = make_state_with_lines(&["#3498db"]);
    state.cursor_pos = kasane_core::protocol::Coord { line: 0, column: 0 };
    plugin.on_state_changed(&state, DirtyFlags::BUFFER);

    let ctx = default_overlay_ctx();
    let overlay = plugin.contribute_overlay_with_ctx(&state, &ctx);
    assert!(overlay.is_some());
}

#[test]
fn color_preview_no_overlay_on_plain_line() {
    let mut plugin = load_color_preview_plugin();
    let mut state = make_state_with_lines(&["no colors here", "#ff0000"]);
    state.cursor_pos = kasane_core::protocol::Coord { line: 0, column: 0 };
    plugin.on_state_changed(&state, DirtyFlags::BUFFER);

    let ctx = default_overlay_ctx();
    assert!(plugin.contribute_overlay_with_ctx(&state, &ctx).is_none());
}

#[test]
fn color_preview_state_hash_changes() {
    let mut plugin = load_color_preview_plugin();
    let h1 = plugin.state_hash();

    let state = make_state_with_lines(&["#aabbcc"]);
    plugin.on_state_changed(&state, DirtyFlags::BUFFER);
    let h2 = plugin.state_hash();

    assert_ne!(h1, h2);
}

#[test]
fn color_preview_skips_non_buffer_dirty() {
    let mut plugin = load_color_preview_plugin();
    let h1 = plugin.state_hash();

    let state = make_state_with_lines(&["#aabbcc"]);
    plugin.on_state_changed(&state, DirtyFlags::STATUS);
    let h2 = plugin.state_hash();

    assert_eq!(h1, h2);
}

#[test]
fn color_preview_handle_mouse_increments() {
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
fn color_preview_handle_mouse_consumes_release() {
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

// --- bundled plugin tests ---

#[test]
fn register_bundled_plugins_loads_three() {
    let config = PluginsConfig {
        auto_discover: false,
        path: None,
        disabled: vec![],
    };
    let mut registry = PluginRegistry::new();
    crate::register_bundled_plugins(&config, &mut registry);

    assert_eq!(registry.plugin_count(), 3);
}

#[test]
fn register_bundled_plugins_respects_disabled() {
    let config = PluginsConfig {
        auto_discover: false,
        path: None,
        disabled: vec!["color_preview".to_string()],
    };
    let mut registry = PluginRegistry::new();
    crate::register_bundled_plugins(&config, &mut registry);

    assert_eq!(registry.plugin_count(), 2);
}

#[test]
fn filesystem_plugin_overrides_bundled() {
    let config = PluginsConfig {
        auto_discover: false,
        path: None,
        disabled: vec![],
    };
    let mut registry = PluginRegistry::new();
    crate::register_bundled_plugins(&config, &mut registry);
    assert_eq!(registry.plugin_count(), 3);

    // Register another plugin with the same ID
    let loader = WasmPluginLoader::new().unwrap();
    let bytes = crate::load_wasm_fixture("cursor-line.wasm").unwrap();
    let plugin = loader.load(&bytes).unwrap();
    assert_eq!(plugin.id().0, "cursor_line");
    registry.register(Box::new(plugin));

    // Should still be 3, not 4 (replaced, not added)
    assert_eq!(registry.plugin_count(), 3);
}

#[test]
fn sdk_wit_matches_host_wit() {
    let host_wit = include_str!("../wit/plugin.wit");
    let sdk_wit = include_str!("../../kasane-plugin-sdk/wit/plugin.wit");
    assert_eq!(
        host_wit, sdk_wit,
        "SDK WIT and host WIT are out of sync — update kasane-plugin-sdk/wit/plugin.wit"
    );
}
