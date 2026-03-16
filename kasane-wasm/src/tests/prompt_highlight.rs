use kasane_core::element::{Element, Style};
use kasane_core::plugin::{TransformContext, TransformTarget};
use kasane_core::protocol::Face;

use super::*;

#[test]
fn plugin_id() {
    let plugin = load_prompt_highlight_plugin();
    assert_eq!(plugin.id().0, "prompt_highlight");
}

#[test]
fn passthrough_in_buffer_mode() {
    let mut plugin = load_prompt_highlight_plugin();
    let state = AppState::default(); // cursor_mode = Buffer
    plugin.on_state_changed(&state, DirtyFlags::STATUS);

    let element = Element::text("status content", Face::default());
    let ctx = TransformContext {
        is_default: true,
        chain_position: 0,
    };
    let result = plugin.transform(&TransformTarget::StatusBar, element.clone(), &state, &ctx);

    // In buffer mode, element should pass through unchanged
    match (&result, &element) {
        (Element::Text(a, _), Element::Text(b, _)) => assert_eq!(a, b),
        _ => panic!("expected passthrough, got {result:?}"),
    }
}

#[test]
fn wraps_in_prompt_mode() {
    let mut plugin = load_prompt_highlight_plugin();
    let mut state = AppState::default();
    state.cursor_mode = kasane_core::protocol::CursorMode::Prompt;
    plugin.on_state_changed(&state, DirtyFlags::STATUS);

    let element = Element::text("prompt content", Face::default());
    let ctx = TransformContext {
        is_default: true,
        chain_position: 0,
    };
    let result = plugin.transform(&TransformTarget::StatusBar, element, &state, &ctx);

    // In prompt mode, should be wrapped in a Container
    match result {
        Element::Container { style, .. } => {
            // Should have yellow background
            match style {
                Style::Direct(face) => {
                    assert_eq!(
                        face.bg,
                        Color::Named(kasane_core::protocol::NamedColor::Yellow)
                    );
                }
                _ => panic!("expected Direct style, got {style:?}"),
            }
        }
        other => panic!("expected Container wrap, got {other:?}"),
    }
}

#[test]
fn ignores_non_status_targets() {
    let mut plugin = load_prompt_highlight_plugin();
    let mut state = AppState::default();
    state.cursor_mode = kasane_core::protocol::CursorMode::Prompt;
    plugin.on_state_changed(&state, DirtyFlags::STATUS);

    let element = Element::text("buffer content", Face::default());
    let ctx = TransformContext {
        is_default: true,
        chain_position: 0,
    };
    // Buffer target should not be wrapped even in prompt mode
    let result = plugin.transform(&TransformTarget::Buffer, element.clone(), &state, &ctx);
    match (&result, &element) {
        (Element::Text(a, _), Element::Text(b, _)) => assert_eq!(a, b),
        _ => panic!("expected passthrough for Buffer target, got {result:?}"),
    }
}

#[test]
fn state_hash_changes_with_mode() {
    let mut plugin = load_prompt_highlight_plugin();
    let h1 = plugin.state_hash();

    let mut state = AppState::default();
    state.cursor_mode = kasane_core::protocol::CursorMode::Prompt;
    plugin.on_state_changed(&state, DirtyFlags::STATUS);
    let h2 = plugin.state_hash();

    assert_ne!(h1, h2);
}

#[test]
fn transform_deps_status_bar() {
    let plugin = load_prompt_highlight_plugin();
    let deps = plugin.transform_deps(&TransformTarget::StatusBar);
    assert!(deps.intersects(DirtyFlags::STATUS));
}

#[test]
fn transform_priority_default() {
    let plugin = load_prompt_highlight_plugin();
    assert_eq!(plugin.transform_priority(), 0);
}
