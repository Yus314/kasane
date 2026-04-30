//! Validation, line-dirty computation, and status line building.

use crate::protocol::{Atom, Coord, Line, WireFace};

use super::atom_metrics::atom_display_width;

/// Describes a mismatch between Kakoune's `cursor_pos.column` and the
/// column computed by walking atoms with `atom_display_width`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WidthDivergence {
    /// Column reported by Kakoune's `cursor_pos` in the `draw` message.
    pub protocol_column: u32,
    /// Column computed by summing `atom_display_width` up to the cursor atom.
    pub computed_column: u32,
    /// Text of the atom at the cursor position (for diagnostics).
    pub atom_text: String,
}

/// Check whether `cursor_pos.column` from the protocol is consistent with
/// the column computed by walking atoms on that line via `atom_display_width`.
///
/// # Inference Rule: R-1
/// **Assumption**: `atom_display_width` (unicode-width) matches Kakoune's
/// internal `char_column_offset` width calculation.
/// **Failure mode**: CJK characters, emoji, or other wide/combining characters
/// cause cursor to render at wrong column — all buffer-relative overlays
/// (menus, info windows, cursor highlight) are mispositioned.
/// **Severity**: Catastrophic (visual corruption of entire buffer area)
///
/// Returns `None` if consistent, `Some(WidthDivergence)` on mismatch.
pub fn check_cursor_width_consistency(
    lines: &[Line],
    cursor_pos: Coord,
) -> Option<WidthDivergence> {
    let line_idx = cursor_pos.line as usize;
    let line = lines.get(line_idx)?;
    let target_col = cursor_pos.column as u32;

    let mut col: u32 = 0;
    for atom in line.iter() {
        let width = atom_display_width(atom);
        if col <= target_col && target_col < col + width.max(1) {
            // Cursor falls within this atom — consistent
            return None;
        }
        col += width;
    }

    // cursor_pos.column == total line width is valid (cursor at EOL)
    if target_col == col {
        return None;
    }

    // Divergence: cursor column doesn't fall within any atom's range
    let atom_text = line
        .last()
        .map(|a| a.contents.to_string())
        .unwrap_or_default();
    Some(WidthDivergence {
        protocol_column: target_col,
        computed_column: col,
        atom_text,
    })
}

/// Compute per-line dirty flags by comparing old and new line data.
///
/// # Inference Rule: R-3
/// **Assumption**: Line equality (`PartialEq` on atom contents + face) implies
/// visual equality — if atoms are identical, the rendered output is identical.
/// **Failure mode**: If external state (e.g. terminal width, font metrics) affects
/// rendering beyond atom data, unchanged lines may need repainting.
/// **Severity**: Degraded (stale line content displayed)
///
/// Returns a `Vec<bool>` with one entry per new line. If face or line count
/// changed, all lines are marked dirty.
pub fn compute_lines_dirty(
    old_lines: &[Line],
    new_lines: &[Line],
    old_default_face: &WireFace,
    new_default_face: &WireFace,
    old_padding_face: &WireFace,
    new_padding_face: &WireFace,
) -> Vec<bool> {
    let face_changed =
        *old_default_face != *new_default_face || *old_padding_face != *new_padding_face;
    let len_changed = old_lines.len() != new_lines.len();

    if face_changed || len_changed {
        vec![true; new_lines.len()]
    } else {
        old_lines
            .iter()
            .zip(new_lines.iter())
            .map(|(old, new)| old != new)
            .collect()
    }
}

/// Concatenate status prompt and content atoms into a single status line.
pub fn build_status_line(prompt: &[Atom], content: &[Atom]) -> Line {
    let mut combined = prompt.to_vec();
    combined.extend_from_slice(content);
    combined
}
