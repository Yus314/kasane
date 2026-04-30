//! Selection range detection from buffer atoms.

use crate::protocol::{Color, Coord, Line, WireFace};

use super::atom_metrics::atom_display_width;

/// A detected selection range.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Selection {
    /// Start of the selection (earlier position in document order).
    pub anchor: Coord,
    /// End of the selection (cursor position).
    pub cursor: Coord,
    /// Whether this is the primary selection.
    pub is_primary: bool,
}

/// Detect selection ranges from buffer atoms by scanning for contiguous runs of
/// highlighted (non-default) atoms adjacent to each cursor position.
///
/// # Inference Rule: I-7
/// **Assumption**: Selection atoms have a non-default face (bg != Default) that
/// differs from the cursor face. The selection extends as a contiguous run
/// on the same line from/to the cursor.
/// **Failure mode**: If the theme uses default bg for selections, detection fails
/// and an empty Vec is returned. Multi-line selections are currently
/// detected per-line only (cross-line continuity not verified).
/// **Severity**: Degraded (selection-dependent plugin features unavailable)
pub fn detect_selections(
    lines: &[Line],
    primary_cursor_pos: Coord,
    secondary_cursors: &[Coord],
    default_face: &WireFace,
) -> Vec<Selection> {
    let mut all_cursors = vec![primary_cursor_pos];
    all_cursors.extend_from_slice(secondary_cursors);

    // Safety valve: too many cursors makes heuristic unreliable
    if all_cursors.len() > 64 {
        return Vec::new();
    }

    let mut selections = Vec::new();
    for (i, &cursor_pos) in all_cursors.iter().enumerate() {
        let is_primary = i == 0;
        if let Some(sel) = detect_single_selection(lines, cursor_pos, default_face, is_primary) {
            selections.push(sel);
        }
    }
    selections
}

/// Detect the selection range around a single cursor position.
///
/// Scans the line containing the cursor for contiguous atoms with non-default
/// bg that are adjacent to the cursor atom. Returns `None` if no selection
/// face is detected (cursor-only, 1-char selection).
fn detect_single_selection(
    lines: &[Line],
    cursor_pos: Coord,
    default_face: &WireFace,
    is_primary: bool,
) -> Option<Selection> {
    let line_idx = cursor_pos.line as usize;
    let line = lines.get(line_idx)?;
    let cursor_col = cursor_pos.column as u32;

    // Build a column map: (start_col, end_col, face) for each atom
    let mut segments: Vec<(u32, u32, WireFace)> = Vec::new();
    let mut col: u32 = 0;
    for atom in line.iter() {
        let width = atom_display_width(atom);
        if width > 0 {
            segments.push((col, col + width, atom.unresolved_style().to_face()));
        }
        col += width;
    }

    // Find the cursor's segment index
    let cursor_seg_idx = segments
        .iter()
        .position(|(start, end, _)| cursor_col >= *start && cursor_col < *end)?;

    let cursor_face = segments[cursor_seg_idx].2;

    // A selection face is any non-default bg face that differs from the cursor face.
    // We look for atoms that share the same bg as the cursor OR have a non-default bg
    // that looks like a selection highlight.
    //
    // Strategy: scan left and right from cursor, looking for atoms with non-default bg
    // that aren't the cursor face itself. If the cursor has REVERSE attribute, the
    // selection face typically shares bg or has a related highlight face.

    // Determine the "selection bg" by looking at atoms adjacent to the cursor
    let selection_bg = find_selection_bg(&segments, cursor_seg_idx, &cursor_face, default_face)?;

    // Scan left from cursor
    let mut sel_start_col = segments[cursor_seg_idx].0;
    for i in (0..cursor_seg_idx).rev() {
        let (start, _, face) = &segments[i];
        if face.bg == selection_bg || (face.bg != default_face.bg && face.bg != Color::Default) {
            sel_start_col = *start;
        } else {
            break;
        }
    }

    // Scan right from cursor
    let mut sel_end_col = segments[cursor_seg_idx].1.saturating_sub(1);
    for (_, end, face) in &segments[(cursor_seg_idx + 1)..] {
        if face.bg == selection_bg || (face.bg != default_face.bg && face.bg != Color::Default) {
            sel_end_col = end.saturating_sub(1);
        } else {
            break;
        }
    }

    // If the selection is just the cursor itself (1 char), return anchor == cursor
    let anchor = Coord {
        line: cursor_pos.line,
        column: sel_start_col as i32,
    };
    let cursor_end = Coord {
        line: cursor_pos.line,
        column: sel_end_col as i32,
    };

    Some(Selection {
        anchor,
        cursor: cursor_end,
        is_primary,
    })
}

/// Find the background color used for selection highlighting by examining
/// atoms adjacent to the cursor.
fn find_selection_bg(
    segments: &[(u32, u32, WireFace)],
    cursor_idx: usize,
    cursor_face: &WireFace,
    default_face: &WireFace,
) -> Option<Color> {
    // Check immediate neighbors for a non-default, non-cursor bg
    let neighbors = [
        cursor_idx.checked_sub(1),
        if cursor_idx + 1 < segments.len() {
            Some(cursor_idx + 1)
        } else {
            None
        },
    ];

    for idx in neighbors.into_iter().flatten() {
        let face = &segments[idx].2;
        if face.bg != Color::Default && face.bg != default_face.bg && face.bg != cursor_face.bg {
            return Some(face.bg);
        }
    }

    // If cursor itself has a non-default bg (e.g., REVERSE makes bg visible),
    // use it as the selection indicator
    if cursor_face.bg != Color::Default && cursor_face.bg != default_face.bg {
        return Some(cursor_face.bg);
    }

    None
}
