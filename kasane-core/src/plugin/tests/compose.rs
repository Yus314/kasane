use proptest::prelude::*;

use crate::display::{DirectiveSet, DisplayDirective, TaggedDirective};
use crate::element::Element;
use crate::element::OverlayAnchor;
use crate::plugin::PluginId;
use crate::plugin::compose::{
    Composable, ContributionSet, FirstWins, MenuTransformChain, OverlaySet,
};
use crate::plugin::context::{
    ContribSizeHint, Contribution, OverlayContribution, SourcedContribution,
};

// ---------------------------------------------------------------------------
// Strategies
// ---------------------------------------------------------------------------

fn arb_plugin_id() -> impl Strategy<Value = PluginId> {
    "[a-z]{1,8}".prop_map(|s| PluginId(s))
}

fn arb_sourced_contribution() -> impl Strategy<Value = SourcedContribution> {
    (arb_plugin_id(), -100i16..100i16).prop_map(|(id, priority)| SourcedContribution {
        contributor: id,
        contribution: Contribution {
            element: Element::Empty,
            priority,
            size_hint: ContribSizeHint::Auto,
        },
    })
}

fn arb_contribution_set() -> impl Strategy<Value = ContributionSet> {
    prop::collection::vec(arb_sourced_contribution(), 0..8)
        .prop_map(|items| ContributionSet::from_vec(items))
}

fn arb_overlay_contribution() -> impl Strategy<Value = OverlayContribution> {
    (arb_plugin_id(), -100i16..100i16).prop_map(|(id, z_index)| OverlayContribution {
        element: Element::Empty,
        anchor: OverlayAnchor::Fill,
        z_index,
        plugin_id: id,
    })
}

fn arb_overlay_set() -> impl Strategy<Value = OverlaySet> {
    prop::collection::vec(arb_overlay_contribution(), 0..8)
        .prop_map(|items| OverlaySet::from_vec(items))
}

fn arb_tagged_directive() -> impl Strategy<Value = TaggedDirective> {
    (
        arb_plugin_id(),
        -100i16..100i16,
        0usize..20usize,
        1usize..5usize,
    )
        .prop_map(|(id, priority, start, len)| TaggedDirective {
            directive: DisplayDirective::Hide {
                range: start..start + len,
            },
            priority,
            plugin_id: id,
        })
}

fn arb_directive_set() -> impl Strategy<Value = DirectiveSet> {
    prop::collection::vec(arb_tagged_directive(), 0..8).prop_map(|mut directives| {
        directives.sort_by(|a, b| {
            a.priority
                .cmp(&b.priority)
                .then_with(|| a.plugin_id.cmp(&b.plugin_id))
        });
        DirectiveSet { directives }
    })
}

fn arb_first_wins() -> impl Strategy<Value = FirstWins<i32>> {
    prop::option::of(-1000i32..1000i32).prop_map(|v| match v {
        Some(val) => FirstWins::some(val),
        None => FirstWins::none(),
    })
}

fn arb_menu_transform_chain() -> impl Strategy<Value = MenuTransformChain> {
    prop::collection::vec(arb_plugin_id(), 0..8)
        .prop_map(|plugins| MenuTransformChain::from_vec(plugins))
}

// ---------------------------------------------------------------------------
// ContributionSet — monoid laws
// ---------------------------------------------------------------------------

proptest! {
    #[test]
    fn contribution_set_left_identity(x in arb_contribution_set()) {
        let result = ContributionSet::empty().compose(x.clone());
        prop_assert_eq!(result, x);
    }

    #[test]
    fn contribution_set_right_identity(x in arb_contribution_set()) {
        let result = x.clone().compose(ContributionSet::empty());
        prop_assert_eq!(result, x);
    }

    #[test]
    fn contribution_set_associativity(
        a in arb_contribution_set(),
        b in arb_contribution_set(),
        c in arb_contribution_set(),
    ) {
        let left = a.clone().compose(b.clone()).compose(c.clone());
        let right = a.compose(b.compose(c));
        prop_assert_eq!(left, right);
    }

    #[test]
    fn contribution_set_commutativity(
        a in arb_contribution_set(),
        b in arb_contribution_set(),
    ) {
        let ab = a.clone().compose(b.clone());
        let ba = b.compose(a);
        prop_assert_eq!(ab, ba);
    }
}

// ---------------------------------------------------------------------------
// OverlaySet — monoid laws
// ---------------------------------------------------------------------------

proptest! {
    #[test]
    fn overlay_set_left_identity(x in arb_overlay_set()) {
        let result = OverlaySet::empty().compose(x.clone());
        prop_assert_eq!(result, x);
    }

    #[test]
    fn overlay_set_right_identity(x in arb_overlay_set()) {
        let result = x.clone().compose(OverlaySet::empty());
        prop_assert_eq!(result, x);
    }

    #[test]
    fn overlay_set_associativity(
        a in arb_overlay_set(),
        b in arb_overlay_set(),
        c in arb_overlay_set(),
    ) {
        let left = a.clone().compose(b.clone()).compose(c.clone());
        let right = a.compose(b.compose(c));
        prop_assert_eq!(left, right);
    }

    #[test]
    fn overlay_set_commutativity(
        a in arb_overlay_set(),
        b in arb_overlay_set(),
    ) {
        let ab = a.clone().compose(b.clone());
        let ba = b.compose(a);
        prop_assert_eq!(ab, ba);
    }
}

// ---------------------------------------------------------------------------
// DirectiveSet — monoid laws
// ---------------------------------------------------------------------------

proptest! {
    #[test]
    fn directive_set_left_identity(x in arb_directive_set()) {
        let result = DirectiveSet::empty().compose(x.clone());
        prop_assert_eq!(result, x);
    }

    #[test]
    fn directive_set_right_identity(x in arb_directive_set()) {
        let result = x.clone().compose(DirectiveSet::empty());
        prop_assert_eq!(result, x);
    }

    #[test]
    fn directive_set_associativity(
        a in arb_directive_set(),
        b in arb_directive_set(),
        c in arb_directive_set(),
    ) {
        let left = a.clone().compose(b.clone()).compose(c.clone());
        let right = a.compose(b.compose(c));
        prop_assert_eq!(left, right);
    }

    #[test]
    fn directive_set_commutativity(
        a in arb_directive_set(),
        b in arb_directive_set(),
    ) {
        let ab = a.clone().compose(b.clone());
        let ba = b.compose(a);
        prop_assert_eq!(ab, ba);
    }
}

// ---------------------------------------------------------------------------
// MenuTransformChain — monoid laws (NOT commutative)
// ---------------------------------------------------------------------------

proptest! {
    #[test]
    fn menu_transform_chain_left_identity(x in arb_menu_transform_chain()) {
        let result = MenuTransformChain::empty().compose(x.clone());
        prop_assert_eq!(result, x);
    }

    #[test]
    fn menu_transform_chain_right_identity(x in arb_menu_transform_chain()) {
        let result = x.clone().compose(MenuTransformChain::empty());
        prop_assert_eq!(result, x);
    }

    #[test]
    fn menu_transform_chain_associativity(
        a in arb_menu_transform_chain(),
        b in arb_menu_transform_chain(),
        c in arb_menu_transform_chain(),
    ) {
        let left = a.clone().compose(b.clone()).compose(c.clone());
        let right = a.compose(b.compose(c));
        prop_assert_eq!(left, right);
    }
}

// ---------------------------------------------------------------------------
// FirstWins<i32> — monoid laws (NOT commutative)
// ---------------------------------------------------------------------------

proptest! {
    #[test]
    fn first_wins_left_identity(x in arb_first_wins()) {
        let result = FirstWins::<i32>::empty().compose(x.clone());
        prop_assert_eq!(result, x);
    }

    #[test]
    fn first_wins_right_identity(x in arb_first_wins()) {
        let result = x.clone().compose(FirstWins::<i32>::empty());
        prop_assert_eq!(result, x);
    }

    #[test]
    fn first_wins_associativity(
        a in arb_first_wins(),
        b in arb_first_wins(),
        c in arb_first_wins(),
    ) {
        let left = a.clone().compose(b.clone()).compose(c.clone());
        let right = a.compose(b.compose(c));
        prop_assert_eq!(left, right);
    }
}

// ---------------------------------------------------------------------------
// Negative tests — explicit counterexamples for non-commutativity
// ---------------------------------------------------------------------------

#[test]
fn menu_transform_chain_not_commutative() {
    let a = MenuTransformChain::from_vec(vec![PluginId("alpha".into())]);
    let b = MenuTransformChain::from_vec(vec![PluginId("beta".into())]);
    let ab = a.clone().compose(b.clone());
    let ba = b.compose(a);
    assert_ne!(ab, ba, "MenuTransformChain must not be commutative");
}

#[test]
fn first_wins_not_commutative() {
    let a = FirstWins::some(1);
    let b = FirstWins::some(2);
    let ab = a.clone().compose(b.clone());
    let ba = b.compose(a);
    assert_ne!(ab, ba, "FirstWins must not be commutative");
}

// ---------------------------------------------------------------------------
// Annotation-specific tests (direct, not through Composable trait)
// ---------------------------------------------------------------------------

#[test]
fn annotation_background_max_selection_commutative() {
    // Background annotation resolution picks the highest z_order.
    // This is commutative: max(a, b) == max(b, a).
    fn select_bg(layers: &[(i16, i32)]) -> Option<(i16, i32)> {
        layers.iter().copied().max_by_key(|(z, _)| *z)
    }

    let a = vec![(1, 10), (3, 30)];
    let b = vec![(2, 20), (3, 30)];

    // a then b
    let mut combined_ab = a.clone();
    combined_ab.extend_from_slice(&b);
    let result_ab = select_bg(&combined_ab);

    // b then a
    let mut combined_ba = b;
    combined_ba.extend_from_slice(&a);
    let result_ba = select_bg(&combined_ba);

    assert_eq!(result_ab, result_ba);
}

#[test]
fn annotation_gutter_merge_associative() {
    // Gutter merge = append + sort by priority. Same as ContributionSet semantics.
    fn merge(mut items: Vec<(i16, i32)>) -> Vec<(i16, i32)> {
        items.sort_by_key(|(priority, _)| *priority);
        items
    }

    let a = vec![(2, 1)];
    let b = vec![(1, 2)];
    let c = vec![(3, 3)];

    // (a ++ b ++ c) sorted == (a ++ (b ++ c)) sorted
    let mut ab = a.clone();
    ab.extend_from_slice(&b);
    let mut abc_left = ab;
    abc_left.extend_from_slice(&c);

    let mut bc = b.clone();
    bc.extend_from_slice(&c);
    let mut abc_right = a;
    abc_right.extend_from_slice(&bc);

    assert_eq!(merge(abc_left), merge(abc_right));
}
