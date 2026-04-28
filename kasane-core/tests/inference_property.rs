//! Property-based tests for inference rule correctness.
//!
//! Verifies invariants of the inference functions in `derived.rs`:
//! - I-1: detect_cursors idempotency, monotonicity, and self-consistency
//! - I-2: derive_cursor_style determinism
//! - R-1: check_cursor_width_consistency self-consistency
//! - R-3: compute_lines_dirty reflexivity

use proptest::prelude::*;

use kasane_core::protocol::{Atom, Attributes, Color, Coord, CursorMode, Face, NamedColor};
use kasane_core::state::derived::{self, CursorCache};

// ---------------------------------------------------------------------------
// Strategies (reuse patterns from dirty_flags_property.rs)
// ---------------------------------------------------------------------------

fn arb_face() -> impl Strategy<Value = Face> {
    (0u8..5, 0u8..5).prop_map(|(fg_idx, bg_idx)| {
        let fg = match fg_idx {
            0 => Color::Default,
            1 => Color::Named(NamedColor::Red),
            2 => Color::Named(NamedColor::Green),
            3 => Color::Named(NamedColor::Blue),
            _ => Color::Named(NamedColor::Yellow),
        };
        let bg = match bg_idx {
            0 => Color::Default,
            1 => Color::Named(NamedColor::Red),
            2 => Color::Named(NamedColor::Green),
            _ => Color::Default,
        };
        Face {
            fg,
            bg,
            ..Face::default()
        }
    })
}

fn arb_line() -> impl Strategy<Value = Vec<Atom>> {
    prop::collection::vec(
        ("[a-z]{1,10}", arb_face()).prop_map(|(contents, face)| Atom::from_face(face, contents)),
        1..5,
    )
}

fn arb_lines() -> impl Strategy<Value = Vec<Vec<Atom>>> {
    prop::collection::vec(arb_line(), 1..30)
}

/// Generate a line that contains exactly one cursor atom (FINAL_FG+REVERSE)
/// at a known position. Returns (line, cursor_column).
fn arb_line_with_cursor() -> impl Strategy<Value = (Vec<Atom>, u32)> {
    // prefix: 0-5 ASCII atoms, then one cursor atom, then 0-3 suffix atoms
    let prefix = prop::collection::vec(
        "[a-z]{1,5}".prop_map(|s: String| Atom::from_face(Face::default(), s)),
        0..5,
    );
    let cursor_text = "[a-z]{1,3}".prop_map(|s: String| {
        Atom::from_face(
            Face {
                attributes: Attributes::FINAL_FG | Attributes::REVERSE,
                ..Face::default()
            },
            s,
        )
    });
    let suffix = prop::collection::vec(
        "[a-z]{1,5}".prop_map(|s: String| Atom::from_face(Face::default(), s)),
        0..3,
    );
    (prefix, cursor_text, suffix).prop_map(|(mut pre, cursor, suf)| {
        // Compute cursor column = sum of prefix atom display widths
        let col: u32 = pre
            .iter()
            .map(|a| a.contents.len() as u32) // ASCII-only, so len == display width
            .sum();
        pre.push(cursor);
        pre.extend(suf);
        (pre, col)
    })
}

// ---------------------------------------------------------------------------
// I-1: detect_cursors properties
// ---------------------------------------------------------------------------

proptest! {
    /// detect_cursors is idempotent: calling twice on same input returns same result.
    #[test]
    fn detect_cursors_idempotent(
        lines in arb_lines(),
        cursor_line in 0i32..30,
        cursor_col in 0i32..50,
    ) {
        let pos = Coord { line: cursor_line, column: cursor_col };
        let r1 = derived::detect_cursors(&lines, pos);
        let r2 = derived::detect_cursors(&lines, pos);
        prop_assert_eq!(r1, r2);
    }

    /// Adding a FINAL_FG+REVERSE atom to a line that already has at least one
    /// such atom increases cursor_count by exactly 1. (The precondition ensures
    /// we stay on the attribute-detection path for both before/after.)
    #[test]
    fn detect_cursors_monotonicity(
        (line, cursor_col) in arb_line_with_cursor(),
        extra_lines in prop::collection::vec(arb_line(), 0..5),
    ) {
        let mut lines = vec![line];
        lines.extend(extra_lines);
        let pos = Coord { line: 0, column: cursor_col as i32 };

        let (count_before, _) = derived::detect_cursors(&lines, pos);

        // Add another cursor atom at end of last line
        let last = lines.len() - 1;
        lines[last].push(Atom::from_face(
            Face {
                attributes: Attributes::FINAL_FG | Attributes::REVERSE,
                ..Face::default()
            },
            "x",
        ));

        let (count_after, _) = derived::detect_cursors(&lines, pos);
        prop_assert_eq!(count_after, count_before + 1);
    }

    /// I-1 self-consistency: for lines with properly placed cursor atoms,
    /// check_primary_cursor_in_set returns true.
    #[test]
    fn detect_cursors_primary_in_set(
        (line, cursor_col) in arb_line_with_cursor(),
    ) {
        let lines = vec![line];
        let primary = Coord { line: 0, column: cursor_col as i32 };
        let (count, secondaries) = derived::detect_cursors(&lines, primary);
        prop_assert!(
            derived::check_primary_cursor_in_set(count, &secondaries, primary),
            "I-1 violated: count={count}, secondaries={}, primary={primary:?}",
            secondaries.len(),
        );
    }
}

// ---------------------------------------------------------------------------
// I-1: detect_cursors_incremental ≡ detect_cursors (all-dirty)
// ---------------------------------------------------------------------------

proptest! {
    /// detect_cursors_incremental with all-true dirty flags produces identical
    /// results to detect_cursors for arbitrary inputs.
    #[test]
    fn detect_cursors_incremental_matches_full(
        lines in arb_lines(),
        cursor_line in 0i32..30,
        cursor_col in 0i32..50,
    ) {
        let pos = Coord { line: cursor_line, column: cursor_col };
        let all_dirty = vec![true; lines.len()];
        let mut cache = CursorCache::default();

        let (inc_count, inc_sec) =
            derived::detect_cursors_incremental(&lines, pos, &all_dirty, &mut cache);
        let (full_count, full_sec) = derived::detect_cursors(&lines, pos);

        prop_assert_eq!(inc_count, full_count);
        prop_assert_eq!(inc_sec, full_sec);
    }
}

// ---------------------------------------------------------------------------
// R-1: check_cursor_width_consistency properties
// ---------------------------------------------------------------------------

proptest! {
    /// R-1 self-consistency: cursor placed at an atom_display_width-computed
    /// position passes check_cursor_width_consistency.
    #[test]
    fn width_consistency_at_computed_position(
        (line, cursor_col) in arb_line_with_cursor(),
    ) {
        let lines = vec![line];
        let pos = Coord { line: 0, column: cursor_col as i32 };
        prop_assert!(
            derived::check_cursor_width_consistency(&lines, pos).is_none(),
            "R-1: unexpected divergence at computed position {cursor_col}",
        );
    }
}

// ---------------------------------------------------------------------------
// I-2: derive_cursor_style properties
// ---------------------------------------------------------------------------

proptest! {
    /// derive_cursor_style is deterministic: same inputs → same output.
    #[test]
    fn cursor_style_deterministic(
        focused in prop::bool::ANY,
        mode in prop_oneof![Just(CursorMode::Buffer), Just(CursorMode::Prompt)],
        mode_line in arb_line(),
    ) {
        let opts = std::collections::HashMap::new();
        let r1 = derived::derive_cursor_style(&opts, focused, mode, &mode_line);
        let r2 = derived::derive_cursor_style(&opts, focused, mode, &mode_line);
        prop_assert_eq!(r1, r2);
    }
}

// ---------------------------------------------------------------------------
// R-3: compute_lines_dirty properties
// ---------------------------------------------------------------------------

proptest! {
    /// Reflexivity: compute_lines_dirty(lines, lines, face, face, face, face) → all false.
    #[test]
    fn lines_dirty_reflexive(
        lines in arb_lines(),
        face in arb_face(),
    ) {
        let dirty = derived::compute_lines_dirty(&lines, &lines, &face, &face, &face, &face);
        for (i, &d) in dirty.iter().enumerate() {
            prop_assert!(!d, "line {i} marked dirty in reflexive comparison");
        }
    }
}
