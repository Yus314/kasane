//! Structural witness tests for DisplayDirective classification (ADR-030 Level 4).

use std::collections::BTreeSet;

use crate::display::{self, BufferLine, DisplayDirective, DisplayMap, FoldToggleState};
use crate::plugin::handler_registry::HandlerRegistry;
use crate::plugin::safe_directive::SafeDisplayDirective;
use crate::plugin::{RecoveryMechanism, RecoveryWitness};
use crate::protocol::{Atom, Face};

fn make_all_directive_instances() -> Vec<DisplayDirective> {
    vec![
        DisplayDirective::Fold {
            range: 0..1,
            summary: vec![Atom {
                face: Face::default(),
                contents: "…".into(),
            }],
        },
        DisplayDirective::Hide { range: 0..1 },
        DisplayDirective::InsertAfter {
            after: 0,
            content: vec![],
        },
        DisplayDirective::InsertBefore {
            before: 0,
            content: vec![],
        },
    ]
}

// =========================================================================
// Classification structural tests (mirrors command_classification.rs 1–6)
// =========================================================================

#[test]
fn all_variant_names_count() {
    let instances = make_all_directive_instances();
    assert_eq!(
        instances.len(),
        display::ALL_VARIANT_NAMES.len(),
        "make_all_directive_instances() must produce exactly one instance per variant"
    );
}

#[test]
fn all_variant_names_are_unique() {
    let set: BTreeSet<_> = display::ALL_VARIANT_NAMES.iter().copied().collect();
    assert_eq!(
        set.len(),
        display::ALL_VARIANT_NAMES.len(),
        "ALL_VARIANT_NAMES must not contain duplicates"
    );
}

#[test]
fn variant_name_covers_all() {
    let from_instances: BTreeSet<_> = make_all_directive_instances()
        .iter()
        .map(|d| d.variant_name())
        .collect();
    let from_const: BTreeSet<_> = display::ALL_VARIANT_NAMES.iter().copied().collect();
    assert_eq!(
        from_instances, from_const,
        "variant_name() must match ALL_VARIANT_NAMES exactly"
    );
}

#[test]
fn destructive_set_matches_semantics() {
    assert_eq!(display::DESTRUCTIVE_VARIANTS, &["Hide"]);
}

#[test]
fn safe_covers_exactly_non_destructive() {
    let safe: BTreeSet<_> = SafeDisplayDirective::VARIANT_NAMES
        .iter()
        .copied()
        .collect();
    let destructive: BTreeSet<_> = display::DESTRUCTIVE_VARIANTS.iter().copied().collect();
    let all: BTreeSet<_> = display::ALL_VARIANT_NAMES.iter().copied().collect();
    assert_eq!(safe, &all - &destructive);
}

#[test]
fn is_destructive_matches_constants() {
    for d in make_all_directive_instances() {
        let name = d.variant_name();
        assert_eq!(
            d.is_destructive(),
            display::DESTRUCTIVE_VARIANTS.contains(&name),
            "classification mismatch for {name}"
        );
    }
}

#[test]
fn classification_is_exhaustive_partition() {
    let additive: BTreeSet<_> = display::ADDITIVE_VARIANTS.iter().copied().collect();
    let preserving: BTreeSet<_> = display::PRESERVING_VARIANTS.iter().copied().collect();
    let destructive: BTreeSet<_> = display::DESTRUCTIVE_VARIANTS.iter().copied().collect();
    let all: BTreeSet<_> = display::ALL_VARIANT_NAMES.iter().copied().collect();

    // Union == ALL
    let union: BTreeSet<_> = additive
        .iter()
        .chain(preserving.iter())
        .chain(destructive.iter())
        .copied()
        .collect();
    assert_eq!(
        union, all,
        "ADDITIVE ∪ PRESERVING ∪ DESTRUCTIVE must equal ALL"
    );

    // Pairwise disjoint
    assert!(
        additive.is_disjoint(&preserving),
        "ADDITIVE ∩ PRESERVING must be empty"
    );
    assert!(
        additive.is_disjoint(&destructive),
        "ADDITIVE ∩ DESTRUCTIVE must be empty"
    );
    assert!(
        preserving.is_disjoint(&destructive),
        "PRESERVING ∩ DESTRUCTIVE must be empty"
    );
}

#[test]
fn preserving_has_framework_recovery() {
    // Fold(10..20) → toggle → all lines recover to Some in buffer_to_display
    let directives = vec![DisplayDirective::Fold {
        range: 10..20,
        summary: vec![Atom {
            face: Face::default(),
            contents: "…".into(),
        }],
    }];

    let line_count = 30;

    // Before toggle: lines 10..20 map to the fold summary display line
    let dm_folded = DisplayMap::build(line_count, &directives);
    for bl in 10..20 {
        // All map to the same fold summary line
        let dl = dm_folded.buffer_to_display(BufferLine(bl));
        assert!(dl.is_some(), "folded line {bl} must map to summary");
    }

    // After toggle: expanded fold → identity-like for those lines
    let mut toggle = FoldToggleState::default();
    toggle.toggle(&(10..20));
    let mut filtered = directives.clone();
    toggle.filter_directives(&mut filtered);

    let dm_expanded = DisplayMap::build(line_count, &filtered);
    for bl in 10..20 {
        assert!(
            dm_expanded.buffer_to_display(BufferLine(bl)).is_some(),
            "after toggle, line {bl} must have a display mapping"
        );
    }
}

// =========================================================================
// Recovery flag auto-derivation tests (Step 9)
// =========================================================================

#[derive(Debug, Clone, PartialEq, Default)]
struct S;

#[test]
fn no_handler_is_visually_faithful() {
    let registry = HandlerRegistry::<S>::new();
    assert!(
        registry.is_display_recoverable(),
        "no display handler → NotRegistered → faithful"
    );
}

#[test]
fn safe_handler_is_visually_faithful() {
    let mut registry = HandlerRegistry::<S>::new();
    registry.on_display_safe(|_state, _app| vec![]);
    assert!(
        registry.is_display_recoverable(),
        "on_display_safe → NonDestructive → faithful"
    );
}

#[test]
fn witnessed_handler_is_visually_faithful() {
    let mut registry = HandlerRegistry::<S>::new();
    registry.on_display_witnessed(
        RecoveryWitness {
            mechanism: RecoveryMechanism::KeyToggle {
                description: "press <ret> on fold line",
            },
        },
        |_state, _app| vec![],
    );
    assert!(
        registry.is_display_recoverable(),
        "on_display_witnessed → Witnessed → faithful"
    );
}

#[test]
fn unwitnessed_handler_is_not_visually_faithful() {
    let mut registry = HandlerRegistry::<S>::new();
    registry.on_display(|_state, _app| vec![]);
    assert!(
        !registry.is_display_recoverable(),
        "on_display → Unwitnessed → NOT faithful"
    );
}
