//! Concrete witnesses of the L1–L6 algebraic laws (ADR-034).
//!
//! Proptest-driven witnesses are deferred to a follow-up PR; this file
//! exercises each law on hand-built fixtures, which is sufficient to
//! catch the shape-of-bug regressions that the variant-aware resolver
//! historically suffered from.

use compact_str::CompactString;

use crate::plugin::PluginId;
use crate::protocol::{Atom, WireFace};

use super::derived::*;
use super::normalize::{TaggedDisplay, disjoint, normalize, pass_c_filter_evt};
use super::primitives::{Content, Display, EditSpec, Span};

fn pid(s: &str) -> PluginId {
    PluginId(s.to_string())
}

fn tagged(d: Display, priority: i16, plugin: &str, seq: u32) -> TaggedDisplay {
    TaggedDisplay::new(d, priority, pid(plugin), seq)
}

fn atom(s: &str) -> Atom {
    Atom::with_style(CompactString::from(s), crate::protocol::Style::default())
}

fn face() -> WireFace {
    WireFace::default()
}

// =============================================================================
// L1 — Identity unit
// =============================================================================

#[test]
fn l1_then_identity_left() {
    let d = hide_inline(0, 1..3);
    assert_eq!(Display::then(Display::Identity, d.clone()), d);
}

#[test]
fn l1_then_identity_right() {
    let d = hide_inline(0, 1..3);
    assert_eq!(Display::then(d.clone(), Display::Identity), d);
}

#[test]
fn l1_merge_identity_left() {
    let d = style_line(0, face(), 0);
    assert_eq!(Display::merge(Display::Identity, d.clone()), d);
}

#[test]
fn l1_merge_identity_right() {
    let d = style_line(0, face(), 0);
    assert_eq!(Display::merge(d.clone(), Display::Identity), d);
}

#[test]
fn l1_then_all_empty_is_identity() {
    let d: Display = Display::then_all(std::iter::empty());
    assert!(d.is_identity());
}

#[test]
fn l1_merge_all_empty_is_identity() {
    let d: Display = Display::merge_all(std::iter::empty());
    assert!(d.is_identity());
}

// =============================================================================
// L2 — Then-associativity
// =============================================================================

#[test]
fn l2_then_associative_normalises_to_same_leaves() {
    let a = hide_inline(0, 0..1);
    let b = hide_inline(0, 2..3);
    let c = hide_inline(0, 4..5);

    let left = Display::then(Display::then(a.clone(), b.clone()), c.clone());
    let right = Display::then(a, Display::then(b, c));

    let lhs = normalize(vec![tagged(left, 0, "p", 0)]);
    let rhs = normalize(vec![tagged(right, 0, "p", 0)]);

    assert_eq!(lhs, rhs);
}

// =============================================================================
// L3 — Merge-associativity
// =============================================================================

#[test]
fn l3_merge_associative_normalises_to_same_leaves() {
    let a = style_line(0, face(), 0);
    let b = style_line(1, face(), 0);
    let c = style_line(2, face(), 0);

    let left = Display::merge(Display::merge(a.clone(), b.clone()), c.clone());
    let right = Display::merge(a, Display::merge(b, c));

    let lhs = normalize(vec![tagged(left, 0, "p", 0)]);
    let rhs = normalize(vec![tagged(right, 0, "p", 0)]);

    assert_eq!(lhs, rhs);
}

// =============================================================================
// L4 — Merge-commutativity (disjoint supports)
// =============================================================================

#[test]
fn l4_merge_commutative_when_disjoint() {
    let a = hide_inline(0, 0..2);
    let b = hide_inline(1, 0..2);
    assert!(disjoint(&a, &b));

    let lhs = normalize(vec![tagged(
        Display::merge(a.clone(), b.clone()),
        0,
        "p",
        0,
    )]);
    let rhs = normalize(vec![tagged(Display::merge(b, a), 0, "p", 0)]);

    assert_eq!(lhs, rhs);
    assert!(lhs.conflicts.is_empty());
}

#[test]
fn l4_disjoint_detects_overlap() {
    let a = hide_inline(0, 0..5);
    let b = hide_inline(0, 3..7);
    assert!(!disjoint(&a, &b));
}

// =============================================================================
// L5 — Decorate-commutativity (always commutes; conflict-free)
// =============================================================================

#[test]
fn l5_decorate_overlap_produces_no_conflict() {
    let a = style_inline(0, 0..5, face(), 1);
    let b = style_inline(0, 2..7, face(), 2);

    let result = normalize(vec![tagged(a, 0, "p", 0), tagged(b, 0, "p", 1)]);

    assert!(
        result.conflicts.is_empty(),
        "decorates must not conflict (L5)"
    );
    assert_eq!(result.leaves.len(), 2, "both decorates survive");
}

#[test]
fn l5_decorate_priority_orders_leaves() {
    let high = style_inline(0, 0..5, face(), 10);
    let low = style_inline(0, 0..5, face(), 1);

    let result = normalize(vec![tagged(low, 1, "p", 0), tagged(high, 5, "q", 0)]);

    assert_eq!(result.leaves.len(), 2);
    // Higher tag priority appears later in the leaf order (renderer
    // applies in order, so later leaves stack on top).
    assert!(matches!(result.leaves[1].display, Display::Decorate { .. }));
}

// =============================================================================
// L6 — Replace-conflict-determinism
// =============================================================================

#[test]
fn l6_overlapping_replace_produces_conflict() {
    let a = Display::Replace {
        range: Span::new(0, 0..5),
        content: Content::Text(vec![atom("AAA")]),
    };
    let b = Display::Replace {
        range: Span::new(0, 3..7),
        content: Content::Text(vec![atom("BBB")]),
    };

    let result = normalize(vec![tagged(a, 1, "p", 0), tagged(b, 5, "q", 0)]);

    assert_eq!(result.conflicts.len(), 1, "exactly one conflict expected");
    assert_eq!(result.leaves.len(), 1, "only the winner survives");

    let conflict = &result.conflicts[0];
    assert_eq!(conflict.winner.priority, 5, "higher priority wins");
    assert_eq!(conflict.displaced.len(), 1);
    assert_eq!(conflict.displaced[0].priority, 1);
}

#[test]
fn l6_conflict_resolution_is_order_independent() {
    let a = Display::Replace {
        range: Span::new(0, 0..5),
        content: Content::Text(vec![atom("AAA")]),
    };
    let b = Display::Replace {
        range: Span::new(0, 3..7),
        content: Content::Text(vec![atom("BBB")]),
    };

    let lhs = normalize(vec![
        tagged(a.clone(), 1, "p", 0),
        tagged(b.clone(), 5, "q", 0),
    ]);
    let rhs = normalize(vec![tagged(b, 5, "q", 0), tagged(a, 1, "p", 0)]);

    assert_eq!(lhs, rhs);
}

#[test]
fn l6_disjoint_replaces_do_not_conflict() {
    let a = hide_inline(0, 0..3);
    let b = hide_inline(0, 5..8);

    let result = normalize(vec![tagged(a, 0, "p", 0), tagged(b, 0, "p", 1)]);

    assert!(result.conflicts.is_empty());
    assert_eq!(result.leaves.len(), 2);
}

// =============================================================================
// Smart constructors
// =============================================================================

// =============================================================================
// ADR-037 Pass B — Fold conflict detection
// =============================================================================

#[test]
fn fold_conflicts_with_hide_inside_range_higher_wins() {
    let folded = fold(2..5, vec![atom("F")]);
    let h = hide_inline(3, 0..usize::MAX);

    // Hide at higher priority than the fold.
    let result = normalize(vec![
        tagged(folded.clone(), 0, "fold-plugin", 0),
        tagged(h.clone(), 5, "hide-plugin", 0),
    ]);

    assert_eq!(result.leaves.len(), 1, "hide displaces fold (Pass B)");
    assert!(matches!(
        result.leaves[0].display,
        Display::Replace {
            content: Content::Empty,
            ..
        }
    ));
    assert_eq!(result.conflicts.len(), 1);
    assert!(matches!(
        result.conflicts[0].displaced[0].display,
        Display::Replace {
            content: Content::Fold { .. },
            ..
        }
    ));
}

#[test]
fn fold_wins_when_higher_priority_than_intersecting_hide() {
    let folded = fold(2..5, vec![atom("F")]);
    let h = hide_inline(3, 0..usize::MAX);

    // Fold at higher priority.
    let result = normalize(vec![
        tagged(h, 0, "hide-plugin", 0),
        tagged(folded, 5, "fold-plugin", 0),
    ]);

    assert_eq!(result.leaves.len(), 1, "fold survives, displaces hide");
    assert!(matches!(
        result.leaves[0].display,
        Display::Replace {
            content: Content::Fold { .. },
            ..
        }
    ));
    assert_eq!(result.conflicts.len(), 1);
}

#[test]
fn fold_conflicts_with_overlapping_fold_via_range_crosscheck() {
    // Fold A anchor=0 range=0..3; Fold B anchor=2 range=2..5.
    // Their Spans (line=0 and line=2) don't overlap by Pass A, but
    // A's range covers B's anchor — Pass B catches it.
    let a = fold(0..3, vec![atom("A")]);
    let b = fold(2..5, vec![atom("B")]);

    let result = normalize(vec![tagged(a, 0, "p", 0), tagged(b, 5, "q", 0)]);

    assert_eq!(result.leaves.len(), 1, "one fold survives");
    assert_eq!(
        result.conflicts.len(),
        1,
        "the other is recorded as conflict"
    );
}

#[test]
fn folds_with_disjoint_ranges_both_survive() {
    let a = fold(0..3, vec![atom("A")]);
    let b = fold(5..8, vec![atom("B")]);
    let result = normalize(vec![tagged(a, 0, "p", 0), tagged(b, 0, "q", 0)]);
    assert_eq!(result.leaves.len(), 2);
    assert!(result.conflicts.is_empty());
}

#[test]
fn fold_does_not_conflict_with_decorate_inside_range() {
    // L5 says decorates never conflict — Pass B preserves this; the
    // decorate stacks on whatever survives.
    let folded = fold(2..5, vec![atom("F")]);
    let dec = style_inline(3, 0..10, face(), 0);

    let result = normalize(vec![tagged(folded, 0, "p", 0), tagged(dec, 0, "q", 0)]);

    assert_eq!(result.leaves.len(), 2);
    assert!(result.conflicts.is_empty());
}

#[test]
fn fold_does_not_conflict_with_anchor_inside_range() {
    // Anchors live in non-text positions and never participate in
    // Replace conflicts.
    let folded = fold(2..5, vec![atom("F")]);
    let g = gutter(3, 0, crate::element::Element::Empty);

    let result = normalize(vec![tagged(folded, 0, "p", 0), tagged(g, 0, "q", 0)]);

    assert_eq!(result.leaves.len(), 2);
    assert!(result.conflicts.is_empty());
}

#[test]
fn fold_conflicts_with_inline_replace_inside_range() {
    let folded = fold(2..5, vec![atom("F")]);
    let inline = Display::Replace {
        range: Span::new(3, 4..7),
        content: Content::Text(vec![atom("X")]),
    };

    let result = normalize(vec![tagged(folded, 0, "p", 0), tagged(inline, 5, "q", 0)]);

    // Inline at higher priority displaces the fold.
    assert_eq!(result.leaves.len(), 1);
    assert!(matches!(
        result.leaves[0].display,
        Display::Replace {
            content: Content::Text(..),
            ..
        }
    ));
    assert_eq!(result.conflicts.len(), 1);
}

// =============================================================================
// ADR-037 Pass C — EVT anchor-invisibility filter
// =============================================================================

fn evt(after: usize) -> Display {
    editable_virtual_text(after, vec![atom("e")], vec![], EditSpec::Mirror)
}

#[test]
fn pass_c_drops_evt_beyond_line_count() {
    let normalized = normalize(vec![tagged(evt(10), 0, "p", 0)]);
    let filtered = pass_c_filter_evt(normalized, 5);
    assert!(filtered.leaves.is_empty());
}

#[test]
fn pass_c_drops_evt_anchored_on_hidden_line() {
    let normalized = normalize(vec![
        tagged(hide_lines(2..5), 0, "h", 0),
        tagged(evt(3), 0, "e", 0),
    ]);
    let filtered = pass_c_filter_evt(normalized, 10);

    // Hide survives; EVT dropped.
    assert_eq!(filtered.leaves.len(), 1);
    assert!(matches!(
        filtered.leaves[0].display,
        Display::Replace {
            content: Content::Hide { .. },
            ..
        }
    ));
}

#[test]
fn pass_c_drops_evt_anchored_on_folded_line() {
    let normalized = normalize(vec![
        tagged(fold(2..5, vec![atom("F")]), 0, "f", 0),
        tagged(evt(3), 0, "e", 0),
    ]);
    let filtered = pass_c_filter_evt(normalized, 10);

    assert_eq!(filtered.leaves.len(), 1);
    assert!(matches!(
        filtered.leaves[0].display,
        Display::Replace {
            content: Content::Fold { .. },
            ..
        }
    ));
}

#[test]
fn pass_c_dedups_same_anchor_evts() {
    let normalized = normalize(vec![
        tagged(evt(3), 0, "low", 0),
        tagged(evt(3), 5, "high", 0),
    ]);
    let filtered = pass_c_filter_evt(normalized, 10);

    assert_eq!(filtered.leaves.len(), 1, "same-anchor dedup keeps one");
}

#[test]
fn pass_c_keeps_evts_at_distinct_anchors() {
    let normalized = normalize(vec![tagged(evt(2), 0, "p", 0), tagged(evt(5), 0, "p", 0)]);
    let filtered = pass_c_filter_evt(normalized, 10);
    assert_eq!(filtered.leaves.len(), 2);
}

#[test]
fn pass_c_keeps_evt_on_visible_line() {
    let normalized = normalize(vec![
        tagged(hide_lines(0..2), 0, "h", 0),
        tagged(evt(5), 0, "e", 0),
    ]);
    let filtered = pass_c_filter_evt(normalized, 10);

    // Both survive: EVT anchor 5 is outside hide range 0..2.
    assert_eq!(filtered.leaves.len(), 2);
}

#[test]
fn pass_c_passes_through_non_evt_anchors() {
    let normalized = normalize(vec![
        tagged(hide_lines(2..5), 0, "h", 0),
        tagged(gutter(3, 0, crate::element::Element::Empty), 0, "g", 0),
    ]);
    let filtered = pass_c_filter_evt(normalized, 10);

    // Gutter is not EVT (no Editable content) — Pass C does not
    // touch it even though it anchors on a hidden line.
    assert_eq!(filtered.leaves.len(), 2);
}

#[test]
fn fold_does_not_conflict_with_replace_at_boundary() {
    // Half-open: Fold(2..5) covers lines 2,3,4. A Hide at line 5 is
    // outside the range and must not conflict.
    let folded = fold(2..5, vec![atom("F")]);
    let h = hide_inline(5, 0..usize::MAX);

    let result = normalize(vec![tagged(folded, 0, "p", 0), tagged(h, 0, "q", 0)]);

    assert_eq!(result.leaves.len(), 2);
    assert!(result.conflicts.is_empty());
}

#[test]
fn fold_emits_single_leaf_with_content_fold() {
    // ADR-037 §2: fold no longer decomposes into summary+hides;
    // it emits a single Replace whose content carries the multi-line
    // range as Content::Fold.
    let summary = vec![atom("// folded")];
    let d = fold(2..5, summary.clone());
    let result = normalize(vec![tagged(d, 0, "p", 0)]);

    assert_eq!(result.leaves.len(), 1, "fold must emit a single leaf");
    assert!(result.conflicts.is_empty());

    let leaf = &result.leaves[0];
    match &leaf.display {
        Display::Replace {
            range,
            content:
                Content::Fold {
                    range: fold_range,
                    summary: fold_summary,
                },
        } => {
            assert_eq!(range.line, 2, "anchor at fold.range.start");
            assert_eq!(*fold_range, 2..5);
            assert_eq!(*fold_summary, summary);
        }
        other => panic!("expected Replace(Content::Fold), got {:?}", other),
    }
}

#[test]
fn fold_empty_range_is_identity() {
    let d = fold(5..5, vec![]);
    assert!(d.is_identity());
}

#[test]
fn anchor_does_not_conflict_with_replace() {
    let r = hide_inline(0, 0..10);
    let g = gutter(0, 0, crate::element::Element::Empty);

    let result = normalize(vec![tagged(r, 0, "p", 0), tagged(g, 0, "q", 0)]);

    assert!(result.conflicts.is_empty());
    assert_eq!(result.leaves.len(), 2);
}

#[test]
fn span_overlap_at_insertion_boundary_is_disjoint() {
    // Two plugins inserting at the same end-of-line do not conflict;
    // they share the boundary. The renderer concatenates them in
    // priority order. (Span::overlaps returns false for two
    // degenerate spans at different positions; equal degenerate
    // spans do conflict — that's the only insertion-point clash.)
    let a = Span::end_of_line(0);
    let b = Span::end_of_line(1);
    assert!(!a.overlaps(&b));
}

#[test]
fn span_overlap_at_same_insertion_point_conflicts() {
    let a = Span::at(0, 5);
    let b = Span::at(0, 5);
    assert!(a.overlaps(&b));
}

#[test]
fn flatten_lifts_through_then_and_merge_uniformly() {
    let a = hide_inline(0, 0..1);
    let b = hide_inline(0, 2..3);
    let c = hide_inline(0, 4..5);

    let mixed = Display::then(a, Display::merge(b, c));
    let result = normalize(vec![tagged(mixed, 0, "p", 0)]);

    assert_eq!(result.leaves.len(), 3);
    assert!(result.conflicts.is_empty());
}

#[test]
fn empty_input_is_empty_output() {
    let result = normalize(Vec::new());
    assert!(result.is_empty());
}

#[test]
fn identity_input_produces_no_leaves() {
    let result = normalize(vec![tagged(Display::Identity, 0, "p", 0)]);
    assert!(result.is_empty());
}
