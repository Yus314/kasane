//! Keyboard state machine for the shadow cursor.

use unicode_segmentation::UnicodeSegmentation;

use crate::history::VersionId;
use crate::input::KeyEvent;
use crate::plugin::Command;
use crate::state::DirtyFlags;

use super::{ShadowCursor, ShadowKeyResult, ShadowPhase};

/// edits within `Editing` preserve the existing stamp.
pub fn handle_shadow_cursor_key(
    shadow: &mut ShadowCursor,
    key: &KeyEvent,
    span_text: &str,
    current_version: VersionId,
) -> ShadowKeyResult {
    use crate::input::Key;

    match key.key {
        // Escape always deactivates
        Key::Escape => ShadowKeyResult::Deactivate,

        // Up/Down/PageUp/PageDown deactivate (return to buffer navigation)
        Key::Up | Key::Down | Key::PageUp | Key::PageDown => ShadowKeyResult::Deactivate,

        // Enter commits the edit
        Key::Enter => {
            if let ShadowPhase::Editing {
                ref working_text,
                ref original_text,
                ..
            } = shadow.phase
            {
                let commands =
                    build_commit_commands(working_text, original_text, shadow.span_index);
                ShadowKeyResult::Commit(commands)
            } else {
                ShadowKeyResult::Deactivate
            }
        }

        // Character input
        Key::Char(c) => {
            match &mut shadow.phase {
                ShadowPhase::Navigating => {
                    // Transition to Editing with initial char
                    let mut working_text = span_text.to_string();
                    let grapheme_count = working_text.graphemes(true).count();
                    // Place cursor at end, then insert char
                    let cursor = grapheme_count;
                    let byte_pos: usize = working_text.graphemes(true).map(|g| g.len()).sum();
                    let mut buf = [0u8; 4];
                    let s = c.encode_utf8(&mut buf);
                    working_text.insert_str(byte_pos, s);
                    shadow.phase = ShadowPhase::Editing {
                        working_text,
                        original_text: span_text.to_string(),
                        cursor_grapheme_offset: cursor + 1,
                        base_version: current_version,
                    };
                    ShadowKeyResult::Consumed(DirtyFlags::BUFFER_CONTENT)
                }
                ShadowPhase::Editing {
                    working_text,
                    cursor_grapheme_offset,
                    ..
                } => {
                    let offset = *cursor_grapheme_offset;
                    let byte_pos: usize = working_text
                        .graphemes(true)
                        .take(offset)
                        .map(|g| g.len())
                        .sum();
                    let mut buf = [0u8; 4];
                    let s = c.encode_utf8(&mut buf);
                    working_text.insert_str(byte_pos, s);
                    *cursor_grapheme_offset += 1;
                    ShadowKeyResult::Consumed(DirtyFlags::BUFFER_CONTENT)
                }
            }
        }

        // Backspace: delete grapheme before cursor
        Key::Backspace => {
            if let ShadowPhase::Editing {
                working_text,
                cursor_grapheme_offset,
                ..
            } = &mut shadow.phase
            {
                if *cursor_grapheme_offset > 0 {
                    let offset = *cursor_grapheme_offset;
                    let graphemes: Vec<&str> = working_text.graphemes(true).collect();
                    let byte_start: usize =
                        graphemes.iter().take(offset - 1).map(|g| g.len()).sum();
                    let byte_end: usize = graphemes.iter().take(offset).map(|g| g.len()).sum();
                    working_text.replace_range(byte_start..byte_end, "");
                    *cursor_grapheme_offset -= 1;
                }
                ShadowKeyResult::Consumed(DirtyFlags::BUFFER_CONTENT)
            } else {
                ShadowKeyResult::Consumed(DirtyFlags::empty())
            }
        }

        // Delete: delete grapheme after cursor
        Key::Delete => {
            if let ShadowPhase::Editing {
                working_text,
                cursor_grapheme_offset,
                ..
            } = &mut shadow.phase
            {
                let offset = *cursor_grapheme_offset;
                let graphemes: Vec<&str> = working_text.graphemes(true).collect();
                if offset < graphemes.len() {
                    let byte_start: usize = graphemes.iter().take(offset).map(|g| g.len()).sum();
                    let byte_end: usize = graphemes.iter().take(offset + 1).map(|g| g.len()).sum();
                    working_text.replace_range(byte_start..byte_end, "");
                }
                ShadowKeyResult::Consumed(DirtyFlags::BUFFER_CONTENT)
            } else {
                ShadowKeyResult::Consumed(DirtyFlags::empty())
            }
        }

        // Left: move cursor left one grapheme
        Key::Left => {
            if let ShadowPhase::Editing {
                cursor_grapheme_offset,
                ..
            } = &mut shadow.phase
            {
                if *cursor_grapheme_offset > 0 {
                    *cursor_grapheme_offset -= 1;
                }
                ShadowKeyResult::Consumed(DirtyFlags::BUFFER_CONTENT)
            } else {
                ShadowKeyResult::Consumed(DirtyFlags::empty())
            }
        }

        // Right: move cursor right one grapheme
        Key::Right => {
            if let ShadowPhase::Editing {
                working_text,
                cursor_grapheme_offset,
                ..
            } = &mut shadow.phase
            {
                let max = working_text.graphemes(true).count();
                if *cursor_grapheme_offset < max {
                    *cursor_grapheme_offset += 1;
                }
                ShadowKeyResult::Consumed(DirtyFlags::BUFFER_CONTENT)
            } else {
                ShadowKeyResult::Consumed(DirtyFlags::empty())
            }
        }

        // Home: move cursor to start
        Key::Home => {
            if let ShadowPhase::Editing {
                cursor_grapheme_offset,
                ..
            } = &mut shadow.phase
            {
                *cursor_grapheme_offset = 0;
                ShadowKeyResult::Consumed(DirtyFlags::BUFFER_CONTENT)
            } else {
                ShadowKeyResult::Consumed(DirtyFlags::empty())
            }
        }

        // End: move cursor to end
        Key::End => {
            if let ShadowPhase::Editing {
                working_text,
                cursor_grapheme_offset,
                ..
            } = &mut shadow.phase
            {
                *cursor_grapheme_offset = working_text.graphemes(true).count();
                ShadowKeyResult::Consumed(DirtyFlags::BUFFER_CONTENT)
            } else {
                ShadowKeyResult::Consumed(DirtyFlags::empty())
            }
        }

        // Tab and other keys: consume but no-op
        _ => ShadowKeyResult::Consumed(DirtyFlags::empty()),
    }
}

/// Build commit commands for projecting the edit back to the buffer.
///
/// Implements the Hippocratic condition: if working_text == original_text,
/// return empty commands (no-op).
fn build_commit_commands(
    working_text: &str,
    original_text: &str,
    _span_index: usize,
) -> Vec<Command> {
    // Hippocratic condition: no change → no commands
    if working_text == original_text {
        return vec![];
    }

    // The actual buffer projection requires the EditableSpan
    // (`projection_target`) which is resolved by the caller. This
    // function returns placeholder commands; the real projection
    // happens in `build_mirror_commit` which is called from the
    // update dispatch.
    vec![]
}
