//! Prompt-highlight: a transform example plugin.
//!
//! Wraps the status bar in a highlighted container when the editor is in
//! prompt mode (`:`, `/`, etc.), making the mode visually distinct.
//! In normal buffer mode the status bar is passed through unchanged.

kasane_plugin_sdk::generate!();

use std::cell::Cell;

use kasane_plugin_sdk::{dirty, plugin};

/// Cursor mode constants (matches host encoding).
const MODE_BUFFER: u8 = 0;
const MODE_PROMPT: u8 = 1;

thread_local! {
    static CURSOR_MODE: Cell<u8> = const { Cell::new(MODE_BUFFER) };
}

struct PromptHighlightPlugin;

#[plugin]
impl Guest for PromptHighlightPlugin {
    fn get_id() -> String {
        "prompt_highlight".to_string()
    }

    fn on_state_changed(dirty_flags: u16) -> Vec<Command> {
        if dirty_flags & dirty::STATUS != 0 {
            CURSOR_MODE.set(host_state::get_cursor_mode());
        }
        vec![]
    }

    fn state_hash() -> u64 {
        CURSOR_MODE.get() as u64
    }

    fn transform_element(
        target: TransformTarget,
        element: ElementHandle,
        _ctx: TransformContext,
    ) -> ElementHandle {
        if !matches!(target, TransformTarget::StatusBarT) {
            return element;
        }

        if CURSOR_MODE.get() != MODE_PROMPT {
            return element;
        }

        // Wrap the status bar in a container with a distinct background
        let padding = Edges {
            top: 0,
            right: 0,
            bottom: 0,
            left: 0,
        };
        element_builder::create_container_styled(
            element,
            None,
            false,
            padding,
            face(named(NamedColor::Black), named(NamedColor::Yellow)),
            None,
        )
    }

    fn transform_priority() -> i16 {
        0
    }

    fn transform_deps(target: TransformTarget) -> u16 {
        match target {
            TransformTarget::StatusBarT => dirty::STATUS,
            _ => 0,
        }
    }
}

export!(PromptHighlightPlugin);
