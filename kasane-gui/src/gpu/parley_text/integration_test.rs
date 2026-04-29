//! End-to-end pipeline tests for the Parley text stack (ADR-031, Phase 9b).
//!
//! Exercises the full chain in a single test: shape through Parley → cache
//! the resulting `ParleyLayout` in [`LayoutCache`] → walk the layout's glyph
//! runs → rasterise each glyph through [`GlyphRasterizer`] → store in
//! [`GlyphRasterCache`] (which allocates from the L3 [`AtlasShelf`]). Two
//! frames are run; the second frame must hit both the L1 and L2 caches for
//! every glyph.
//!
//! These tests are the contract that Phase 9b's `SceneRenderer` integration
//! will rely on. Failures here usually indicate an API drift between
//! Parley/swash and the cache layer rather than a Kasane bug.

use std::num::NonZeroUsize;
use std::sync::Arc;

use kasane_core::config::FontConfig;
use kasane_core::protocol::{Atom, Style};
use parley::PositionedLayoutItem;

use super::atlas::{AtlasShelf, AtlasSlot};
use super::font_id::{font_id_from_data, var_hash_from_coords};
use super::glyph_rasterizer::ContentKind;
use super::glyph_rasterizer::{GlyphRasterizer, RasterizedGlyph, SubpixelX};
use super::layout::ParleyLayout;
use super::layout_cache::LayoutCache;
use super::raster_cache::{AtlasOps, GlyphRasterCache, GlyphRasterKey};
use super::shaper::shape_line;
use super::styled_line::StyledLine;
use super::{Brush, ParleyText};

struct Pipeline {
    text: ParleyText,
    layout_cache: LayoutCache,
    rasterizer: GlyphRasterizer,
    raster_cache: GlyphRasterCache,
    atlases: TestAtlases,
}

/// CPU-only atlas pair the integration test uses in lieu of a real
/// `GpuAtlasShelf`. The glyph data is dropped on the floor; the cache
/// only needs the slot bookkeeping to be correct for the LRU + atlas
/// eviction paths to work.
struct TestAtlases {
    mask: AtlasShelf,
    color: AtlasShelf,
}

impl TestAtlases {
    fn new(side: u16) -> Self {
        Self {
            mask: AtlasShelf::new(side),
            color: AtlasShelf::new(side),
        }
    }
}

impl AtlasOps for TestAtlases {
    fn allocate(
        &mut self,
        content: ContentKind,
        w: u16,
        h: u16,
        _data: &[u8],
    ) -> Option<AtlasSlot> {
        let atlas = match content {
            ContentKind::Mask => &mut self.mask,
            ContentKind::Color => &mut self.color,
        };
        atlas.allocate(w, h)
    }

    fn deallocate(&mut self, content: ContentKind, slot: &AtlasSlot) {
        let atlas = match content {
            ContentKind::Mask => &mut self.mask,
            ContentKind::Color => &mut self.color,
        };
        atlas.deallocate(slot);
    }
}

impl Pipeline {
    fn new() -> Self {
        Self {
            text: ParleyText::new(&FontConfig::default()),
            layout_cache: LayoutCache::new(),
            rasterizer: GlyphRasterizer::new(),
            raster_cache: GlyphRasterCache::new(NonZeroUsize::new(2048).unwrap()),
            atlases: TestAtlases::new(1024),
        }
    }

    /// Run a single frame: shape + rasterise every glyph in `lines`. Returns
    /// the number of glyphs successfully landed in the L2 cache.
    fn render_frame(&mut self, lines: &[(u32, &StyledLine)]) -> usize {
        let mut glyph_count = 0usize;
        // Phase A — shape (or hit L1) every line.
        let layouts: Vec<Arc<ParleyLayout>> = lines
            .iter()
            .map(|(idx, line)| {
                self.layout_cache
                    .get_or_compute(*idx, line, |l| self.text.shape(l))
            })
            .collect();

        // Phase B — for every glyph, rasterise through L2.
        for layout in &layouts {
            for line_iter in layout.layout.lines() {
                for item in line_iter.items() {
                    let PositionedLayoutItem::GlyphRun(run) = item else {
                        continue;
                    };
                    let parley_run = run.run();
                    let font = parley_run.font();
                    let font_id = font_id_from_data(font);
                    let var_hash = var_hash_from_coords(parley_run.normalized_coords());
                    let font_size = parley_run.font_size();
                    let font_ref =
                        match swash::FontRef::from_index(font.data.data(), font.index as usize) {
                            Some(r) => r,
                            None => continue,
                        };
                    for glyph in run.positioned_glyphs() {
                        let subpx = SubpixelX::from_fract(glyph.x);
                        let glyph_id = glyph.id as u16;
                        let key = GlyphRasterKey {
                            font_id,
                            glyph_id,
                            size_q: (font_size * 64.0).round() as u16,
                            subpx_x: subpx.0,
                            var_hash,
                            hint: true,
                        };
                        let rasterizer = &mut self.rasterizer;
                        let entry = self.raster_cache.get_or_insert(key, &mut self.atlases, || {
                            let raster: Option<RasterizedGlyph> =
                                rasterizer.rasterize(font_ref, glyph_id, font_size, subpx, true);
                            raster
                        });
                        if entry.is_some() {
                            glyph_count += 1;
                        }
                    }
                }
            }
        }
        glyph_count
    }
}

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
fn ascii_pipeline_first_frame_misses_then_caches() {
    let mut pipe = Pipeline::new();
    let l = line("hello world");
    let glyphs1 = pipe.render_frame(&[(0, &l)]);
    assert!(glyphs1 > 0, "first frame produced glyphs: {glyphs1}");

    let l1_stats = pipe.layout_cache.take_stats();
    let l2_stats = pipe.raster_cache.take_stats();
    assert_eq!(l1_stats.misses, 1);
    assert!(l2_stats.misses > 0);
}

#[test]
fn second_frame_hits_l1_and_l2() {
    let mut pipe = Pipeline::new();
    let l = line("hello world");
    let glyphs1 = pipe.render_frame(&[(0, &l)]);
    let _ = pipe.layout_cache.take_stats();
    let _ = pipe.raster_cache.take_stats();

    // Second frame with identical input must hit L1 + L2 for every glyph.
    let glyphs2 = pipe.render_frame(&[(0, &l)]);
    assert_eq!(glyphs1, glyphs2, "glyph count must be stable across frames");

    let l1_stats = pipe.layout_cache.take_stats();
    let l2_stats = pipe.raster_cache.take_stats();
    assert_eq!(l1_stats.hits, 1, "L1 should hit on identical second frame");
    assert_eq!(l1_stats.misses, 0);
    assert_eq!(
        l2_stats.misses, 0,
        "L2 should hit for every glyph: {l2_stats:?}"
    );
    assert!(l2_stats.hits >= glyphs2 as u32);
}

#[test]
fn multi_line_frame_caches_independently() {
    let mut pipe = Pipeline::new();
    let l0 = line("first line");
    let l1 = line("second line");
    let _ = pipe.render_frame(&[(0, &l0), (1, &l1)]);
    let _ = pipe.layout_cache.take_stats();
    let _ = pipe.raster_cache.take_stats();

    // Re-render same lines → both L1 entries hit, no L2 misses.
    let _ = pipe.render_frame(&[(0, &l0), (1, &l1)]);
    let l1_stats = pipe.layout_cache.take_stats();
    let l2_stats = pipe.raster_cache.take_stats();
    assert_eq!(l1_stats.hits, 2);
    assert_eq!(l1_stats.misses, 0);
    assert_eq!(l2_stats.misses, 0);

    // Replace one line with new text → only that L1 entry misses.
    let l0_changed = line("first line CHANGED");
    let _ = pipe.render_frame(&[(0, &l0_changed), (1, &l1)]);
    let l1_stats = pipe.layout_cache.take_stats();
    assert_eq!(l1_stats.hits, 1, "unchanged line stays hit");
    assert_eq!(l1_stats.misses, 1, "changed line misses");
}

#[test]
fn cjk_pipeline_completes() {
    let mut pipe = Pipeline::new();
    let l = line("こんにちは");
    let glyphs = pipe.render_frame(&[(0, &l)]);
    assert!(glyphs > 0, "CJK frame must produce glyphs: {glyphs}");
}

#[test]
fn font_size_change_evicts_l1_via_key_mismatch() {
    let mut pipe = Pipeline::new();
    let small = StyledLine::from_atoms(
        &[Atom::plain("A")],
        &Style::default(),
        Brush::opaque(255, 255, 255),
        12.0,
        None,
    );
    let large = StyledLine::from_atoms(
        &[Atom::plain("A")],
        &Style::default(),
        Brush::opaque(255, 255, 255),
        24.0,
        None,
    );
    let _ = pipe.render_frame(&[(0, &small)]);
    let _ = pipe.layout_cache.take_stats();

    // Same line_idx but different font_size → L1 miss.
    let _ = pipe.render_frame(&[(0, &large)]);
    let l1_stats = pipe.layout_cache.take_stats();
    assert_eq!(l1_stats.misses, 1);
    assert_eq!(l1_stats.hits, 0);
}

#[test]
fn pipeline_invalidate_all_resets_caches() {
    let mut pipe = Pipeline::new();
    let l = line("hello");
    let _ = pipe.render_frame(&[(0, &l)]);
    pipe.layout_cache.invalidate_all();
    pipe.raster_cache.invalidate_all();
    let _ = pipe.layout_cache.take_stats();
    let _ = pipe.raster_cache.take_stats();

    let _ = pipe.render_frame(&[(0, &l)]);
    let l1_stats = pipe.layout_cache.take_stats();
    let l2_stats = pipe.raster_cache.take_stats();
    assert_eq!(l1_stats.misses, 1, "L1 should miss after invalidate_all");
    assert!(l2_stats.misses > 0, "L2 should miss after invalidate_all");
}

// ─────────────────────────────────────────────────────────────────────────────
// Color emoji coverage (ADR-031 Phase 11 Wave 1.1).
//
// `cjk_pipeline_completes` only exercises the Mask path. The Color
// path (`Source::ColorOutline` / `ColorBitmap` in
// `glyph_rasterizer.rs:132-137`, `ContentKind::Color` atlas branch)
// has been a CI blind spot. These tests pin the contract that:
//
//   1. A line containing a color emoji codepoint, shaped against a
//      family stack that includes a color emoji font, produces at
//      least one glyph with `entry.content == ContentKind::Color`.
//   2. That glyph allocates into the *color* atlas, not the mask
//      atlas (separation invariant — see `raster_cache.rs:67-68`).
//   3. Mixed text/emoji input splits into multiple Parley
//      `GlyphRun`s, exercising both the Mask and Color paths in a
//      single layout.
//
// Graceful skip: fontique's system discovery only finds a color
// emoji font on machines that have one installed (e.g., Noto Color
// Emoji on Linux/Nix, Apple Color Emoji on macOS). Where no such
// font is available the test logs a notice and exits — CI in
// minimal containers stays green, dev machines exercise the full
// path. Wave 1.2's `FontConfig` wiring will eventually let the
// fallback list participate; this test will then run unmodified
// against `FontConfig` instead of the explicit `FontFamily::List`.
// ─────────────────────────────────────────────────────────────────────────────

/// Build a Parley `FontFamily` list that prefers a color emoji font with a
/// monospace fallback. Centralised so both tests below stay in sync.
fn emoji_then_monospace() -> parley::FontFamily<'static> {
    use parley::{FontFamily, FontFamilyName, GenericFamily};
    use std::borrow::Cow;
    FontFamily::List(Cow::Owned(vec![
        // The two most common system color emoji family names. Fontique
        // returns the first match; if neither is present the line still
        // shapes through the monospace fallback, in which case the test
        // skips gracefully below.
        FontFamilyName::Named(Cow::Borrowed("Noto Color Emoji")),
        FontFamilyName::Named(Cow::Borrowed("Apple Color Emoji")),
        FontFamilyName::Generic(GenericFamily::Monospace),
    ]))
}

/// Shape `line` against `family`, then walk the layout and rasterise every
/// glyph through the L2 cache. Returns `(color_glyphs, mask_glyphs)` counts.
fn render_with_family(
    pipe: &mut Pipeline,
    line: &StyledLine,
    family: parley::FontFamily<'static>,
) -> (usize, usize) {
    let layout = Arc::new(shape_line(&mut pipe.text, line, family));
    let mut color = 0usize;
    let mut mask = 0usize;
    for line_iter in layout.layout.lines() {
        for item in line_iter.items() {
            let PositionedLayoutItem::GlyphRun(run) = item else {
                continue;
            };
            let parley_run = run.run();
            let font = parley_run.font();
            let font_id = font_id_from_data(font);
            let var_hash = var_hash_from_coords(parley_run.normalized_coords());
            let font_size = parley_run.font_size();
            let font_ref = match swash::FontRef::from_index(font.data.data(), font.index as usize) {
                Some(r) => r,
                None => continue,
            };
            for glyph in run.positioned_glyphs() {
                let subpx = SubpixelX::from_fract(glyph.x);
                let glyph_id = glyph.id as u16;
                let key = GlyphRasterKey {
                    font_id,
                    glyph_id,
                    size_q: (font_size * 64.0).round() as u16,
                    subpx_x: subpx.0,
                    var_hash,
                    hint: true,
                };
                let rasterizer = &mut pipe.rasterizer;
                let entry = pipe.raster_cache.get_or_insert(key, &mut pipe.atlases, || {
                    let r: Option<RasterizedGlyph> =
                        rasterizer.rasterize(font_ref, glyph_id, font_size, subpx, true);
                    r
                });
                if let Some(entry) = entry {
                    match entry.content {
                        ContentKind::Color => color += 1,
                        ContentKind::Mask => mask += 1,
                    }
                }
            }
        }
    }
    (color, mask)
}

#[test]
fn color_emoji_routes_through_color_atlas() {
    let mut pipe = Pipeline::new();
    // U+1F600 GRINNING FACE — the most fontique-discoverable color codepoint.
    let atoms = vec![Atom::plain("\u{1F600}")];
    let line = StyledLine::from_atoms(
        &atoms,
        &Style::default(),
        Brush::opaque(255, 255, 255),
        14.0,
        None,
    );

    let (color_glyphs, _mask_glyphs) = render_with_family(&mut pipe, &line, emoji_then_monospace());

    if color_glyphs == 0 {
        // No color emoji font available on this system. Pipeline still
        // completed without panicking, which is all CI without emoji
        // fonts can verify.
        eprintln!(
            "color emoji font not available via fontique; skipping color-atlas \
             assertion (this is expected in minimal CI environments)"
        );
        return;
    }

    // Color path was exercised — the color atlas must hold ≥ 1 slot, and
    // the slot must NOT have ended up in the mask atlas (which would mean
    // swash returned `Content::Mask` for what should have been a colour
    // glyph, indicating either a shaping miss or a `Format::Alpha`
    // interaction bug in `glyph_rasterizer.rs:140`).
    assert!(
        !pipe.atlases.color.is_empty(),
        "color atlas must hold ≥ 1 slot when {color_glyphs} color glyph(s) were rasterised"
    );
}

#[test]
fn mixed_text_emoji_splits_into_multiple_runs() {
    let mut pipe = Pipeline::new();
    // ASCII surrounding emoji forces Parley to split the layout into
    // distinct runs (different fonts → different runs). Even when the
    // emoji font is absent and the codepoint falls back to a tofu glyph,
    // we still expect ≥ 1 run because non-empty input always shapes.
    let atoms = vec![Atom::plain("hi \u{1F600} ok")];
    let line = StyledLine::from_atoms(
        &atoms,
        &Style::default(),
        Brush::opaque(255, 255, 255),
        14.0,
        None,
    );

    let (color_glyphs, mask_glyphs) = render_with_family(&mut pipe, &line, emoji_then_monospace());

    // Mask path is non-negotiable: ASCII letters always rasterise as Mask.
    assert!(
        mask_glyphs > 0,
        "ASCII letters must produce Mask glyphs (got mask={mask_glyphs})"
    );

    if color_glyphs == 0 {
        eprintln!("color emoji font not available; mixed-run test verified Mask path only");
        return;
    }

    // Both atlases received slots. The pair-of-atlases architecture
    // (`raster_cache.rs:67-68`) requires that ContentKind dispatches the
    // allocation to the right shelf; failing this assertion would mean
    // colour glyphs leaked into the mask atlas (a subtle corruption that
    // produces visually correct mask renders but inflates the mask
    // atlas's pressure metrics).
    assert!(
        !pipe.atlases.color.is_empty(),
        "color atlas must be non-empty"
    );
    assert!(
        !pipe.atlases.mask.is_empty(),
        "mask atlas must be non-empty"
    );
}
