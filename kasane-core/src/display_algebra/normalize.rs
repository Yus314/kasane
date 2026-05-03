//! Normalisation: flatten a `Display` tree into a deterministic list
//! of leaf primitives, reporting `MergeConflict`s where overlapping
//! `Replace`s cannot coexist (ADR-034 §5).
//!
//! The normalised form preserves enough information that a downstream
//! `DisplayMap` projection can compute per-line coordinate compression
//! without re-walking composition operators.

use std::cmp::Ordering;

use crate::plugin::PluginId;

use super::primitives::{AnchorPosition, Content, Display, Side, Span};

/// A leaf-level `Display` tagged with its emitting plugin and
/// resolution priority. The tag is the unit at which conflicts
/// resolve (L6 Replace-conflict-determinism).
#[derive(Debug, Clone, PartialEq)]
pub struct TaggedDisplay {
    pub display: Display,
    pub priority: i16,
    pub plugin_id: PluginId,
    /// Per-plugin sequence number, set when this leaf was emitted
    /// during a single plugin invocation. Breaks ties when two leaves
    /// share `(priority, plugin_id)`.
    pub seq: u32,
}

impl TaggedDisplay {
    pub fn new(display: Display, priority: i16, plugin_id: PluginId, seq: u32) -> Self {
        Self {
            display,
            priority,
            plugin_id,
            seq,
        }
    }

    /// Total order used by L6 for deterministic conflict resolution.
    /// Higher priority wins; ties broken by plugin id then sequence.
    fn cmp_key(&self) -> (i16, &PluginId, u32) {
        (self.priority, &self.plugin_id, self.seq)
    }
}

/// A `Replace` directive that lost a conflict, kept around so that
/// recovery handlers can reconstruct what was suppressed (ADR-030
/// Level 6 transparency).
#[derive(Debug, Clone, PartialEq)]
pub struct MergeConflict {
    pub winner: TaggedDisplay,
    pub displaced: Vec<TaggedDisplay>,
}

/// Output of `normalize`: the surviving leaves in a stable order plus
/// any merge conflicts. Decorates do not produce conflicts (L5); they
/// stack by priority, encoded in the leaf order.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct NormalizedDisplay {
    pub leaves: Vec<TaggedDisplay>,
    pub conflicts: Vec<MergeConflict>,
}

impl NormalizedDisplay {
    pub fn is_empty(&self) -> bool {
        self.leaves.is_empty() && self.conflicts.is_empty()
    }
}

/// Flatten and resolve a list of tagged displays.
///
/// Each input `TaggedDisplay`'s `Display` may itself be a tree
/// (`Then` / `Merge`); we walk the tree, lifting each leaf with the
/// inherited tag. Then we collapse conflicts.
pub fn normalize(inputs: Vec<TaggedDisplay>) -> NormalizedDisplay {
    if inputs.is_empty() {
        return NormalizedDisplay::default();
    }

    // Step 1: flatten composition operators into a leaf list.
    let mut leaves: Vec<TaggedDisplay> = Vec::with_capacity(inputs.len());
    for tagged in inputs {
        let TaggedDisplay {
            display,
            priority,
            plugin_id,
            seq,
        } = tagged;
        flatten(display, priority, &plugin_id, seq, &mut leaves);
    }

    // Step 2: total-order sort. Primary key is L6's `(priority,
    // plugin_id, seq)`; positional tertiary key (span line then start
    // offset) makes the order canonical even when two callers emit
    // leaves with identical tags from different composition trees
    // (e.g. `merge(a, b)` vs `merge(b, a)` with disjoint supports).
    // Anchors are positioned by their AnchorPosition; we lift each
    // variant into a comparable shape.
    leaves.sort_by(|a, b| {
        let ka = a.cmp_key();
        let kb = b.cmp_key();
        ka.0.cmp(&kb.0)
            .then_with(|| ka.1.cmp(kb.1))
            .then_with(|| ka.2.cmp(&kb.2))
            .then_with(|| position_key(&a.display).cmp(&position_key(&b.display)))
    });

    // Step 3: detect Replace conflicts. Two `Replace`s conflict when
    // their spans overlap (Span::overlaps). Higher cmp_key wins; the
    // rest are recorded as `displaced`.
    //
    // `Decorate` overlaps never conflict (L5): they all survive,
    // ordered by priority — the renderer stacks them.
    // `Anchor` never conflicts: anchors live in non-text positions.
    let mut conflicts: Vec<MergeConflict> = Vec::new();
    let mut survivors: Vec<TaggedDisplay> = Vec::with_capacity(leaves.len());

    for leaf in leaves.into_iter() {
        match &leaf.display {
            Display::Replace { range, content } => {
                let conflict_idx = survivors
                    .iter()
                    .enumerate()
                    .find(|(_, surv)| match &surv.display {
                        Display::Replace {
                            range: surv_range,
                            content: surv_content,
                        } => replace_conflicts((surv_range, surv_content), (range, content)),
                        _ => false,
                    })
                    .map(|(i, _)| i);

                match conflict_idx {
                    None => survivors.push(leaf),
                    Some(idx) => {
                        // Compare keys: higher wins. Because we sorted
                        // ascending and iterate front-to-back, the
                        // already-placed `survivor` may have a lower or
                        // higher key than the incoming `leaf` — both are
                        // possible after stable sort with ties.
                        let surv_key = survivors[idx].cmp_key();
                        let leaf_key = leaf.cmp_key();
                        let leaf_wins = match leaf_key.0.cmp(&surv_key.0) {
                            Ordering::Greater => true,
                            Ordering::Less => false,
                            Ordering::Equal => match leaf_key.1.cmp(surv_key.1) {
                                Ordering::Greater => true,
                                Ordering::Less => false,
                                Ordering::Equal => leaf_key.2 > surv_key.2,
                            },
                        };

                        if leaf_wins {
                            let displaced = survivors.remove(idx);
                            // Try to merge into an existing conflict
                            // record for the same winner; otherwise
                            // create a new one. We keyed conflicts by
                            // span identity at the leaf level, which is
                            // sufficient because `Replace` carries its
                            // own span.
                            record_conflict(&mut conflicts, leaf.clone(), displaced);
                            survivors.push(leaf);
                        } else {
                            record_conflict(&mut conflicts, survivors[idx].clone(), leaf);
                        }
                    }
                }
            }
            _ => survivors.push(leaf),
        }
    }

    NormalizedDisplay {
        leaves: survivors,
        conflicts,
    }
}

fn flatten(
    display: Display,
    priority: i16,
    plugin_id: &PluginId,
    seq: u32,
    out: &mut Vec<TaggedDisplay>,
) {
    match display {
        Display::Identity => {}
        Display::Then(a, b) | Display::Merge(a, b) => {
            // Both Then and Merge contribute their leaves to the same
            // pool at this resolution level. The distinction matters
            // for *evaluation order* against a buffer (Then must apply
            // sequentially), but at normalisation time we are resolving
            // *conflicts* between leaves; sequential vs parallel does
            // not change which leaves survive — only the buffer-walk
            // sees the difference. (See `apply.rs`, future work.)
            flatten(*a, priority, plugin_id, seq, out);
            flatten(*b, priority, plugin_id, seq, out);
        }
        leaf @ (Display::Replace { .. } | Display::Decorate { .. } | Display::Anchor { .. }) => {
            out.push(TaggedDisplay {
                display: leaf,
                priority,
                plugin_id: plugin_id.clone(),
                seq,
            });
        }
    }
}

fn record_conflict(
    conflicts: &mut Vec<MergeConflict>,
    winner: TaggedDisplay,
    displaced: TaggedDisplay,
) {
    let winner_span = match &winner.display {
        Display::Replace { range, .. } => range.clone(),
        _ => unreachable!("record_conflict is only called for Replace conflicts"),
    };

    if let Some(existing) = conflicts.iter_mut().find(|c| {
        matches!(&c.winner.display,
            Display::Replace { range, .. } if *range == winner_span)
    }) {
        existing.displaced.push(displaced);
    } else {
        conflicts.push(MergeConflict {
            winner,
            displaced: vec![displaced],
        });
    }
}

/// ADR-037 Pass B — extended conflict detection between two `Replace`
/// leaves.
///
/// Pass A (the `Span::overlaps` check) handles single-line spatial
/// overlap. Pass B extends this to multi-line `Content::Fold` and
/// `Content::Hide` coverage: each "claims" every line in its `range`,
/// not just the anchor line that its `Span` references.
///
/// **Hide-Hide commutativity.** Two overlapping `Content::Hide` leaves
/// are explicitly *not* in conflict — Hide is set-union idempotent, so
/// two plugins both asking to hide overlapping lines compose into the
/// union of their ranges (matching legacy `hidden_set` semantics).
/// The downstream consumer (`DisplayMap::build` or its successor)
/// treats multiple Hide directives as a union.
///
/// The conflict is symmetric. Resolution (which leaf wins, when there
/// is one) is dispatched by the standard L6 total order — this helper
/// only identifies *whether* a conflict exists.
fn replace_conflicts(a: (&Span, &Content), b: (&Span, &Content)) -> bool {
    // Hide-Hide: commutative; never in conflict regardless of overlap.
    let both_hide = matches!((a.1, b.1), (Content::Hide { .. }, Content::Hide { .. }));
    if both_hide {
        return false;
    }

    // Pass A: per-line Span overlap (existing semantics).
    if a.0.overlaps(b.0) {
        return true;
    }
    // Pass B: range coverage cross-check (Fold and Hide).
    if let Some(range_a) = content_range(a.1)
        && range_a.contains(&b.0.line)
    {
        return true;
    }
    if let Some(range_b) = content_range(b.1)
        && range_b.contains(&a.0.line)
    {
        return true;
    }
    false
}

/// Extract the multi-line range payload from a `Content`, if any.
/// `Content::Fold` and `Content::Hide` carry such ranges; everything
/// else is line-local (covers only the carrying `Span`).
fn content_range(c: &Content) -> Option<&std::ops::Range<usize>> {
    match c {
        Content::Fold { range, .. } => Some(range),
        Content::Hide { range, .. } => Some(range),
        _ => None,
    }
}

/// Canonical positional key used as the tertiary sort criterion in
/// `normalize`. The lexicographic order is `(line, byte_start, variant_tag)`,
/// giving a total order that depends only on the leaf's own position —
/// independent of the composition tree it came from.
fn position_key(d: &Display) -> (usize, usize, u8) {
    match d {
        Display::Replace { range, .. } => (range.line, range.byte_range.start, 0),
        Display::Decorate { range, .. } => (range.line, range.byte_range.start, 1),
        Display::Anchor { position, .. } => match position {
            AnchorPosition::Gutter { line, lane } => (*line, *lane as usize, 2),
            AnchorPosition::Ornament { line, side } => (*line, *side as usize, 3),
            AnchorPosition::Overlay { rect } => (rect.line, rect.column, 4),
        },
        // Identity / Then / Merge are stripped before this function
        // is called (flatten produces only leaf variants); fall back
        // to a max-key so a logic bug surfaces rather than silently
        // sorting them to the front.
        _ => (usize::MAX, usize::MAX, u8::MAX),
    }
}

/// ADR-037 Pass C — EditableVirtualText anchor-invisibility filter.
///
/// Mirrors legacy `display::resolve` Rules 8-10: drops EVT leaves that
///   1. anchor at or beyond `line_count` (out of buffer bounds),
///   2. anchor on a line covered by a surviving `Content::Hide` or
///      `Content::Fold` leaf, or
///   3. duplicate another EVT at the same anchor line (dedup).
///
/// EVT leaves live in the algebra as
/// `Display::Anchor { position: AnchorPosition::Ornament { side:
/// Side::After }, content: Content::Editable { .. } }` — exactly the
/// shape `derived::editable_virtual_text` produces. Other Anchor
/// variants (Gutter, Overlay, non-Editable Ornament) are passed
/// through unchanged.
///
/// Rule 3 dedup uses **legacy compatibility**: ascending-priority
/// retain-first ⇒ the *lowest* priority wins on same-anchor
/// collisions, matching the existing `display::resolve` behaviour.
/// (The legacy code's comment claims "highest" but the implementation
/// — sort ASC + retain-first — keeps the lowest. Pass C preserves the
/// observed behaviour, not the misleading comment.)
pub fn pass_c_filter_evt(normalized: NormalizedDisplay, line_count: usize) -> NormalizedDisplay {
    use std::collections::HashMap;

    // Step 1: build the invisible-line set from surviving Hide and
    // Fold leaves.
    let mut invisible: std::collections::HashSet<usize> = std::collections::HashSet::new();
    for leaf in &normalized.leaves {
        if let Display::Replace {
            content: Content::Hide { range } | Content::Fold { range, .. },
            ..
        } = &leaf.display
        {
            for line in range.clone() {
                invisible.insert(line);
            }
        }
    }

    // Step 2: partition leaves into EVT vs others.
    let mut evts: Vec<TaggedDisplay> = Vec::new();
    let mut others: Vec<TaggedDisplay> = Vec::with_capacity(normalized.leaves.len());
    for leaf in normalized.leaves {
        if is_evt_leaf(&leaf.display) {
            evts.push(leaf);
        } else {
            others.push(leaf);
        }
    }

    // Step 3: drop out-of-bounds and invisible-anchor EVTs.
    evts.retain(|leaf| {
        let line = evt_anchor_line(&leaf.display).expect("partition keeps EVT-shaped leaves");
        line < line_count && !invisible.contains(&line)
    });

    // Step 4: same-anchor dedup. Sort ascending by cmp_key (matches
    // legacy `priority.cmp(...).then plugin_id.cmp(...)`); retain
    // first occurrence per anchor — the lowest-priority survivor.
    evts.sort_by(|a, b| {
        let ka = a.cmp_key();
        let kb = b.cmp_key();
        ka.0.cmp(&kb.0)
            .then_with(|| ka.1.cmp(kb.1))
            .then_with(|| ka.2.cmp(&kb.2))
    });
    let mut seen_anchors: HashMap<usize, ()> = HashMap::new();
    evts.retain(|leaf| {
        let line = evt_anchor_line(&leaf.display).unwrap();
        seen_anchors.insert(line, ()).is_none()
    });

    // Step 5: combine.
    others.extend(evts);

    NormalizedDisplay {
        leaves: others,
        conflicts: normalized.conflicts,
    }
}

fn is_evt_leaf(d: &Display) -> bool {
    matches!(
        d,
        Display::Anchor {
            position: AnchorPosition::Ornament {
                side: Side::After,
                ..
            },
            content: Content::Editable { .. },
        }
    )
}

fn evt_anchor_line(d: &Display) -> Option<usize> {
    match d {
        Display::Anchor {
            position: AnchorPosition::Ornament { line, .. },
            ..
        } => Some(*line),
        _ => None,
    }
}

/// Convenience: a single `Span` (via `support`) supports L4 disjointness checks.
/// Two displays are disjoint iff every span of `a` is non-overlapping with
/// every span of `b`.
pub fn disjoint(a: &Display, b: &Display) -> bool {
    let sa = a.support();
    let sb = b.support();
    for s_a in &sa {
        for s_b in &sb {
            if s_a.overlaps(s_b) {
                return false;
            }
        }
    }
    true
}
