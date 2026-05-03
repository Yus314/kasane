//! Apply: project a `NormalizedDisplay` against a buffer to produce a
//! per-line render plan (ADR-034 §5 follow-up).
//!
//! `normalize` produces a deterministic list of conflict-resolved
//! leaves; `apply` walks that list against the actual buffer text to
//! produce `LineRender` entries — the unit a downstream `DisplayMap`
//! projection (or the renderer directly) consumes.
//!
//! This is the *bridge* between the abstract algebra and the concrete
//! per-line cell-grid that the TUI / GPU backends present. A future
//! step rewires the existing `DisplayMap::build` to consume
//! `Vec<LineRender>` instead of `Vec<DisplayDirective>`.

use std::ops::Range;

use crate::protocol::Atom;

use super::normalize::{NormalizedDisplay, TaggedDisplay};
use super::primitives::{AnchorPosition, Content, Display, Side, Span, Style};

/// Per-buffer-line render plan.
///
/// The structure is lossy w.r.t. the algebra — `Then` vs `Merge` and
/// the exact tree shape are gone — but it carries everything the
/// renderer needs: replaced byte ranges, decorate stacks, and anchored
/// content (gutters, ornaments, overlays). Adjacent-line ornaments
/// (`Side::Before` / `Side::After`) are emitted on their own
/// `LineRender` with `BufferLine::Virtual` so the consumer can splice
/// them into the display-line stream without re-walking the algebra.
#[derive(Debug, Clone, PartialEq)]
pub struct LineRender {
    pub line: BufferLine,
    /// Byte ranges that get replaced (in source-line byte coordinates).
    /// `Empty` content here means "hide". The renderer walks these
    /// in start-byte order to produce the final atom stream.
    pub replacements: Vec<Replacement>,
    /// Decorate ranges, ordered by application priority (lower first;
    /// the renderer applies in order so higher-priority styles end up
    /// on top).
    pub decorations: Vec<Decoration>,
    /// Gutter / ornament / overlay anchored content for this line.
    pub anchors: Vec<Anchor>,
}

/// A buffer line, possibly virtual (for ornaments that produce extra
/// display lines without a corresponding source line).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum BufferLine {
    /// A real source line at index `0..buffer_line_count`.
    Real(usize),
    /// A virtual line slotted before / after a real line.
    Virtual {
        host_line: usize,
        side: Side,
        order: u32,
    },
}

#[derive(Debug, Clone, PartialEq)]
pub struct Replacement {
    pub byte_range: Range<usize>,
    pub content: Content,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Decoration {
    pub byte_range: Range<usize>,
    pub style: Style,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Anchor {
    pub position: AnchorPosition,
    pub content: Content,
}

/// Apply a normalised display against a buffer of `line_lengths`,
/// producing one `LineRender` per affected real line plus zero-or-more
/// virtual lines for `Side::Before` / `Side::After` ornaments.
///
/// `line_lengths[i]` is the byte length of source line `i`. This is
/// the only information `apply` needs about the buffer's content; the
/// actual text is irrelevant at this stage (only ranges are projected).
/// The `usize::MAX` sentinel from `Span::end_of_line` is mapped to
/// `line_lengths[line]` here.
pub fn apply(normalized: &NormalizedDisplay, line_lengths: &[usize]) -> Vec<LineRender> {
    let mut by_line: std::collections::BTreeMap<BufferLine, LineRender> =
        std::collections::BTreeMap::new();

    for tagged in &normalized.leaves {
        match &tagged.display {
            Display::Replace { range, content } => {
                let line = real_line(range.line, line_lengths);
                let entry = by_line.entry(line).or_insert_with(|| empty_render(line));
                entry.replacements.push(Replacement {
                    byte_range: resolve_range(range, line_lengths),
                    content: content.clone(),
                });
            }
            Display::Decorate { range, style } => {
                let line = real_line(range.line, line_lengths);
                let entry = by_line.entry(line).or_insert_with(|| empty_render(line));
                entry.decorations.push(Decoration {
                    byte_range: resolve_range(range, line_lengths),
                    style: style.clone(),
                });
            }
            Display::Anchor { position, content } => {
                let host_line = match position {
                    AnchorPosition::Gutter { line, .. } | AnchorPosition::Ornament { line, .. } => {
                        *line
                    }
                    AnchorPosition::Overlay { rect } => rect.line,
                };
                let line_key = match position {
                    AnchorPosition::Ornament { line, side } => BufferLine::Virtual {
                        host_line: *line,
                        side: *side,
                        order: virtual_order_for(side, tagged),
                    },
                    _ => BufferLine::Real(host_line),
                };
                let entry = by_line
                    .entry(line_key)
                    .or_insert_with(|| empty_render(line_key));
                entry.anchors.push(Anchor {
                    position: position.clone(),
                    content: content.clone(),
                });
            }
            // `flatten` (in `normalize.rs`) strips Identity / Then /
            // Merge before tagging leaves; reaching them here is a bug.
            Display::Identity | Display::Then(..) | Display::Merge(..) => {
                debug_assert!(false, "non-leaf reached apply: {:?}", tagged.display);
            }
        }
    }

    // Sort within each line for deterministic renderer input.
    for render in by_line.values_mut() {
        render.replacements.sort_by_key(|r| r.byte_range.start);
        // Decorations are pre-ordered by `normalize` (priority); preserve.
        // Anchors: stable order from leaf order (sort_by_key preserves on tie).
        render
            .anchors
            .sort_by_key(|a| anchor_position_key(&a.position));
    }

    by_line.into_values().collect()
}

fn empty_render(line: BufferLine) -> LineRender {
    LineRender {
        line,
        replacements: Vec::new(),
        decorations: Vec::new(),
        anchors: Vec::new(),
    }
}

fn real_line(line: usize, line_lengths: &[usize]) -> BufferLine {
    // We allow lines past EOF for plugins that emit ornaments off the
    // end of the buffer — the renderer treats them as virtual.
    let _ = line_lengths;
    BufferLine::Real(line)
}

fn resolve_range(range: &Span, line_lengths: &[usize]) -> Range<usize> {
    let line_len = line_lengths.get(range.line).copied().unwrap_or(0);
    let start = if range.byte_range.start == usize::MAX {
        line_len
    } else {
        range.byte_range.start.min(line_len)
    };
    let end = if range.byte_range.end == usize::MAX {
        line_len
    } else {
        range.byte_range.end.min(line_len)
    };
    start..end
}

fn virtual_order_for(side: &Side, tagged: &TaggedDisplay) -> u32 {
    // Order virtual lines deterministically by side then sequence.
    // Before-lines accumulate from low to high; after-lines likewise.
    let _ = side;
    tagged.seq
}

fn anchor_position_key(pos: &AnchorPosition) -> u32 {
    match pos {
        AnchorPosition::Gutter { lane, .. } => 1_000 + *lane as u32,
        AnchorPosition::Ornament { side, .. } => match side {
            Side::Before => 100,
            Side::After => 200,
            Side::Left => 300,
            Side::Right => 400,
        },
        AnchorPosition::Overlay { rect } => 10_000 + rect.column as u32,
    }
}

/// Convenience accessor: extract atoms from a `Replacement`'s content
/// when present. Useful for renderer consumers that primarily care
/// about text payloads.
pub fn replacement_atoms(rep: &Replacement) -> &[Atom] {
    match &rep.content {
        Content::Text(atoms) => atoms.as_slice(),
        Content::Editable { atoms, .. } => atoms.as_slice(),
        Content::Fold { summary, .. } => summary.as_slice(),
        Content::Empty
        | Content::Hide { .. }
        | Content::InlineBox { .. }
        | Content::Reference(_)
        | Content::Element(_) => &[],
    }
}

#[cfg(test)]
mod tests {
    use compact_str::CompactString;

    use crate::plugin::PluginId;
    use crate::protocol::{Atom, WireFace};

    use super::super::derived::{fold, hide_inline, style_inline};
    use super::super::normalize::{TaggedDisplay, normalize};
    use super::super::primitives::{Content, Display, Side, Span};
    use super::*;

    fn pid(s: &str) -> PluginId {
        PluginId(s.to_string())
    }

    fn t(d: Display, prio: i16, plugin: &str, seq: u32) -> TaggedDisplay {
        TaggedDisplay::new(d, prio, pid(plugin), seq)
    }

    fn atom(s: &str) -> Atom {
        Atom::with_style(CompactString::from(s), crate::protocol::Style::default())
    }

    #[test]
    fn empty_normalised_input_produces_no_renders() {
        let result = apply(&NormalizedDisplay::default(), &[10, 10]);
        assert!(result.is_empty());
    }

    #[test]
    fn single_replace_produces_one_line_render() {
        let n = normalize(vec![t(hide_inline(0, 2..5), 0, "p", 0)]);
        let renders = apply(&n, &[10]);
        assert_eq!(renders.len(), 1);
        assert_eq!(renders[0].line, BufferLine::Real(0));
        assert_eq!(renders[0].replacements.len(), 1);
        assert_eq!(renders[0].replacements[0].byte_range, 2..5);
        assert!(matches!(renders[0].replacements[0].content, Content::Empty));
    }

    #[test]
    fn end_of_line_sentinel_resolves_to_line_length() {
        let d = Display::Replace {
            range: Span::end_of_line(0),
            content: Content::Text(vec![atom(" END")]),
        };
        let n = normalize(vec![t(d, 0, "p", 0)]);
        let renders = apply(&n, &[7]);
        assert_eq!(renders[0].replacements[0].byte_range, 7..7);
    }

    #[test]
    fn decorate_and_replace_on_same_line_share_one_render() {
        let n = normalize(vec![
            t(hide_inline(0, 0..2), 0, "p", 0),
            t(style_inline(0, 5..10, WireFace::default(), 0), 0, "q", 0),
        ]);
        let renders = apply(&n, &[20]);
        assert_eq!(renders.len(), 1);
        assert_eq!(renders[0].replacements.len(), 1);
        assert_eq!(renders[0].decorations.len(), 1);
    }

    #[test]
    fn ornaments_produce_virtual_lines_around_host() {
        let before =
            crate::display_algebra::derived::insert_before(5, crate::element::Element::Empty);
        let after =
            crate::display_algebra::derived::insert_after(5, crate::element::Element::Empty);

        let n = normalize(vec![t(before, 0, "p", 0), t(after, 0, "p", 1)]);
        let renders = apply(&n, &[0; 10]);

        // Two virtual lines: one Before, one After. Both at host line 5.
        assert_eq!(renders.len(), 2);
        let kinds: Vec<_> = renders.iter().map(|r| r.line).collect();
        assert!(kinds.contains(&BufferLine::Virtual {
            host_line: 5,
            side: Side::Before,
            order: 0,
        }));
        assert!(kinds.contains(&BufferLine::Virtual {
            host_line: 5,
            side: Side::After,
            order: 1,
        }));
    }

    #[test]
    fn fold_emits_single_anchor_line_with_content_fold() {
        // ADR-037 §4: fold emits one LineRender at range.start
        // carrying Content::Fold; the consumed lines (range.start+1..)
        // are NOT emitted as separate renders — the consumer (e.g.
        // DisplayMap::build) treats them as hidden by virtue of the
        // fold's range. This test pins the new emission shape.
        let summary = vec![atom("// folded 3 lines")];
        let n = normalize(vec![t(fold(2..5, summary), 0, "p", 0)]);
        let renders = apply(&n, &[10; 6]);

        assert_eq!(renders.len(), 1, "fold emits one LineRender");
        assert_eq!(renders[0].line, BufferLine::Real(2));
        assert_eq!(renders[0].replacements.len(), 1);
        assert!(matches!(
            renders[0].replacements[0].content,
            Content::Fold { .. }
        ));
    }

    #[test]
    fn replacements_sorted_within_line_by_byte_start() {
        let n = normalize(vec![
            t(hide_inline(0, 10..15), 0, "p", 0),
            t(hide_inline(0, 0..5), 0, "p", 1),
            t(hide_inline(0, 5..8), 0, "p", 2),
        ]);
        let renders = apply(&n, &[20]);
        assert_eq!(renders.len(), 1);
        let starts: Vec<_> = renders[0]
            .replacements
            .iter()
            .map(|r| r.byte_range.start)
            .collect();
        assert_eq!(starts, vec![0, 5, 10]);
    }

    #[test]
    fn replacement_atoms_extracts_text_content() {
        let rep = Replacement {
            byte_range: 0..3,
            content: Content::Text(vec![atom("abc"), atom("def")]),
        };
        assert_eq!(replacement_atoms(&rep).len(), 2);

        let rep_empty = Replacement {
            byte_range: 0..3,
            content: Content::Empty,
        };
        assert!(replacement_atoms(&rep_empty).is_empty());
    }
}
