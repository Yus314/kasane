//! Parley shaper — turns a [`StyledLine`] into a [`ParleyLayout`].
//!
//! ADR-031, Phase 7. This is the boundary where Kasane talks to Parley's
//! `RangedBuilder`. Each [`StyleRun`] in the line is pushed as a span of
//! `StyleProperty::Brush` + `FontWeight` + `FontStyle` properties; the
//! line's `font_size` and base font family come from the line's `base_style`
//! plus the `ParleyText` font configuration.
//!
//! The shaper is allocation-conscious: `LayoutContext` reuses its internal
//! buffers across calls, so the only per-line allocation is the `Layout`
//! itself (Parley does not currently expose a `build_into` for a reusable
//! `Layout`, so each call creates a new one). The L1
//! [`super::layout_cache::LayoutCache`] amortises this by caching the
//! `Arc<ParleyLayout>` across frames.

use parley::{FontFamily, FontStyle as PFontStyle, StyleProperty};

use super::ParleyText;
use super::layout::ParleyLayout;
use super::styled_line::StyledLine;

/// Default font family used when the line's base style does not specify one.
/// Matches the [`FontConfig::default`](kasane_core::config::FontConfig::default)
/// `monospace` choice.
fn default_family() -> FontFamily<'static> {
    FontFamily::Single(parley::FontFamilyName::Generic(
        parley::GenericFamily::Monospace,
    ))
}

/// Shape a [`StyledLine`] into a [`ParleyLayout`].
///
/// `family` is the resolved font family stack (see [`super::font_stack`]);
/// pass it explicitly so the caller can cache it on `ParleyText` rather than
/// rebuilding on every shape call.
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

        // Underline — Parley's StyleProperty::Underline is a bool toggle. The
        // styled (curly/dotted/dashed/double) variants are deferred to
        // Phase 10's quad pipeline; here we set the plain underline so the
        // glyph metrics include the offset/size hint.
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

    let mut layout = builder.build(&line.text);
    layout.break_all_lines(line.max_width);
    ParleyLayout::from_layout(layout)
}

/// Convenience that pulls the family from the [`ParleyText`] state when the
/// caller has not yet implemented font-family caching.
///
/// Phase 9 will retire this in favour of the explicit family argument.
pub fn shape_line_with_default_family(
    text_state: &mut ParleyText,
    line: &StyledLine,
) -> ParleyLayout {
    shape_line(text_state, line, default_family())
}

#[cfg(test)]
mod tests {
    use super::*;
    use kasane_core::config::FontConfig;
    use kasane_core::protocol::{Atom, Style};

    use super::super::Brush;

    fn ascii_atoms(s: &str) -> Vec<Atom> {
        vec![Atom::from_face(kasane_core::protocol::Face::default(), s)]
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
        let parley_layout = shape_line_with_default_family(&mut text, &line);
        assert_eq!(parley_layout.line_count, 1);
        assert!(parley_layout.width > 0.0, "expected non-zero width");
        assert!(parley_layout.height > 0.0, "expected non-zero height");
        assert!(parley_layout.baseline_ascent > 0.0);
    }

    #[test]
    fn empty_line_shapes_to_zero_width() {
        let mut text = ParleyText::new(&FontConfig::default());
        let line = StyledLine::from_atoms(&[], &Style::default(), Brush::default(), 14.0, None);
        let parley_layout = shape_line_with_default_family(&mut text, &line);
        // Parley produces a single zero-width line for empty input.
        assert!(parley_layout.line_count <= 1);
        assert_eq!(parley_layout.width, 0.0);
    }

    #[test]
    fn multi_run_line_shapes() {
        use kasane_core::protocol::{Color, Face, NamedColor};
        let mut text = ParleyText::new(&FontConfig::default());
        let atoms = vec![
            Atom::from_face(
                Face {
                    fg: Color::Named(NamedColor::Red),
                    ..Face::default()
                },
                "red ",
            ),
            Atom::from_face(
                Face {
                    fg: Color::Named(NamedColor::Blue),
                    ..Face::default()
                },
                "blue",
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
        let parley_layout = shape_line_with_default_family(&mut text, &line);
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
        let parley_layout = shape_line_with_default_family(&mut text, &line);
        // Some ICU4X data sets emit "No segmentation model for language: ja"
        // diagnostics; the layout still completes successfully and produces a
        // single visual line.
        assert_eq!(parley_layout.line_count, 1);
        assert!(parley_layout.width > 0.0);
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
        let l1 = shape_line_with_default_family(&mut text, &line1);
        let l2 = shape_line_with_default_family(&mut text, &line2);
        assert_eq!(l1.line_count, 1);
        assert_eq!(l2.line_count, 1);
        assert!(l2.width > l1.width, "second is longer than first");
    }
}
