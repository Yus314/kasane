//! Shadow cursor state for editable virtual text (BDT).
//!
//! A [`ShadowCursor`] represents a cursor within an editable virtual
//! text span. It operates outside the Kakoune protocol: text editing
//! happens locally in `working_text`, and only on commit (Enter) is
//! the result projected back to the buffer via `exec -draft`.
//!
//! ## Coordinates
//!
//! The shadow cursor's position lives in *synthetic* `working_text`
//! bytes. `cursor_grapheme_offset` indexes graphemes within the
//! editable span, not buffer columns. Keyboard handling
//! ([`handle_shadow_cursor_key`]) is grapheme arithmetic; it does
//! not re-shape onto buffer-space `SelectionSet` algebra.
//!
//! [`EditableSpan::projection_target`] is the buffer-space target
//! a Mirror commit lands at — a single [`Selection`] whose anchor
//! and cursor share one buffer line, with their columns delimiting
//! the byte range.
//!
//! ## Commit pipeline
//!
//! Commits flow through two layers:
//!
//! - [`mirror_edit`] computes the algebraic [`BufferEdit`] from a
//!   shadow cursor + its [`EditableSpan`]. This is the payload for
//!   the plugin commit-intercept hook (`on_buffer_edit_intercept`).
//! - [`edit_to_commands`] serialises a [`BufferEdit`] into the
//!   Kakoune `exec -draft` commands that land it.
//! - [`build_mirror_commit`] composes the two for the dispatch-side
//!   entry point.
//!
//! ## Version stamping
//!
//! When an edit transitions `Navigating → Editing`, the current
//! history [`VersionId`] is recorded in
//! [`ShadowPhase::Editing::base_version`] and propagated through to
//! [`BufferEdit::base_version`]. [`BufferEdit::is_stale_against`]
//! lets a downstream consumer detect that the buffer advanced past
//! the version the edit was authored against; callers can also
//! compose with `Time::At(v)` queries to materialise the buffer
//! state the edit targeted.

use std::ops::Range;

use unicode_segmentation::UnicodeSegmentation;

use crate::history::VersionId;
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
        /// History `VersionId` at the moment the user transitioned
        /// from `Navigating` into `Editing`. Stamped once at
        /// activation and preserved across in-place keystroke edits
        /// within the span; flows through to
        /// `BufferEdit::base_version` on commit so a downstream
        /// consumer can detect a stale commit (the buffer advanced
        /// underneath the edit) or compose with `Time::At(v)`
        /// queries.
        base_version: VersionId,
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

mod commit;
mod keyboard;

pub use commit::{
    BufferEdit, BufferEditVerdict, build_mirror_commit, edit_to_commands, mirror_edit,
};
pub use keyboard::handle_shadow_cursor_key;

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

    fn mk_editing(working: &str, original: &str, cursor: usize) -> ShadowPhase {
        ShadowPhase::Editing {
            working_text: working.into(),
            original_text: original.into(),
            cursor_grapheme_offset: cursor,
            base_version: VersionId::INITIAL,
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
        let result = handle_shadow_cursor_key(
            &mut shadow,
            &make_key(Key::Escape),
            "hello",
            VersionId::INITIAL,
        );
        assert!(matches!(result, ShadowKeyResult::Deactivate));
    }

    #[test]
    fn escape_deactivates_from_editing() {
        let mut shadow = make_shadow(mk_editing("hello", "hello", 5));
        let result = handle_shadow_cursor_key(
            &mut shadow,
            &make_key(Key::Escape),
            "hello",
            VersionId::INITIAL,
        );
        assert!(matches!(result, ShadowKeyResult::Deactivate));
    }

    #[test]
    fn char_transitions_navigating_to_editing() {
        let mut shadow = make_shadow(ShadowPhase::Navigating);
        let result = handle_shadow_cursor_key(
            &mut shadow,
            &make_key(Key::Char('a')),
            "hello",
            VersionId::INITIAL,
        );
        assert!(matches!(result, ShadowKeyResult::Consumed(_)));
        match &shadow.phase {
            ShadowPhase::Editing {
                working_text,
                original_text,
                cursor_grapheme_offset,
                ..
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
        let mut shadow = make_shadow(mk_editing("hllo", "hllo", 1));
        let result = handle_shadow_cursor_key(
            &mut shadow,
            &make_key(Key::Char('e')),
            "hllo",
            VersionId::INITIAL,
        );
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
        let mut shadow = make_shadow(mk_editing("hello", "hello", 3));
        let result = handle_shadow_cursor_key(
            &mut shadow,
            &make_key(Key::Backspace),
            "hello",
            VersionId::INITIAL,
        );
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
        let mut shadow = make_shadow(mk_editing("hello", "hello", 0));
        let result = handle_shadow_cursor_key(
            &mut shadow,
            &make_key(Key::Backspace),
            "hello",
            VersionId::INITIAL,
        );
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
        let mut shadow = make_shadow(mk_editing("hello", "hello", 2));
        let result = handle_shadow_cursor_key(
            &mut shadow,
            &make_key(Key::Delete),
            "hello",
            VersionId::INITIAL,
        );
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
        let mut shadow = make_shadow(mk_editing("hello", "hello", 3));
        // Left
        handle_shadow_cursor_key(
            &mut shadow,
            &make_key(Key::Left),
            "hello",
            VersionId::INITIAL,
        );
        assert_eq!(editing_offset(&shadow), 2);
        // Right
        handle_shadow_cursor_key(
            &mut shadow,
            &make_key(Key::Right),
            "hello",
            VersionId::INITIAL,
        );
        assert_eq!(editing_offset(&shadow), 3);
        // Home
        handle_shadow_cursor_key(
            &mut shadow,
            &make_key(Key::Home),
            "hello",
            VersionId::INITIAL,
        );
        assert_eq!(editing_offset(&shadow), 0);
        // End
        handle_shadow_cursor_key(
            &mut shadow,
            &make_key(Key::End),
            "hello",
            VersionId::INITIAL,
        );
        assert_eq!(editing_offset(&shadow), 5);
    }

    #[test]
    fn up_down_deactivate() {
        let mut shadow = make_shadow(ShadowPhase::Navigating);
        assert!(matches!(
            handle_shadow_cursor_key(&mut shadow, &make_key(Key::Up), "", VersionId::INITIAL),
            ShadowKeyResult::Deactivate
        ));
        assert!(matches!(
            handle_shadow_cursor_key(&mut shadow, &make_key(Key::Down), "", VersionId::INITIAL),
            ShadowKeyResult::Deactivate
        ));
    }

    #[test]
    fn enter_commits_from_editing() {
        let mut shadow = make_shadow(mk_editing("world", "hello", 5));
        let result = handle_shadow_cursor_key(
            &mut shadow,
            &make_key(Key::Enter),
            "hello",
            VersionId::INITIAL,
        );
        assert!(matches!(result, ShadowKeyResult::Commit(_)));
    }

    #[test]
    fn enter_deactivates_from_navigating() {
        let mut shadow = make_shadow(ShadowPhase::Navigating);
        let result = handle_shadow_cursor_key(
            &mut shadow,
            &make_key(Key::Enter),
            "hello",
            VersionId::INITIAL,
        );
        assert!(matches!(result, ShadowKeyResult::Deactivate));
    }

    #[test]
    fn hippocratic_unchanged_returns_empty_commands() {
        let cmds = build_commit_commands("hello", "hello", 0);
        assert!(cmds.is_empty());
    }

    #[test]
    fn mirror_commit_hippocratic() {
        let shadow = make_shadow(mk_editing("hello", "hello", 5));
        let span = mk_span(0, 0, 5);
        let cmds = build_mirror_commit(&shadow, &span, 10);
        assert!(cmds.is_empty(), "Hippocratic: no change → no commands");
    }

    #[test]
    fn mirror_commit_changed() {
        let shadow = make_shadow(mk_editing("world", "hello", 5));
        let span = mk_span(2, 0, 5);
        let cmds = build_mirror_commit(&shadow, &span, 10);
        assert_eq!(cmds.len(), 1, "Mirror: one exec -draft command");
    }

    #[test]
    fn mirror_commit_anchor_out_of_range() {
        let shadow = make_shadow(mk_editing("world", "hello", 5));
        let span = mk_span(100, 0, 5);
        let cmds = build_mirror_commit(&shadow, &span, 10);
        assert!(cmds.is_empty(), "anchor out of range → no commands");
    }

    // -----------------------------------------------------------------
    // ADR-035 §Migration ShadowCursor Phase 3 — algebraic BufferEdit
    // -----------------------------------------------------------------

    #[test]
    fn mirror_edit_returns_buffer_edit_with_target_and_text_pair() {
        let shadow = make_shadow(mk_editing("world", "hello", 5));
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
        let shadow = make_shadow(mk_editing("hello", "hello", 5));
        let span = mk_span(0, 0, 5);
        assert!(mirror_edit(&shadow, &span, 10).is_none());
    }

    #[test]
    fn mirror_edit_returns_none_for_anchor_out_of_range() {
        let shadow = make_shadow(mk_editing("world", "hello", 5));
        let span = mk_span(100, 0, 5);
        assert!(mirror_edit(&shadow, &span, 10).is_none());
    }

    #[test]
    fn mirror_edit_returns_none_for_plugin_defined_projection() {
        let shadow = make_shadow(mk_editing("world", "hello", 5));
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

    fn mk_buffer_edit(
        line: u32,
        start: u32,
        end: u32,
        original: &str,
        replacement: &str,
    ) -> BufferEdit {
        use crate::state::selection::{BufferPos, Selection};
        BufferEdit {
            target: Selection::new(BufferPos::new(line, start), BufferPos::new(line, end)),
            original: original.into(),
            replacement: replacement.into(),
            base_version: VersionId::INITIAL,
        }
    }

    #[test]
    fn edit_to_commands_substitutes_non_empty_range() {
        let edit = mk_buffer_edit(2, 0, 5, "hello", "world");
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
        let edit = mk_buffer_edit(0, 3, 3, "", "X");
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
        let shadow = make_shadow(mk_editing("world", "hello", 5));
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
        let edit = mk_buffer_edit(0, 0, 5, "hello", "hello");
        assert!(edit.is_hippocratic_noop());
    }

    // -----------------------------------------------------------------
    // ADR-035 §Migration ShadowCursor Phase 4 — VersionId stamp
    // -----------------------------------------------------------------

    #[test]
    fn buffer_edit_is_stale_against_advanced_version() {
        let mut edit = mk_buffer_edit(0, 0, 5, "hello", "world");
        edit.base_version = VersionId(7);
        assert!(
            !edit.is_stale_against(VersionId(7)),
            "same version: not stale"
        );
        assert!(
            !edit.is_stale_against(VersionId(6)),
            "older current: not stale"
        );
        assert!(
            edit.is_stale_against(VersionId(8)),
            "advanced current: stale"
        );
    }

    #[test]
    fn handle_shadow_cursor_key_stamps_base_version_at_activation() {
        let mut shadow = make_shadow(ShadowPhase::Navigating);
        let stamp = VersionId(42);
        let _ = handle_shadow_cursor_key(&mut shadow, &make_key(Key::Char('a')), "hello", stamp);
        match &shadow.phase {
            ShadowPhase::Editing { base_version, .. } => assert_eq!(*base_version, stamp),
            _ => panic!("expected Editing phase after Char activation"),
        }
    }

    #[test]
    fn handle_shadow_cursor_key_preserves_base_version_across_in_place_edits() {
        // Activate at v=5 with first Char.
        let mut shadow = make_shadow(ShadowPhase::Navigating);
        let activation = VersionId(5);
        let _ =
            handle_shadow_cursor_key(&mut shadow, &make_key(Key::Char('a')), "hello", activation);
        // A subsequent in-place edit with a *different* current_version
        // (the buffer advanced underneath, but the user kept typing).
        let later = VersionId(99);
        let _ = handle_shadow_cursor_key(&mut shadow, &make_key(Key::Char('b')), "hello", later);
        match &shadow.phase {
            ShadowPhase::Editing { base_version, .. } => assert_eq!(
                *base_version, activation,
                "in-place edits must preserve the activation stamp"
            ),
            _ => panic!("expected Editing phase"),
        }
    }

    #[test]
    fn mirror_edit_surfaces_base_version_from_editing_phase() {
        let stamp = VersionId(13);
        let mut shadow = make_shadow(ShadowPhase::Navigating);
        let _ = handle_shadow_cursor_key(&mut shadow, &make_key(Key::Char('a')), "hello", stamp);
        let span = mk_span(0, 0, 6); // "hello" + 'a' = 6 bytes
        let edit = mirror_edit(&shadow, &span, 10).expect("edit produced");
        assert_eq!(edit.base_version, stamp);
    }

    #[test]
    fn mirror_commit_cjk_escape() {
        let escaped = escape_for_kakoune_insert("日<本>語\n行");
        assert_eq!(escaped, "日<lt>本<gt>語<ret>行");
    }

    #[test]
    fn cjk_grapheme_operations() {
        let mut shadow = make_shadow(mk_editing("日本語", "日本語", 1));
        // Backspace at offset 1: delete '日'
        handle_shadow_cursor_key(
            &mut shadow,
            &make_key(Key::Backspace),
            "日本語",
            VersionId::INITIAL,
        );
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
use crate::plugin::{
    BuiltinTarget, CursorPositionOrn, Effects, FrameworkAccess, HandlerRegistry,
    KeyPreDispatchResult, MousePreDispatchResult, OrnamentBatch, OrnamentModality, Plugin,
    StateUpdates, TextInputPreDispatchResult,
};

/// Builtin plugin that implements the shadow cursor key/text pre-dispatch.
///
/// Reads the shadow cursor state from `RuntimeState` (via `FrameworkAccess`),
/// handles key events and text input, and writes the updated cursor back via
/// `Effects::state_updates.shadow_cursor` (R4 typed channel).
pub struct BuiltinShadowCursorPlugin;

impl Plugin for BuiltinShadowCursorPlugin {
    type State = ();

    fn id(&self) -> PluginId {
        PluginId("kasane.builtin.shadow_cursor".into())
    }

    fn register(&self, r: &mut HandlerRegistry<()>) {
        r.declare_interests(DirtyFlags::BUFFER_CONTENT);

        r.on_state_changed(|_state, app, dirty| {
            if !dirty.contains(DirtyFlags::BUFFER_CONTENT) {
                return ((), Effects::default());
            }
            let app_state = app.as_app_state();
            let shadow = match app_state.runtime.shadow_cursor.as_ref() {
                Some(s) => s,
                None => return ((), Effects::default()),
            };
            if let Some(dum) = &app_state.runtime.display_unit_map {
                if let Some(unit) = dum.unit_at_line(shadow.display_line) {
                    if let crate::display::UnitSource::ProjectedLine { anchor, .. } = &unit.source {
                        if app_state
                            .inference
                            .lines_dirty
                            .get(*anchor)
                            .copied()
                            .unwrap_or(true)
                        {
                            return ((), Effects::default().with_shadow_cursor(None));
                        }
                    } else {
                        return ((), Effects::default().with_shadow_cursor(None));
                    }
                } else {
                    return ((), Effects::default().with_shadow_cursor(None));
                }
            }
            ((), Effects::default())
        });

        r.on_key_pre_dispatch(|_state, key, app| {
            let app_state = app.as_app_state();
            let shadow = match app_state.runtime.shadow_cursor.as_ref() {
                Some(s) => s,
                None => {
                    return (
                        (),
                        KeyPreDispatchResult::Pass {
                            commands: vec![],
                            state_updates: StateUpdates::default(),
                        },
                    );
                }
            };

            let display_line = shadow.display_line;
            let span_index = shadow.span_index;

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
            let current_version = {
                use crate::history::HistoryBackend;
                app_state.history.current_version()
            };
            let result =
                match handle_shadow_cursor_key(&mut shadow_mut, key, &span_text, current_version) {
                    ShadowKeyResult::Consumed(flags) => KeyPreDispatchResult::Consumed {
                        flags,
                        commands: vec![],
                        state_updates: StateUpdates {
                            shadow_cursor: Some(Some(shadow_mut)),
                            ..Default::default()
                        },
                        pending_buffer_edit: None,
                    },
                    ShadowKeyResult::Deactivate => KeyPreDispatchResult::Pass {
                        commands: vec![],
                        state_updates: StateUpdates {
                            shadow_cursor: Some(None),
                            ..Default::default()
                        },
                    },
                    ShadowKeyResult::Commit(_) => {
                        // ADR-035 ShadowCursor follow-up: surface the algebraic
                        // BufferEdit for intercept-chain dispatch instead of
                        // pre-serializing. The dispatch loop runs
                        // `intercept_buffer_edit` across registered plugins.
                        let pending_buffer_edit =
                            app_state.runtime.display_map.as_ref().and_then(|dm| {
                                let entry = dm.entry(crate::display::DisplayLine(display_line))?;
                                if let crate::display::SourceMapping::Projected { spans, .. } =
                                    entry.source()
                                {
                                    let span = spans.get(span_index)?;
                                    mirror_edit(&shadow_mut, span, app_state.observed.lines.len())
                                } else {
                                    None
                                }
                            });
                        KeyPreDispatchResult::Consumed {
                            flags: DirtyFlags::BUFFER_CONTENT,
                            commands: vec![],
                            state_updates: StateUpdates {
                                shadow_cursor: Some(None),
                                ..Default::default()
                            },
                            pending_buffer_edit,
                        }
                    }
                };
            ((), result)
        });

        r.on_text_input_pre_dispatch(|_state, text, app| {
            let app_state = app.as_app_state();
            let shadow = match app_state.runtime.shadow_cursor.as_ref() {
                Some(s) => s,
                None => return ((), TextInputPreDispatchResult::Pass),
            };

            let mut shadow_mut = shadow.clone();
            let result = if let ShadowPhase::Editing {
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
                    state_updates: StateUpdates {
                        shadow_cursor: Some(Some(shadow_mut)),
                        ..Default::default()
                    },
                }
            } else {
                TextInputPreDispatchResult::Pass
            };
            ((), result)
        });

        r.on_mouse_pre_dispatch(|_state, event, app| {
            let app_state = app.as_app_state();
            if app_state.runtime.shadow_cursor.is_none() {
                return (
                    (),
                    MousePreDispatchResult::Pass {
                        commands: vec![],
                        state_updates: StateUpdates::default(),
                    },
                );
            }
            if !matches!(event.kind, crate::input::MouseEventKind::Press(_)) {
                return (
                    (),
                    MousePreDispatchResult::Pass {
                        commands: vec![],
                        state_updates: StateUpdates::default(),
                    },
                );
            }
            if app_state
                .runtime
                .suppressed_builtins
                .contains(&BuiltinTarget::ShadowCursor)
            {
                return (
                    (),
                    MousePreDispatchResult::Pass {
                        commands: vec![],
                        state_updates: StateUpdates::default(),
                    },
                );
            }
            let hit_editable = app_state
                .runtime
                .display_unit_map
                .as_ref()
                .and_then(|dum| dum.hit_test(event.line, app_state.runtime.display_scroll_offset))
                .is_some_and(|u| u.interaction == InteractionPolicy::Editable);
            let result = if !hit_editable {
                MousePreDispatchResult::Pass {
                    commands: vec![],
                    state_updates: StateUpdates {
                        shadow_cursor: Some(None),
                        ..Default::default()
                    },
                }
            } else {
                MousePreDispatchResult::Pass {
                    commands: vec![],
                    state_updates: StateUpdates::default(),
                }
            };
            ((), result)
        });

        r.on_render_ornaments(|_state, app, ctx| {
            let app_state = app.as_app_state();
            let shadow = match app_state.runtime.shadow_cursor.as_ref() {
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
        });
    }
}
