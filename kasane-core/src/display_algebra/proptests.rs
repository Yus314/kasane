//! Proptest fixtures for ADR-034 algebraic laws.
//!
//! Concrete witnesses (`tests.rs`) catch hand-picked regressions; this
//! file exercises L1–L6 over a randomised distribution of `Display`
//! trees, providing structural assurance against both stale and
//! freshly-introduced bugs in `normalize`.
//!
//! Strategies are deliberately small (max line/byte ≤ 16, max tree
//! depth ≤ 4, max forest size ≤ 6) so each case runs in microseconds —
//! the value here is *coverage*, not stress.

use proptest::collection::vec;
use proptest::prelude::*;

use crate::plugin::PluginId;

use super::derived::{fold, hide_inline, style_inline};
use super::normalize::{TaggedDisplay, disjoint, normalize};
use super::primitives::Display;

const MAX_LINE: u32 = 8;
const MAX_BYTE: u32 = 16;
const MAX_FOREST: usize = 6;

// =============================================================================
// Strategies
// =============================================================================

/// A simple `Replace` (via `hide_inline`) at a random position.
fn arb_hide() -> impl Strategy<Value = Display> {
    (0u32..MAX_LINE, 0u32..MAX_BYTE)
        .prop_flat_map(|(line, start)| (Just(line), Just(start), (start + 1)..(MAX_BYTE + 1)))
        .prop_map(|(line, s, e)| hide_inline(line as usize, s as usize..e as usize))
}

/// A simple `Decorate` (via `style_inline`) at a random position.
fn arb_style() -> impl Strategy<Value = Display> {
    (0u32..MAX_LINE, 0u32..MAX_BYTE)
        .prop_flat_map(|(line, start)| {
            (
                Just(line),
                Just(start),
                (start + 1)..(MAX_BYTE + 1),
                -8i16..8,
            )
        })
        .prop_map(|(line, s, e, prio)| {
            style_inline(
                line as usize,
                s as usize..e as usize,
                crate::protocol::WireFace::default(),
                prio,
            )
        })
}

/// A short multi-line `Fold` (ADR-037 Pass B coverage).
fn arb_fold() -> impl Strategy<Value = Display> {
    (0u32..(MAX_LINE - 1))
        .prop_flat_map(|start| {
            let max_end = (start + 4).min(MAX_LINE);
            (Just(start), (start + 1)..(max_end + 1))
        })
        .prop_map(|(s, e)| fold(s as usize..e as usize, vec![]))
}

/// A leaf primitive — `Replace`, `Decorate`, `Fold`, or `Identity`.
fn arb_leaf() -> impl Strategy<Value = Display> {
    prop_oneof![
        4 => arb_hide(),
        4 => arb_style(),
        2 => arb_fold(),
        1 => Just(Display::Identity),
    ]
}

/// A `Display` tree of bounded depth, mixing `Then` and `Merge`.
fn arb_display() -> impl Strategy<Value = Display> {
    arb_leaf().prop_recursive(4, 16, 3, |inner| {
        prop_oneof![
            (inner.clone(), inner.clone()).prop_map(|(a, b)| Display::then(a, b)),
            (inner.clone(), inner).prop_map(|(a, b)| Display::merge(a, b)),
        ]
    })
}

/// A `TaggedDisplay` carrying an arbitrary tree and tag.
fn arb_tagged() -> impl Strategy<Value = TaggedDisplay> {
    (arb_display(), -4i16..4, "[a-z]{1,4}", 0u32..3)
        .prop_map(|(d, prio, plugin, seq)| TaggedDisplay::new(d, prio, PluginId(plugin), seq))
}

/// A small forest of tagged displays.
fn arb_forest() -> impl Strategy<Value = Vec<TaggedDisplay>> {
    vec(arb_tagged(), 0..MAX_FOREST)
}

// =============================================================================
// L1 — Identity unit
// =============================================================================

proptest! {
    #![proptest_config(ProptestConfig::with_cases(64))]

    #[test]
    fn l1_then_identity_is_unit(d in arb_display()) {
        prop_assert_eq!(Display::then(Display::Identity, d.clone()), d.clone());
        prop_assert_eq!(Display::then(d.clone(), Display::Identity), d);
    }

    #[test]
    fn l1_merge_identity_is_unit(d in arb_display()) {
        prop_assert_eq!(Display::merge(Display::Identity, d.clone()), d.clone());
        prop_assert_eq!(Display::merge(d.clone(), Display::Identity), d);
    }

    // =========================================================================
    // L2 — Then-associativity (over normalisation)
    // =========================================================================

    #[test]
    fn l2_then_associative(
        a in arb_display(),
        b in arb_display(),
        c in arb_display(),
    ) {
        let left = Display::then(Display::then(a.clone(), b.clone()), c.clone());
        let right = Display::then(a, Display::then(b, c));
        let lhs = normalize(vec![TaggedDisplay::new(left, 0, PluginId("p".into()), 0)]);
        let rhs = normalize(vec![TaggedDisplay::new(right, 0, PluginId("p".into()), 0)]);
        prop_assert_eq!(lhs, rhs);
    }

    // =========================================================================
    // L3 — Merge-associativity (over normalisation)
    // =========================================================================

    #[test]
    fn l3_merge_associative(
        a in arb_display(),
        b in arb_display(),
        c in arb_display(),
    ) {
        let left = Display::merge(Display::merge(a.clone(), b.clone()), c.clone());
        let right = Display::merge(a, Display::merge(b, c));
        let lhs = normalize(vec![TaggedDisplay::new(left, 0, PluginId("p".into()), 0)]);
        let rhs = normalize(vec![TaggedDisplay::new(right, 0, PluginId("p".into()), 0)]);
        prop_assert_eq!(lhs, rhs);
    }

    // =========================================================================
    // L4 — Merge-commutativity on disjoint supports
    //
    // The discovery from §5.1 (positional tertiary key) is exactly what
    // makes this property hold across composition orders.
    // =========================================================================

    #[test]
    fn l4_merge_commutative_on_disjoint(
        a in arb_display(),
        b in arb_display(),
    ) {
        prop_assume!(disjoint(&a, &b));
        let lhs = normalize(vec![TaggedDisplay::new(
            Display::merge(a.clone(), b.clone()),
            0,
            PluginId("p".into()),
            0,
        )]);
        let rhs = normalize(vec![TaggedDisplay::new(
            Display::merge(b, a),
            0,
            PluginId("p".into()),
            0,
        )]);
        prop_assert_eq!(lhs, rhs);
    }

    // =========================================================================
    // L5 — Decorate overlaps never produce conflicts
    // =========================================================================

    #[test]
    fn l5_decorates_never_conflict(forest in vec(arb_style(), 0..MAX_FOREST)) {
        let tagged: Vec<_> = forest
            .into_iter()
            .enumerate()
            .map(|(i, d)| TaggedDisplay::new(d, 0, PluginId("p".into()), i as u32))
            .collect();
        let result = normalize(tagged);
        prop_assert!(result.conflicts.is_empty());
    }

    // =========================================================================
    // L6 — Replace-conflict-determinism
    //
    // For any forest, the same input order shuffled produces the same
    // output (set of leaves and conflicts). Witnessed by reversing the
    // input vector — a stronger statement than just "deterministic per
    // input order" because it requires the implementation to be
    // order-independent.
    // =========================================================================

    #[test]
    fn l6_normalize_is_input_order_independent(forest in arb_forest()) {
        let lhs = normalize(forest.clone());
        let mut reversed = forest;
        reversed.reverse();
        let rhs = normalize(reversed);
        prop_assert_eq!(lhs, rhs);
    }
}
