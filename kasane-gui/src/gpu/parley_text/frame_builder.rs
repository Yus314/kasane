//! Build a frame's worth of [`DrawableGlyph`] from a list of styled lines
//! (ADR-031 Phase 9b).
//!
//! Sits one layer above [`glyph_emitter`](super::glyph_emitter): drives the
//! L1 [`LayoutCache`](super::layout_cache::LayoutCache) → emit → L2
//! [`GlyphRasterCache`](super::raster_cache::GlyphRasterCache) chain, and
//! returns a flat list of glyphs *with* their atlas slots resolved and
//! pixel positions assigned. The result is wgpu-agnostic — the caller
//! converts it to vertex data in a separate step (a follow-up commit will
//! port that step from the cosmic-text TextRenderer).
//!
//! Why one more layer:
//!
//! - The SceneRenderer's frame loop has many call sites
//!   (`process_draw_text`, `process_render_paragraph`, `process_draw_atoms`,
//!   `process_draw_padding_row`). Each shapes one line and emits one
//!   rectangle; without a shared utility, every site re-implements the
//!   cache plumbing.
//! - Putting the entire pipeline behind one entry point makes the
//!   SceneRenderer migration a structural rename rather than a
//!   re-derivation.
//!
//! Why no font registry: parley's `Layout::lines()` iterator chain produces
//! short-lived borrows of `FontData`, so attempting to pre-populate a
//! `(font_id → FontRef)` map runs into lifetime contortions. Instead we
//! build the swash `FontRef` inline in the layout walk and rasterise on
//! the spot.

use std::sync::Arc;

use parley::PositionedLayoutItem;
use swash::FontRef;

use super::Brush;
use super::ParleyText;
use super::atlas::AtlasSlot;
use super::font_id::{font_id_from_data, var_hash_from_coords};
use super::glyph_rasterizer::{ContentKind, GlyphRasterizer, SubpixelX};
use super::layout::ParleyLayout;
use super::layout_cache::LayoutCache;
use super::raster_cache::{AtlasOps, GlyphRasterCache, GlyphRasterKey};
use super::shaper::shape_line_with_default_family;
use super::styled_line::StyledLine;

/// One glyph ready to be written to the GPU vertex buffer.
///
/// All fields are in physical pixel units, anchored at the screen-space
/// origin of the layout. The atlas slot's `(x, y, w, h)` is in atlas
/// coordinates (0..atlas_side); the renderer converts to UV at vertex time.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct DrawableGlyph {
    /// Top-left of the glyph quad in physical pixels.
    pub px: f32,
    pub py: f32,
    /// Glyph dimensions in pixels (atlas slot width / height).
    pub width: u16,
    pub height: u16,
    /// Per-glyph offset from the pen position (swash bitmap placement).
    pub left: i16,
    pub top: i16,
    /// Mask vs Color routes the renderer to the appropriate atlas + sampler.
    pub content: ContentKind,
    /// Atlas region the renderer samples from.
    pub atlas_slot: AtlasSlot,
    /// Foreground brush (linear-space RGBA8). Mask glyphs use this as the
    /// tint colour; Color glyphs ignore it (the bitmap supplies its own).
    pub brush: Brush,
}

/// One source line for the frame builder. The caller pairs each line with
/// its stable cache identity (typically the kasane buffer line index) and
/// the screen-space origin where the layout's top-left should land.
pub struct FrameLine<'a> {
    pub line_idx: u32,
    pub origin_x: f32,
    pub origin_y: f32,
    pub line: &'a StyledLine,
}

/// Per-frame statistics — handy for tracing the warm-cache hit rate from
/// production runs without re-deriving them from the L1 / L2 stats.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct FrameBuildStats {
    pub layouts_walked: u32,
    pub glyphs_emitted: u32,
    pub glyphs_rastered: u32,
    pub glyphs_dropped_no_font_ref: u32,
    pub glyphs_dropped_atlas_full: u32,
}

/// Top-level frame build entry point.
///
/// Drives the full Parley pipeline:
///
/// 1. For each [`FrameLine`], shape (or hit L1) into an `Arc<ParleyLayout>`.
/// 2. Walk each layout: for every glyph, derive the L2 raster key, build
///    a swash `FontRef` from the parley `FontData`, and look up or
///    rasterise via the L2 cache.
/// 3. Append a [`DrawableGlyph`] for every successful lookup, anchored at
///    the line's screen origin.
///
/// Returns the flat glyph list (renderer iterates it once) and build stats.
#[allow(clippy::too_many_arguments)]
pub fn build_frame(
    text: &mut ParleyText,
    layout_cache: &mut LayoutCache,
    rasterizer: &mut GlyphRasterizer,
    raster_cache: &mut GlyphRasterCache,
    atlases: &mut dyn AtlasOps,
    lines: &[FrameLine<'_>],
    hint: bool,
) -> (Vec<DrawableGlyph>, FrameBuildStats) {
    let mut out = Vec::new();
    let mut stats = FrameBuildStats::default();

    // Phase A — shape (or hit L1) every line. The Arc<ParleyLayout>s pin
    // the FontData blobs so the FontRefs we derive in Phase B remain
    // valid for the rest of the frame.
    let layouts: Vec<Arc<ParleyLayout>> = lines
        .iter()
        .map(|fl| {
            layout_cache.get_or_compute(fl.line_idx, fl.line, |l| {
                shape_line_with_default_family(text, l)
            })
        })
        .collect();
    stats.layouts_walked = layouts.len() as u32;

    // Phase B — walk each layout, derive raster keys, rasterise, emit.
    for (i, layout) in layouts.iter().enumerate() {
        let fl = &lines[i];
        walk_one_layout(
            layout,
            fl.origin_x,
            fl.origin_y,
            hint,
            rasterizer,
            raster_cache,
            atlases,
            &mut out,
            &mut stats,
        );
    }

    (out, stats)
}

#[allow(clippy::too_many_arguments)]
fn walk_one_layout(
    layout: &Arc<ParleyLayout>,
    origin_x: f32,
    origin_y: f32,
    hint: bool,
    rasterizer: &mut GlyphRasterizer,
    raster_cache: &mut GlyphRasterCache,
    atlases: &mut dyn AtlasOps,
    out: &mut Vec<DrawableGlyph>,
    stats: &mut FrameBuildStats,
) {
    for line in layout.layout.lines() {
        // ADR-031 Phase 9b: Parley's `positioned_glyphs()` already
        // includes `run.baseline()` in each glyph.y; do not add it
        // again. (Same fix landed in scene_renderer / glyph_emitter.)
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
            let brush = first_brush_in_run(&run);

            // Build a FontRef once per run and reuse for every glyph.
            let Some(font_ref) = FontRef::from_index(font.data.data(), font.index as usize) else {
                stats.glyphs_dropped_no_font_ref += run.positioned_glyphs().count() as u32;
                continue;
            };

            for glyph in run.positioned_glyphs() {
                stats.glyphs_emitted += 1;

                let abs_x = origin_x + glyph.x;
                let abs_y = origin_y + glyph.y;
                let subpx = SubpixelX::from_fract(abs_x);
                let glyph_id = glyph.id as u16;
                let key = GlyphRasterKey {
                    font_id,
                    glyph_id,
                    size_q,
                    subpx_x: subpx.0,
                    var_hash,
                    hint,
                };

                let entry = raster_cache.get_or_insert(key, atlases, || {
                    rasterizer.rasterize(font_ref, glyph_id, font_size, subpx, hint)
                });

                let Some(entry) = entry else {
                    stats.glyphs_dropped_atlas_full += 1;
                    continue;
                };

                out.push(DrawableGlyph {
                    px: abs_x,
                    py: abs_y,
                    width: entry.width,
                    height: entry.height,
                    left: entry.left,
                    top: entry.top,
                    content: entry.content,
                    atlas_slot: entry.atlas_slot,
                    brush,
                });
                stats.glyphs_rastered += 1;
            }
        }
    }
}

fn first_brush_in_run(run: &parley::layout::GlyphRun<'_, Brush>) -> Brush {
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
    use std::num::NonZeroUsize;

    use kasane_core::config::FontConfig;
    use kasane_core::protocol::{Atom, Face, Style};

    fn line(text: &str) -> StyledLine {
        let atoms = vec![Atom {
            face: Face::default(),
            contents: text.into(),
        }];
        StyledLine::from_atoms(
            &atoms,
            &Style::default(),
            Brush::opaque(255, 255, 255),
            14.0,
            None,
        )
    }

    /// CPU-only atlas pair used by the unit tests in lieu of a real
    /// `GpuAtlasShelf`. Mirrors the test stub that lives in
    /// `raster_cache::tests` so build_frame can drive the cache without
    /// pulling in wgpu.
    struct TestAtlases {
        mask: super::super::atlas::AtlasShelf,
        color: super::super::atlas::AtlasShelf,
    }

    impl TestAtlases {
        fn new(side: u16) -> Self {
            Self {
                mask: super::super::atlas::AtlasShelf::new(side),
                color: super::super::atlas::AtlasShelf::new(side),
            }
        }
    }

    impl AtlasOps for TestAtlases {
        fn allocate(
            &mut self,
            content: super::super::glyph_rasterizer::ContentKind,
            w: u16,
            h: u16,
            _data: &[u8],
        ) -> Option<super::super::atlas::AtlasSlot> {
            let atlas = match content {
                super::super::glyph_rasterizer::ContentKind::Mask => &mut self.mask,
                super::super::glyph_rasterizer::ContentKind::Color => &mut self.color,
            };
            atlas.allocate(w, h)
        }

        fn deallocate(
            &mut self,
            content: super::super::glyph_rasterizer::ContentKind,
            slot: &super::super::atlas::AtlasSlot,
        ) {
            let atlas = match content {
                super::super::glyph_rasterizer::ContentKind::Mask => &mut self.mask,
                super::super::glyph_rasterizer::ContentKind::Color => &mut self.color,
            };
            atlas.deallocate(slot);
        }
    }

    fn make_state() -> (
        ParleyText,
        LayoutCache,
        GlyphRasterizer,
        GlyphRasterCache,
        TestAtlases,
    ) {
        (
            ParleyText::new(&FontConfig::default()),
            LayoutCache::new(),
            GlyphRasterizer::new(),
            GlyphRasterCache::new(NonZeroUsize::new(2048).unwrap()),
            TestAtlases::new(1024),
        )
    }

    #[test]
    fn build_frame_emits_glyphs_for_one_line() {
        let (mut text, mut layout_cache, mut rasterizer, mut raster_cache, mut atlases) =
            make_state();
        let l = line("hello");
        let lines = [FrameLine {
            line_idx: 0,
            origin_x: 100.0,
            origin_y: 50.0,
            line: &l,
        }];
        let (glyphs, stats) = build_frame(
            &mut text,
            &mut layout_cache,
            &mut rasterizer,
            &mut raster_cache,
            &mut atlases,
            &lines,
            true,
        );
        assert!(!glyphs.is_empty(), "expected glyphs from 'hello'");
        assert_eq!(stats.layouts_walked, 1);
        assert_eq!(stats.glyphs_rastered as usize, glyphs.len());
        assert_eq!(stats.glyphs_dropped_no_font_ref, 0);
        assert_eq!(stats.glyphs_dropped_atlas_full, 0);
    }

    #[test]
    fn build_frame_drawables_anchored_to_origin() {
        let (mut text, mut layout_cache, mut rasterizer, mut raster_cache, mut atlases) =
            make_state();
        let l = line("a");
        let make_lines = |x: f32, y: f32| {
            [FrameLine {
                line_idx: 0,
                origin_x: x,
                origin_y: y,
                line: &l,
            }]
        };
        let (a, _) = build_frame(
            &mut text,
            &mut layout_cache,
            &mut rasterizer,
            &mut raster_cache,
            &mut atlases,
            &make_lines(0.0, 0.0),
            true,
        );
        // Reset L1 so the second build re-emits at the new origin
        // (placement depends on origin, which is not in the L1 key).
        layout_cache.invalidate_all();
        let (b, _) = build_frame(
            &mut text,
            &mut layout_cache,
            &mut rasterizer,
            &mut raster_cache,
            &mut atlases,
            &make_lines(50.0, 100.0),
            true,
        );
        assert!(!a.is_empty());
        assert!(!b.is_empty());
        assert!(
            (b[0].px - a[0].px - 50.0).abs() < 0.1,
            "{} vs {}",
            b[0].px,
            a[0].px
        );
        assert!(
            (b[0].py - a[0].py - 100.0).abs() < 0.1,
            "{} vs {}",
            b[0].py,
            a[0].py
        );
    }

    #[test]
    fn build_frame_warm_l1_l2_path_drops_to_zero_misses() {
        let (mut text, mut layout_cache, mut rasterizer, mut raster_cache, mut atlases) =
            make_state();
        let l = line("hello world");
        let lines = [FrameLine {
            line_idx: 0,
            origin_x: 0.0,
            origin_y: 0.0,
            line: &l,
        }];
        // Warm-up frame.
        let _ = build_frame(
            &mut text,
            &mut layout_cache,
            &mut rasterizer,
            &mut raster_cache,
            &mut atlases,
            &lines,
            true,
        );
        let _ = layout_cache.take_stats();
        let _ = raster_cache.take_stats();
        // Hot frame.
        let (drawables, _) = build_frame(
            &mut text,
            &mut layout_cache,
            &mut rasterizer,
            &mut raster_cache,
            &mut atlases,
            &lines,
            true,
        );
        let l1 = layout_cache.take_stats();
        let l2 = raster_cache.take_stats();
        assert_eq!(l1.misses, 0, "warm L1 must have zero misses");
        assert_eq!(l1.hits, 1);
        assert_eq!(l2.misses, 0, "warm L2 must have zero misses");
        assert!(l2.hits >= drawables.len() as u32);
    }

    #[test]
    fn build_frame_multi_line_independent_caching() {
        let (mut text, mut layout_cache, mut rasterizer, mut raster_cache, mut atlases) =
            make_state();
        let l0 = line("first");
        let l1 = line("second");
        let lines = [
            FrameLine {
                line_idx: 0,
                origin_x: 0.0,
                origin_y: 0.0,
                line: &l0,
            },
            FrameLine {
                line_idx: 1,
                origin_x: 0.0,
                origin_y: 18.0,
                line: &l1,
            },
        ];
        let (drawables, stats) = build_frame(
            &mut text,
            &mut layout_cache,
            &mut rasterizer,
            &mut raster_cache,
            &mut atlases,
            &lines,
            true,
        );
        assert_eq!(stats.layouts_walked, 2);
        // Combined emission is the sum of both lines. With "first" + "second"
        // (5 + 6 chars) we expect ≥ 11 glyphs.
        assert!(
            drawables.len() >= 11,
            "two lines should yield ≥ 11 glyphs (got {})",
            drawables.len()
        );
        // Hot frame: every line hits L1.
        let _ = layout_cache.take_stats();
        let _ = build_frame(
            &mut text,
            &mut layout_cache,
            &mut rasterizer,
            &mut raster_cache,
            &mut atlases,
            &lines,
            true,
        );
        let l1 = layout_cache.take_stats();
        assert_eq!(l1.hits, 2, "both lines should hit on second frame");
        assert_eq!(l1.misses, 0);
    }

    #[test]
    fn empty_lines_yield_zero_drawables() {
        let (mut text, mut layout_cache, mut rasterizer, mut raster_cache, mut atlases) =
            make_state();
        let (drawables, stats) = build_frame(
            &mut text,
            &mut layout_cache,
            &mut rasterizer,
            &mut raster_cache,
            &mut atlases,
            &[],
            true,
        );
        assert!(drawables.is_empty());
        assert_eq!(stats.layouts_walked, 0);
        assert_eq!(stats.glyphs_emitted, 0);
    }

    #[test]
    fn cjk_line_produces_drawables() {
        let (mut text, mut layout_cache, mut rasterizer, mut raster_cache, mut atlases) =
            make_state();
        let l = line("こんにちは");
        let lines = [FrameLine {
            line_idx: 0,
            origin_x: 0.0,
            origin_y: 0.0,
            line: &l,
        }];
        let (drawables, stats) = build_frame(
            &mut text,
            &mut layout_cache,
            &mut rasterizer,
            &mut raster_cache,
            &mut atlases,
            &lines,
            true,
        );
        assert!(!drawables.is_empty(), "CJK should produce drawables");
        assert_eq!(stats.glyphs_dropped_no_font_ref, 0);
    }
}
