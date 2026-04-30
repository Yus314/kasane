//! Walk a [`ParleyLayout`] and emit fully positioned [`GlyphPlacement`]s
//! ready for the L2 raster cache + GPU vertex generation.
//!
//! Sits between [`ParleyLayout`](super::layout::ParleyLayout) (the
//! cached shape result) and the wgpu vertex emission stage. Splitting
//! the "walk Parley" step out of the renderer gives:
//!
//! - **Testability**: the emitter is pure — given a layout and an origin, it
//!   produces a deterministic list of placements that we can inspect without
//!   touching wgpu.
//! - **Cache key derivation**: the emitter computes the L2
//!   [`GlyphRasterKey`](super::raster_cache::GlyphRasterKey) from the same
//!   font_id / size / subpx pair that the rasteriser will see, so the
//!   cache and the renderer never disagree about which glyph is which.
//! - **Decoration metadata**: line-level underline / strikethrough come from
//!   `parley::Line::metrics()` and need to flow into the quad pipeline; the
//!   emitter surfaces them as side data so the caller can issue quad draws
//!   alongside the glyph atlas reads.

use std::sync::Arc;

use parley::PositionedLayoutItem;

use super::Brush;
use super::font_id::{font_id_from_data, var_hash_from_coords};
use super::glyph_rasterizer::SubpixelX;
use super::layout::ParleyLayout;
use super::raster_cache::GlyphRasterKey;

/// One glyph in absolute pixel coordinates, ready for L2 lookup + vertex
/// emission. `px`/`py` are in screen-space physical pixels (origin =
/// top-left of the layout's owning rectangle as supplied to [`emit`]).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct GlyphPlacement {
    pub px: f32,
    pub py: f32,
    pub raster_key: GlyphRasterKey,
    pub brush: Brush,
    /// Reference back to the per-(font, size) state needed by the
    /// rasteriser. The caller indexes this into a font-data table to build
    /// a swash `FontRef`.
    pub font_id: u32,
    /// Logical font size before subpixel quantisation. Pre-cached so the
    /// rasteriser does not need to redivide `raster_key.size_q` by 64.
    pub font_size: f32,
}

/// Aggregate output of [`emit`]. Decoration metrics (underline /
/// strikethrough offsets and thicknesses) flow through
/// [`super::metrics`]; this emitter handles only glyph placements.
#[derive(Debug, Default, Clone)]
pub struct EmittedFrame {
    pub glyphs: Vec<GlyphPlacement>,
}

/// Walk every glyph in `layout` and return a flat placement list anchored at
/// `(origin_x, origin_y)` (top-left of the layout's containing rect). The
/// hint flag is stored on each [`GlyphRasterKey`] so warm-cache hits remain
/// stable across frames.
pub fn emit(layout: &Arc<ParleyLayout>, origin_x: f32, origin_y: f32, hint: bool) -> EmittedFrame {
    let mut frame = EmittedFrame::default();
    for line in layout.layout.lines() {
        // Parley's `positioned_glyphs()` already includes
        // `run.baseline()` in each glyph's `y` (parley v0.9:
        // `Run::positioned_glyphs` sets `y = run.baseline() + glyph.y`).
        // So `origin_y + glyph.y` is the baseline y in absolute
        // coords; adding `metrics.baseline` again would double-count.
        for item in line.items() {
            let PositionedLayoutItem::GlyphRun(run) = item else {
                continue;
            };
            let parley_run = run.run();
            let font = parley_run.font();
            let font_id = font_id_from_data(font);
            let var_hash = var_hash_from_coords(parley_run.normalized_coords());
            let font_size = parley_run.font_size();
            let size_q = (font_size * 64.0).round().clamp(0.0, u16::MAX as f32) as u16;
            // Brush tracking: positioned_glyphs() does not emit brush per
            // glyph; the run carries the active style from the StyleProperty
            // pushes. We use the run's first style brush.
            let brush = first_brush_in_run(&run);

            for glyph in run.positioned_glyphs() {
                let abs_x = origin_x + glyph.x;
                let abs_y = origin_y + glyph.y;
                let subpx = SubpixelX::from_fract(abs_x);
                let glyph_id = glyph.id as u16;
                frame.glyphs.push(GlyphPlacement {
                    px: abs_x,
                    py: abs_y,
                    raster_key: GlyphRasterKey {
                        font_id,
                        glyph_id,
                        size_q,
                        subpx_x: subpx.0,
                        var_hash,
                        hint,
                    },
                    brush,
                    font_id,
                    font_size,
                });
            }
        }
    }
    frame
}

fn first_brush_in_run(run: &parley::layout::GlyphRun<'_, Brush>) -> Brush {
    // Parley GlyphRun exposes a single style for the whole run via the
    // first cluster. We snapshot that brush; the L1 cache key hashes
    // styles so two runs with different brushes never share cache entries.
    let parley_run = run.run();
    parley_run
        .clusters()
        .next()
        .map(|c| c.first_style().brush)
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;
    use kasane_core::config::FontConfig;
    use kasane_core::protocol::{Atom, Style};

    use super::super::ParleyText;
    use super::super::styled_line::StyledLine;

    fn line(text: &str) -> StyledLine {
        let atoms = vec![Atom::plain(text)];
        StyledLine::from_atoms(
            &atoms,
            &Style::default(),
            Brush::opaque(255, 255, 255),
            14.0,
            None,
        )
    }

    #[test]
    fn emit_produces_glyph_per_character() {
        let mut text = ParleyText::new(&FontConfig::default());
        let layout = Arc::new(text.shape(&line("hello")));
        let frame = emit(&layout, 100.0, 50.0, true);
        assert!(
            frame.glyphs.len() >= 5,
            "expected ≥ 5 glyphs for 'hello': {}",
            frame.glyphs.len()
        );
        // x positions are non-decreasing.
        for window in frame.glyphs.windows(2) {
            assert!(
                window[1].px >= window[0].px,
                "glyph x must be monotonic across the line"
            );
        }
        // First glyph starts at or after the origin.
        assert!(frame.glyphs[0].px >= 100.0);
    }

    #[test]
    fn emit_y_anchored_to_baseline() {
        let mut text = ParleyText::new(&FontConfig::default());
        let layout = Arc::new(text.shape(&line("a")));
        let frame_a = emit(&layout, 0.0, 0.0, true);
        let frame_b = emit(&layout, 0.0, 100.0, true);
        // Same layout at different origin_y → glyph y shifts by 100.
        let dy = frame_b.glyphs[0].py - frame_a.glyphs[0].py;
        assert!((dy - 100.0).abs() < 0.01, "dy = {dy}");
    }

    #[test]
    fn emit_raster_key_is_hint_flag_aware() {
        let mut text = ParleyText::new(&FontConfig::default());
        let layout = Arc::new(text.shape(&line("hi")));
        let frame_hint = emit(&layout, 0.0, 0.0, true);
        let frame_no_hint = emit(&layout, 0.0, 0.0, false);
        assert!(frame_hint.glyphs[0].raster_key.hint);
        assert!(!frame_no_hint.glyphs[0].raster_key.hint);
        // Other key fields agree.
        assert_eq!(
            frame_hint.glyphs[0].raster_key.glyph_id,
            frame_no_hint.glyphs[0].raster_key.glyph_id
        );
    }

    #[test]
    fn emit_size_q_quantises_consistently() {
        let mut text = ParleyText::new(&FontConfig::default());
        let layout = Arc::new(text.shape(&line("x")));
        let frame = emit(&layout, 0.0, 0.0, true);
        let g = frame.glyphs[0];
        // size_q = round(font_size * 64) — for a 14 px font that's 14 * 64 = 896.
        assert_eq!(g.raster_key.size_q, 14 * 64);
        assert_eq!(g.font_size, 14.0);
    }

    #[test]
    fn emit_empty_layout_yields_empty_frame() {
        let mut text = ParleyText::new(&FontConfig::default());
        let layout = Arc::new(text.shape(&line("")));
        let frame = emit(&layout, 0.0, 0.0, true);
        assert!(frame.glyphs.is_empty());
    }

    #[test]
    fn emit_cjk_yields_glyphs() {
        let mut text = ParleyText::new(&FontConfig::default());
        let layout = Arc::new(text.shape(&line("こ")));
        let frame = emit(&layout, 0.0, 0.0, true);
        assert!(!frame.glyphs.is_empty(), "CJK layout produced no glyphs");
    }

    #[test]
    fn emit_origin_x_shifts_all_glyphs_uniformly() {
        let mut text = ParleyText::new(&FontConfig::default());
        let layout = Arc::new(text.shape(&line("hello")));
        let a = emit(&layout, 0.0, 0.0, true);
        let b = emit(&layout, 50.0, 0.0, true);
        for (ga, gb) in a.glyphs.iter().zip(b.glyphs.iter()) {
            assert!((gb.px - ga.px - 50.0).abs() < 0.01);
            assert!((gb.py - ga.py).abs() < 0.01);
        }
    }
}
