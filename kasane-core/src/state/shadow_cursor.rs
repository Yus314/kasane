//! Shadow cursor state for editable virtual text (BDT).
//!
//! A `ShadowCursor` represents a cursor within an editable virtual text span.
//! It operates outside the Kakoune protocol: text editing happens locally in
//! `working_text`, and only on commit (Enter) is the result projected back to
//! the buffer via `exec -draft`.
//!
//! ## ADR-035 §Migration — multi-phase re-host on Selection / Time
//!
//! ADR-035's §Migration line on re-hosting `state::shadow_cursor`
//! atop the new `Selection` and `Time` primitives is staged because
//! the existing types serve a fundamentally different domain than
//! `Selection`.
//!
//! - `Selection` (in `state::selection`) addresses positions in a real
//!   buffer (`BufferPos { line, column }`).
//! - `ShadowCursor`'s "cursor" lives in *synthetic* `working_text`
//!   bytes — its `cursor_grapheme_offset` indexes graphemes within
//!   the editable span, not buffer columns. The keyboard handling
//!   logic (`handle_shadow_cursor_key`) is grapheme arithmetic that
//!   doesn't naturally re-shape onto multi-cursor algebra.
//!
//! What *can* be re-hosted on `Selection` is the **projection target**:
//! the buffer-space range that the active edit will commit into via
//! `exec -draft`. `EditableSpan` now stores this as a single
//! `projection_target: Selection` field (anchor and cursor share
//! one buffer line; their columns are the byte-range bounds).
//! `ShadowCursor::buffer_projection_target` indexes into the parent
//! span vector and returns that field.
//!
//! Phase 3 splits the commit pipeline into an algebraic and a
//! serialization layer:
//!
//! - `mirror_edit(shadow, span, line_count) -> Option<BufferEdit>`
//!   computes the algebraic shape of the commit — a `Selection`
//!   target plus pre/post text — and is the natural payload for a
//!   future plugin commit-intercept hook.
//! - `edit_to_commands(edit) -> Vec<Command>` serializes a
//!   `BufferEdit` into the Kakoune `exec -draft` command(s) that
//!   land it.
//! - `build_mirror_commit` remains a thin composition of the two,
//!   preserving the dispatch-side entry point.
//!
//! The keyboard-handling state machine (`handle_shadow_cursor_key`)
//! still operates in synthetic grapheme space — it does not
//! re-shape onto buffer-space `SelectionSet` algebra, so that part
//! of the original Phase 3 sketch is intentionally not pursued.
//!
//! Subsequent phase (deferred):
//!
//! - **Phase 4** — version the working_text via `BufferVersion` so
//!   shadow edits compose with `Time::At(v)` queries.

use std::ops::Range;

use unicode_segmentation::UnicodeSegmentation;

use crate::input::KeyEvent;
use crate::plugin::{Command, PluginId};
use crate::state::DirtyFlags;
use crate::state::selection::Selection;

/// How the virtual text maps back to the buffer on commit.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum EditProjection {
    /// Buffer content is directly mirrored in the virtual text.
    /// Commit replaces `buffer_byte_range` with `working_text`.
    Mirror,
    /// Plugin's `on_virtual_edit` handler transforms the edit.
    PluginDefined,
}

/// A contiguous editable region within a virtual text line.
///
/// Byte ranges are aligned to `Atom` boundaries in the synthetic content.
///
/// `projection_target` carries the buffer-space target as a single
/// `Selection`; by invariant `anchor.line == cursor.line`
/// (Mirror projections target a single buffer line), and
/// `min().column..max().column` is the byte range within that line.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct EditableSpan {
    /// Byte range within the virtual text's synthetic content.
    pub display_byte_range: Range<usize>,
    /// Buffer-space projection target. Subsumes the previous
    /// `anchor_line` + `buffer_byte_range` pair.
    pub projection_target: Selection,
    /// How edits are projected back to the buffer.
    pub projection: EditProjection,
}

/// Lifecycle phase of a shadow cursor.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ShadowPhase {
    /// Cursor is positioned but no editing has started.
    Navigating,
    /// Active text editing within the span.
    Editing {
        /// Current content being edited (may differ from original).
        working_text: String,
        /// Original content at activation time (for Hippocratic check).
        original_text: String,
        /// Cursor position as grapheme cluster offset from span start.
        cursor_grapheme_offset: usize,
    },
}

/// A cursor within an editable virtual text span, independent of Kakoune's cursor.
///
/// The shadow cursor lives entirely in display space. It intercepts key events
/// while active, updates `working_text` locally, and on commit generates
/// `exec -draft` commands to project the edit back to the buffer.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ShadowCursor {
    /// Display line where the editable virtual text is rendered.
    pub display_line: usize,
    /// Index into the `EditableSpan` vector of the display entry.
    pub span_index: usize,
    /// Current lifecycle phase.
    pub phase: ShadowPhase,
    /// Plugin that owns the editable virtual text.
    pub owner_plugin: PluginId,
}

impl ShadowCursor {
    /// ADR-035 §1 — Selection-shaped view of the buffer-space range
    /// the active edit will commit into when the user presses
    /// Enter. Caller supplies the `EditableSpan` vector for the
    /// shadow cursor's display line (the cursor only knows its
    /// `span_index`, not the spans themselves).
    ///
    /// Returns `None` when `span_index` is out of bounds for the
    /// supplied vector (e.g. the underlying display entry's spans
    /// were re-emitted with fewer entries this frame).
    pub fn buffer_projection_target(&self, spans: &[EditableSpan]) -> Option<Selection> {
        spans.get(self.span_index).map(|s| s.projection_target)
    }
}

/// Result of handling a key event in shadow cursor context.
pub enum ShadowKeyResult {
    /// Key was consumed by the shadow cursor.
    Consumed(DirtyFlags),
    /// Shadow cursor should deactivate; fall through to normal key handling.
    Deactivate,
    /// Editing committed; send commands to Kakoune.
    Commit(Vec<Command>),
}

/// Handle a key event within an active shadow cursor.
///
/// In `Navigating` phase, the first printable character transitions to `Editing`.
/// In `Editing` phase, character keys modify `working_text` with grapheme-correct ops.
pub fn handle_shadow_cursor_key(
    shadow: &mut ShadowCursor,
    key: &KeyEvent,
    span_text: &str,
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

/// Algebraic representation of a buffer edit produced by a shadow
/// cursor commit. ADR-035 §Migration ShadowCursor Phase 3 — the
/// `BufferEdit` shape is the algebraic source of truth; the
/// Kakoune `exec -draft` command is a thin serialization on top
/// (`edit_to_commands`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BufferEdit {
    /// Buffer-space target of the edit. For a Mirror projection,
    /// `target.anchor.line == target.cursor.line` and
    /// `min().column..max().column` is the byte range being
    /// replaced.
    pub target: Selection,
    /// Pre-edit content (what currently lives at `target`). Used
    /// for Hippocratic checks and round-tripping.
    pub original: String,
    /// Post-edit content (what `target` should contain after the
    /// commit). Empty string represents a pure deletion.
    pub replacement: String,
}

impl BufferEdit {
    /// True when the edit would not change the buffer
    /// (`original == replacement`). Callers should skip command
    /// emission for hippocratic edits.
    pub fn is_hippocratic_noop(&self) -> bool {
        self.original == self.replacement
    }
}

/// Compute the `BufferEdit` for a Mirror-projection shadow cursor
/// commit, or `None` when no edit should be produced.
///
/// Returns `None` when:
/// - the shadow cursor is in `Navigating` phase (nothing to commit),
/// - the working text matches the original (Hippocratic noop),
/// - `span.projection_target.anchor.line >= line_count`
///   (anchor line out of range — graceful degradation),
/// - `span.projection != Mirror` (`PluginDefined` projections are
///   handled by `on_virtual_edit`, deferred to BDT-7).
pub fn mirror_edit(
    shadow: &ShadowCursor,
    span: &EditableSpan,
    line_count: usize,
) -> Option<BufferEdit> {
    let (working_text, original_text) = match &shadow.phase {
        ShadowPhase::Editing {
            working_text,
            original_text,
            ..
        } => (working_text.as_str(), original_text.as_str()),
        ShadowPhase::Navigating => return None,
    };

    if working_text == original_text {
        return None;
    }

    if span.projection != EditProjection::Mirror {
        return None;
    }

    let target = span.projection_target;
    if (target.anchor.line as usize) >= line_count {
        return None;
    }

    Some(BufferEdit {
        target,
        original: original_text.to_string(),
        replacement: working_text.to_string(),
    })
}

/// Serialize a `BufferEdit` into the Kakoune `exec -draft` command(s)
/// that will land it. The single-command form selects the byte range
/// and either inserts (empty range) or substitutes (non-empty range).
pub fn edit_to_commands(edit: &BufferEdit) -> Vec<Command> {
    let line_1indexed = edit.target.anchor.line as usize + 1;
    let col_min = edit.target.min().column as usize;
    let col_max = edit.target.max().column as usize;
    let col_start = col_min + 1; // 1-indexed
    let col_end = col_max; // inclusive end in Kakoune

    let escaped = escape_for_kakoune_insert(&edit.replacement);

    let cmd = if col_min == col_max {
        format!("exec -draft {line_1indexed}g {col_start}li{escaped}<esc>")
    } else {
        format!("exec -draft {line_1indexed}g {col_start}l{col_end}lsc{escaped}<esc>")
    };

    vec![Command::kakoune_command(&cmd)]
}

/// Build mirror-projection commit commands from a shadow cursor and
/// its span. Thin composition of `mirror_edit` + `edit_to_commands`;
/// preserved as the public entry point for the update dispatch.
pub fn build_mirror_commit(
    shadow: &ShadowCursor,
    span: &EditableSpan,
    line_count: usize,
) -> Vec<Command> {
    mirror_edit(shadow, span, line_count)
        .as_ref()
        .map(edit_to_commands)
        .unwrap_or_default()
}

/// Escape text for Kakoune insert mode within `exec -draft`.
fn escape_for_kakoune_insert(text: &str) -> String {
    let mut result = String::with_capacity(text.len());
    for c in text.chars() {
        match c {
            '<' => result.push_str("<lt>"),
            '>' => result.push_str("<gt>"),
            '\n' => result.push_str("<ret>"),
            _ => result.push(c),
        }
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::input::{Key, KeyEvent, Modifiers};

    // -----------------------------------------------------------------
    // ADR-035 §1 Phase 1 — Selection-based projection accessors
    // -----------------------------------------------------------------

    fn mk_span(line: u32, start_col: u32, end_col: u32) -> EditableSpan {
        use crate::state::selection::{BufferPos, Selection};
        EditableSpan {
            display_byte_range: 0..(end_col - start_col) as usize,
            projection_target: Selection::new(
                BufferPos::new(line, start_col),
                BufferPos::new(line, end_col),
            ),
            projection: EditProjection::Mirror,
        }
    }

    #[test]
    fn editable_span_projection_target_field_stores_selection() {
        let span = mk_span(7, 12, 18);
        assert_eq!(span.projection_target.anchor.line, 7);
        assert_eq!(span.projection_target.anchor.column, 12);
        assert_eq!(span.projection_target.cursor.line, 7);
        assert_eq!(span.projection_target.cursor.column, 18);
        assert_eq!(span.projection_target.min().column, 12);
        assert_eq!(span.projection_target.max().column, 18);
    }

    #[test]
    fn shadow_cursor_buffer_projection_target_indexes_spans_by_span_index() {
        let spans = vec![mk_span(1, 0, 3), mk_span(2, 5, 10)];
        let shadow = ShadowCursor {
            display_line: 0,
            span_index: 1,
            phase: ShadowPhase::Navigating,
            owner_plugin: PluginId("p".into()),
        };

        let sel = shadow.buffer_projection_target(&spans).unwrap();
        assert_eq!(sel.anchor.line, 2);
        assert_eq!(sel.anchor.column, 5);
        assert_eq!(sel.cursor.column, 10);
    }

    #[test]
    fn shadow_cursor_buffer_projection_target_out_of_bounds_returns_none() {
        let spans = vec![mk_span(0, 0, 3)];
        let shadow = ShadowCursor {
            display_line: 0,
            span_index: 5, // out of bounds
            phase: ShadowPhase::Navigating,
            owner_plugin: PluginId("p".into()),
        };
        assert!(shadow.buffer_projection_target(&spans).is_none());
    }

    fn make_key(k: Key) -> KeyEvent {
        KeyEvent {
            key: k,
            modifiers: Modifiers::empty(),
        }
    }

    fn make_shadow(phase: ShadowPhase) -> ShadowCursor {
        ShadowCursor {
            display_line: 0,
            span_index: 0,
            phase,
            owner_plugin: PluginId(String::new()),
        }
    }

    #[test]
    fn escape_deactivates_from_navigating() {
        let mut shadow = make_shadow(ShadowPhase::Navigating);
        let result = handle_shadow_cursor_key(&mut shadow, &make_key(Key::Escape), "hello");
        assert!(matches!(result, ShadowKeyResult::Deactivate));
    }

    #[test]
    fn escape_deactivates_from_editing() {
        let mut shadow = make_shadow(ShadowPhase::Editing {
            working_text: "hello".into(),
            original_text: "hello".into(),
            cursor_grapheme_offset: 5,
        });
        let result = handle_shadow_cursor_key(&mut shadow, &make_key(Key::Escape), "hello");
        assert!(matches!(result, ShadowKeyResult::Deactivate));
    }

    #[test]
    fn char_transitions_navigating_to_editing() {
        let mut shadow = make_shadow(ShadowPhase::Navigating);
        let result = handle_shadow_cursor_key(&mut shadow, &make_key(Key::Char('a')), "hello");
        assert!(matches!(result, ShadowKeyResult::Consumed(_)));
        match &shadow.phase {
            ShadowPhase::Editing {
                working_text,
                original_text,
                cursor_grapheme_offset,
            } => {
                assert_eq!(working_text, "helloa");
                assert_eq!(original_text, "hello");
                assert_eq!(*cursor_grapheme_offset, 6);
            }
            _ => panic!("expected Editing phase"),
        }
    }

    #[test]
    fn char_insert_at_cursor() {
        let mut shadow = make_shadow(ShadowPhase::Editing {
            working_text: "hllo".into(),
            original_text: "hllo".into(),
            cursor_grapheme_offset: 1,
        });
        let result = handle_shadow_cursor_key(&mut shadow, &make_key(Key::Char('e')), "hllo");
        assert!(matches!(result, ShadowKeyResult::Consumed(_)));
        if let ShadowPhase::Editing {
            working_text,
            cursor_grapheme_offset,
            ..
        } = &shadow.phase
        {
            assert_eq!(working_text, "hello");
            assert_eq!(*cursor_grapheme_offset, 2);
        }
    }

    #[test]
    fn backspace_deletes_before_cursor() {
        let mut shadow = make_shadow(ShadowPhase::Editing {
            working_text: "hello".into(),
            original_text: "hello".into(),
            cursor_grapheme_offset: 3,
        });
        let result = handle_shadow_cursor_key(&mut shadow, &make_key(Key::Backspace), "hello");
        assert!(matches!(result, ShadowKeyResult::Consumed(_)));
        if let ShadowPhase::Editing {
            working_text,
            cursor_grapheme_offset,
            ..
        } = &shadow.phase
        {
            assert_eq!(working_text, "helo");
            assert_eq!(*cursor_grapheme_offset, 2);
        }
    }

    #[test]
    fn backspace_at_position_zero_is_noop() {
        let mut shadow = make_shadow(ShadowPhase::Editing {
            working_text: "hello".into(),
            original_text: "hello".into(),
            cursor_grapheme_offset: 0,
        });
        let result = handle_shadow_cursor_key(&mut shadow, &make_key(Key::Backspace), "hello");
        assert!(matches!(result, ShadowKeyResult::Consumed(_)));
        if let ShadowPhase::Editing {
            working_text,
            cursor_grapheme_offset,
            ..
        } = &shadow.phase
        {
            assert_eq!(working_text, "hello");
            assert_eq!(*cursor_grapheme_offset, 0);
        }
    }

    #[test]
    fn delete_after_cursor() {
        let mut shadow = make_shadow(ShadowPhase::Editing {
            working_text: "hello".into(),
            original_text: "hello".into(),
            cursor_grapheme_offset: 2,
        });
        let result = handle_shadow_cursor_key(&mut shadow, &make_key(Key::Delete), "hello");
        assert!(matches!(result, ShadowKeyResult::Consumed(_)));
        if let ShadowPhase::Editing {
            working_text,
            cursor_grapheme_offset,
            ..
        } = &shadow.phase
        {
            assert_eq!(working_text, "helo");
            assert_eq!(*cursor_grapheme_offset, 2);
        }
    }

    #[test]
    fn cursor_movement_left_right_home_end() {
        let mut shadow = make_shadow(ShadowPhase::Editing {
            working_text: "hello".into(),
            original_text: "hello".into(),
            cursor_grapheme_offset: 3,
        });
        // Left
        handle_shadow_cursor_key(&mut shadow, &make_key(Key::Left), "hello");
        assert_eq!(editing_offset(&shadow), 2);
        // Right
        handle_shadow_cursor_key(&mut shadow, &make_key(Key::Right), "hello");
        assert_eq!(editing_offset(&shadow), 3);
        // Home
        handle_shadow_cursor_key(&mut shadow, &make_key(Key::Home), "hello");
        assert_eq!(editing_offset(&shadow), 0);
        // End
        handle_shadow_cursor_key(&mut shadow, &make_key(Key::End), "hello");
        assert_eq!(editing_offset(&shadow), 5);
    }

    #[test]
    fn up_down_deactivate() {
        let mut shadow = make_shadow(ShadowPhase::Navigating);
        assert!(matches!(
            handle_shadow_cursor_key(&mut shadow, &make_key(Key::Up), ""),
            ShadowKeyResult::Deactivate
        ));
        assert!(matches!(
            handle_shadow_cursor_key(&mut shadow, &make_key(Key::Down), ""),
            ShadowKeyResult::Deactivate
        ));
    }

    #[test]
    fn enter_commits_from_editing() {
        let mut shadow = make_shadow(ShadowPhase::Editing {
            working_text: "world".into(),
            original_text: "hello".into(),
            cursor_grapheme_offset: 5,
        });
        let result = handle_shadow_cursor_key(&mut shadow, &make_key(Key::Enter), "hello");
        assert!(matches!(result, ShadowKeyResult::Commit(_)));
    }

    #[test]
    fn enter_deactivates_from_navigating() {
        let mut shadow = make_shadow(ShadowPhase::Navigating);
        let result = handle_shadow_cursor_key(&mut shadow, &make_key(Key::Enter), "hello");
        assert!(matches!(result, ShadowKeyResult::Deactivate));
    }

    #[test]
    fn hippocratic_unchanged_returns_empty_commands() {
        let cmds = build_commit_commands("hello", "hello", 0);
        assert!(cmds.is_empty());
    }

    #[test]
    fn mirror_commit_hippocratic() {
        let shadow = make_shadow(ShadowPhase::Editing {
            working_text: "hello".into(),
            original_text: "hello".into(),
            cursor_grapheme_offset: 5,
        });
        let span = mk_span(0, 0, 5);
        let cmds = build_mirror_commit(&shadow, &span, 10);
        assert!(cmds.is_empty(), "Hippocratic: no change → no commands");
    }

    #[test]
    fn mirror_commit_changed() {
        let shadow = make_shadow(ShadowPhase::Editing {
            working_text: "world".into(),
            original_text: "hello".into(),
            cursor_grapheme_offset: 5,
        });
        let span = mk_span(2, 0, 5);
        let cmds = build_mirror_commit(&shadow, &span, 10);
        assert_eq!(cmds.len(), 1, "Mirror: one exec -draft command");
    }

    #[test]
    fn mirror_commit_anchor_out_of_range() {
        let shadow = make_shadow(ShadowPhase::Editing {
            working_text: "world".into(),
            original_text: "hello".into(),
            cursor_grapheme_offset: 5,
        });
        let span = mk_span(100, 0, 5);
        let cmds = build_mirror_commit(&shadow, &span, 10);
        assert!(cmds.is_empty(), "anchor out of range → no commands");
    }

    // -----------------------------------------------------------------
    // ADR-035 §Migration ShadowCursor Phase 3 — algebraic BufferEdit
    // -----------------------------------------------------------------

    #[test]
    fn mirror_edit_returns_buffer_edit_with_target_and_text_pair() {
        let shadow = make_shadow(ShadowPhase::Editing {
            working_text: "world".into(),
            original_text: "hello".into(),
            cursor_grapheme_offset: 5,
        });
        let span = mk_span(2, 0, 5);
        let edit = mirror_edit(&shadow, &span, 10).expect("edit produced");
        assert_eq!(edit.target, span.projection_target);
        assert_eq!(edit.original, "hello");
        assert_eq!(edit.replacement, "world");
        assert!(!edit.is_hippocratic_noop());
    }

    #[test]
    fn mirror_edit_returns_none_for_navigating_phase() {
        let shadow = make_shadow(ShadowPhase::Navigating);
        let span = mk_span(0, 0, 5);
        assert!(mirror_edit(&shadow, &span, 10).is_none());
    }

    #[test]
    fn mirror_edit_returns_none_for_hippocratic_noop() {
        let shadow = make_shadow(ShadowPhase::Editing {
            working_text: "hello".into(),
            original_text: "hello".into(),
            cursor_grapheme_offset: 5,
        });
        let span = mk_span(0, 0, 5);
        assert!(mirror_edit(&shadow, &span, 10).is_none());
    }

    #[test]
    fn mirror_edit_returns_none_for_anchor_out_of_range() {
        let shadow = make_shadow(ShadowPhase::Editing {
            working_text: "world".into(),
            original_text: "hello".into(),
            cursor_grapheme_offset: 5,
        });
        let span = mk_span(100, 0, 5);
        assert!(mirror_edit(&shadow, &span, 10).is_none());
    }

    #[test]
    fn mirror_edit_returns_none_for_plugin_defined_projection() {
        let shadow = make_shadow(ShadowPhase::Editing {
            working_text: "world".into(),
            original_text: "hello".into(),
            cursor_grapheme_offset: 5,
        });
        let mut span = mk_span(0, 0, 5);
        span.projection = EditProjection::PluginDefined;
        assert!(mirror_edit(&shadow, &span, 10).is_none());
    }

    /// Decode a `Command::SendToKakoune(Keys(...))` back into the
    /// keysym-substituted command string. Inverse of
    /// `Command::kakoune_command`'s key-by-key encoding.
    fn render_kakoune_command(cmd: &Command) -> String {
        use crate::plugin::Command;
        use crate::protocol::KasaneRequest;
        let keys = match cmd {
            Command::SendToKakoune(KasaneRequest::Keys(k)) => k,
            _ => panic!("expected SendToKakoune(Keys)"),
        };
        let mut s = String::new();
        for k in keys {
            match k.as_str() {
                "<space>" => s.push(' '),
                "<minus>" => s.push('-'),
                "<lt>" => s.push('<'),
                "<gt>" => s.push('>'),
                "<ret>" => s.push('\n'),
                "<esc>" => s.push_str("<esc>"),
                other => s.push_str(other),
            }
        }
        s
    }

    #[test]
    fn edit_to_commands_substitutes_non_empty_range() {
        use crate::state::selection::{BufferPos, Selection};
        let edit = BufferEdit {
            target: Selection::new(BufferPos::new(2, 0), BufferPos::new(2, 5)),
            original: "hello".into(),
            replacement: "world".into(),
        };
        let cmds = edit_to_commands(&edit);
        assert_eq!(cmds.len(), 1);
        let rendered = render_kakoune_command(&cmds[0]);
        assert!(
            rendered.contains("exec -draft 3g 1l5lscworld<esc>"),
            "non-empty range expected substitute form; got {rendered}"
        );
    }

    #[test]
    fn edit_to_commands_inserts_at_empty_range() {
        use crate::state::selection::{BufferPos, Selection};
        let edit = BufferEdit {
            target: Selection::new(BufferPos::new(0, 3), BufferPos::new(0, 3)),
            original: "".into(),
            replacement: "X".into(),
        };
        let cmds = edit_to_commands(&edit);
        assert_eq!(cmds.len(), 1);
        let rendered = render_kakoune_command(&cmds[0]);
        assert!(
            rendered.contains("exec -draft 1g 4liX<esc>"),
            "empty range expected insert form; got {rendered}"
        );
    }

    #[test]
    fn build_mirror_commit_matches_compose_of_mirror_edit_and_edit_to_commands() {
        let shadow = make_shadow(ShadowPhase::Editing {
            working_text: "world".into(),
            original_text: "hello".into(),
            cursor_grapheme_offset: 5,
        });
        let span = mk_span(2, 0, 5);
        let composed = mirror_edit(&shadow, &span, 10)
            .as_ref()
            .map(edit_to_commands)
            .unwrap_or_default();
        let direct = build_mirror_commit(&shadow, &span, 10);
        assert_eq!(composed.len(), direct.len());
        for (c, d) in composed.iter().zip(direct.iter()) {
            assert_eq!(render_kakoune_command(c), render_kakoune_command(d));
        }
    }

    #[test]
    fn buffer_edit_hippocratic_noop_detects_equal_strings() {
        use crate::state::selection::{BufferPos, Selection};
        let edit = BufferEdit {
            target: Selection::new(BufferPos::new(0, 0), BufferPos::new(0, 5)),
            original: "hello".into(),
            replacement: "hello".into(),
        };
        assert!(edit.is_hippocratic_noop());
    }

    #[test]
    fn mirror_commit_cjk_escape() {
        let escaped = escape_for_kakoune_insert("日<本>語\n行");
        assert_eq!(escaped, "日<lt>本<gt>語<ret>行");
    }

    #[test]
    fn cjk_grapheme_operations() {
        let mut shadow = make_shadow(ShadowPhase::Editing {
            working_text: "日本語".into(),
            original_text: "日本語".into(),
            cursor_grapheme_offset: 1,
        });
        // Backspace at offset 1: delete '日'
        handle_shadow_cursor_key(&mut shadow, &make_key(Key::Backspace), "日本語");
        if let ShadowPhase::Editing {
            working_text,
            cursor_grapheme_offset,
            ..
        } = &shadow.phase
        {
            assert_eq!(working_text, "本語");
            assert_eq!(*cursor_grapheme_offset, 0);
        }
    }

    fn editing_offset(shadow: &ShadowCursor) -> usize {
        match &shadow.phase {
            ShadowPhase::Editing {
                cursor_grapheme_offset,
                ..
            } => *cursor_grapheme_offset,
            _ => panic!("expected Editing phase"),
        }
    }
}

// =============================================================================
// BuiltinShadowCursorPlugin
// =============================================================================

use crate::display::InteractionPolicy;
use crate::input::MouseEvent;
use crate::plugin::{
    AppView, BuiltinTarget, CursorPositionOrn, Effects, FrameworkAccess, KeyPreDispatchResult,
    MousePreDispatchResult, OrnamentBatch, OrnamentModality, PluginBackend, PluginCapabilities,
    RenderOrnamentContext, TextInputPreDispatchResult,
};

/// Builtin plugin that implements the shadow cursor key/text pre-dispatch.
///
/// Reads the shadow cursor state from `RuntimeState` (via `FrameworkAccess`),
/// handles key events and text input, and writes the updated cursor back via
/// `Effects::state_updates.shadow_cursor` (R4 typed channel).
pub struct BuiltinShadowCursorPlugin;

impl PluginBackend for BuiltinShadowCursorPlugin {
    fn id(&self) -> PluginId {
        PluginId("kasane.builtin.shadow_cursor".into())
    }

    fn capabilities(&self) -> PluginCapabilities {
        PluginCapabilities::KEY_PRE_DISPATCH
            | PluginCapabilities::MOUSE_PRE_DISPATCH
            | PluginCapabilities::RENDER_ORNAMENT
    }

    fn on_state_changed_effects(&mut self, state: &AppView<'_>, dirty: DirtyFlags) -> Effects {
        if !dirty.contains(DirtyFlags::BUFFER_CONTENT) {
            return Effects::default();
        }
        let app = state.as_app_state();
        let shadow = match app.runtime.shadow_cursor.as_ref() {
            Some(s) => s,
            None => return Effects::default(),
        };
        if let Some(dum) = &app.runtime.display_unit_map {
            if let Some(unit) = dum.unit_at_line(shadow.display_line) {
                if let crate::display::UnitSource::ProjectedLine { anchor, .. } = &unit.source {
                    if app
                        .inference
                        .lines_dirty
                        .get(*anchor)
                        .copied()
                        .unwrap_or(true)
                    {
                        return Effects::default().with_shadow_cursor(None);
                    }
                } else {
                    // Display unit no longer projected -> deactivate
                    return Effects::default().with_shadow_cursor(None);
                }
            } else {
                // Display line no longer exists -> deactivate
                return Effects::default().with_shadow_cursor(None);
            }
        }
        Effects::default()
    }

    fn handle_key_pre_dispatch(
        &mut self,
        key: &KeyEvent,
        state: &AppView<'_>,
    ) -> KeyPreDispatchResult {
        let app_state = state.as_app_state();
        let shadow = match app_state.runtime.shadow_cursor.as_ref() {
            Some(s) => s,
            None => {
                return KeyPreDispatchResult::Pass {
                    commands: vec![],
                    state_updates: crate::plugin::StateUpdates::default(),
                };
            }
        };

        let display_line = shadow.display_line;
        let span_index = shadow.span_index;

        // Get the span text for transitioning from Navigating → Editing
        let span_text = app_state
            .runtime
            .display_map
            .as_ref()
            .and_then(|dm| {
                let entry = dm.entry(crate::display::DisplayLine(display_line))?;
                let syn = entry.synthetic()?;
                Some(syn.text())
            })
            .unwrap_or_default();

        let mut shadow_mut = shadow.clone();
        match handle_shadow_cursor_key(&mut shadow_mut, key, &span_text) {
            ShadowKeyResult::Consumed(flags) => KeyPreDispatchResult::Consumed {
                flags,
                commands: vec![],
                state_updates: crate::plugin::StateUpdates {
                    shadow_cursor: Some(Some(shadow_mut)),
                    ..Default::default()
                },
            },
            ShadowKeyResult::Deactivate => KeyPreDispatchResult::Pass {
                commands: vec![],
                state_updates: crate::plugin::StateUpdates {
                    shadow_cursor: Some(None),
                    ..Default::default()
                },
            },
            ShadowKeyResult::Commit(_) => {
                // Resolve the editable span and build mirror commit commands
                let commit_commands = app_state
                    .runtime
                    .display_map
                    .as_ref()
                    .and_then(|dm| {
                        let entry = dm.entry(crate::display::DisplayLine(display_line))?;
                        if let crate::display::SourceMapping::Projected { spans, .. } =
                            entry.source()
                        {
                            let span = spans.get(span_index)?;
                            Some(build_mirror_commit(
                                &shadow_mut,
                                span,
                                app_state.observed.lines.len(),
                            ))
                        } else {
                            None
                        }
                    })
                    .unwrap_or_default();
                KeyPreDispatchResult::Consumed {
                    flags: DirtyFlags::BUFFER_CONTENT,
                    commands: commit_commands,
                    state_updates: crate::plugin::StateUpdates {
                        shadow_cursor: Some(None),
                        ..Default::default()
                    },
                }
            }
        }
    }

    fn handle_text_input_pre_dispatch(
        &mut self,
        text: &str,
        state: &AppView<'_>,
    ) -> TextInputPreDispatchResult {
        let app_state = state.as_app_state();
        let shadow = match app_state.runtime.shadow_cursor.as_ref() {
            Some(s) => s,
            None => return TextInputPreDispatchResult::Pass,
        };

        let mut shadow_mut = shadow.clone();
        if let ShadowPhase::Editing {
            ref mut working_text,
            ref mut cursor_grapheme_offset,
            ..
        } = shadow_mut.phase
        {
            let offset = *cursor_grapheme_offset;
            let byte_pos: usize = working_text
                .graphemes(true)
                .take(offset)
                .map(|g| g.len())
                .sum();
            working_text.insert_str(byte_pos, text);
            *cursor_grapheme_offset += text.graphemes(true).count();
            TextInputPreDispatchResult::Consumed {
                flags: DirtyFlags::BUFFER_CONTENT,
                commands: vec![],
                state_updates: crate::plugin::StateUpdates {
                    shadow_cursor: Some(Some(shadow_mut)),
                    ..Default::default()
                },
            }
        } else {
            TextInputPreDispatchResult::Pass
        }
    }

    fn handle_mouse_pre_dispatch(
        &mut self,
        event: &MouseEvent,
        state: &AppView<'_>,
    ) -> MousePreDispatchResult {
        let app = state.as_app_state();
        if app.runtime.shadow_cursor.is_none() {
            return MousePreDispatchResult::Pass {
                commands: vec![],
                state_updates: crate::plugin::StateUpdates::default(),
            };
        }
        if !matches!(event.kind, crate::input::MouseEventKind::Press(_)) {
            return MousePreDispatchResult::Pass {
                commands: vec![],
                state_updates: crate::plugin::StateUpdates::default(),
            };
        }
        if app
            .runtime
            .suppressed_builtins
            .contains(&BuiltinTarget::ShadowCursor)
        {
            return MousePreDispatchResult::Pass {
                commands: vec![],
                state_updates: crate::plugin::StateUpdates::default(),
            };
        }
        let hit_editable = app
            .runtime
            .display_unit_map
            .as_ref()
            .and_then(|dum| dum.hit_test(event.line, app.runtime.display_scroll_offset))
            .is_some_and(|u| u.interaction == InteractionPolicy::Editable);
        if !hit_editable {
            MousePreDispatchResult::Pass {
                commands: vec![],
                state_updates: crate::plugin::StateUpdates {
                    shadow_cursor: Some(None),
                    ..Default::default()
                },
            }
        } else {
            MousePreDispatchResult::Pass {
                commands: vec![],
                state_updates: crate::plugin::StateUpdates::default(),
            }
        }
    }

    fn render_ornaments(&self, state: &AppView<'_>, ctx: &RenderOrnamentContext) -> OrnamentBatch {
        let app = state.as_app_state();
        let shadow = match app.runtime.shadow_cursor.as_ref() {
            Some(s) => s,
            None => return OrnamentBatch::default(),
        };
        if let ShadowPhase::Editing {
            cursor_grapheme_offset,
            working_text,
            ..
        } = &shadow.phase
        {
            use unicode_width::UnicodeWidthStr;
            // Compute display column from grapheme offset
            let display_col: u16 = working_text
                .graphemes(true)
                .take(*cursor_grapheme_offset)
                .map(|g| UnicodeWidthStr::width(g) as u16)
                .sum();
            let cx = display_col + ctx.buffer_x_offset;
            let display_scroll_offset = ctx.visible_line_start as u16;
            let cy = (shadow.display_line as u16).saturating_sub(display_scroll_offset)
                + ctx.buffer_y_offset;
            OrnamentBatch {
                cursor_position: Some(CursorPositionOrn {
                    x: cx,
                    y: cy,
                    style: crate::render::CursorStyle::Bar,
                    color: crate::protocol::Color::Default,
                    priority: 100,
                    modality: OrnamentModality::Must,
                }),
                ..Default::default()
            }
        } else {
            OrnamentBatch::default()
        }
    }
}
