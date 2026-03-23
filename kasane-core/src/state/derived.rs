//! Pure functions for derived state computation.
//!
//! These functions extract deterministic computations from `apply.rs` into
//! standalone, testable pure functions. They form the Layer 2 boundary
//! for Salsa tracked function integration.
//!
//! # Inference Catalog
//!
//! Kasane infers semantic information from Kakoune's display-only JSON-RPC
//! protocol. Each inference rule is documented with its assumptions, failure
//! modes, and severity rating.
//!
//! | ID  | Function                     | Assumption                                              | Severity    | Cross-validated | Proptest |
//! |-----|------------------------------|---------------------------------------------------------|-------------|-----------------|----------|
//! | I-1 | `detect_cursors`             | Cursor atoms have `FINAL_FG+REVERSE` or matching fg     | Degraded    | Yes (Phase C)   | Yes      |
//! | I-2 | `derive_cursor_style`        | Mode line contains "insert"/"replace"/other             | Cosmetic    | No              | Yes      |
//! | I-3 | `derive_cursor_mode`         | `content_cursor_pos >= 0` means prompt mode             | Degraded    | No              | Yes      |
//! | I-4 | `split_single_item` (menu)   | Docstring atoms have non-Default fg after padding       | Cosmetic    | No              | No       |
//! | I-6 | `make_secondary_cursor_face` | Cursor face uses `REVERSE` for visual highlight         | Cosmetic    | No              | No       |
//! | I-7 | `detect_selections`          | Selection atoms have non-default bg adjacent to cursor  | Degraded    | No              | No       |
//! | R-1 | `check_cursor_width_consistency` | `atom_display_width` matches Kakoune's width calc    | Catastrophic| Yes (Phase B)   | Yes      |
//! | R-3 | `compute_lines_dirty`        | Line equality implies visual equality                   | Degraded    | No              | Yes      |

use unicode_segmentation::UnicodeSegmentation;
use unicode_width::UnicodeWidthStr;

use crate::protocol::{Atom, Attributes, Color, Coord, CursorMode, Face, Line};
use crate::render::CursorStyle;

/// Parsed editor mode derived from cursor mode and status mode line.
///
/// Provides a higher-level abstraction than `CursorMode` (which only distinguishes
/// Buffer vs Prompt). `EditorMode` further classifies Buffer mode into Normal,
/// Insert, and Replace based on the mode line heuristic (I-2).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum EditorMode {
    #[default]
    Normal,
    Insert,
    Replace,
    Prompt,
    Unknown,
}

/// Derive the editor mode from cursor mode and status mode line.
///
/// Uses the same heuristic as `derive_cursor_style()` (I-2) but returns
/// a semantic mode enum instead of a cursor shape.
///
/// - `CursorMode::Prompt` → `EditorMode::Prompt`
/// - mode_line contains "insert" → `EditorMode::Insert`
/// - mode_line contains "replace" → `EditorMode::Replace`
/// - otherwise → `EditorMode::Normal`
pub fn derive_editor_mode(cursor_mode: CursorMode, status_mode_line: &Line) -> EditorMode {
    if cursor_mode == CursorMode::Prompt {
        return EditorMode::Prompt;
    }
    status_mode_line
        .iter()
        .find_map(|atom| match atom.contents.as_str() {
            "insert" => Some(EditorMode::Insert),
            "replace" => Some(EditorMode::Replace),
            _ => None,
        })
        .unwrap_or(EditorMode::Normal)
}

/// Detect all cursor positions (primary + secondary) from draw atoms.
///
/// # Inference Rule: I-1
/// **Assumption**: Cursor atoms have `FINAL_FG+REVERSE` attributes (default theme)
/// or share the same fg color as the primary cursor face (third-party themes).
/// **Failure mode**: If the theme uses neither pattern, secondary cursors are missed
/// and cursor_count is 1 regardless of actual selections.
/// **Severity**: Degraded (multi-cursor features work incorrectly)
///
/// Returns `(cursor_count, secondary_cursors)` where `secondary_cursors`
/// excludes the primary cursor at `primary_cursor_pos`.
///
/// Uses two strategies:
/// 1. **Attribute heuristic**: scan for `FINAL_FG + REVERSE` (Kakoune's default
///    PrimaryCursor face uses `+rfg`).
/// 2. **Face-matching fallback**: if (1) finds nothing, identify the face at
///    `primary_cursor_pos` and scan for atoms with the same foreground color
///    (covers third-party themes that omit `+rfg` from cursor faces).
pub fn detect_cursors(lines: &[Line], primary_cursor_pos: Coord) -> (usize, Vec<Coord>) {
    let all_cursors = detect_cursors_by_attributes(lines);
    if !all_cursors.is_empty() {
        let cursor_count = all_cursors.len();
        let secondary_cursors: Vec<Coord> = all_cursors
            .into_iter()
            .filter(|c| *c != primary_cursor_pos)
            .collect();
        debug_assert!(
            check_primary_cursor_in_set(cursor_count, &secondary_cursors, primary_cursor_pos),
            "I-1: primary cursor not in detected set (count={cursor_count}, secondaries={}, primary={primary_cursor_pos:?})",
            secondary_cursors.len(),
        );
        return (cursor_count, secondary_cursors);
    }

    // Fallback: use the face at primary_cursor_pos as a template to find
    // secondary cursors.  Third-party themes typically set PrimaryCursor and
    // SecondaryCursor with the same fg but different bg; matching on fg
    // catches both.
    let all_cursors = detect_cursors_by_face(lines, primary_cursor_pos);
    let cursor_count = all_cursors.len();
    let secondary_cursors = all_cursors
        .into_iter()
        .filter(|c| *c != primary_cursor_pos)
        .collect();
    (cursor_count, secondary_cursors)
}

/// Scan atoms for the traditional `FINAL_FG + REVERSE` attribute pattern.
fn detect_cursors_by_attributes(lines: &[Line]) -> Vec<Coord> {
    let mut cursors = Vec::new();
    for (line_idx, line) in lines.iter().enumerate() {
        scan_line_cursors_by_attributes(line, line_idx, &mut cursors);
    }
    cursors
}

/// Scan a single line's atoms, pushing cursor column indices into `out`.
///
/// This is the shared attribute-check primitive used by both
/// `scan_line_cursors_by_attributes` (full scan, producing `Coord`) and
/// `detect_cursors_incremental` (dirty-line scan, producing column-only `u32`).
fn scan_line_cursor_columns(line: &[Atom], out: &mut Vec<u32>) {
    let mut col: u32 = 0;
    for atom in line.iter() {
        let is_cursor = atom.face.attributes.contains(Attributes::FINAL_FG)
            && atom.face.attributes.contains(Attributes::REVERSE);
        if is_cursor {
            out.push(col);
        }
        col += atom_display_width(atom);
    }
}

/// Scan a single line for cursor atoms (FINAL_FG + REVERSE pattern).
///
/// Appends cursor positions to `out`. This is the per-line primitive used by
/// `detect_cursors_by_attributes` (full scan).
fn scan_line_cursors_by_attributes(line: &[Atom], line_idx: usize, out: &mut Vec<Coord>) {
    let mut cols = Vec::new();
    scan_line_cursor_columns(line, &mut cols);
    for col in cols {
        out.push(Coord {
            line: line_idx as i32,
            column: col as i32,
        });
    }
}

/// Find the face at a given coordinate, then scan for atoms with matching fg.
fn detect_cursors_by_face(lines: &[Line], primary_pos: Coord) -> Vec<Coord> {
    let primary_face = match face_at_coord(lines, primary_pos) {
        Some(f) => f,
        None => return vec![],
    };

    // Only use fallback if the primary cursor has a distinctive face
    // (explicit fg — not Default).
    if primary_face.fg == Color::Default {
        return vec![primary_pos];
    }

    let mut cursors = Vec::new();
    for (line_idx, line) in lines.iter().enumerate() {
        let mut col: u32 = 0;
        for atom in line.iter() {
            if atom.face.fg == primary_face.fg && atom.face.bg != Color::Default {
                cursors.push(Coord {
                    line: line_idx as i32,
                    column: col as i32,
                });
            }
            col += atom_display_width(atom);
        }
    }

    // If matching found too many positions (>64), the heuristic is unreliable;
    // fall back to just the primary cursor.
    if cursors.len() > 64 {
        return vec![primary_pos];
    }

    if cursors.is_empty() {
        vec![primary_pos]
    } else {
        cursors
    }
}

// ---------------------------------------------------------------------------
// CursorCache: incremental cursor detection
// ---------------------------------------------------------------------------

/// Per-line cursor position cache for incremental `detect_cursors`.
///
/// Stores the attribute-scan results per line so that only dirty lines need
/// re-scanning on each frame.
#[derive(Debug, Clone, Default)]
pub struct CursorCache {
    /// Column positions of cursor atoms per line (attribute scan results).
    per_line: Vec<Vec<u32>>,
    /// Whether the last detection fell back to face-matching (not incrementable).
    used_fallback: bool,
}

/// Incremental cursor detection: re-scan only dirty lines, reuse cached results
/// for clean lines.
///
/// Falls back to a full scan when the cache is invalid (line count changed,
/// face-matching fallback was used, or no dirty info is available).
///
/// Returns `(cursor_count, secondary_cursors)` — same contract as `detect_cursors`.
pub fn detect_cursors_incremental(
    lines: &[Line],
    primary_cursor_pos: Coord,
    lines_dirty: &[bool],
    cache: &mut CursorCache,
) -> (usize, Vec<Coord>) {
    let needs_full_scan = cache.per_line.len() != lines.len()
        || cache.used_fallback
        || lines_dirty.is_empty()
        || lines_dirty.len() != lines.len();

    if needs_full_scan {
        // Full scan: rebuild entire cache
        cache.per_line.clear();
        cache.per_line.resize(lines.len(), Vec::new());
        cache.used_fallback = false;

        for (i, line) in lines.iter().enumerate() {
            cache.per_line[i].clear();
            scan_line_cursor_columns(line, &mut cache.per_line[i]);
        }
    } else {
        // Incremental: only re-scan dirty lines
        for (i, &dirty) in lines_dirty.iter().enumerate() {
            if dirty {
                cache.per_line[i].clear();
                scan_line_cursor_columns(&lines[i], &mut cache.per_line[i]);
            }
        }
    }

    // Reconstruct all cursor positions from cache
    let mut all_cursors = Vec::new();
    for (line_idx, cols) in cache.per_line.iter().enumerate() {
        for &col in cols {
            all_cursors.push(Coord {
                line: line_idx as i32,
                column: col as i32,
            });
        }
    }

    if !all_cursors.is_empty() {
        let cursor_count = all_cursors.len();
        let secondary_cursors: Vec<Coord> = all_cursors
            .into_iter()
            .filter(|c| *c != primary_cursor_pos)
            .collect();
        return (cursor_count, secondary_cursors);
    }

    // Attribute scan found nothing — fall back to face-matching (not incrementable)
    cache.used_fallback = true;
    let all_cursors = detect_cursors_by_face(lines, primary_cursor_pos);
    let cursor_count = all_cursors.len();
    let secondary_cursors = all_cursors
        .into_iter()
        .filter(|c| *c != primary_cursor_pos)
        .collect();
    (cursor_count, secondary_cursors)
}

/// Look up the face of the atom at a given (line, column) coordinate.
fn face_at_coord(lines: &[Line], pos: Coord) -> Option<Face> {
    let line = lines.get(pos.line as usize)?;
    let target_col = pos.column as u32;
    let mut col: u32 = 0;
    for atom in line.iter() {
        let width = atom_display_width(atom);
        if col <= target_col && target_col < col + width.max(1) {
            return Some(atom.face);
        }
        col += width;
    }
    None
}

/// Compute the display width of an atom's contents.
pub(crate) fn atom_display_width(atom: &Atom) -> u32 {
    let mut width: u32 = 0;
    for grapheme in atom.contents.as_str().graphemes(true) {
        if grapheme.starts_with(|c: char| c.is_control()) {
            continue;
        }
        width += UnicodeWidthStr::width(grapheme) as u32;
    }
    debug_assert!(
        width < 10_000,
        "atom_display_width: unreasonable width {width} for atom {:?}",
        atom.contents.as_str(),
    );
    width
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
    old_default_face: &Face,
    new_default_face: &Face,
    old_padding_face: &Face,
    new_padding_face: &Face,
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

/// Derive cursor mode from the status content cursor position.
///
/// # Inference Rule: I-3
/// **Assumption**: `content_cursor_pos >= 0` means Kakoune is in prompt mode
/// (command, search, etc.), while `< 0` means buffer (normal editing) mode.
/// **Failure mode**: If Kakoune changes the sign convention, cursor mode is
/// inverted — prompt commands would be sent to the buffer and vice versa.
/// **Severity**: Degraded (input routing broken)
///
/// `content_cursor_pos >= 0` means prompt mode (`:`, `/`, etc.),
/// `< 0` means buffer (normal editing) mode.
pub fn derive_cursor_mode(content_cursor_pos: i32) -> CursorMode {
    if content_cursor_pos >= 0 {
        CursorMode::Prompt
    } else {
        CursorMode::Buffer
    }
}

/// Concatenate status prompt and content atoms into a single status line.
pub fn build_status_line(prompt: &[Atom], content: &[Atom]) -> Line {
    let mut combined = prompt.to_vec();
    combined.extend_from_slice(content);
    combined
}

/// Derive cursor style from state fields (without plugin override).
///
/// # Inference Rule: I-2
/// **Assumption**: The status mode line contains literal strings "insert" or
/// "replace" to indicate Kakoune's editing mode. Other mode strings (including
/// custom modes) default to Block.
/// **Failure mode**: If Kakoune localizes mode names or changes strings, the
/// wrong cursor shape is displayed.
/// **Severity**: Cosmetic (cursor shape mismatch only)
///
/// Priority:
/// 1. Explicit `kasane_cursor_style` ui_option
/// 2. Unfocused → Outline
/// 3. Prompt mode → Bar
/// 4. Mode line heuristic (`"insert"` → Bar, `"replace"` → Underline)
/// 5. Default → Block
pub fn derive_cursor_style(
    ui_options: &std::collections::HashMap<String, String>,
    focused: bool,
    cursor_mode: CursorMode,
    status_mode_line: &Line,
) -> CursorStyle {
    if let Some(style) = ui_options.get("kasane_cursor_style") {
        return match style.as_str() {
            "bar" => CursorStyle::Bar,
            "underline" => CursorStyle::Underline,
            _ => CursorStyle::Block,
        };
    }
    if !focused {
        return CursorStyle::Outline;
    }
    if cursor_mode == CursorMode::Prompt {
        return CursorStyle::Bar;
    }
    let mode = status_mode_line
        .iter()
        .find_map(|atom| match atom.contents.as_str() {
            "insert" => Some(CursorStyle::Bar),
            "replace" => Some(CursorStyle::Underline),
            _ => None,
        });
    mode.unwrap_or(CursorStyle::Block)
}

// ---------------------------------------------------------------------------
// R-1: Character width divergence detection
// ---------------------------------------------------------------------------

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

/// Compute the total display width of a line by summing atom widths.
pub(crate) fn line_atom_display_width(line: &[Atom]) -> u32 {
    line.iter().map(atom_display_width).sum()
}

// ---------------------------------------------------------------------------
// I-1: Primary cursor in detected set (self-consistency check)
// ---------------------------------------------------------------------------

/// Check that the primary cursor is accounted for in the detected cursor set.
///
/// After `detect_cursors` filters out the primary cursor from the full set,
/// the invariant is either:
/// - `cursor_count == secondary_cursors.len() + 1` (primary was in the set and was filtered out)
/// - `cursor_count == secondary_cursors.len()` (primary position didn't match any detected cursor,
///   which is valid when the primary cursor face differs from the detection heuristic)
/// - `cursor_count == 0` (no cursors detected)
///
/// Returns `true` if consistent, `false` if the counts are impossible.
pub fn check_primary_cursor_in_set(
    cursor_count: usize,
    secondary_cursors: &[Coord],
    _primary_pos: Coord,
) -> bool {
    cursor_count == 0
        || cursor_count == secondary_cursors.len() + 1
        || cursor_count == secondary_cursors.len()
}

// ---------------------------------------------------------------------------
// I-7: Selection range detection
// ---------------------------------------------------------------------------

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
    default_face: &Face,
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
    default_face: &Face,
    is_primary: bool,
) -> Option<Selection> {
    let line_idx = cursor_pos.line as usize;
    let line = lines.get(line_idx)?;
    let cursor_col = cursor_pos.column as u32;

    // Build a column map: (start_col, end_col, face) for each atom
    let mut segments: Vec<(u32, u32, Face)> = Vec::new();
    let mut col: u32 = 0;
    for atom in line.iter() {
        let width = atom_display_width(atom);
        if width > 0 {
            segments.push((col, col + width, atom.face));
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
    segments: &[(u32, u32, Face)],
    cursor_idx: usize,
    cursor_face: &Face,
    default_face: &Face,
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::protocol::{Atom, Color, NamedColor};

    fn make_atom(text: &str) -> Atom {
        Atom {
            face: Face::default(),
            contents: text.into(),
        }
    }

    fn make_cursor_atom(text: &str) -> Atom {
        Atom {
            face: Face {
                attributes: Attributes::FINAL_FG | Attributes::REVERSE,
                ..Face::default()
            },
            contents: text.into(),
        }
    }

    // --- detect_cursors tests ---

    #[test]
    fn detect_cursors_empty_buffer() {
        let (count, secondary) = detect_cursors(&[], Coord::default());
        assert_eq!(count, 0);
        assert!(secondary.is_empty());
    }

    #[test]
    fn detect_cursors_single_primary() {
        let lines = vec![vec![
            make_atom("hel"),
            make_cursor_atom("l"),
            make_atom("o"),
        ]];
        let primary = Coord { line: 0, column: 3 };
        let (count, secondary) = detect_cursors(&lines, primary);
        assert_eq!(count, 1);
        assert!(secondary.is_empty());
    }

    #[test]
    fn detect_cursors_with_secondary() {
        let lines = vec![
            vec![make_cursor_atom("h"), make_atom("ello")],
            vec![make_atom("wor"), make_cursor_atom("l"), make_atom("d")],
        ];
        let primary = Coord { line: 0, column: 0 };
        let (count, secondary) = detect_cursors(&lines, primary);
        assert_eq!(count, 2);
        assert_eq!(secondary.len(), 1);
        assert_eq!(secondary[0], Coord { line: 1, column: 3 });
    }

    #[test]
    fn detect_cursors_cjk_width() {
        // CJK character "漢" is 2 cells wide
        let lines = vec![vec![make_atom("漢"), make_cursor_atom("x")]];
        let primary = Coord { line: 0, column: 2 };
        let (count, secondary) = detect_cursors(&lines, primary);
        assert_eq!(count, 1);
        assert!(secondary.is_empty());
    }

    // --- detect_cursors face-matching fallback tests ---

    /// Helper: create an atom with an explicit fg+bg face (no REVERSE/FINAL_FG),
    /// mimicking third-party themes like anhsirk0/kakoune-themes.
    fn make_themed_cursor_atom(text: &str, fg: Color, bg: Color) -> Atom {
        Atom {
            face: Face {
                fg,
                bg,
                ..Face::default()
            },
            contents: text.into(),
        }
    }

    #[test]
    fn detect_cursors_fallback_single_primary() {
        // Theme: PrimaryCursor = dark,purple (no +rfg)
        let dark = Color::Rgb {
            r: 0x1e,
            g: 0x21,
            b: 0x27,
        };
        let purple = Color::Rgb {
            r: 0xc6,
            g: 0x78,
            b: 0xdd,
        };
        let lines = vec![vec![
            make_atom("hel"),
            make_themed_cursor_atom("l", dark, purple),
            make_atom("o"),
        ]];
        let primary = Coord { line: 0, column: 3 };
        let (count, secondary) = detect_cursors(&lines, primary);
        assert_eq!(count, 1);
        assert!(secondary.is_empty());
    }

    #[test]
    fn detect_cursors_fallback_with_secondary() {
        // PrimaryCursor = dark,purple; SecondaryCursor = dark,blue
        let dark = Color::Rgb {
            r: 0x1e,
            g: 0x21,
            b: 0x27,
        };
        let purple = Color::Rgb {
            r: 0xc6,
            g: 0x78,
            b: 0xdd,
        };
        let blue = Color::Rgb {
            r: 0x61,
            g: 0xaf,
            b: 0xef,
        };
        let lines = vec![
            vec![
                make_themed_cursor_atom("h", dark, purple),
                make_atom("ello"),
            ],
            vec![
                make_atom("wor"),
                make_themed_cursor_atom("l", dark, blue),
                make_atom("d"),
            ],
        ];
        let primary = Coord { line: 0, column: 0 };
        let (count, secondary) = detect_cursors(&lines, primary);
        assert_eq!(count, 2);
        assert_eq!(secondary.len(), 1);
        assert_eq!(secondary[0], Coord { line: 1, column: 3 });
    }

    // --- compute_lines_dirty tests ---

    #[test]
    fn lines_dirty_same_content() {
        let lines = vec![vec![make_atom("hello")]];
        let face = Face::default();
        let dirty = compute_lines_dirty(&lines, &lines, &face, &face, &face, &face);
        assert_eq!(dirty, vec![false]);
    }

    #[test]
    fn lines_dirty_changed_content() {
        let old = vec![vec![make_atom("hello")]];
        let new = vec![vec![make_atom("world")]];
        let face = Face::default();
        let dirty = compute_lines_dirty(&old, &new, &face, &face, &face, &face);
        assert_eq!(dirty, vec![true]);
    }

    #[test]
    fn lines_dirty_length_change_marks_all() {
        let old = vec![vec![make_atom("a")]];
        let new = vec![vec![make_atom("a")], vec![make_atom("b")]];
        let face = Face::default();
        let dirty = compute_lines_dirty(&old, &new, &face, &face, &face, &face);
        assert_eq!(dirty, vec![true, true]);
    }

    #[test]
    fn lines_dirty_face_change_marks_all() {
        let lines = vec![vec![make_atom("hello")]];
        let old_face = Face::default();
        let new_face = Face {
            fg: Color::Named(NamedColor::Red),
            ..Face::default()
        };
        let dirty = compute_lines_dirty(&lines, &lines, &old_face, &new_face, &old_face, &old_face);
        assert_eq!(dirty, vec![true]);
    }

    // --- derive_cursor_mode tests ---

    #[test]
    fn cursor_mode_prompt() {
        assert_eq!(derive_cursor_mode(0), CursorMode::Prompt);
        assert_eq!(derive_cursor_mode(5), CursorMode::Prompt);
    }

    #[test]
    fn cursor_mode_buffer() {
        assert_eq!(derive_cursor_mode(-1), CursorMode::Buffer);
        assert_eq!(derive_cursor_mode(-100), CursorMode::Buffer);
    }

    // --- build_status_line tests ---

    #[test]
    fn build_status_line_combines() {
        let prompt = vec![make_atom(":")];
        let content = vec![make_atom("edit foo")];
        let result = build_status_line(&prompt, &content);
        assert_eq!(result.len(), 2);
        assert_eq!(result[0].contents.as_str(), ":");
        assert_eq!(result[1].contents.as_str(), "edit foo");
    }

    #[test]
    fn build_status_line_empty_prompt() {
        let content = vec![make_atom("normal")];
        let result = build_status_line(&[], &content);
        assert_eq!(result.len(), 1);
    }

    // --- derive_editor_mode tests ---

    #[test]
    fn editor_mode_normal() {
        let mode_line = vec![make_atom("normal")];
        assert_eq!(
            derive_editor_mode(CursorMode::Buffer, &mode_line),
            EditorMode::Normal
        );
    }

    #[test]
    fn editor_mode_insert() {
        let mode_line = vec![make_atom("insert")];
        assert_eq!(
            derive_editor_mode(CursorMode::Buffer, &mode_line),
            EditorMode::Insert
        );
    }

    #[test]
    fn editor_mode_replace() {
        let mode_line = vec![make_atom("replace")];
        assert_eq!(
            derive_editor_mode(CursorMode::Buffer, &mode_line),
            EditorMode::Replace
        );
    }

    #[test]
    fn editor_mode_prompt() {
        let mode_line = vec![make_atom("insert")];
        // Prompt takes priority over mode_line content
        assert_eq!(
            derive_editor_mode(CursorMode::Prompt, &mode_line),
            EditorMode::Prompt
        );
    }

    #[test]
    fn editor_mode_empty_mode_line() {
        assert_eq!(
            derive_editor_mode(CursorMode::Buffer, &vec![]),
            EditorMode::Normal
        );
    }

    // --- derive_cursor_style tests ---

    #[test]
    fn cursor_style_ui_option_override() {
        let mut opts = std::collections::HashMap::new();
        opts.insert("kasane_cursor_style".to_string(), "bar".to_string());
        assert_eq!(
            derive_cursor_style(&opts, true, CursorMode::Buffer, &vec![]),
            CursorStyle::Bar
        );
    }

    #[test]
    fn cursor_style_unfocused() {
        let opts = std::collections::HashMap::new();
        assert_eq!(
            derive_cursor_style(&opts, false, CursorMode::Buffer, &vec![]),
            CursorStyle::Outline
        );
    }

    #[test]
    fn cursor_style_prompt_mode() {
        let opts = std::collections::HashMap::new();
        assert_eq!(
            derive_cursor_style(&opts, true, CursorMode::Prompt, &vec![]),
            CursorStyle::Bar
        );
    }

    #[test]
    fn cursor_style_insert_mode() {
        let opts = std::collections::HashMap::new();
        let mode_line = vec![make_atom("insert")];
        assert_eq!(
            derive_cursor_style(&opts, true, CursorMode::Buffer, &mode_line),
            CursorStyle::Bar
        );
    }

    #[test]
    fn cursor_style_replace_mode() {
        let opts = std::collections::HashMap::new();
        let mode_line = vec![make_atom("replace")];
        assert_eq!(
            derive_cursor_style(&opts, true, CursorMode::Buffer, &mode_line),
            CursorStyle::Underline
        );
    }

    #[test]
    fn cursor_style_normal_mode() {
        let opts = std::collections::HashMap::new();
        let mode_line = vec![make_atom("normal")];
        assert_eq!(
            derive_cursor_style(&opts, true, CursorMode::Buffer, &mode_line),
            CursorStyle::Block
        );
    }

    // --- R-1: check_cursor_width_consistency tests ---

    #[test]
    fn width_consistency_ascii() {
        let lines = vec![vec![
            make_atom("hel"),
            make_cursor_atom("l"),
            make_atom("o"),
        ]];
        let cursor_pos = Coord { line: 0, column: 3 };
        assert_eq!(check_cursor_width_consistency(&lines, cursor_pos), None);
    }

    #[test]
    fn width_consistency_cjk() {
        // "漢" is 2 columns wide, cursor at column 2
        let lines = vec![vec![make_atom("漢"), make_cursor_atom("x")]];
        let cursor_pos = Coord { line: 0, column: 2 };
        assert_eq!(check_cursor_width_consistency(&lines, cursor_pos), None);
    }

    #[test]
    fn width_consistency_divergence_detected() {
        // Cursor claims to be at column 5 but line is only 4 columns wide ("hell")
        let lines = vec![vec![make_atom("hell")]];
        let cursor_pos = Coord { line: 0, column: 5 };
        let result = check_cursor_width_consistency(&lines, cursor_pos);
        assert!(result.is_some());
        let div = result.unwrap();
        assert_eq!(div.protocol_column, 5);
        assert_eq!(div.computed_column, 4);
    }

    // --- I-1: check_primary_cursor_in_set tests ---

    #[test]
    fn primary_in_set_single_cursor() {
        assert!(check_primary_cursor_in_set(
            1,
            &[],
            Coord { line: 0, column: 0 },
        ));
    }

    #[test]
    fn primary_in_set_multi_cursor() {
        let secondaries = vec![Coord { line: 1, column: 3 }];
        assert!(check_primary_cursor_in_set(
            2,
            &secondaries,
            Coord { line: 0, column: 0 },
        ));
    }

    #[test]
    fn primary_in_set_primary_not_detected() {
        // cursor_count=2 and 2 secondaries → primary wasn't in detected set
        // (valid: primary face may differ from detection heuristic)
        let secondaries = vec![Coord { line: 0, column: 0 }, Coord { line: 1, column: 3 }];
        assert!(check_primary_cursor_in_set(
            2,
            &secondaries,
            Coord { line: 2, column: 0 },
        ));
    }

    #[test]
    fn primary_in_set_impossible_count() {
        // cursor_count=1 but 2 secondaries → impossible
        let secondaries = vec![Coord { line: 0, column: 0 }, Coord { line: 1, column: 3 }];
        assert!(!check_primary_cursor_in_set(
            1,
            &secondaries,
            Coord { line: 2, column: 0 },
        ));
    }

    #[test]
    fn primary_in_set_empty_buffer() {
        assert!(check_primary_cursor_in_set(0, &[], Coord::default(),));
    }

    // --- detect_cursors_incremental tests ---

    #[test]
    fn detect_cursors_incremental_matches_full_on_all_dirty() {
        let lines = vec![
            vec![make_cursor_atom("h"), make_atom("ello")],
            vec![make_atom("wor"), make_cursor_atom("l"), make_atom("d")],
        ];
        let primary = Coord { line: 0, column: 0 };
        let all_dirty = vec![true; lines.len()];
        let mut cache = CursorCache::default();

        let (inc_count, inc_sec) =
            detect_cursors_incremental(&lines, primary, &all_dirty, &mut cache);
        let (full_count, full_sec) = detect_cursors(&lines, primary);

        assert_eq!(inc_count, full_count);
        assert_eq!(inc_sec, full_sec);
    }

    #[test]
    fn detect_cursors_incremental_with_partial_dirty() {
        // Initial: cursors on lines 0 and 1
        let lines_v1 = vec![
            vec![make_cursor_atom("h"), make_atom("ello")],
            vec![make_atom("wor"), make_cursor_atom("l"), make_atom("d")],
            vec![make_atom("line3")],
        ];
        let primary = Coord { line: 0, column: 0 };
        let mut cache = CursorCache::default();

        // Warm the cache with a full scan
        let all_dirty = vec![true; lines_v1.len()];
        detect_cursors_incremental(&lines_v1, primary, &all_dirty, &mut cache);

        // Now change only line 1 (move cursor away)
        let lines_v2 = vec![
            vec![make_cursor_atom("h"), make_atom("ello")],
            vec![make_atom("world")],
            vec![make_atom("line3")],
        ];
        let partial_dirty = vec![false, true, false];
        let (count, sec) =
            detect_cursors_incremental(&lines_v2, primary, &partial_dirty, &mut cache);

        // Only line 0 should have a cursor now
        assert_eq!(count, 1);
        assert!(sec.is_empty());

        // Verify matches full scan
        let (full_count, full_sec) = detect_cursors(&lines_v2, primary);
        assert_eq!(count, full_count);
        assert_eq!(sec, full_sec);
    }

    #[test]
    fn detect_cursors_incremental_line_count_change_forces_full_scan() {
        let lines_v1 = vec![
            vec![make_cursor_atom("a"), make_atom("bc")],
            vec![make_atom("def")],
        ];
        let primary = Coord { line: 0, column: 0 };
        let mut cache = CursorCache::default();

        // Warm cache
        let all_dirty = vec![true; lines_v1.len()];
        detect_cursors_incremental(&lines_v1, primary, &all_dirty, &mut cache);
        assert_eq!(cache.per_line.len(), 2);

        // Change to 3 lines — should force full scan
        let lines_v2 = vec![
            vec![make_cursor_atom("a"), make_atom("bc")],
            vec![make_atom("def")],
            vec![make_cursor_atom("g")],
        ];
        let dirty_2 = vec![false, false, true]; // wrong length vs cache
        let (count, sec) = detect_cursors_incremental(&lines_v2, primary, &dirty_2, &mut cache);

        assert_eq!(count, 2); // cursors on line 0 and 2
        assert_eq!(sec.len(), 1);
        assert_eq!(cache.per_line.len(), 3);
    }

    #[test]
    fn detect_cursors_incremental_face_fallback_forces_full_rescan() {
        // Lines with no FINAL_FG+REVERSE — will trigger face fallback
        let dark = Color::Rgb {
            r: 0x1e,
            g: 0x21,
            b: 0x27,
        };
        let purple = Color::Rgb {
            r: 0xc6,
            g: 0x78,
            b: 0xdd,
        };
        let lines = vec![vec![
            make_atom("hel"),
            make_themed_cursor_atom("l", dark, purple),
            make_atom("o"),
        ]];
        let primary = Coord { line: 0, column: 3 };
        let mut cache = CursorCache::default();

        let all_dirty = vec![true];
        let (count, _sec) = detect_cursors_incremental(&lines, primary, &all_dirty, &mut cache);

        // Face fallback should be used
        assert!(cache.used_fallback);
        assert_eq!(count, 1);

        // Next call should force full scan since used_fallback is set
        let (count2, _sec2) = detect_cursors_incremental(&lines, primary, &[false], &mut cache);
        assert_eq!(count2, 1);
    }

    #[test]
    fn scan_line_cursors_by_attributes_per_line() {
        // "hel" (3) + cursor "l" (1) + "o" (1) + cursor "!" (1) = columns 0..6
        let line = vec![
            make_atom("hel"),
            make_cursor_atom("l"),
            make_atom("o"),
            make_cursor_atom("!"),
        ];
        let mut out = Vec::new();
        scan_line_cursors_by_attributes(&line, 5, &mut out);
        assert_eq!(out.len(), 2);
        assert_eq!(out[0], Coord { line: 5, column: 3 });
        assert_eq!(out[1], Coord { line: 5, column: 5 }); // 3+1+1 = 5
    }

    // --- detect_selections tests ---

    fn make_selection_atom(text: &str) -> Atom {
        Atom {
            face: Face {
                bg: Color::Named(NamedColor::Blue),
                ..Face::default()
            },
            contents: text.into(),
        }
    }

    #[test]
    fn detect_selections_single_char_cursor() {
        // No selection highlight around cursor → returns selection with anchor == cursor
        let lines = vec![vec![
            make_atom("hel"),
            make_cursor_atom("l"),
            make_atom("o"),
        ]];
        let cursor = Coord { line: 0, column: 3 };
        let sels = detect_selections(&lines, cursor, &[], &Face::default());
        // Cursor has REVERSE+FINAL_FG bg which is Default (no bg set) → detection
        // depends on whether cursor face bg is non-default. Since default cursor
        // atom has bg=Default, no selection bg is found.
        assert!(sels.is_empty() || sels[0].anchor == sels[0].cursor);
    }

    #[test]
    fn detect_selections_with_selection_face() {
        // "he" + selection "ll" + cursor "o" + selection " w" + "orld"
        // Selection face: blue bg. Cursor face: REVERSE+FINAL_FG.
        let lines = vec![vec![
            make_atom("he"),
            make_selection_atom("ll"),
            make_cursor_atom("o"),
            make_selection_atom(" w"),
            make_atom("orld"),
        ]];
        let cursor = Coord { line: 0, column: 4 }; // "he"=2, "ll"=2, cursor at 4
        let sels = detect_selections(&lines, cursor, &[], &Face::default());
        assert_eq!(sels.len(), 1);
        assert!(sels[0].is_primary);
        // Selection should span from "ll" start (col 2) to " w" end (col 6)
        assert_eq!(sels[0].anchor.column, 2);
        assert_eq!(sels[0].cursor.column, 6); // "o"=1 + " w"=2 → col 4+1+2-1=6
    }

    #[test]
    fn detect_selections_empty_lines() {
        let sels = detect_selections(&[], Coord::default(), &[], &Face::default());
        assert!(sels.is_empty());
    }

    #[test]
    fn detect_selections_too_many_cursors() {
        let lines = vec![vec![make_atom("text")]];
        let cursor = Coord { line: 0, column: 0 };
        // 65 secondary cursors → exceeds safety valve
        let secondaries: Vec<Coord> = (0..65).map(|i| Coord { line: 0, column: i }).collect();
        let sels = detect_selections(&lines, cursor, &secondaries, &Face::default());
        assert!(sels.is_empty());
    }
}
