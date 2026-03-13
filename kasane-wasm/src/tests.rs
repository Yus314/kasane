use std::path::PathBuf;

use kasane_core::config::PluginsConfig;
use kasane_core::element::{Direction, Element};
use kasane_core::plugin::{Plugin, PluginRegistry, Slot};
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

#[test]
fn plugin_id() {
    let plugin = load_cursor_line_plugin();
    assert_eq!(plugin.id().0, "wasm_cursor_line");
}

#[test]
fn highlight_active_line() {
    let mut plugin = load_cursor_line_plugin();
    let mut state = AppState::default();
    state.cursor_pos.line = 3;
    plugin.on_state_changed(&state, DirtyFlags::BUFFER);

    let dec = plugin.contribute_line(3, &state);
    assert!(dec.is_some());
    let dec = dec.unwrap();
    assert!(dec.background.is_some());
    assert_eq!(
        dec.background.unwrap().bg,
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

    assert!(plugin.contribute_line(0, &state).is_none());
    assert!(plugin.contribute_line(2, &state).is_none());
    assert!(plugin.contribute_line(4, &state).is_none());
}

#[test]
fn tracks_cursor_movement() {
    let mut plugin = load_cursor_line_plugin();
    let mut state = AppState::default();

    state.cursor_pos.line = 0;
    plugin.on_state_changed(&state, DirtyFlags::BUFFER);
    assert!(plugin.contribute_line(0, &state).is_some());
    assert!(plugin.contribute_line(5, &state).is_none());

    state.cursor_pos.line = 5;
    plugin.on_state_changed(&state, DirtyFlags::BUFFER);
    assert!(plugin.contribute_line(0, &state).is_none());
    assert!(plugin.contribute_line(5, &state).is_some());
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
fn slot_deps_returns_empty() {
    let plugin = load_cursor_line_plugin();
    assert_eq!(plugin.slot_deps(Slot::BufferLeft), DirtyFlags::empty());
    assert_eq!(plugin.slot_deps(Slot::StatusRight), DirtyFlags::empty());
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
    // cursor-line plugin has no slot contributions
    assert!(plugin.contribute(Slot::BufferLeft, &state).is_none());
    assert!(plugin.contribute(Slot::Overlay, &state).is_none());
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

    let element = plugin.contribute(Slot::BufferLeft, &state);
    assert!(element.is_some());

    // Should be a column with 3 children
    match element.unwrap() {
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

    assert!(plugin.contribute(Slot::BufferRight, &state).is_none());
    assert!(plugin.contribute(Slot::StatusLeft, &state).is_none());
    assert!(plugin.contribute(Slot::Overlay, &state).is_none());
}

#[test]
fn line_numbers_empty_buffer_returns_none() {
    let plugin = load_line_numbers_plugin();
    let state = AppState::default();
    // default lines is empty
    assert!(plugin.contribute(Slot::BufferLeft, &state).is_none());
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
fn line_numbers_slot_deps() {
    let plugin = load_line_numbers_plugin();
    assert_eq!(plugin.slot_deps(Slot::BufferLeft), DirtyFlags::BUFFER);
    assert_eq!(plugin.slot_deps(Slot::Overlay), DirtyFlags::empty());
}

#[test]
fn line_numbers_width_adapts_to_line_count() {
    let mut plugin = load_line_numbers_plugin();
    let mut state = AppState::default();
    // 100 lines → 3-digit width
    state.lines = (0..100).map(|_| vec![]).collect();
    plugin.on_state_changed(&state, DirtyFlags::BUFFER);

    let element = plugin.contribute(Slot::BufferLeft, &state).unwrap();
    match element {
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
        disabled: vec!["wasm_cursor_line".to_string()],
    };
    let mut registry = PluginRegistry::new();
    crate::discover_and_register(&config, &mut registry);

    // Only line-numbers should be loaded
    assert_eq!(registry.plugin_count(), 1);
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
