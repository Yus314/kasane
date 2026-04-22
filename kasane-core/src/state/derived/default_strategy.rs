//! Default inference strategy that delegates to the existing free functions.

use crate::protocol::{Coord, CursorMode, Face, Line};

use super::cursor::CursorCache;
use super::selection::Selection;
use super::{EditorMode, InferenceStrategy};

/// Default inference strategy using the built-in heuristics.
///
/// Delegates to the existing free functions:
/// - [`detect_cursors_incremental`](super::detect_cursors_incremental) for I-1
/// - [`detect_selections`](super::detect_selections) for I-7
/// - [`derive_editor_mode`](super::derive_editor_mode) for I-2
pub struct DefaultInferenceStrategy;

impl InferenceStrategy for DefaultInferenceStrategy {
    fn detect_cursors(
        &self,
        lines: &[Line],
        primary_cursor_pos: Coord,
        lines_dirty: &[bool],
        cache: &mut CursorCache,
    ) -> (usize, Vec<Coord>) {
        super::detect_cursors_incremental(lines, primary_cursor_pos, lines_dirty, cache)
    }

    fn detect_selections(
        &self,
        lines: &[Line],
        primary_cursor_pos: Coord,
        secondary_cursors: &[Coord],
        default_face: &Face,
    ) -> Vec<Selection> {
        super::detect_selections(lines, primary_cursor_pos, secondary_cursors, default_face)
    }

    fn derive_editor_mode(&self, cursor_mode: CursorMode, status_mode_line: &Line) -> EditorMode {
        super::derive_editor_mode(cursor_mode, status_mode_line)
    }
}
