//! Structural witness tests for DisplayDirective classification (ADR-030 Level 4).

use std::collections::BTreeSet;

use crate::display::{
    self, BufferLine, DirectiveCategory, DisplayDirective, DisplayMap, FoldToggleState, GutterSide,
    InlineBoxAlignment, InlineInteraction, VirtualTextPosition,
};
use crate::element::Element;
use crate::plugin::handler_registry::HandlerRegistry;
use crate::plugin::safe_directive::SafeDisplayDirective;
use crate::plugin::{RecoveryMechanism, RecoveryWitness};
use crate::protocol::Atom;

fn make_all_directive_instances() -> Vec<DisplayDirective> {
    vec![
        DisplayDirective::EditableVirtualText {
            after: 0,
            content: vec![Atom::plain("edit")],
            editable_spans: vec![],
        },
        DisplayDirective::Fold {
            range: 0..1,
            summary: vec![Atom::plain("…")],
        },
        DisplayDirective::Gutter {
            line: 0,
            side: GutterSide::Left,
            content: Element::Empty,
            priority: 0,
        },
        DisplayDirective::Hide { range: 0..1 },
        DisplayDirective::HideInline {
            line: 0,
            byte_range: 0..1,
        },
        DisplayDirective::InsertAfter {
            line: 0,
            content: Element::Empty,
            priority: 0,
        },
        DisplayDirective::InsertBefore {
            line: 0,
            content: Element::Empty,
            priority: 0,
        },
        DisplayDirective::InsertInline {
            line: 0,
            byte_offset: 0,
            content: vec![],
            interaction: InlineInteraction::None,
        },
        DisplayDirective::InlineBox {
            line: 0,
            byte_offset: 0,
            width_cells: 1.0,
            height_lines: 1.0,
            box_id: 0,
            alignment: InlineBoxAlignment::Center,
        },
        DisplayDirective::StyleInline {
            line: 0,
            byte_range: 0..1,
            face: crate::protocol::WireFace::default(),
        },
        DisplayDirective::StyleLine {
            line: 0,
            face: crate::protocol::WireFace::default(),
            z_order: 0,
        },
        DisplayDirective::VirtualText {
            line: 0,
            position: VirtualTextPosition::EndOfLine,
            content: vec![],
            priority: 0,
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
    assert_eq!(display::DESTRUCTIVE_VARIANTS, &["Hide", "HideInline"]);
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
    let preserving: BTreeSet<_> = display::PRESERVING_VARIANTS.iter().copied().collect();
    let destructive: BTreeSet<_> = display::DESTRUCTIVE_VARIANTS.iter().copied().collect();
    let all: BTreeSet<_> = display::ALL_VARIANT_NAMES.iter().copied().collect();

    // Union == ALL
    let union: BTreeSet<_> = preserving
        .iter()
        .chain(destructive.iter())
        .copied()
        .collect();
    assert_eq!(union, all, "PRESERVING ∪ DESTRUCTIVE must equal ALL");

    // Pairwise disjoint
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
        summary: vec![Atom::plain("…")],
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
// Category classification tests
// =========================================================================

#[test]
fn every_variant_has_a_category() {
    for d in make_all_directive_instances() {
        // Just ensure category() doesn't panic and returns a valid value
        let _cat = d.category();
    }
}

#[test]
fn spatial_variants_are_fold_and_hide() {
    for d in make_all_directive_instances() {
        let is_spatial = d.category() == DirectiveCategory::Spatial;
        let is_fold_or_hide = matches!(
            d,
            DisplayDirective::Fold { .. } | DisplayDirective::Hide { .. }
        );
        assert_eq!(
            is_spatial,
            is_fold_or_hide,
            "spatial classification mismatch for {}",
            d.variant_name()
        );
    }
}

#[test]
fn is_spatial_matches_category() {
    for d in make_all_directive_instances() {
        assert_eq!(
            d.is_spatial(),
            d.category() == DirectiveCategory::Spatial,
            "is_spatial() disagrees with category() for {}",
            d.variant_name()
        );
    }
}

// =========================================================================
// InlineBox classification (ADR-031 Phase 10 Step 1)
// =========================================================================

#[test]
fn inline_box_is_inline_category() {
    let d = DisplayDirective::InlineBox {
        line: 0,
        byte_offset: 0,
        width_cells: 1.0,
        height_lines: 1.0,
        box_id: 42,
        alignment: InlineBoxAlignment::Center,
    };
    assert_eq!(d.category(), DirectiveCategory::Inline);
    assert!(!d.is_destructive(), "InlineBox is not destructive");
    assert!(!d.is_spatial(), "InlineBox is not spatial");
}

#[test]
fn inline_box_is_preserving() {
    assert!(
        display::PRESERVING_VARIANTS.contains(&"InlineBox"),
        "InlineBox must be a preserving variant"
    );
    assert!(
        !display::DESTRUCTIVE_VARIANTS.contains(&"InlineBox"),
        "InlineBox must not be a destructive variant"
    );
}

#[test]
fn inline_box_is_safe_constructible() {
    // SafeDisplayDirective must permit InlineBox (it is non-destructive).
    let _safe = SafeDisplayDirective::inline_box(0, 0, 2.0, 1.0, 42, InlineBoxAlignment::Top);
    assert!(SafeDisplayDirective::VARIANT_NAMES.contains(&"InlineBox"));
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
