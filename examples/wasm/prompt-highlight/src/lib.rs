//! Prompt-highlight: a transform example plugin.
//!
//! Wraps the status bar in a highlighted container when the editor is in
//! prompt mode (`:`, `/`, etc.), making the mode visually distinct.
//! In normal buffer mode the status bar is passed through unchanged.

/// Cursor mode constants (matches host encoding).
const MODE_PROMPT: u8 = 1;

kasane_plugin_sdk::define_plugin! {
    manifest: "kasane-plugin.toml",

    state {
        #[bind(host_state::get_cursor_mode(), on: dirty::STATUS)]
        cursor_mode: u8 = 0,
    },

    transform(target, subject, _ctx) {
        if target != "kasane.status-bar" {
            return subject;
        }

        if state.cursor_mode != MODE_PROMPT {
            return subject;
        }

        // Wrap the status bar in a container with a distinct background.
        match subject {
            TransformSubject::ElementS(element) => {
                TransformSubject::ElementS(
                    container(element)
                        .style(style_with(named(NamedColor::Black), named(NamedColor::Yellow)))
                        .build(),
                )
            }
            other => other,
        }
    },

    transform_priority: 0,
}
