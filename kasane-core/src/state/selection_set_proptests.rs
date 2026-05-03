//! Proptest fixtures for ADR-035 §1 SelectionSet algebra.
//!
//! Strategies are bounded (≤ 8 lines, ≤ 16 columns, ≤ 6 selections per
//! set) so each case completes in microseconds. The properties we
//! witness:
//!
//! - **Idempotency**: `a ∪ a = a`, `a ∩ a = a`, `a − a = ∅`.
//! - **Commutativity**: `a ∪ b = b ∪ a`, `a ∩ b = b ∩ a`, sym-diff symmetric.
//! - **Associativity**: union and intersect are associative.
//! - **Identity (empty set)**: `a ∪ ∅ = a`, `a ∩ ∅ = ∅`, `a − ∅ = a`.
//! - **Absorption**: `a ∪ (a ∩ b) = a` and `a ∩ (a ∪ b) = a`.
//! - **Distributive**: `a ∩ (b ∪ c) = (a ∩ b) ∪ (a ∩ c)`.
//! - **Difference characterisation**: `a − b = a ∩ (univ − b)` ⟺
//!   `(a − b) ∩ b = ∅` and `(a − b) ⊆ a`.
//! - **Symmetric difference**: `a △ b = (a ∪ b) − (a ∩ b)` and
//!   `a △ a = ∅`.
//!
//! All operations are normalising, so equality checks compare the
//! canonical sorted-disjoint form.

use proptest::collection::vec;
use proptest::prelude::*;

use super::selection::{BufferId, BufferPos, BufferVersion, Selection};
use super::selection_set::SelectionSet;

const MAX_LINE: u32 = 8;
const MAX_COL: u32 = 16;
const MAX_SELS: usize = 6;

fn buf() -> BufferId {
    BufferId::new("proptest")
}

fn ver() -> BufferVersion {
    BufferVersion::INITIAL
}

// =============================================================================
// Strategies
// =============================================================================

/// Non-degenerate selections only: `cursor > anchor` so every selection
/// has positive byte extent. Point selections (`anchor == cursor`) are
/// not first-class set members in the current SelectionSet algebra —
/// they represent a position, not selected content, and a future ADR
/// will tackle their semantics. Excluding them here keeps the witnessed
/// laws in their cleanest form.
fn arb_selection() -> impl Strategy<Value = Selection> {
    (0u32..MAX_LINE, 0u32..MAX_COL)
        .prop_flat_map(|(line, c0)| (Just(line), Just(c0), (c0 + 1)..(MAX_COL + 1)))
        .prop_map(|(line, c0, c1)| {
            Selection::new(BufferPos::new(line, c0), BufferPos::new(line, c1))
        })
}

fn arb_set() -> impl Strategy<Value = SelectionSet> {
    vec(arb_selection(), 0..MAX_SELS).prop_map(|sels| SelectionSet::from_iter(sels, buf(), ver()))
}

fn empty() -> SelectionSet {
    SelectionSet::empty(buf(), ver())
}

// =============================================================================
// Idempotency
// =============================================================================

proptest! {
    #![proptest_config(ProptestConfig::with_cases(64))]

    #[test]
    fn union_idempotent(a in arb_set()) {
        prop_assert_eq!(a.union(&a), a.clone());
    }

    #[test]
    fn intersect_idempotent(a in arb_set()) {
        prop_assert_eq!(a.intersect(&a), a.clone());
    }

    #[test]
    fn difference_with_self_is_empty(a in arb_set()) {
        prop_assert!(a.difference(&a).is_empty());
    }

    // ----- Commutativity -----

    #[test]
    fn union_commutative(a in arb_set(), b in arb_set()) {
        prop_assert_eq!(a.union(&b), b.union(&a));
    }

    #[test]
    fn intersect_commutative(a in arb_set(), b in arb_set()) {
        prop_assert_eq!(a.intersect(&b), b.intersect(&a));
    }

    #[test]
    fn symmetric_difference_commutative(a in arb_set(), b in arb_set()) {
        prop_assert_eq!(a.symmetric_difference(&b), b.symmetric_difference(&a));
    }

    // ----- Associativity -----

    #[test]
    fn union_associative(a in arb_set(), b in arb_set(), c in arb_set()) {
        prop_assert_eq!(a.union(&b).union(&c), a.union(&b.union(&c)));
    }

    #[test]
    fn intersect_associative(a in arb_set(), b in arb_set(), c in arb_set()) {
        prop_assert_eq!(a.intersect(&b).intersect(&c), a.intersect(&b.intersect(&c)));
    }

    // ----- Identity (empty as the unit of union; empty as zero of intersect) -----

    #[test]
    fn union_with_empty_is_identity(a in arb_set()) {
        prop_assert_eq!(a.union(&empty()), a.clone());
        prop_assert_eq!(empty().union(&a), a);
    }

    #[test]
    fn intersect_with_empty_is_empty(a in arb_set()) {
        prop_assert!(a.intersect(&empty()).is_empty());
        prop_assert!(empty().intersect(&a).is_empty());
    }

    #[test]
    fn difference_with_empty_is_self(a in arb_set()) {
        prop_assert_eq!(a.difference(&empty()), a);
    }

    // ----- Absorption -----

    #[test]
    fn absorption_union(a in arb_set(), b in arb_set()) {
        let inner = a.intersect(&b);
        prop_assert_eq!(a.union(&inner), a);
    }

    #[test]
    fn absorption_intersect(a in arb_set(), b in arb_set()) {
        let inner = a.union(&b);
        prop_assert_eq!(a.intersect(&inner), a);
    }

    // ----- Distributive -----

    #[test]
    fn intersect_distributes_over_union(
        a in arb_set(),
        b in arb_set(),
        c in arb_set(),
    ) {
        let lhs = a.intersect(&b.union(&c));
        let rhs = a.intersect(&b).union(&a.intersect(&c));
        prop_assert_eq!(lhs, rhs);
    }

    #[test]
    fn union_distributes_over_intersect(
        a in arb_set(),
        b in arb_set(),
        c in arb_set(),
    ) {
        let lhs = a.union(&b.intersect(&c));
        let rhs = a.union(&b).intersect(&a.union(&c));
        prop_assert_eq!(lhs, rhs);
    }

    // ----- Difference characterisation -----

    #[test]
    fn difference_disjoint_from_subtrahend(a in arb_set(), b in arb_set()) {
        let d = a.difference(&b);
        prop_assert!(d.intersect(&b).is_empty());
    }

    #[test]
    fn difference_is_subset_of_minuend(a in arb_set(), b in arb_set()) {
        let d = a.difference(&b);
        // d ⊆ a iff d ∪ a = a.
        prop_assert_eq!(d.union(&a), a);
    }

    // ----- Symmetric difference -----

    #[test]
    fn symmetric_difference_via_union_minus_intersect(
        a in arb_set(),
        b in arb_set(),
    ) {
        let lhs = a.symmetric_difference(&b);
        let rhs = a.union(&b).difference(&a.intersect(&b));
        prop_assert_eq!(lhs, rhs);
    }

    #[test]
    fn symmetric_difference_self_is_empty(a in arb_set()) {
        prop_assert!(a.symmetric_difference(&a).is_empty());
    }

    // ----- Disjointness ↔ intersect-empty -----

    #[test]
    fn disjoint_iff_intersect_empty(a in arb_set(), b in arb_set()) {
        let disjoint = a.is_disjoint(&b);
        let intersect_empty = a.intersect(&b).is_empty();
        prop_assert_eq!(disjoint, intersect_empty);
    }
}
