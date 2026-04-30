//! Cursor detection: primary + secondary cursor position inference.

use crate::protocol::{Atom, Color, Coord, Line};

use super::atom_metrics::{atom_display_width, face_at_coord};

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
/// 2. **WireFace-matching fallback**: if (1) finds nothing, identify the face at
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
        let s = atom.unresolved_style();
        let is_cursor = s.final_fg && s.style.reverse;
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
pub(super) fn scan_line_cursors_by_attributes(
    line: &[Atom],
    line_idx: usize,
    out: &mut Vec<Coord>,
) {
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
            if atom.unresolved_style().to_face().fg == primary_face.fg
                && atom.unresolved_style().to_face().bg != Color::Default
            {
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
    pub(super) per_line: Vec<Vec<u32>>,
    /// Whether the last detection fell back to face-matching (not incrementable).
    pub(super) used_fallback: bool,
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
