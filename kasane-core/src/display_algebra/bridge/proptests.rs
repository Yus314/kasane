//! Bridge round-trip proptests (algebra-only).
//!
//! Replaces the legacy-vs-algebra equivalence proptests deleted in
//! ADR-037 Phase 5 (the legacy resolver itself is gone). The new
//! properties witness invariants of the algebra-only bridge:
//!
//! 1. **Round-trip preservation for round-trippable variants**:
//!    `Hide`, `HideInline`, `Fold`, `StyleLine`, `StyleInline`,
//!    `VirtualText`, `InsertInline`, `InlineBox` survive
//!    `directive_to_display` + `display_to_directive` with their
//!    payloads intact (modulo the lossiness documented in the
//!    bridge module's rustdoc — `InlineInteraction`,
//!    `InlineBoxAlignment`, internal priority on
//!    Insert/Gutter/VirtualText).
//! 2. **`resolve_via_algebra` length bound**: output is bounded by
//!    `input.len()` for non-overlapping inputs.
//! 3. **Empty input invariant**: `resolve_via_algebra(empty, _)` is
//!    empty.

use std::collections::HashSet;

use compact_str::CompactString;
use proptest::collection::vec;
use proptest::prelude::*;

use crate::display::{
    DirectiveSet as LegacyDirectiveSet, DisplayDirective, GutterSide, InlineBoxAlignment,
    InlineInteraction, VirtualTextPosition,
};
use crate::element::Element;
use crate::plugin::PluginId;
use crate::protocol::{Atom, WireFace};

use super::{directive_to_display, display_to_directive, resolve_via_algebra};

const LINE_COUNT: usize = 16;
const MAX_LINE: usize = LINE_COUNT - 1;
const MAX_BYTE: usize = 16;

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
        DisplayDirective::Hide { range } => format!("Hide({}..{})", range.start, range.end),
        DisplayDirective::HideInline { line, byte_range } => format!(
            "HideInline({},{}..{})",
            line, byte_range.start, byte_range.end
        ),
        DisplayDirective::Fold { range, .. } => format!("Fold({}..{})", range.start, range.end),
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
// Strategies
// =============================================================================

fn arb_hide() -> impl Strategy<Value = DisplayDirective> {
    (0usize..MAX_LINE)
        .prop_flat_map(|start| (Just(start), (start + 1)..LINE_COUNT))
        .prop_map(|(s, e)| DisplayDirective::Hide { range: s..e })
}

fn arb_hide_inline() -> impl Strategy<Value = DisplayDirective> {
    (0usize..LINE_COUNT, 0usize..MAX_BYTE)
        .prop_flat_map(|(line, start)| (Just(line), Just(start), (start + 1)..(MAX_BYTE + 1)))
        .prop_map(|(line, s, e)| DisplayDirective::HideInline {
            line,
            byte_range: s..e,
        })
}

fn arb_fold() -> impl Strategy<Value = DisplayDirective> {
    (0usize..MAX_LINE)
        .prop_flat_map(|start| (Just(start), (start + 1)..LINE_COUNT))
        .prop_map(|(s, e)| DisplayDirective::Fold {
            range: s..e,
            summary: vec![atom("F")],
        })
}

fn arb_style_line() -> impl Strategy<Value = DisplayDirective> {
    (0usize..LINE_COUNT, -8i16..8).prop_map(|(line, z)| DisplayDirective::StyleLine {
        line,
        face: WireFace::default(),
        z_order: z,
    })
}

fn arb_style_inline() -> impl Strategy<Value = DisplayDirective> {
    (0usize..LINE_COUNT, 0usize..MAX_BYTE)
        .prop_flat_map(|(line, start)| (Just(line), Just(start), (start + 1)..(MAX_BYTE + 1)))
        .prop_map(|(line, s, e)| DisplayDirective::StyleInline {
            line,
            byte_range: s..e,
            face: WireFace::default(),
        })
}

fn arb_virtual_text_eol() -> impl Strategy<Value = DisplayDirective> {
    (0usize..LINE_COUNT, -4i16..4).prop_map(|(line, prio)| DisplayDirective::VirtualText {
        line,
        position: VirtualTextPosition::EndOfLine,
        content: vec![atom(" END")],
        priority: prio,
    })
}

fn arb_insert_inline() -> impl Strategy<Value = DisplayDirective> {
    (0usize..LINE_COUNT, 0usize..MAX_BYTE).prop_map(|(line, byte_offset)| {
        DisplayDirective::InsertInline {
            line,
            byte_offset,
            content: vec![atom("X")],
            interaction: InlineInteraction::None,
        }
    })
}

fn arb_inline_box() -> impl Strategy<Value = DisplayDirective> {
    (0usize..LINE_COUNT, 0usize..MAX_BYTE).prop_map(|(line, byte_offset)| {
        DisplayDirective::InlineBox {
            line,
            byte_offset,
            width_cells: 1.0,
            height_lines: 1.0,
            box_id: 0,
            alignment: InlineBoxAlignment::Center,
        }
    })
}

fn arb_gutter_left() -> impl Strategy<Value = DisplayDirective> {
    (0usize..LINE_COUNT).prop_map(|line| DisplayDirective::Gutter {
        line,
        side: GutterSide::Left,
        content: Element::Empty,
        priority: 0,
    })
}

/// Round-trippable variants — those that survive `directive_to_display`
/// + `display_to_directive` with stable signature shape (lossy
/// payload metadata documented in the bridge module).
fn arb_round_trippable() -> impl Strategy<Value = DisplayDirective> {
    prop_oneof![
        arb_hide(),
        arb_hide_inline(),
        arb_fold(),
        arb_style_line(),
        arb_style_inline(),
        arb_virtual_text_eol(),
        arb_insert_inline(),
        arb_inline_box(),
        arb_gutter_left(),
    ]
}

// =============================================================================
// Properties
// =============================================================================

proptest! {
    #![proptest_config(ProptestConfig::with_cases(64))]

    /// Property 1 — every round-trippable directive survives the
    /// forward + reverse translator with a stable signature.
    #[test]
    fn directive_roundtrips_through_translators(d in arb_round_trippable()) {
        let display = directive_to_display(&d);
        let back = display_to_directive(&display);
        prop_assert!(back.is_some(), "round-trip lost the directive: {:?}", d);
        let signature_in = directive_signature(&d);
        let signature_out = directive_signature(&back.unwrap());
        prop_assert_eq!(signature_in, signature_out);
    }

    /// Property 2 — `resolve_via_algebra` output is bounded by
    /// `input.len()`. This holds because every input directive maps
    /// to at most one normalised leaf (Pass A/B may displace some,
    /// but never duplicate).
    #[test]
    fn resolve_output_bounded_by_input(items in vec(arb_round_trippable(), 0..6)) {
        let mut set = LegacyDirectiveSet::default();
        for (i, d) in items.iter().cloned().enumerate() {
            set.push(d, 0, pid(&format!("p{}", i)));
        }
        let out = resolve_via_algebra(&set, LINE_COUNT);
        prop_assert!(
            out.len() <= items.len(),
            "output has {} directives, input had {}",
            out.len(),
            items.len(),
        );
    }

    /// Property 3 — empty input produces empty output.
    #[test]
    fn empty_input_produces_empty_output(_dummy in any::<u8>()) {
        let set = LegacyDirectiveSet::default();
        let out = resolve_via_algebra(&set, LINE_COUNT);
        prop_assert!(out.is_empty());
    }

    /// Property 4 — independent decorates (different lines) all
    /// survive resolve. L5 says decorates never conflict; this
    /// pins it through the bridge.
    #[test]
    fn disjoint_decorates_all_survive(items in vec(arb_style_inline(), 0..6)) {
        // Filter to one-decorate-per-line so we have no Pass A
        // overlap; the property is about lines not interfering.
        let mut seen_lines: HashSet<usize> = HashSet::new();
        let unique: Vec<_> = items
            .into_iter()
            .filter(|d| match d {
                DisplayDirective::StyleInline { line, .. } => seen_lines.insert(*line),
                _ => false,
            })
            .collect();

        let mut set = LegacyDirectiveSet::default();
        for (i, d) in unique.iter().cloned().enumerate() {
            set.push(d, 0, pid(&format!("p{}", i)));
        }

        let out = resolve_via_algebra(&set, LINE_COUNT);
        let out_sigs = signatures(&out);
        for d in &unique {
            let sig = directive_signature(d);
            prop_assert!(
                out_sigs.contains(&sig),
                "decorate dropped: {sig}; output sigs: {:?}",
                out_sigs
            );
        }
    }

    /// Property 5 — folds with strictly disjoint ranges (gap ≥ 1)
    /// all survive the bridge as Fold directives.
    #[test]
    fn strictly_disjoint_folds_all_survive(items in vec(arb_fold(), 0..4)) {
        let folds: Vec<_> = items.iter().filter_map(|d| match d {
            DisplayDirective::Fold { range, .. } => Some(range.clone()),
            _ => None,
        }).collect();

        // Strict pairwise disjointness with one-line gap.
        for i in 0..folds.len() {
            for j in (i + 1)..folds.len() {
                prop_assume!(folds[i].end < folds[j].start || folds[j].end < folds[i].start);
            }
        }

        let mut set = LegacyDirectiveSet::default();
        for (i, d) in items.iter().cloned().enumerate() {
            set.push(d, 0, pid(&format!("p{}", i)));
        }
        let out = resolve_via_algebra(&set, LINE_COUNT);

        for f in &folds {
            let sig = format!("Fold({}..{})", f.start, f.end);
            let out_sigs = signatures(&out);
            prop_assert!(
                out_sigs.contains(&sig),
                "fold dropped: {sig}; output sigs: {:?}",
                out_sigs
            );
        }
    }
}
