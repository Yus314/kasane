//! Inference state sub-struct.
//!
//! Contains fields derived or heuristically inferred from observed state.
//! These are the `I` component of the world model `W = (T, I, Π, S)`.

use crate::protocol::{Coord, CursorMode, Line};
use crate::render::color_context::ColorContext;

use super::derived::{EditorMode, Selection};
use super::selection_set::SelectionSet;

/// Derived and heuristic state inferred from protocol observations.
///
/// Every field here carries `#[epistemic(derived)]` or `#[epistemic(heuristic)]`
/// semantics: it is deterministically computed from observed fields, or inferred
/// from Kakoune internal implementation details.
#[derive(Debug, Clone, PartialEq)]
pub struct InferenceState {
    /// Per-line dirty flags computed by diffing old vs new `lines`.
    pub lines_dirty: Vec<bool>,
    /// Inferred from `status_content_cursor_pos >= 0` (Buffer vs Prompt).
    pub cursor_mode: CursorMode,
    /// Concatenation of `status_prompt` + `status_content` for rendering.
    pub status_line: Line,
    /// Parsed editor mode from cursor_mode + status_mode_line heuristic (I-2).
    pub editor_mode: EditorMode,
    /// Color context derived from default_face luminance analysis.
    pub color_context: ColorContext,
    /// Total cursor count (primary + secondary), detected via face attributes.
    pub cursor_count: usize,
    /// Positions of secondary cursors (all cursors except primary).
    pub secondary_cursors: Vec<Coord>,
    /// Detected selection ranges from buffer atoms (I-7).
    pub selections: Vec<Selection>,
    /// Canonical `SelectionSet` projected from `selections` (ADR-035 §1).
    /// Populated by `apply_protocol` whenever it recomputes the heuristic
    /// detection. Plugins should prefer this over the legacy `selections`
    /// field, which carries a different shape (`derived::Selection` with
    /// `Coord` i32 / `is_primary` flag) and is retained for backward
    /// compatibility until its consumers migrate.
    pub selection_set: SelectionSet,
}

impl Default for InferenceState {
    fn default() -> Self {
        Self {
            lines_dirty: Vec::new(),
            cursor_mode: CursorMode::Buffer,
            status_line: Vec::new(),
            editor_mode: EditorMode::default(),
            color_context: ColorContext::default(),
            cursor_count: 0,
            secondary_cursors: Vec::new(),
            selections: Vec::new(),
            selection_set: SelectionSet::default_empty(),
        }
    }
}
