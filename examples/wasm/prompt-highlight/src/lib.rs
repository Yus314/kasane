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

fn refresh_cursor_mode(dirty_flags: u16) {
    if dirty_flags & dirty::STATUS != 0 {
        CURSOR_MODE.set(host_state::get_cursor_mode());
    }
}

#[plugin]
impl Guest for PromptHighlightPlugin {
    fn get_id() -> String {
        "prompt_highlight".to_string()
    }

    fn on_state_changed_effects(dirty_flags: u16) -> RuntimeEffects {
        refresh_cursor_mode(dirty_flags);
        RuntimeEffects::default()
    }

    fn state_hash() -> u64 {
        CURSOR_MODE.get() as u64
    }

    fn transform(
        target: TransformTarget,
        subject: TransformSubject,
        _ctx: TransformContext,
    ) -> TransformSubject {
        if !matches!(target, TransformTarget::StatusBarT) {
            return subject;
        }

        if CURSOR_MODE.get() != MODE_PROMPT {
            return subject;
        }

        // Wrap the status bar in a container with a distinct background.
        // StatusBar is always an Element variant, so map_element is appropriate.
        match subject {
            TransformSubject::ElementS(element) => {
                TransformSubject::ElementS(
                    container(element)
                        .style(face(named(NamedColor::Black), named(NamedColor::Yellow)))
                        .build(),
                )
            }
            other => other,
        }
    }

    fn transform_priority() -> i16 {
        0
    }
}

export!(PromptHighlightPlugin);
