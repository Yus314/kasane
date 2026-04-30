//! Parley shaper — turns a [`StyledLine`] into a [`ParleyLayout`].
//!
//! Boundary where Kasane talks to Parley's `RangedBuilder`. Each
//! [`StyleRun`](super::styled_line::StyleRun) in the line is pushed as
//! a span of `StyleProperty::Brush` + `FontWeight` + `FontStyle` +
//! optional `LetterSpacing` / `FontFeatures` / `FontVariations` /
//! `Underline` / `Strikethrough`; the line's `font_size` and base
//! font family come from the line's `base_style` plus the
//! [`ParleyText`] font configuration.
//!
//! The shaper is allocation-conscious: `LayoutContext` reuses its
//! internal buffers across calls, so the only per-line allocation is
//! the `Layout` itself (Parley 0.9 does not expose a `build_into` for
//! a reusable `Layout`, so each call creates a new one). The L1
//! [`super::layout_cache::LayoutCache`] amortises this by caching the
//! `Arc<ParleyLayout>` across frames.

use parley::{FontFamily, FontStyle as PFontStyle, InlineBox, InlineBoxKind, StyleProperty};

use super::ParleyText;
use super::layout::ParleyLayout;
use super::styled_line::StyledLine;

/// Shape a [`StyledLine`] into a [`ParleyLayout`].
///
/// `family` is the resolved font family stack (see [`super::font_stack`]);
/// the production entry point [`ParleyText::shape`] caches the stack on
/// the renderer state so this signature stays cheap. Tests that want to
/// exercise an explicit family (e.g. emoji-first fallback chains) call
/// this function directly.
pub fn shape_line(
    text_state: &mut ParleyText,
    line: &StyledLine,
    family: FontFamily<'static>,
) -> ParleyLayout {
    let scale = 1.0_f32; // Already-scaled font_size; no display scale fold-in here.
    let mut builder =
        text_state
            .layout_cx
            .ranged_builder(&mut text_state.font_cx, &line.text, scale, true);

    // Defaults: applied to the whole line and overridden per StyleRun.
    builder.push_default(StyleProperty::FontFamily(family));
    builder.push_default(StyleProperty::FontSize(line.font_size));

    for run in &line.runs {
        let range = (run.byte_range.start as usize)..(run.byte_range.end as usize);

        // Brush — text colour.
        builder.push(StyleProperty::Brush(run.resolved.fg), range.clone());

        // Weight — Parley FontWeight is a wrapped f32.
        builder.push(
            StyleProperty::FontWeight(parley::FontWeight::new(run.resolved.weight)),
            range.clone(),
        );

        // Slant — italic/oblique are mutually exclusive (encoded as one enum).
        let slant = match run.resolved.slant {
            super::style_resolver::SlantKind::Normal => PFontStyle::Normal,
            super::style_resolver::SlantKind::Italic => PFontStyle::Italic,
            super::style_resolver::SlantKind::Oblique => PFontStyle::Oblique(None),
        };
        builder.push(StyleProperty::FontStyle(slant), range.clone());

        // Letter spacing (only when non-zero to avoid extra runs).
        if run.resolved.letter_spacing != 0.0 {
            builder.push(
                StyleProperty::LetterSpacing(run.resolved.letter_spacing),
                range.clone(),
            );
        }

        // OpenType feature toggles (calt / clig / dlig / hlig / liga / zero).
        // The `parley::FontFeature` slice is short-lived (max 6 entries) and
        // built once per non-empty bitset; empty bitsets skip the push so
        // Parley uses the font's default features.
        if run.resolved.font_features.0 != 0 {
            let features =
                super::style_resolver::kasane_features_to_parley(run.resolved.font_features);
            builder.push(
                StyleProperty::FontFeatures(features.as_slice().into()),
                range.clone(),
            );
        }

        // Variable-font axis settings (e.g. `wght=350`, `wdth=80`).
        if !run.resolved.font_variations.is_empty() {
            let variations =
                super::style_resolver::kasane_variations_to_parley(&run.resolved.font_variations);
            builder.push(
                StyleProperty::FontVariations(variations.as_slice().into()),
                range.clone(),
            );
        }

        // Underline — Parley's StyleProperty::Underline is a bool
        // toggle. Styled (curly/dotted/dashed/double) variants are
        // drawn separately by the quad pipeline; here we set the
        // plain underline so the glyph metrics include the offset /
        // size hint.
        if !matches!(
            run.resolved.underline,
            super::style_resolver::DecorationKind::None
        ) {
            builder.push(StyleProperty::Underline(true), range.clone());
        }

        // Strikethrough — same shape as underline.
        if !matches!(
            run.resolved.strikethrough,
            super::style_resolver::DecorationKind::None
        ) {
            builder.push(StyleProperty::Strikethrough(true), range.clone());
        }
    }

    // Reserve inline-box slots in the layout. Each slot in
    // `StyledLine::inline_boxes` becomes a Parley `InlineBox` so the
    // layout engine flows surrounding text around the declared
    // geometry. The actual paint content is queried via the host's
    // `paint_inline_box(box_id)` callback at render time; the layout
    // only knows the slot's id, byte offset, width, and height.
    for slot in &line.inline_boxes {
        builder.push_inline_box(InlineBox {
            id: slot.id,
            kind: InlineBoxKind::InFlow,
            index: slot.byte_offset as usize,
            width: slot.width,
            height: slot.height,
        });
    }

    let mut layout = builder.build(&line.text);
    layout.break_all_lines(line.max_width);
    ParleyLayout::from_layout(layout)
}

#[cfg(test)]
mod tests {
    use super::*;
    use kasane_core::config::FontConfig;
    use kasane_core::protocol::{Atom, Style};

    use super::super::Brush;

    fn ascii_atoms(s: &str) -> Vec<Atom> {
        vec![Atom::plain(s)]
    }

    #[test]
    fn ascii_line_shapes_into_layout() {
        let mut text = ParleyText::new(&FontConfig::default());
        let atoms = ascii_atoms("hello");
        let line = StyledLine::from_atoms(
            &atoms,
            &Style::default(),
            Brush::opaque(255, 255, 255),
            14.0,
            None,
        );
        let parley_layout = text.shape(&line);
        assert_eq!(parley_layout.line_count, 1);
        assert!(parley_layout.width > 0.0, "expected non-zero width");
        assert!(parley_layout.height > 0.0, "expected non-zero height");
        assert!(parley_layout.baseline_ascent > 0.0);
    }

    #[test]
    fn empty_line_shapes_to_zero_width() {
        let mut text = ParleyText::new(&FontConfig::default());
        let line = StyledLine::from_atoms(&[], &Style::default(), Brush::default(), 14.0, None);
        let parley_layout = text.shape(&line);
        // Parley produces a single zero-width line for empty input.
        assert!(parley_layout.line_count <= 1);
        assert_eq!(parley_layout.width, 0.0);
    }

    #[test]
    fn multi_run_line_shapes() {
        use kasane_core::protocol::{Color, NamedColor, WireFace};
        let mut text = ParleyText::new(&FontConfig::default());
        let atoms = vec![
            Atom::with_style(
                "red ",
                Style::from_face(&WireFace {
                    fg: Color::Named(NamedColor::Red),
                    ..WireFace::default()
                }),
            ),
            Atom::with_style(
                "blue",
                Style::from_face(&WireFace {
                    fg: Color::Named(NamedColor::Blue),
                    ..WireFace::default()
                }),
            ),
        ];
        let line = StyledLine::from_atoms(
            &atoms,
            &Style::default(),
            Brush::opaque(255, 255, 255),
            14.0,
            None,
        );
        assert_eq!(line.runs.len(), 2);
        let parley_layout = text.shape(&line);
        assert_eq!(parley_layout.line_count, 1);
        assert!(parley_layout.width > 0.0);
    }

    #[test]
    fn cjk_line_shapes() {
        let mut text = ParleyText::new(&FontConfig::default());
        let atoms = ascii_atoms("こんにちは");
        let line = StyledLine::from_atoms(
            &atoms,
            &Style::default(),
            Brush::opaque(255, 255, 255),
            14.0,
            None,
        );
        let parley_layout = text.shape(&line);
        // Some ICU4X data sets emit "No segmentation model for language: ja"
        // diagnostics; the layout still completes successfully and produces a
        // single visual line.
        assert_eq!(parley_layout.line_count, 1);
        assert!(parley_layout.width > 0.0);
    }

    #[test]
    fn inline_box_widens_layout() {
        // ADR-031 Phase 10 Step 2-renderer: an inline-box slot reserved
        // via push_inline_box must add to the laid-out line width, since
        // the layout engine flows surrounding text around the slot.
        use super::super::styled_line::InlineBoxSlot;

        let mut text = ParleyText::new(&FontConfig::default());
        let atoms = ascii_atoms("hi");
        let plain = StyledLine::from_atoms(
            &atoms,
            &Style::default(),
            Brush::opaque(255, 255, 255),
            14.0,
            None,
        );
        let plain_layout = text.shape(&plain);

        let with_box = StyledLine::from_atoms(
            &atoms,
            &Style::default(),
            Brush::opaque(255, 255, 255),
            14.0,
            None,
        )
        .with_inline_boxes(vec![InlineBoxSlot {
            id: 1,
            byte_offset: 1,
            width: 30.0,
            height: 14.0,
        }]);
        let with_box_layout = text.shape(&with_box);

        assert_eq!(with_box_layout.line_count, 1);
        assert!(
            with_box_layout.width > plain_layout.width + 20.0,
            "inline box of width 30 must add to layout width: \
             plain={} with_box={}",
            plain_layout.width,
            with_box_layout.width
        );
    }

    #[test]
    fn shaper_reuses_layout_context() {
        // Two consecutive shape calls reuse the LayoutContext's internal
        // scratch buffers — verified indirectly by both shapes succeeding.
        let mut text = ParleyText::new(&FontConfig::default());
        let line1 = StyledLine::from_atoms(
            &ascii_atoms("first"),
            &Style::default(),
            Brush::default(),
            14.0,
            None,
        );
        let line2 = StyledLine::from_atoms(
            &ascii_atoms("second"),
            &Style::default(),
            Brush::default(),
            14.0,
            None,
        );
        let l1 = text.shape(&line1);
        let l2 = text.shape(&line2);
        assert_eq!(l1.line_count, 1);
        assert_eq!(l2.line_count, 1);
        assert!(l2.width > l1.width, "second is longer than first");
    }
}
