use kasane_core::element::{Element, ElementStyle};
use kasane_core::plugin::{TransformContext, TransformSubject, TransformTarget};

use super::*;

fn apply_prompt_state_change(plugin: &mut crate::WasmPlugin, state: &AppState, dirty: DirtyFlags) {
    let effects = plugin.on_state_changed_effects(&AppView::new(state), dirty);
    assert!(effects.redraw.is_empty());
    assert!(effects.commands.is_empty());
    assert!(effects.scroll_plans.is_empty());
}

#[test]
fn plugin_id() {
    let plugin = load_prompt_highlight_plugin();
    assert_eq!(plugin.id().0, "prompt_highlight");
}

#[test]
fn passthrough_in_buffer_mode() {
    let mut plugin = load_prompt_highlight_plugin();
    let state = AppState::default(); // cursor_mode = Buffer
    apply_prompt_state_change(&mut plugin, &state, DirtyFlags::STATUS);

    let element = Element::plain_text("status content");
    let ctx = TransformContext {
        is_default: true,
        chain_position: 0,
        pane_surface_id: None,
        pane_focused: true,
        target_line: None,
    };
    let result = plugin.transform(
        &TransformTarget::STATUS_BAR,
        TransformSubject::Element(element.clone()),
        &AppView::new(&state),
        &ctx,
    );

    // In buffer mode, element should pass through unchanged
    let result_el = result.into_element();
    match (&result_el, &element) {
        (Element::Text(a, _), Element::Text(b, _)) => assert_eq!(a, b),
        _ => panic!("expected passthrough, got {result_el:?}"),
    }
}

#[test]
fn wraps_in_prompt_mode() {
    let mut plugin = load_prompt_highlight_plugin();
    let mut state = AppState::default();
    state.inference.cursor_mode = kasane_core::protocol::CursorMode::Prompt;
    apply_prompt_state_change(&mut plugin, &state, DirtyFlags::STATUS);

    let element = Element::plain_text("prompt content");
    let ctx = TransformContext {
        is_default: true,
        chain_position: 0,
        pane_surface_id: None,
        pane_focused: true,
        target_line: None,
    };
    let result = plugin.transform(
        &TransformTarget::STATUS_BAR,
        TransformSubject::Element(element),
        &AppView::new(&state),
        &ctx,
    );

    // In prompt mode, should be wrapped in a Container
    let result_el = result.into_element();
    match result_el {
        Element::Container { style, .. } => {
            // Should have yellow background
            match style {
                ElementStyle::Inline(arc) => {
                    let face = arc.to_face();
                    assert_eq!(
                        face.bg,
                        Color::Named(kasane_core::protocol::NamedColor::Yellow)
                    );
                }
                _ => panic!("expected Inline style, got {style:?}"),
            }
        }
        other => panic!("expected Container wrap, got {other:?}"),
    }
}

#[test]
fn ignores_non_status_targets() {
    let mut plugin = load_prompt_highlight_plugin();
    let mut state = AppState::default();
    state.inference.cursor_mode = kasane_core::protocol::CursorMode::Prompt;
    apply_prompt_state_change(&mut plugin, &state, DirtyFlags::STATUS);

    let element = Element::plain_text("buffer content");
    let ctx = TransformContext {
        is_default: true,
        chain_position: 0,
        pane_surface_id: None,
        pane_focused: true,
        target_line: None,
    };
    // Buffer target should not be wrapped even in prompt mode
    let result = plugin.transform(
        &TransformTarget::BUFFER,
        TransformSubject::Element(element.clone()),
        &AppView::new(&state),
        &ctx,
    );
    let result_el = result.into_element();
    match (&result_el, &element) {
        (Element::Text(a, _), Element::Text(b, _)) => assert_eq!(a, b),
        _ => panic!("expected passthrough for Buffer target, got {result_el:?}"),
    }
}

#[test]
fn state_hash_changes_with_mode() {
    let mut plugin = load_prompt_highlight_plugin();
    let h1 = plugin.state_hash();

    let mut state = AppState::default();
    state.inference.cursor_mode = kasane_core::protocol::CursorMode::Prompt;
    apply_prompt_state_change(&mut plugin, &state, DirtyFlags::STATUS);
    let h2 = plugin.state_hash();

    assert_ne!(h1, h2);
}

#[test]
fn transform_priority_default() {
    let plugin = load_prompt_highlight_plugin();
    assert_eq!(plugin.transform_priority(), 0);
}
