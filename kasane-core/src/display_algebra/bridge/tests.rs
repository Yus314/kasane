//! Bridge round-trip tests.
//!
//! Validates that `resolve_via_algebra` produces well-formed output
//! for representative inputs across all 12 directive variants. Now
//! that legacy `display::resolve` is deleted (ADR-037 Phase 5),
//! these tests focus on the algebra's own contract — round-trip
//! shapes, conflict-resolution outcomes, hybrid-bridge invariants
//! — without comparing against a legacy reference.

use std::collections::HashSet;

use compact_str::CompactString;

use crate::display::{
    DirectiveSet as LegacyDirectiveSet, DisplayDirective, GutterSide, InlineInteraction,
    VirtualTextPosition,
};
use crate::element::Element;
use crate::plugin::PluginId;
use crate::protocol::{Atom, WireFace};

use super::resolve_via_algebra;

fn pid(s: &str) -> PluginId {
    PluginId(s.to_string())
}

fn atom(s: &str) -> Atom {
    Atom::with_style(CompactString::from(s), crate::protocol::Style::default())
}

fn signatures(out: &[DisplayDirective]) -> HashSet<String> {
    out.iter().map(directive_signature).collect()
}

fn directive_signature(d: &DisplayDirective) -> String {
    match d {
        DisplayDirective::Hide { range } => {
            format!("Hide({}..{})", range.start, range.end)
        }
        DisplayDirective::HideInline { line, byte_range } => {
            format!(
                "HideInline({},{}..{})",
                line, byte_range.start, byte_range.end
            )
        }
        DisplayDirective::Fold { range, .. } => {
            format!("Fold({}..{})", range.start, range.end)
        }
        DisplayDirective::InsertBefore { line, .. } => format!("InsertBefore({})", line),
        DisplayDirective::InsertAfter { line, .. } => format!("InsertAfter({})", line),
        DisplayDirective::InsertInline {
            line, byte_offset, ..
        } => format!("InsertInline({},{})", line, byte_offset),
        DisplayDirective::StyleInline {
            line, byte_range, ..
        } => format!(
            "StyleInline({},{}..{})",
            line, byte_range.start, byte_range.end
        ),
        DisplayDirective::InlineBox {
            line, byte_offset, ..
        } => format!("InlineBox({},{})", line, byte_offset),
        DisplayDirective::StyleLine { line, .. } => format!("StyleLine({})", line),
        DisplayDirective::Gutter { line, side, .. } => format!("Gutter({},{:?})", line, side),
        DisplayDirective::VirtualText { line, position, .. } => {
            format!("VirtualText({},{:?})", line, position)
        }
        DisplayDirective::EditableVirtualText { after, .. } => {
            format!("EditableVirtualText({})", after)
        }
    }
}

// =============================================================================
// Empty / single-directive round-trip
// =============================================================================

#[test]
fn empty_set_returns_empty() {
    let set = LegacyDirectiveSet::default();
    assert!(resolve_via_algebra(&set, 10).is_empty());
}

#[test]
fn single_hide_round_trips() {
    let mut set = LegacyDirectiveSet::default();
    set.push(DisplayDirective::Hide { range: 2..3 }, 0, pid("p"));
    let new = resolve_via_algebra(&set, 10);
    assert_eq!(new.len(), 1);
    assert!(matches!(&new[0], DisplayDirective::Hide { range } if *range == (2..3)));
}

#[test]
fn single_hide_inline_round_trips() {
    let mut set = LegacyDirectiveSet::default();
    set.push(
        DisplayDirective::HideInline {
            line: 0,
            byte_range: 2..5,
        },
        0,
        pid("p"),
    );
    let new = resolve_via_algebra(&set, 10);
    assert_eq!(new.len(), 1);
    assert!(matches!(
        &new[0],
        DisplayDirective::HideInline {
            line: 0,
            byte_range,
        } if byte_range == &(2..5)
    ));
}

#[test]
fn single_fold_round_trips() {
    let mut set = LegacyDirectiveSet::default();
    set.push(
        DisplayDirective::Fold {
            range: 2..5,
            summary: vec![atom("// folded")],
        },
        0,
        pid("p"),
    );
    let new = resolve_via_algebra(&set, 10);
    assert_eq!(new.len(), 1);
    assert!(matches!(&new[0], DisplayDirective::Fold { range, .. } if *range == (2..5)));
}

#[test]
fn single_style_line_round_trips() {
    let mut set = LegacyDirectiveSet::default();
    set.push(
        DisplayDirective::StyleLine {
            line: 3,
            face: WireFace::default(),
            z_order: 0,
        },
        0,
        pid("p"),
    );
    let new = resolve_via_algebra(&set, 10);
    assert_eq!(new.len(), 1);
    assert!(matches!(
        &new[0],
        DisplayDirective::StyleLine { line: 3, .. }
    ));
}

#[test]
fn single_gutter_left_round_trips() {
    let mut set = LegacyDirectiveSet::default();
    set.push(
        DisplayDirective::Gutter {
            line: 0,
            side: GutterSide::Left,
            content: Element::Empty,
            priority: 0,
        },
        0,
        pid("p"),
    );
    let new = resolve_via_algebra(&set, 10);
    assert_eq!(new.len(), 1);
    assert!(matches!(
        &new[0],
        DisplayDirective::Gutter {
            side: GutterSide::Left,
            ..
        }
    ));
}

#[test]
fn single_insert_before_round_trips() {
    let mut set = LegacyDirectiveSet::default();
    set.push(
        DisplayDirective::InsertBefore {
            line: 5,
            content: Element::Empty,
            priority: 0,
        },
        0,
        pid("p"),
    );
    let new = resolve_via_algebra(&set, 10);
    assert_eq!(new.len(), 1);
    assert!(matches!(
        &new[0],
        DisplayDirective::InsertBefore { line: 5, .. }
    ));
}

#[test]
fn single_virtual_text_eol_round_trips() {
    let mut set = LegacyDirectiveSet::default();
    set.push(
        DisplayDirective::VirtualText {
            line: 0,
            position: VirtualTextPosition::EndOfLine,
            content: vec![atom(" END")],
            priority: 0,
        },
        0,
        pid("p"),
    );
    let new = resolve_via_algebra(&set, 10);
    assert_eq!(new.len(), 1);
    assert!(matches!(
        &new[0],
        DisplayDirective::VirtualText {
            position: VirtualTextPosition::EndOfLine,
            ..
        }
    ));
}

// =============================================================================
// Multi-directive scenarios
// =============================================================================

#[test]
fn multiple_disjoint_hides_all_survive() {
    let mut set = LegacyDirectiveSet::default();
    set.push(DisplayDirective::Hide { range: 0..1 }, 0, pid("p"));
    set.push(DisplayDirective::Hide { range: 3..4 }, 0, pid("p"));
    set.push(DisplayDirective::Hide { range: 6..7 }, 0, pid("p"));
    let new = resolve_via_algebra(&set, 10);
    assert_eq!(new.len(), 3);
}

#[test]
fn mixed_hide_and_decorate_both_survive() {
    let mut set = LegacyDirectiveSet::default();
    set.push(DisplayDirective::Hide { range: 0..1 }, 0, pid("p"));
    set.push(
        DisplayDirective::StyleLine {
            line: 5,
            face: WireFace::default(),
            z_order: 0,
        },
        0,
        pid("q"),
    );
    let new = resolve_via_algebra(&set, 10);
    let new_sigs = signatures(&new);
    assert!(new_sigs.contains("Hide(0..1)"));
    assert!(new_sigs.contains("StyleLine(5)"));
}

#[test]
fn many_decorates_on_same_range_all_survive() {
    let mut set = LegacyDirectiveSet::default();
    for i in 0..5 {
        set.push(
            DisplayDirective::StyleInline {
                line: 0,
                byte_range: 0..10,
                face: WireFace::default(),
            },
            i,
            pid(&format!("p{}", i)),
        );
    }
    let new = resolve_via_algebra(&set, 10);
    assert_eq!(new.len(), 5, "L5: decorates never conflict");
}

#[test]
fn higher_priority_replace_wins_over_lower() {
    let mut set = LegacyDirectiveSet::default();
    set.push(
        DisplayDirective::HideInline {
            line: 0,
            byte_range: 0..5,
        },
        1,
        pid("low"),
    );
    set.push(
        DisplayDirective::InsertInline {
            line: 0,
            byte_offset: 3,
            content: vec![atom("X")],
            interaction: InlineInteraction::None,
        },
        5,
        pid("high"),
    );
    let new = resolve_via_algebra(&set, 10);
    assert_eq!(new.len(), 1);
    assert!(matches!(&new[0], DisplayDirective::InsertInline { .. }));
}

// =============================================================================
// EVT anchor-invisibility (Pass C invariants exercised through bridge)
// =============================================================================

#[test]
fn evt_on_hidden_line_is_dropped_via_bridge() {
    let mut set = LegacyDirectiveSet::default();
    set.push(DisplayDirective::Hide { range: 2..5 }, 0, pid("h"));
    set.push(
        DisplayDirective::EditableVirtualText {
            after: 3,
            content: vec![atom("e")],
            editable_spans: vec![],
        },
        0,
        pid("e"),
    );
    let new = resolve_via_algebra(&set, 10);
    assert!(
        !new.iter()
            .any(|d| matches!(d, DisplayDirective::EditableVirtualText { .. })),
        "EVT anchored at hidden line 3 must not survive Pass C",
    );
}

#[test]
fn evt_on_visible_line_survives_via_bridge() {
    let mut set = LegacyDirectiveSet::default();
    set.push(DisplayDirective::Hide { range: 0..2 }, 0, pid("h"));
    set.push(
        DisplayDirective::EditableVirtualText {
            after: 5,
            content: vec![atom("e")],
            editable_spans: vec![],
        },
        0,
        pid("e"),
    );
    let new = resolve_via_algebra(&set, 10);
    assert!(
        new.iter()
            .any(|d| matches!(d, DisplayDirective::EditableVirtualText { after: 5, .. })),
        "EVT at line 5 (outside Hide range) must survive",
    );
}
