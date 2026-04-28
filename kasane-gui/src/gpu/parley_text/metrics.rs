//! Parley-backed `CellMetrics` calculation (ADR-031, Phase 9).
//!
//! Parallel implementation of [`crate::gpu::metrics::CellMetrics::calculate`]
//! using Parley + swash instead of cosmic-text. Produces a [`CellMetrics`]
//! with the same field semantics so the rest of the renderer is agnostic to
//! the text-shaping backend.
//!
//! How "cell width" is determined:
//!
//! 1. Shape the single character `"M"` through Parley using the user's font
//!    config (size, family, line height).
//! 2. Walk the resulting layout's first line and take the advance of the
//!    first glyph.
//! 3. Add `letter_spacing` (in physical pixels) to get the canonical cell
//!    advance width.
//!
//! Cell height is the configured line height in physical pixels. Baseline
//! is the first line's ascent in pixels.

use kasane_core::config::FontConfig;
use winit::dpi::PhysicalSize;

use super::ParleyText;
use super::shaper::shape_line_with_default_family;
use super::styled_line::StyledLine;
use crate::gpu::metrics::CellMetrics;
use kasane_core::protocol::{Atom, Style};

use super::Brush;

/// Compute [`CellMetrics`] using Parley.
///
/// Mirrors the public API of [`CellMetrics::calculate`] (which uses
/// cosmic-text) so callers can swap backends behind a configuration toggle.
pub fn calculate_with_parley(
    text_state: &mut ParleyText,
    font_config: &FontConfig,
    scale_factor: f64,
    window_size: PhysicalSize<u32>,
) -> CellMetrics {
    let font_size = font_config.size * scale_factor as f32;
    let line_height_px = font_size * font_config.line_height;
    let letter_spacing_px = font_config.letter_spacing * scale_factor as f32;

    // Build a single-atom "M" line with default style.
    let probe_atom = Atom::from_face(kasane_core::protocol::Face::default(), "M");
    let line = StyledLine::from_atoms(
        &[probe_atom],
        &Style::default(),
        Brush::opaque(255, 255, 255),
        font_size,
        None,
    );
    let layout = shape_line_with_default_family(text_state, &line);

    // First glyph's advance through Parley → cell width.
    let m_advance = first_glyph_advance(&layout).unwrap_or(font_size * 0.6);
    let cell_width = m_advance + letter_spacing_px;
    let cell_height = line_height_px;
    let baseline = layout.baseline_ascent.max(font_size * 0.8);

    let cols = (window_size.width as f32 / cell_width).floor().max(1.0) as u16;
    let rows = (window_size.height as f32 / cell_height).floor().max(1.0) as u16;

    // Underline / strikethrough metrics from the first glyph run's font.
    // Parley exposes these via `Run::metrics()` (parley v0.9). The values
    // are in physical pixels at the line's font size, so they need no
    // additional scaling.
    let (underline_offset, underline_thickness, strikethrough_offset, strikethrough_thickness) =
        first_decoration_metrics(&layout).unwrap_or((0.0, 0.0, 0.0, 0.0));

    CellMetrics {
        cell_width,
        cell_height,
        baseline,
        cols,
        rows,
        underline_offset,
        underline_thickness,
        strikethrough_offset,
        strikethrough_thickness,
    }
}

/// Pull `(underline_offset, underline_size, strikethrough_offset,
/// strikethrough_size)` from the first GlyphRun in `layout`. All four
/// are font-intrinsic and stable across glyphs at a given size, so
/// computing them once per metric refresh is sufficient.
fn first_decoration_metrics(layout: &super::layout::ParleyLayout) -> Option<(f32, f32, f32, f32)> {
    use parley::PositionedLayoutItem;
    layout
        .layout
        .lines()
        .flat_map(|line| line.items())
        .find_map(|item| match item {
            PositionedLayoutItem::GlyphRun(run) => {
                let m = run.run().metrics();
                Some((
                    m.underline_offset,
                    m.underline_size,
                    m.strikethrough_offset,
                    m.strikethrough_size,
                ))
            }
            _ => None,
        })
}

/// Extract the advance width of the first glyph in the first line of a
/// shaped layout. Returns `None` if the layout has no glyphs (e.g. empty
/// text).
fn first_glyph_advance(layout: &super::layout::ParleyLayout) -> Option<f32> {
    use parley::PositionedLayoutItem;
    layout
        .layout
        .lines()
        .flat_map(|line| line.items())
        .find_map(|item| match item {
            PositionedLayoutItem::GlyphRun(run) => {
                run.positioned_glyphs().next().map(|g| g.advance)
            }
            _ => None,
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parley_metrics_produce_positive_dimensions() {
        let mut text = ParleyText::new(&FontConfig::default());
        let cfg = FontConfig::default();
        let metrics = calculate_with_parley(&mut text, &cfg, 1.0, PhysicalSize::new(800, 600));
        assert!(metrics.cell_width > 0.0, "cell_width should be positive");
        assert!(metrics.cell_height > 0.0, "cell_height should be positive");
        assert!(metrics.baseline > 0.0, "baseline should be positive");
        assert!(metrics.cols > 0);
        assert!(metrics.rows > 0);
    }

    #[test]
    fn parley_metrics_scale_with_font_size() {
        let mut text = ParleyText::new(&FontConfig::default());
        let small = calculate_with_parley(
            &mut text,
            &FontConfig {
                size: 10.0,
                ..FontConfig::default()
            },
            1.0,
            PhysicalSize::new(800, 600),
        );
        let large = calculate_with_parley(
            &mut text,
            &FontConfig {
                size: 20.0,
                ..FontConfig::default()
            },
            1.0,
            PhysicalSize::new(800, 600),
        );
        assert!(large.cell_width > small.cell_width);
        assert!(large.cell_height > small.cell_height);
    }

    #[test]
    fn parley_metrics_scale_with_dpi() {
        let mut text = ParleyText::new(&FontConfig::default());
        let cfg = FontConfig::default();
        let dpi1 = calculate_with_parley(&mut text, &cfg, 1.0, PhysicalSize::new(800, 600));
        let dpi2 = calculate_with_parley(&mut text, &cfg, 2.0, PhysicalSize::new(1600, 1200));
        // 2× DPI → ~2× cell dimensions
        let ratio = dpi2.cell_width / dpi1.cell_width;
        assert!((1.8..=2.2).contains(&ratio), "ratio = {ratio}");
    }

    #[test]
    fn parley_metrics_letter_spacing_added() {
        let mut text = ParleyText::new(&FontConfig::default());
        let no_spacing = calculate_with_parley(
            &mut text,
            &FontConfig {
                letter_spacing: 0.0,
                ..FontConfig::default()
            },
            1.0,
            PhysicalSize::new(800, 600),
        );
        let with_spacing = calculate_with_parley(
            &mut text,
            &FontConfig {
                letter_spacing: 2.0,
                ..FontConfig::default()
            },
            1.0,
            PhysicalSize::new(800, 600),
        );
        let delta = with_spacing.cell_width - no_spacing.cell_width;
        // letter_spacing of 2.0 px at scale 1.0 should add exactly 2.0 px.
        assert!((delta - 2.0).abs() < 0.001, "delta = {delta}");
    }
}
