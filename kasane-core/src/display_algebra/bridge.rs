//! Bridge: legacy `display::DisplayDirective` ↔ new `display_algebra::Display`.
//!
//! Existing plugin code, Salsa queries, and `DisplayMap::build` all
//! still speak the legacy 12-variant enum. This module provides the
//! translation layer so the new algebra can be exercised end-to-end
//! through the existing pipeline:
//!
//! ```text
//! plugins ──► DisplayDirective ──► [bridge::directive_to_display]
//!                                          │
//!                                          ▼
//!                            display_algebra::Display
//!                                          │
//!                                          ▼
//!                            display_algebra::normalize
//!                                          │
//!                                          ▼
//!                            [bridge::display_to_directive]
//!                                          │
//!                                          ▼
//!                                  Vec<DisplayDirective> ──► DisplayMap::build
//! ```
//!
//! ### Lossiness
//!
//! Forward translation (legacy → algebra) is lossless except for two
//! pieces of metadata that the new algebra does not yet model:
//!
//! - `DisplayDirective::InsertInline { interaction }` — the
//!   `InlineInteraction` payload (`None | Action(InteractiveId)`) is
//!   dropped. Restoring this requires either an extension to `Content`
//!   or a sidecar map keyed on `(line, byte_offset)`.
//! - `DisplayDirective::InlineBox { alignment }` — the
//!   `InlineBoxAlignment` payload is dropped. The renderer currently
//!   defaults to `Center` when this information is missing.
//!
//! Reverse translation (algebra → legacy) returns `None` for shapes
//! that have no legacy parallel:
//!
//! - `Content::Reference` (cross-buffer inline, ADR-036).
//! - `Content::Editable { spec: EditSpec::Computed { .. } }` — the
//!   bidirectional pair is new in ADR-035 and has no legacy equivalent.
//! - `AnchorPosition::Overlay` — the legacy enum has no overlay variant.
//!
//! The resulting `Vec<DisplayDirective>` is therefore a faithful
//! representation only when the input was itself round-trippable;
//! callers that mix in new shapes must consume `Vec<LineRender>` (via
//! `apply()`) directly.
//!
//! ### Conflict-resolution divergence
//!
//! Legacy `resolve()` has hand-coded rules for fold/hide partial
//! overlap (the fold is dropped). The new algebra treats both as
//! `Replace` and chooses the higher-priority winner with a
//! `MergeConflict` recording the loser. For the common case (no
//! overlapping replaces, or only same-variant overlaps), the two
//! pipelines produce semantically equivalent output. Documented edge
//! cases live in the equivalence tests at the end of this file.

use std::sync::Arc;

use crate::display::{
    DirectiveSet as LegacyDirectiveSet, DisplayDirective, GutterSide, InlineBoxAlignment,
    InlineInteraction, TaggedDirective as LegacyTaggedDirective, VirtualTextPosition,
};

use super::derived;
use super::normalize::{TaggedDisplay, normalize as algebra_normalize};
use super::primitives::{AnchorPosition, Content, Display, EditSpec, Side, Span, Style};

// =============================================================================
// Forward translation: legacy → algebra
// =============================================================================

/// Translate one legacy `DisplayDirective` into a new `Display` tree.
/// Lossless except for the metadata noted in the module docs.
pub fn directive_to_display(d: &DisplayDirective) -> Display {
    match d {
        DisplayDirective::Hide { range } => derived::hide_lines(range.clone()),
        DisplayDirective::HideInline { line, byte_range } => {
            derived::hide_inline(*line, byte_range.clone())
        }
        DisplayDirective::Fold { range, summary } => derived::fold(range.clone(), summary.clone()),
        DisplayDirective::InsertBefore { line, content, .. } => {
            derived::insert_before(*line, content.clone())
        }
        DisplayDirective::InsertAfter { line, content, .. } => {
            derived::insert_after(*line, content.clone())
        }
        DisplayDirective::InsertInline {
            line,
            byte_offset,
            content,
            interaction: _,
        } => derived::insert_inline(*line, *byte_offset, content.clone()),
        DisplayDirective::InlineBox {
            line,
            byte_offset,
            width_cells,
            height_lines,
            box_id,
            alignment: _,
        } => derived::inline_box(*line, *byte_offset, *box_id, *width_cells, *height_lines),
        DisplayDirective::StyleInline {
            line,
            byte_range,
            face,
        } => derived::style_inline(*line, byte_range.clone(), *face, 0),
        DisplayDirective::StyleLine {
            line,
            face,
            z_order,
        } => derived::style_line(*line, *face, *z_order),
        DisplayDirective::Gutter {
            line,
            side,
            content,
            ..
        } => {
            let lane = match side {
                GutterSide::Left => 0,
                GutterSide::Right => 1,
            };
            derived::gutter(*line, lane, content.clone())
        }
        DisplayDirective::VirtualText {
            line,
            position: _,
            content,
            ..
        } => derived::virtual_text_eol(*line, content.clone()),
        DisplayDirective::EditableVirtualText {
            after,
            content,
            editable_spans,
        } => derived::editable_virtual_text(
            *after,
            content.clone(),
            editable_spans.clone(),
            EditSpec::Mirror,
        ),
    }
}

/// Lift a legacy `TaggedDirective` into the new `TaggedDisplay`. Per-
/// directive priority fields (e.g. `InsertBefore { priority }`) are
/// folded into the `seq` so that two directives at the same outer
/// `priority` from the same plugin still get a deterministic order.
pub fn tagged_directive_to_tagged_display(td: &LegacyTaggedDirective, seq: u32) -> TaggedDisplay {
    let inner_priority = inner_priority_of(&td.directive);
    // Combine the inner priority with the supplied seq so multiple
    // directives from one plugin/priority form a stable sequence.
    let combined_seq = seq.wrapping_mul(1_000).wrapping_add(inner_priority as u32);
    TaggedDisplay::new(
        directive_to_display(&td.directive),
        td.priority,
        td.plugin_id.clone(),
        combined_seq,
    )
}

fn inner_priority_of(d: &DisplayDirective) -> u16 {
    match d {
        DisplayDirective::InsertBefore { priority, .. }
        | DisplayDirective::InsertAfter { priority, .. }
        | DisplayDirective::Gutter { priority, .. }
        | DisplayDirective::VirtualText { priority, .. } => {
            // Map signed i16 into u16 monotonically (priority ordering
            // preserved): -32_768 → 0, 0 → 32_768, 32_767 → 65_535.
            (*priority as i32 + 32_768) as u16
        }
        _ => 32_768,
    }
}

// =============================================================================
// Reverse translation: algebra → legacy
// =============================================================================

/// Recover a legacy directive from a normalised algebra leaf, when
/// possible. Returns `None` for shapes the legacy enum cannot
/// represent (see module docs §Lossiness).
pub fn display_to_directive(d: &Display) -> Option<DisplayDirective> {
    match d {
        Display::Replace { range, content } => replace_to_directive(range, content),
        Display::Decorate { range, style } => decorate_to_directive(range, style),
        Display::Anchor { position, content } => anchor_to_directive(position, content),
        Display::Identity | Display::Then(..) | Display::Merge(..) => None,
    }
}

fn replace_to_directive(range: &Span, content: &Content) -> Option<DisplayDirective> {
    // End-of-line sentinel marks a degenerate insertion, not a hide.
    let is_eol_insertion =
        range.byte_range.start == usize::MAX && range.byte_range.end == usize::MAX;
    let is_full_line = range.byte_range.start == 0 && range.byte_range.end == usize::MAX;

    // ADR-037 §1: Content::Fold is the canonical fold representation.
    // Reverse to a single multi-line `DisplayDirective::Fold` carrying
    // the full range and summary directly, regardless of the
    // (full-line / EOL / inline) Span shape.
    if let Content::Fold {
        range: fold_range,
        summary,
    } = content
    {
        return Some(DisplayDirective::Fold {
            range: fold_range.clone(),
            summary: summary.clone(),
        });
    }

    // ADR-037 §6: Content::Hide is the canonical multi-line hide.
    // Reverse to a single `DisplayDirective::Hide` carrying the full
    // range. Per-line emission is no longer used by `derived::hide_lines`,
    // so the legacy `(Empty, full)` arm below is now reached only for
    // single-line full-line hides constructed manually (e.g. by
    // `hide_inline(line, 0..usize::MAX)`).
    if let Content::Hide { range: hide_range } = content {
        return Some(DisplayDirective::Hide {
            range: hide_range.clone(),
        });
    }

    match (content, is_eol_insertion, is_full_line) {
        (Content::Empty, _, true) => Some(DisplayDirective::Hide {
            range: range.line..(range.line + 1),
        }),
        (Content::Empty, _, false) => Some(DisplayDirective::HideInline {
            line: range.line,
            byte_range: range.byte_range.clone(),
        }),
        (Content::Text(atoms), true, _) => Some(DisplayDirective::VirtualText {
            line: range.line,
            position: VirtualTextPosition::EndOfLine,
            content: atoms.clone(),
            priority: 0,
        }),
        (Content::Text(atoms), false, true) => Some(DisplayDirective::Fold {
            range: range.line..(range.line + 1),
            summary: atoms.clone(),
        }),
        (Content::Text(atoms), false, false) => Some(DisplayDirective::InsertInline {
            line: range.line,
            byte_offset: range.byte_range.start,
            content: atoms.clone(),
            interaction: InlineInteraction::None,
        }),
        (
            Content::InlineBox {
                box_id,
                width_cells,
                height_lines,
            },
            _,
            _,
        ) => Some(DisplayDirective::InlineBox {
            line: range.line,
            byte_offset: range.byte_range.start,
            width_cells: *width_cells,
            height_lines: *height_lines,
            box_id: *box_id,
            alignment: InlineBoxAlignment::Center,
        }),
        // Editable / Reference / Element under Replace have no legacy
        // pre-image (Editable is normally Anchored; Reference is
        // ADR-036; Element under Replace is unusual).
        _ => None,
    }
}

fn decorate_to_directive(range: &Span, style: &Style) -> Option<DisplayDirective> {
    let is_full_line = range.byte_range.start == 0 && range.byte_range.end == usize::MAX;
    if is_full_line {
        Some(DisplayDirective::StyleLine {
            line: range.line,
            face: style.face,
            z_order: style.priority,
        })
    } else {
        Some(DisplayDirective::StyleInline {
            line: range.line,
            byte_range: range.byte_range.clone(),
            face: style.face,
        })
    }
}

fn anchor_to_directive(position: &AnchorPosition, content: &Content) -> Option<DisplayDirective> {
    match (position, content) {
        (AnchorPosition::Gutter { line, lane }, Content::Element(el)) => {
            let side = if *lane == 0 {
                GutterSide::Left
            } else {
                GutterSide::Right
            };
            Some(DisplayDirective::Gutter {
                line: *line,
                side,
                content: Arc::unwrap_or_clone(el.clone()),
                priority: 0,
            })
        }
        (
            AnchorPosition::Ornament {
                line,
                side: Side::Before,
            },
            Content::Element(el),
        ) => Some(DisplayDirective::InsertBefore {
            line: *line,
            content: Arc::unwrap_or_clone(el.clone()),
            priority: 0,
        }),
        (
            AnchorPosition::Ornament {
                line,
                side: Side::After,
            },
            Content::Element(el),
        ) => Some(DisplayDirective::InsertAfter {
            line: *line,
            content: Arc::unwrap_or_clone(el.clone()),
            priority: 0,
        }),
        (
            AnchorPosition::Ornament {
                line,
                side: Side::After,
            },
            Content::Editable { atoms, spans, .. },
        ) => Some(DisplayDirective::EditableVirtualText {
            after: *line,
            content: atoms.clone(),
            editable_spans: spans.clone(),
        }),
        // Overlay, Side::Left/Right, and other content shapes have no
        // legacy parallel.
        _ => None,
    }
}

// =============================================================================
// End-to-end pipeline wrapper
// =============================================================================

/// Drop-in companion to legacy `display::resolve::resolve`. All
/// directives now flow through the algebra; the legacy resolver is
/// no longer called from production paths (ADR-037 Phase 3b).
///
/// Pipeline:
///
/// 1. Translate every input directive to a `TaggedDisplay` via the
///    forward translator.
/// 2. `algebra_normalize` resolves Replace conflicts (Pass A: Span
///    overlap; Pass B: Fold/Hide range coverage with Hide-Hide
///    commutativity).
/// 3. `pass_c_filter_evt` applies the EditableVirtualText
///    anchor-invisibility filter against the survivor's Hide/Fold
///    coverage and the buffer's `line_count`.
/// 4. Reverse translate each leaf to a `DisplayDirective`; coalesce
///    redundant leaf shapes for the legacy enum's expected form.
pub fn resolve_via_algebra(set: &LegacyDirectiveSet, line_count: usize) -> Vec<DisplayDirective> {
    if set.is_empty() {
        return Vec::new();
    }

    // Step 1: translate every directive into a TaggedDisplay.
    let tagged: Vec<TaggedDisplay> = set
        .directives
        .iter()
        .enumerate()
        .map(|(i, td)| tagged_directive_to_tagged_display(td, i as u32))
        .collect();

    // Step 2 + 3: normalize, then EVT filter (Pass C).
    let normalized = algebra_normalize(tagged);
    let normalized = crate::display_algebra::pass_c_filter_evt(normalized, line_count);

    // Step 4: reverse translate + coalesce per-line decompositions
    // back into the multi-line legacy enum shape (still used by
    // `derived::hide_inline` for partial-line shapes; multi-line Hide
    // and Fold already use single-leaf Content::Hide / Content::Fold
    // since ADR-037 Phase 1 / §6).
    let raw: Vec<DisplayDirective> = normalized
        .leaves
        .iter()
        .filter_map(|tagged| display_to_directive(&tagged.display))
        .collect();
    coalesce_legacy_directives(raw)
}

/// Re-condense the per-line decomposition of multi-line constructs
/// back into the legacy enum's shape, in a form that satisfies
/// `DisplayMap::build`'s preconditions (no fold/hide overlap, no
/// duplicate or partially-overlapping Hides).
///
/// Algorithm:
///   1. Partition input into `folds`, `hides`, and `others`. Folds
///      from the algebra are always single-line (the multi-line input
///      decomposed into `Replace(summary)` + `Replace(Empty)` runs),
///      so we have at most one Fold per starting line.
///   2. Externally coalesce the Hide ranges into the smallest disjoint
///      union (`coalesce_ranges`), removing duplicates and overlaps.
///   3. For each Fold, extend its range by absorbing every Hide range
///      that touches or is contained in the fold's neighbourhood. The
///      fold becomes a multi-line `Fold(start..end)` and the absorbed
///      Hides are removed from the working set.
///   4. Remove any remaining Hide that is fully contained in a fold's
///      final range (the fold already hides those lines through its
///      own range; an explicit Hide there would trip
///      `DisplayMap::build`'s no-overlap assertion).
///   5. Concatenate folds, surviving hides, and `others` in a stable
///      order.
fn coalesce_legacy_directives(input: Vec<DisplayDirective>) -> Vec<DisplayDirective> {
    let mut folds: Vec<(std::ops::Range<usize>, Vec<crate::protocol::Atom>)> = Vec::new();
    let mut hide_ranges: Vec<std::ops::Range<usize>> = Vec::new();
    let mut others: Vec<DisplayDirective> = Vec::new();

    for d in input {
        match d {
            DisplayDirective::Fold { range, summary } => folds.push((range, summary)),
            DisplayDirective::Hide { range } => hide_ranges.push(range),
            d => others.push(d),
        }
    }

    // Step 2: coalesce hides into a minimal disjoint set.
    let mut hides_disjoint = coalesce_ranges_inplace(hide_ranges);

    // Step 3: extend each fold by absorbing touching/overlapping hides.
    for (fold_range, _) in &mut folds {
        // Absorb any hide whose range *strictly overlaps* the fold —
        // half-open boundaries are NOT absorbed (a Hide(1..3) followed
        // by Fold(3..5) is two separate constructs, not a unified
        // Fold(1..5)). Pass B in `normalize` already rejects in-range
        // hides as conflicts, so by the time we're here, any Hide
        // adjacent to a Fold is a deliberate user intent that must
        // round-trip independently.
        let mut changed = true;
        while changed {
            changed = false;
            hides_disjoint.retain(|h| {
                let strict_overlap = h.start < fold_range.end && fold_range.start < h.end;
                if strict_overlap {
                    if h.start < fold_range.start {
                        fold_range.start = h.start;
                    }
                    if h.end > fold_range.end {
                        fold_range.end = h.end;
                    }
                    changed = true;
                    false
                } else {
                    true
                }
            });
        }
    }

    // Step 4: drop hides fully contained in any fold (defensive — the
    // step-3 pass should have absorbed them, but explicit guarding
    // makes the precondition for `DisplayMap::build` easy to read).
    hides_disjoint.retain(|h| {
        !folds
            .iter()
            .any(|(f, _)| f.start <= h.start && h.end <= f.end)
    });

    // Step 5: assemble. Folds first, then hides, then others.
    let mut out = Vec::with_capacity(folds.len() + hides_disjoint.len() + others.len());
    for (range, summary) in folds {
        out.push(DisplayDirective::Fold { range, summary });
    }
    for range in hides_disjoint {
        out.push(DisplayDirective::Hide { range });
    }
    out.extend(others);
    out
}

/// Coalesce a `Vec<Range<usize>>` into the smallest disjoint union.
/// Equivalent to the test helper `coalesce_ranges` in
/// `bridge/proptests.rs`; duplicated here so the bridge is self-
/// contained (no `cfg(test)` dependency).
fn coalesce_ranges_inplace(mut ranges: Vec<std::ops::Range<usize>>) -> Vec<std::ops::Range<usize>> {
    ranges.sort_by_key(|r| r.start);
    let mut out: Vec<std::ops::Range<usize>> = Vec::with_capacity(ranges.len());
    for r in ranges {
        match out.last_mut() {
            Some(prev) if prev.end >= r.start => {
                if r.end > prev.end {
                    prev.end = r.end;
                }
            }
            _ => out.push(r),
        }
    }
    out
}

#[cfg(test)]
mod tests;
