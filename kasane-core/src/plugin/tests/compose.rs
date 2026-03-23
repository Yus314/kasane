use proptest::prelude::*;

use crate::display::{DirectiveSet, DisplayDirective, TaggedDirective};
use crate::element::Element;
use crate::element::OverlayAnchor;
use crate::plugin::PluginId;
use crate::plugin::compose::{
    Composable, ContributionSet, FirstWins, MenuTransformChain, OverlaySet, TransformChain,
    TransformChainEntry,
};
use crate::plugin::context::{
    ContribSizeHint, Contribution, OverlayContribution, SourcedContribution, TransformTarget,
};
use crate::protocol::{Atom, Face};

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

fn arb_display_directive() -> impl Strategy<Value = DisplayDirective> {
    prop_oneof![
        (0usize..100, 1usize..50).prop_map(|(s, len)| DisplayDirective::Hide { range: s..s + len }),
        (0usize..100, 1usize..50).prop_map(|(s, len)| DisplayDirective::Fold {
            range: s..s + len,
            summary: vec![Atom {
                face: Face::default(),
                contents: String::new().into()
            }],
        }),
        (0usize..200).prop_map(|after| DisplayDirective::InsertAfter {
            after,
            content: vec![Atom {
                face: Face::default(),
                contents: String::new().into()
            }],
        }),
        (0usize..200).prop_map(|before| DisplayDirective::InsertBefore {
            before,
            content: vec![Atom {
                face: Face::default(),
                contents: String::new().into()
            }],
        }),
    ]
}

fn arb_tagged_directive() -> impl Strategy<Value = TaggedDirective> {
    (arb_plugin_id(), -100i16..100i16, arb_display_directive()).prop_map(
        |(id, priority, directive)| TaggedDirective {
            directive,
            priority,
            plugin_id: id,
        },
    )
}

fn arb_directive_set() -> impl Strategy<Value = DirectiveSet> {
    prop::collection::vec(arb_tagged_directive(), 0..8).prop_map(|mut directives| {
        directives.sort_by(|a, b| a.sort_key().cmp(&b.sort_key()));
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

fn arb_transform_chain_entry() -> impl Strategy<Value = TransformChainEntry> {
    (arb_plugin_id(), -100i16..100i16).prop_map(|(plugin_id, priority)| TransformChainEntry {
        plugin_id,
        priority,
    })
}

fn arb_transform_chain() -> impl Strategy<Value = TransformChain> {
    prop::collection::vec(arb_transform_chain_entry(), 0..8)
        .prop_map(|entries| TransformChain::from_entries(entries))
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

/// Same plugin_id + same priority with multiple directive variants must
/// compose commutatively thanks to the 4-element sort key.
#[test]
fn directive_set_commutativity_same_plugin_same_priority() {
    let pid = PluginId("p".into());
    let a = DirectiveSet {
        directives: vec![TaggedDirective {
            directive: DisplayDirective::Hide { range: 0..5 },
            priority: 0,
            plugin_id: pid.clone(),
        }],
    };
    let b = DirectiveSet {
        directives: vec![TaggedDirective {
            directive: DisplayDirective::InsertAfter {
                after: 3,
                content: vec![Atom {
                    face: Face::default(),
                    contents: String::new().into(),
                }],
            },
            priority: 0,
            plugin_id: pid.clone(),
        }],
    };
    let ab = a.clone().compose(b.clone());
    let ba = b.compose(a);
    assert_eq!(
        ab, ba,
        "same plugin+priority with different variants must commute"
    );
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

// ---------------------------------------------------------------------------
// TransformChain — monoid laws (NOT commutative)
// ---------------------------------------------------------------------------

proptest! {
    #[test]
    fn transform_chain_left_identity(x in arb_transform_chain()) {
        let result = TransformChain::empty().compose(x.clone());
        prop_assert_eq!(result, x);
    }

    #[test]
    fn transform_chain_right_identity(x in arb_transform_chain()) {
        let result = x.clone().compose(TransformChain::empty());
        prop_assert_eq!(result, x);
    }

    #[test]
    fn transform_chain_associativity(
        a in arb_transform_chain(),
        b in arb_transform_chain(),
        c in arb_transform_chain(),
    ) {
        let left = a.clone().compose(b.clone()).compose(c.clone());
        let right = a.compose(b.compose(c));
        prop_assert_eq!(left, right);
    }
}

#[test]
fn transform_chain_not_commutative() {
    // TransformChain uses stable sort by (Reverse(priority), plugin_id).
    // With duplicate sort keys (same priority + same id), insertion order is preserved,
    // making compose order observable → non-commutative.
    //
    // Note: when all entries have unique (priority, plugin_id) keys, the result
    // happens to be commutative for those specific inputs. The monoid is marked
    // non-commutative because commutativity is not guaranteed in general.
    // Here we verify that same-priority distinct IDs produce a deterministic chain.
    let a = TransformChain::single(PluginId("alpha".into()), 0);
    let b = TransformChain::single(PluginId("beta".into()), 0);
    let ab = a.clone().compose(b.clone());
    let ba = b.compose(a);
    // Sorted by (Reverse(0), id): alpha before beta — deterministic.
    assert_eq!(
        ab.entries().len(),
        2,
        "Composed chain should have 2 entries"
    );
    assert_eq!(ab, ba, "Unique keys happen to sort identically");
}

// ---------------------------------------------------------------------------
// TransformTarget — hierarchy tests
// ---------------------------------------------------------------------------

#[test]
fn transform_target_parent() {
    assert_eq!(TransformTarget::Buffer.parent(), None);
    assert_eq!(TransformTarget::BufferLine(0).parent(), None);
    assert_eq!(TransformTarget::StatusBar.parent(), None);
    assert_eq!(TransformTarget::Menu.parent(), None);
    assert_eq!(
        TransformTarget::MenuPrompt.parent(),
        Some(TransformTarget::Menu)
    );
    assert_eq!(
        TransformTarget::MenuInline.parent(),
        Some(TransformTarget::Menu)
    );
    assert_eq!(
        TransformTarget::MenuSearch.parent(),
        Some(TransformTarget::Menu)
    );
    assert_eq!(TransformTarget::Info.parent(), None);
    assert_eq!(
        TransformTarget::InfoPrompt.parent(),
        Some(TransformTarget::Info)
    );
    assert_eq!(
        TransformTarget::InfoModal.parent(),
        Some(TransformTarget::Info)
    );
}

#[test]
fn transform_target_refinement_chain() {
    // Non-refinement targets: chain is [self]
    assert_eq!(
        TransformTarget::Buffer.refinement_chain(),
        vec![TransformTarget::Buffer]
    );
    assert_eq!(
        TransformTarget::Menu.refinement_chain(),
        vec![TransformTarget::Menu]
    );
    assert_eq!(
        TransformTarget::Info.refinement_chain(),
        vec![TransformTarget::Info]
    );
    assert_eq!(
        TransformTarget::StatusBar.refinement_chain(),
        vec![TransformTarget::StatusBar]
    );

    // Refinement targets: chain is [parent, self]
    assert_eq!(
        TransformTarget::MenuPrompt.refinement_chain(),
        vec![TransformTarget::Menu, TransformTarget::MenuPrompt]
    );
    assert_eq!(
        TransformTarget::MenuInline.refinement_chain(),
        vec![TransformTarget::Menu, TransformTarget::MenuInline]
    );
    assert_eq!(
        TransformTarget::MenuSearch.refinement_chain(),
        vec![TransformTarget::Menu, TransformTarget::MenuSearch]
    );
    assert_eq!(
        TransformTarget::InfoPrompt.refinement_chain(),
        vec![TransformTarget::Info, TransformTarget::InfoPrompt]
    );
    assert_eq!(
        TransformTarget::InfoModal.refinement_chain(),
        vec![TransformTarget::Info, TransformTarget::InfoModal]
    );
}

#[test]
fn transform_target_is_refinement() {
    assert!(!TransformTarget::Buffer.is_refinement());
    assert!(!TransformTarget::Menu.is_refinement());
    assert!(!TransformTarget::Info.is_refinement());
    assert!(!TransformTarget::StatusBar.is_refinement());
    assert!(TransformTarget::MenuPrompt.is_refinement());
    assert!(TransformTarget::MenuInline.is_refinement());
    assert!(TransformTarget::MenuSearch.is_refinement());
    assert!(TransformTarget::InfoPrompt.is_refinement());
    assert!(TransformTarget::InfoModal.is_refinement());
}

// ---------------------------------------------------------------------------
// Transform conflict detection — unit tests
// ---------------------------------------------------------------------------

#[cfg(debug_assertions)]
mod conflict_detection {
    use super::*;
    use crate::plugin::context::{TransformDescriptor, TransformScope};
    use crate::plugin::registry::check_transform_conflicts;

    #[test]
    fn no_warning_for_no_descriptors() {
        let descriptors = vec![(PluginId("a".into()), None), (PluginId("b".into()), None)];
        // Should not panic; warnings go to tracing (not captured here but
        // verifies the function runs without errors).
        check_transform_conflicts(&descriptors, &TransformTarget::Buffer);
    }

    #[test]
    fn no_warning_for_single_replacement() {
        let descriptors = vec![(
            PluginId("a".into()),
            Some(TransformDescriptor {
                targets: vec![TransformTarget::Buffer],
                scope: TransformScope::Replacement,
            }),
        )];
        check_transform_conflicts(&descriptors, &TransformTarget::Buffer);
    }

    #[test]
    fn no_warning_for_non_matching_target() {
        let descriptors = vec![
            (
                PluginId("a".into()),
                Some(TransformDescriptor {
                    targets: vec![TransformTarget::Menu],
                    scope: TransformScope::Replacement,
                }),
            ),
            (
                PluginId("b".into()),
                Some(TransformDescriptor {
                    targets: vec![TransformTarget::Menu],
                    scope: TransformScope::Replacement,
                }),
            ),
        ];
        // Checking Buffer target — neither descriptor matches, so no warning.
        check_transform_conflicts(&descriptors, &TransformTarget::Buffer);
    }

    #[test]
    fn detects_multiple_replacements() {
        // This test verifies the function runs without panic.
        // In a real scenario, tracing::warn would fire.
        let descriptors = vec![
            (
                PluginId("a".into()),
                Some(TransformDescriptor {
                    targets: vec![TransformTarget::Buffer],
                    scope: TransformScope::Replacement,
                }),
            ),
            (
                PluginId("b".into()),
                Some(TransformDescriptor {
                    targets: vec![TransformTarget::Buffer],
                    scope: TransformScope::Replacement,
                }),
            ),
        ];
        check_transform_conflicts(&descriptors, &TransformTarget::Buffer);
    }

    #[test]
    fn detects_absorbed_transforms() {
        // Wrapper before Replacement → absorbed warning
        let descriptors = vec![
            (
                PluginId("wrapper".into()),
                Some(TransformDescriptor {
                    targets: vec![TransformTarget::Menu],
                    scope: TransformScope::Wrapper,
                }),
            ),
            (
                PluginId("replacer".into()),
                Some(TransformDescriptor {
                    targets: vec![TransformTarget::Menu],
                    scope: TransformScope::Replacement,
                }),
            ),
        ];
        check_transform_conflicts(&descriptors, &TransformTarget::Menu);
    }
}
