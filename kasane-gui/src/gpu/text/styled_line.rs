//! Styled line data model for the Parley shaper (ADR-031, Phase 7).
//!
//! `StyledLine` is the GUI-internal representation of a single Kakoune line
//! shaped by Parley. It is built from a slice of [`Atom`]s plus a base
//! [`Style`] (the line's `default_face` analogue), then handed to
//! [`super::shaper::ParleyShaper`] to produce a [`super::layout::ParleyLayout`].
//!
//! Why a separate type instead of feeding `&[Atom]` directly to Parley:
//!
//! - **Style merging**: Adjacent atoms with identical resolved styles can
//!   share a single [`StyleRun`], reducing the number of properties pushed
//!   into Parley's `RangedBuilder` and improving ligature continuity.
//! - **Cache key**: Hashing a `StyledLine` (small fixed-shape struct) is much
//!   cheaper than re-walking the source atoms each frame.
//! - **InlineBox bridge**: `DisplayDirective::InsertInline` content is
//!   surfaced here as an [`InlineBoxSlot`] so Phase 10's shaper integration
//!   can call `RangedBuilder::push_inline_box` directly.

use std::hash::{Hash, Hasher};
use std::ops::Range;

use kasane_core::protocol::{Atom, Style, resolve_style};
use rustc_hash::FxHasher;

use super::Brush;
use super::style_resolver::{DecorationKind, ResolvedParleyStyle, resolve_for_parley};

/// A line of text styled and ready for Parley shaping.
#[derive(Debug, Clone, PartialEq)]
pub struct StyledLine {
    /// Concatenated UTF-8 text from all source atoms.
    pub text: String,
    /// Style runs covering `text`. Sorted, non-overlapping, contiguous; the
    /// union of all `byte_range`s equals `0..text.len()` for non-empty lines.
    pub runs: Vec<StyleRun>,
    /// Inline-box slots from `DisplayDirective::InsertInline`. Empty until
    /// Phase 10 wires the directive translation.
    pub inline_boxes: Vec<InlineBoxSlot>,
    /// Per-atom byte boundaries in `text`. Length = `source_atom_count + 1`;
    /// `atom_boundaries[i]..atom_boundaries[i+1]` is the byte range of the
    /// i-th source atom. Used for per-atom background fills.
    pub atom_boundaries: Vec<u32>,
    /// Resolved kasane-core [`Style`] per source atom. Same length as
    /// `atom_boundaries.len() - 1`. Carries fields that don't survive the
    /// projection to `ResolvedParleyStyle` (e.g. `bg`, `dim`, `blink`,
    /// `reverse`) so the renderer can paint backgrounds and post-effects.
    pub atom_styles: Vec<Style>,
    /// Font size in physical pixels (already multiplied by scale factor).
    pub font_size: f32,
    /// Optional maximum advance for line wrapping. `None` means no wrap
    /// (Kasane's normal mode — Kakoune already paginated the buffer).
    pub max_width: Option<f32>,
    /// Memoized hash of `text` + `atom_boundaries`. Computed once at
    /// construction so `LayoutCache` lookups (24 lines per frame in the
    /// dominant typing scenario) avoid re-hashing the line content on hits.
    pub content_hash: u64,
    /// Memoized hash of the shape-affecting style fields across all runs
    /// (`fg`, `weight`, `letter_spacing`, `slant`, decoration enablement).
    /// Paint-time properties (`bg`, `reverse`, `dim`, `blink`, decoration
    /// colour / thickness) are intentionally excluded — see the
    /// `LayoutCache` invariant in `semantics.md:493`.
    pub style_hash: u64,
}

/// A single style span within a [`StyledLine`].
#[derive(Debug, Clone, PartialEq)]
pub struct StyleRun {
    pub byte_range: Range<u32>,
    pub resolved: ResolvedParleyStyle,
}

/// Slot for an inline widget injected into the layout via
/// `DisplayDirective::InsertInline`. Phase 10 fills these in; until then the
/// vector is always empty.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct InlineBoxSlot {
    /// Stable identifier (typically the hash of `(line_idx, byte_offset,
    /// content_id)`). Used by the renderer to look up the actual paint
    /// content.
    pub id: u64,
    /// Byte offset in the parent `StyledLine::text` where the box appears.
    pub byte_offset: u32,
    /// Width in physical pixels.
    pub width: f32,
    /// Height in physical pixels.
    pub height: f32,
}

impl StyledLine {
    /// Build a `StyledLine` from a slice of source atoms.
    ///
    /// `fallback_text_color` is the colour applied where an atom's resolved
    /// `Style::fg` is still `Brush::Default` after layering with `base_style`
    /// — this happens when both atom and base want to inherit, in which case
    /// we fall back to the renderer's default text colour (typically pulled
    /// from the user's [`crate::colors::ColorResolver`]).
    pub fn from_atoms(
        atoms: &[Atom],
        base_style: &Style,
        fallback_text_color: Brush,
        font_size: f32,
        max_width: Option<f32>,
    ) -> Self {
        let mut text = String::with_capacity(atoms.iter().map(|a| a.contents.len()).sum());
        let mut atom_boundaries = Vec::with_capacity(atoms.len() + 1);
        let mut atom_styles = Vec::with_capacity(atoms.len());
        let mut runs: Vec<StyleRun> = Vec::with_capacity(atoms.len());

        atom_boundaries.push(0u32);
        let mut current_run_start: u32 = 0;
        let mut current_resolved: Option<ResolvedParleyStyle> = None;

        for atom in atoms {
            let raw = atom.unresolved_style();
            let resolved_style = resolve_style(raw, base_style);
            let resolved_parley = resolve_for_parley(&resolved_style, fallback_text_color);

            let atom_start = text.len() as u32;
            text.push_str(&atom.contents);
            let atom_end = text.len() as u32;
            atom_boundaries.push(atom_end);
            atom_styles.push(resolved_style);

            match &current_resolved {
                Some(prev) if *prev == resolved_parley => {
                    // Continue the current run — no state to flush.
                }
                _ => {
                    if let Some(prev) = current_resolved.take() {
                        runs.push(StyleRun {
                            byte_range: current_run_start..atom_start,
                            resolved: prev,
                        });
                    }
                    current_run_start = atom_start;
                    current_resolved = Some(resolved_parley);
                }
            }
            // Update current run's end implicitly via current_run_start +
            // text growth; the run is not closed until the next style change.
            let _ = atom_end;
        }

        if let Some(prev) = current_resolved {
            runs.push(StyleRun {
                byte_range: current_run_start..text.len() as u32,
                resolved: prev,
            });
        }

        let content_hash = compute_content_hash(&text, &atom_boundaries);
        let style_hash = compute_style_hash(&runs);

        StyledLine {
            text,
            runs,
            inline_boxes: Vec::new(),
            atom_boundaries,
            atom_styles,
            font_size,
            max_width,
            content_hash,
            style_hash,
        }
    }

    /// Number of source atoms this line was built from.
    #[inline]
    pub fn atom_count(&self) -> usize {
        self.atom_boundaries.len().saturating_sub(1)
    }

    /// Returns the byte range of the i-th source atom in `text`.
    pub fn atom_range(&self, atom_idx: usize) -> Option<Range<u32>> {
        if atom_idx + 1 >= self.atom_boundaries.len() {
            return None;
        }
        Some(self.atom_boundaries[atom_idx]..self.atom_boundaries[atom_idx + 1])
    }

    /// Attach inline-box slots to a styled line built from atoms.
    ///
    /// ADR-031 Phase 10 Step 2-renderer: `DisplayDirective::InlineBox`
    /// declarations propagate to the GUI renderer through this builder.
    /// The shaper (`shaper.rs::shape_line`) calls Parley's
    /// `push_inline_box` for each slot so the layout engine reserves the
    /// declared geometry; the host then queries
    /// `PluginBackend::paint_inline_box(box_id)` per slot at render time
    /// to obtain the paint Element.
    ///
    /// `box_id` and slot geometry are part of the L1 LayoutCache content
    /// hash — changing any slot field invalidates the cache, since slot
    /// geometry shapes the surrounding text flow.
    pub fn with_inline_boxes(mut self, slots: Vec<InlineBoxSlot>) -> Self {
        self.inline_boxes = slots;
        // Re-hash with the new slots folded into the content hash.
        self.content_hash =
            compute_content_hash_with_boxes(&self.text, &self.atom_boundaries, &self.inline_boxes);
        self
    }
}

/// Hash the textual content of a styled line. Two lines with identical
/// `text` and atom boundaries share the same content hash regardless of
/// styling. Inline-box-free fast path; `from_atoms` calls this directly
/// because it always constructs lines without slots.
fn compute_content_hash(text: &str, atom_boundaries: &[u32]) -> u64 {
    let mut h = FxHasher::default();
    text.hash(&mut h);
    atom_boundaries.hash(&mut h);
    h.finish()
}

/// Hash the textual content **including** inline-box slots. Used by
/// `with_inline_boxes` to invalidate L1 LayoutCache entries when slot
/// geometry changes — Parley's layout depends on slot id, byte index,
/// width, and height, so any of these crossing a frame boundary must
/// trigger a re-shape.
fn compute_content_hash_with_boxes(
    text: &str,
    atom_boundaries: &[u32],
    boxes: &[InlineBoxSlot],
) -> u64 {
    let mut h = FxHasher::default();
    text.hash(&mut h);
    atom_boundaries.hash(&mut h);
    for slot in boxes {
        slot.id.hash(&mut h);
        slot.byte_offset.hash(&mut h);
        slot.width.to_bits().hash(&mut h);
        slot.height.to_bits().hash(&mut h);
    }
    h.finish()
}

/// Hash the resolved style runs. Captures every shape-affecting input that
/// is not in `compute_content_hash`.
///
/// Paint-only properties (`bg`, `reverse`, `dim`, `blink`, decoration style /
/// brush / thickness *within* an enabled decoration) are intentionally NOT
/// hashed — they are read from the current `StyledLine` at draw time, so a
/// cache hit that returns a layout shaped against a different paint-time
/// state is still correct. Decoration *enablement* (`None` vs `Some`) IS
/// hashed because `shaper.rs` calls `RangedBuilder::push` with
/// `StyleProperty::Underline(true)` / `Strikethrough(true)`, which causes
/// Parley's `RunMetrics` to populate the offset / thickness fields. Two
/// layouts that differ only by which side of that bool was on are not
/// interchangeable at paint time.
///
/// `font_features` and `font_variations` are shape-affecting (they change
/// glyph selection, ligature substitution, and axis-driven advance widths)
/// and are folded in here. The Wave 1.3 invariant: every field that reaches
/// `RangedBuilder::push` in `shaper.rs::shape_line` must contribute to this
/// hash, otherwise stale layouts pollute paint output.
fn compute_style_hash(runs: &[StyleRun]) -> u64 {
    let mut h = FxHasher::default();
    for run in runs {
        run.byte_range.hash(&mut h);
        run.resolved.fg.hash(&mut h);
        // ResolvedParleyStyle uses f32 for weight / letter_spacing — hash by
        // bit pattern so two equal styles produce the same hash.
        run.resolved.weight.to_bits().hash(&mut h);
        run.resolved.letter_spacing.to_bits().hash(&mut h);
        run.resolved.slant.hash(&mut h);
        decoration_enabled(&run.resolved.underline).hash(&mut h);
        decoration_enabled(&run.resolved.strikethrough).hash(&mut h);
        // OpenType features (`u32` bitset) and variable-font axes
        // (`[FontVariation]`). Must mirror the corresponding pushes in
        // `shaper.rs::shape_line`.
        run.resolved.font_features.hash(&mut h);
        run.resolved.font_variations.as_ref().hash(&mut h);
    }
    h.finish()
}

#[inline]
fn decoration_enabled(d: &DecorationKind) -> bool {
    !matches!(d, DecorationKind::None)
}

#[cfg(test)]
mod tests {
    use super::*;
    use kasane_core::protocol::{
        Brush as KBrush, Face, FontWeight as KFontWeight, NamedColor, Style,
    };

    fn atom(text: &str, face: Face) -> Atom {
        Atom::with_style(text, Style::from_face(&face))
    }

    fn red_face() -> Face {
        use kasane_core::protocol::Color;
        Face {
            fg: Color::Named(NamedColor::Red),
            ..Face::default()
        }
    }

    fn blue_face() -> Face {
        use kasane_core::protocol::Color;
        Face {
            fg: Color::Named(NamedColor::Blue),
            ..Face::default()
        }
    }

    #[test]
    fn empty_line_yields_empty_struct() {
        let line = StyledLine::from_atoms(&[], &Style::default(), Brush::default(), 14.0, None);
        assert_eq!(line.text, "");
        assert!(line.runs.is_empty());
        assert!(line.inline_boxes.is_empty());
        assert_eq!(line.atom_boundaries, vec![0]);
        assert!(line.atom_styles.is_empty());
        assert_eq!(line.atom_count(), 0);
    }

    #[test]
    fn single_atom_produces_one_run() {
        let atoms = vec![atom("hello", red_face())];
        let line = StyledLine::from_atoms(
            &atoms,
            &Style::default(),
            Brush::opaque(255, 255, 255),
            14.0,
            None,
        );
        assert_eq!(line.text, "hello");
        assert_eq!(line.runs.len(), 1);
        assert_eq!(line.runs[0].byte_range, 0..5);
        assert_eq!(line.atom_boundaries, vec![0, 5]);
        assert_eq!(line.atom_count(), 1);
    }

    #[test]
    fn adjacent_same_style_atoms_merge_into_one_run() {
        let atoms = vec![
            atom("hel", red_face()),
            atom("lo ", red_face()),
            atom("world", red_face()),
        ];
        let line = StyledLine::from_atoms(
            &atoms,
            &Style::default(),
            Brush::opaque(255, 255, 255),
            14.0,
            None,
        );
        assert_eq!(line.text, "hello world");
        assert_eq!(line.runs.len(), 1, "three same-style atoms should merge");
        assert_eq!(line.runs[0].byte_range, 0..11);
        // Boundaries are still per-atom even though runs merged.
        assert_eq!(line.atom_boundaries, vec![0, 3, 6, 11]);
        assert_eq!(line.atom_count(), 3);
    }

    #[test]
    fn distinct_style_atoms_produce_distinct_runs() {
        let atoms = vec![
            atom("red", red_face()),
            atom(" ", Face::default()),
            atom("blue", blue_face()),
        ];
        let line = StyledLine::from_atoms(
            &atoms,
            &Style::default(),
            Brush::opaque(255, 255, 255),
            14.0,
            None,
        );
        assert_eq!(line.text, "red blue");
        assert_eq!(line.runs.len(), 3);
        assert_eq!(line.runs[0].byte_range, 0..3);
        assert_eq!(line.runs[1].byte_range, 3..4);
        assert_eq!(line.runs[2].byte_range, 4..8);
    }

    #[test]
    fn atom_range_extracts_correct_slice() {
        let atoms = vec![atom("foo", red_face()), atom("bar", blue_face())];
        let line = StyledLine::from_atoms(&atoms, &Style::default(), Brush::default(), 14.0, None);
        assert_eq!(line.atom_range(0), Some(0..3));
        assert_eq!(line.atom_range(1), Some(3..6));
        assert_eq!(line.atom_range(2), None);
    }

    #[test]
    fn base_style_applied_to_default_atom() {
        // Atom with default style picks up base_style's brush during resolution.
        let atoms = vec![atom("x", Face::default())];
        let base = Style {
            fg: KBrush::Named(NamedColor::Cyan),
            font_weight: KFontWeight::BOLD,
            ..Style::default()
        };
        let line = StyledLine::from_atoms(&atoms, &base, Brush::default(), 14.0, None);
        assert_eq!(line.runs.len(), 1);
        // Resolved fg should be cyan (from base), weight 700.
        assert_eq!(line.runs[0].resolved.weight, 700.0);
        // (0, 205, 205) = cyan
        assert_eq!(line.runs[0].resolved.fg, Brush::opaque(0, 205, 205));
    }

    #[test]
    fn with_inline_boxes_replaces_slots_and_rehashes() {
        let atoms = vec![atom("hello", red_face())];
        let line = StyledLine::from_atoms(
            &atoms,
            &Style::default(),
            Brush::opaque(255, 255, 255),
            14.0,
            None,
        );
        let hash_before = line.content_hash;
        assert!(line.inline_boxes.is_empty());

        let with_box = line.clone().with_inline_boxes(vec![InlineBoxSlot {
            id: 0xDEAD_BEEF,
            byte_offset: 2,
            width: 24.0,
            height: 14.0,
        }]);
        assert_eq!(with_box.inline_boxes.len(), 1);
        assert_ne!(
            hash_before, with_box.content_hash,
            "adding an inline box must change the content hash so the L1 \
             LayoutCache treats the new line as a miss"
        );

        // Same atoms + same slot geometry → same hash (deterministic).
        let with_box_again = line.with_inline_boxes(vec![InlineBoxSlot {
            id: 0xDEAD_BEEF,
            byte_offset: 2,
            width: 24.0,
            height: 14.0,
        }]);
        assert_eq!(with_box.content_hash, with_box_again.content_hash);
    }

    #[test]
    fn with_inline_boxes_distinguishes_geometry() {
        // Two slots with the same id but different geometry must produce
        // distinct content hashes — Parley's layout depends on width/height.
        let atoms = vec![atom("hi", red_face())];
        let base = StyledLine::from_atoms(
            &atoms,
            &Style::default(),
            Brush::opaque(255, 255, 255),
            14.0,
            None,
        );
        let small = base.clone().with_inline_boxes(vec![InlineBoxSlot {
            id: 1,
            byte_offset: 1,
            width: 12.0,
            height: 14.0,
        }]);
        let wide = base.with_inline_boxes(vec![InlineBoxSlot {
            id: 1,
            byte_offset: 1,
            width: 24.0,
            height: 14.0,
        }]);
        assert_ne!(small.content_hash, wide.content_hash);
    }

    #[test]
    fn style_hash_changes_with_font_features() {
        // Wave 1.3 cache-stale pin. Two lines with identical text + fg +
        // weight but different `font_features` MUST produce different
        // style hashes — otherwise the L1 LayoutCache would return a
        // layout shaped without the requested ligature toggle.
        use kasane_core::protocol::FontFeatures;
        let plain_atoms = vec![Atom::with_style(
            "==",
            Style {
                font_features: FontFeatures::default(),
                ..Style::default()
            },
        )];
        let liga_atoms = vec![Atom::with_style(
            "==",
            Style {
                font_features: FontFeatures(FontFeatures::COMMON_LIGATURES),
                ..Style::default()
            },
        )];
        let plain = StyledLine::from_atoms(
            &plain_atoms,
            &Style::default(),
            Brush::default(),
            14.0,
            None,
        );
        let liga =
            StyledLine::from_atoms(&liga_atoms, &Style::default(), Brush::default(), 14.0, None);
        assert_eq!(
            plain.content_hash, liga.content_hash,
            "text is identical so content_hash must match"
        );
        assert_ne!(
            plain.style_hash, liga.style_hash,
            "different font_features must produce distinct style_hash; \
             otherwise the L1 LayoutCache returns a stale layout"
        );
    }

    #[test]
    fn style_hash_changes_with_font_variations() {
        // Same Wave 1.3 invariant for the variable-font axis path.
        use kasane_core::protocol::FontVariation;
        let plain_atoms = vec![Atom::with_style("a", Style::default())];
        let weighted_atoms = vec![Atom::with_style(
            "a",
            Style {
                font_variations: vec![FontVariation::new(*b"wght", 350.0)],
                ..Style::default()
            },
        )];
        let plain = StyledLine::from_atoms(
            &plain_atoms,
            &Style::default(),
            Brush::default(),
            14.0,
            None,
        );
        let weighted = StyledLine::from_atoms(
            &weighted_atoms,
            &Style::default(),
            Brush::default(),
            14.0,
            None,
        );
        assert_ne!(
            plain.style_hash, weighted.style_hash,
            "different font_variations must produce distinct style_hash"
        );
    }

    #[test]
    fn cjk_byte_boundaries_correct() {
        // Multi-byte UTF-8 characters must produce byte (not char) offsets.
        let atoms = vec![
            atom("a", red_face()),   // 1 byte
            atom("あ", blue_face()), // 3 bytes
            atom("b", red_face()),   // 1 byte
        ];
        let line = StyledLine::from_atoms(&atoms, &Style::default(), Brush::default(), 14.0, None);
        assert_eq!(line.text, "aあb");
        assert_eq!(line.atom_boundaries, vec![0, 1, 4, 5]);
        assert_eq!(line.runs.len(), 3);
        assert_eq!(line.runs[0].byte_range, 0..1);
        assert_eq!(line.runs[1].byte_range, 1..4);
        assert_eq!(line.runs[2].byte_range, 4..5);
    }
}
