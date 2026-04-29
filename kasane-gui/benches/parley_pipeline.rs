//! Parley pipeline microbenchmarks (ADR-031, Phase 9b prep).
//!
//! Measures the CPU cost of the new Parley + swash + L1/L2/L3 pipeline so
//! we can compare it to the cosmic-text baseline captured at Phase 0
//! (see `baselines/pre-parley.tar.gz`). These numbers inform the Phase 11
//! go/no-go decision: if the steady-state warm-cache cost stays comfortably
//! below the 70 µs full-frame target, Parley is on track.
//!
//! Bench layout — single-line and per-frame variants:
//!
//! - `parley/shape_cold` — single line, fresh `LayoutContext` (worst case)
//! - `parley/shape_warm` — same line shaped repeatedly (LayoutContext
//!   reuse only, no L1)
//! - `parley/frame_cold/24_lines` — 24-line frame, empty L1+L2 (first
//!   frame after font change)
//! - `parley/frame_warm/24_lines` — 24-line frame with both caches hot
//!   (cursor-only edit case — the dominant production path)
//! - `parley/frame_one_line_changed/24_lines` — 24-line frame with one
//!   changed line (typical typing edit)
//!
//! These benches run end-to-end through Parley + swash + atlas eviction;
//! the cosmic-text path is not exercised so the numbers are directly
//! attributable to the Parley stack.

use std::num::NonZeroUsize;
use std::sync::Arc;

use criterion::{Criterion, criterion_group, criterion_main};
use kasane_core::config::FontConfig;
use kasane_core::protocol::{Atom, Color, Face, NamedColor, Style};
use parley::PositionedLayoutItem;

use kasane_gui::gpu::parley_text::atlas::{AtlasShelf, AtlasSlot};
use kasane_gui::gpu::parley_text::font_id::{font_id_from_data, var_hash_from_coords};
use kasane_gui::gpu::parley_text::glyph_rasterizer::ContentKind;
use kasane_gui::gpu::parley_text::glyph_rasterizer::{GlyphRasterizer, SubpixelX};
use kasane_gui::gpu::parley_text::layout::ParleyLayout;
use kasane_gui::gpu::parley_text::layout_cache::LayoutCache;
use kasane_gui::gpu::parley_text::raster_cache::{AtlasOps, GlyphRasterCache, GlyphRasterKey};
use kasane_gui::gpu::parley_text::styled_line::StyledLine;
use kasane_gui::gpu::parley_text::{Brush, ParleyText};

fn realistic_atoms(line_no: usize) -> Vec<Atom> {
    let kw_face = Face {
        fg: Color::Rgb {
            r: 255,
            g: 100,
            b: 0,
        },
        ..Face::default()
    };
    let var_face = Face {
        fg: Color::Rgb {
            r: 0,
            g: 200,
            b: 100,
        },
        ..Face::default()
    };
    let str_face = Face {
        fg: Color::Rgb {
            r: 100,
            g: 100,
            b: 255,
        },
        ..Face::default()
    };
    let semi_face = Face {
        fg: Color::Named(NamedColor::White),
        ..Face::default()
    };
    vec![
        Atom::with_style("let", Style::from_face(&kw_face)),
        Atom::plain(" "),
        Atom::with_style(format!("var_{line_no}"), Style::from_face(&var_face)),
        Atom::plain(" = "),
        Atom::with_style(format!("\"{line_no}_value\""), Style::from_face(&str_face)),
        Atom::with_style(";", Style::from_face(&semi_face)),
    ]
}

fn make_lines(count: usize) -> Vec<StyledLine> {
    (0..count)
        .map(|i| {
            let atoms = realistic_atoms(i);
            StyledLine::from_atoms(
                &atoms,
                &Style::default(),
                Brush::opaque(255, 255, 255),
                14.0,
                None,
            )
        })
        .collect()
}

/// CPU-only atlas pair that mirrors `GpuAtlasShelf` for the bench. The
/// production cache calls `AtlasOps` for allocate / deallocate; here we
/// just defer to a CPU `AtlasShelf` and drop the bitmap data.
struct BenchAtlases {
    mask: AtlasShelf,
    color: AtlasShelf,
}

impl BenchAtlases {
    fn new(side: u16) -> Self {
        Self {
            mask: AtlasShelf::new(side),
            color: AtlasShelf::new(side),
        }
    }
}

impl AtlasOps for BenchAtlases {
    fn allocate(
        &mut self,
        content: ContentKind,
        w: u16,
        h: u16,
        _data: &[u8],
    ) -> Option<AtlasSlot> {
        match content {
            ContentKind::Mask => self.mask.allocate(w, h),
            ContentKind::Color => self.color.allocate(w, h),
        }
    }

    fn deallocate(&mut self, content: ContentKind, slot: &AtlasSlot) {
        match content {
            ContentKind::Mask => self.mask.deallocate(slot),
            ContentKind::Color => self.color.deallocate(slot),
        }
    }
}

struct Pipeline {
    text: ParleyText,
    layout_cache: LayoutCache,
    rasterizer: GlyphRasterizer,
    raster_cache: GlyphRasterCache,
    atlases: BenchAtlases,
}

impl Pipeline {
    fn new() -> Self {
        Self {
            text: ParleyText::new(&FontConfig::default()),
            layout_cache: LayoutCache::new(),
            rasterizer: GlyphRasterizer::new(),
            raster_cache: GlyphRasterCache::new(NonZeroUsize::new(2048).unwrap()),
            atlases: BenchAtlases::new(1024),
        }
    }

    fn shape_line_only(&mut self, line_idx: u32, line: &StyledLine) -> Arc<ParleyLayout> {
        self.layout_cache
            .get_or_compute(line_idx, line, |l| self.text.shape(l))
    }

    fn render_frame(&mut self, lines: &[(u32, &StyledLine)]) -> usize {
        let mut glyph_count = 0usize;
        let layouts: Vec<Arc<ParleyLayout>> = lines
            .iter()
            .map(|(idx, line)| self.shape_line_only(*idx, line))
            .collect();
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
                    let Some(font_ref) =
                        swash::FontRef::from_index(font.data.data(), font.index as usize)
                    else {
                        continue;
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
                            rasterizer.rasterize(font_ref, glyph_id, font_size, subpx, true)
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

fn bench_shape_cold(c: &mut Criterion) {
    let lines = make_lines(1);
    c.bench_function("parley/shape_cold", |b| {
        b.iter_with_setup(
            || ParleyText::new(&FontConfig::default()),
            |mut text| text.shape(&lines[0]),
        );
    });
}

fn bench_shape_warm(c: &mut Criterion) {
    let lines = make_lines(1);
    c.bench_function("parley/shape_warm", |b| {
        let mut text = ParleyText::new(&FontConfig::default());
        b.iter(|| text.shape(&lines[0]));
    });
}

fn bench_frame_cold_24(c: &mut Criterion) {
    let lines = make_lines(24);
    c.bench_function("parley/frame_cold_24_lines", |b| {
        b.iter_with_setup(Pipeline::new, |mut pipe| {
            let refs: Vec<(u32, &StyledLine)> = lines
                .iter()
                .enumerate()
                .map(|(i, l)| (i as u32, l))
                .collect();
            pipe.render_frame(&refs)
        });
    });
}

fn bench_frame_warm_24(c: &mut Criterion) {
    let lines = make_lines(24);
    let mut pipe = Pipeline::new();
    let refs: Vec<(u32, &StyledLine)> = lines
        .iter()
        .enumerate()
        .map(|(i, l)| (i as u32, l))
        .collect();
    // Warm-up: prime both caches.
    let _ = pipe.render_frame(&refs);
    let _ = pipe.layout_cache.take_stats();
    let _ = pipe.raster_cache.take_stats();

    c.bench_function("parley/frame_warm_24_lines", |b| {
        b.iter(|| pipe.render_frame(&refs));
    });
}

fn bench_frame_one_line_changed_24(c: &mut Criterion) {
    let lines = make_lines(24);
    let mut pipe = Pipeline::new();
    let refs: Vec<(u32, &StyledLine)> = lines
        .iter()
        .enumerate()
        .map(|(i, l)| (i as u32, l))
        .collect();
    // Warm both caches with the original lines.
    let _ = pipe.render_frame(&refs);
    let _ = pipe.layout_cache.take_stats();
    let _ = pipe.raster_cache.take_stats();

    // Build an alternate set of lines for the "edit" variant (pre-built so the
    // bench measures the steady-state cost, not the StyledLine construction).
    let edited: Vec<StyledLine> = (0..24)
        .map(|i| {
            let mut atoms = realistic_atoms(i);
            // Mutate line 12 only.
            if i == 12 {
                atoms.push(Atom::plain(" // edited"));
            }
            StyledLine::from_atoms(
                &atoms,
                &Style::default(),
                Brush::opaque(255, 255, 255),
                14.0,
                None,
            )
        })
        .collect();
    let edited_refs: Vec<(u32, &StyledLine)> = edited
        .iter()
        .enumerate()
        .map(|(i, l)| (i as u32, l))
        .collect();

    c.bench_function("parley/frame_one_line_changed_24_lines", |b| {
        // Each iteration alternates between original and edited lines so the
        // cache state mimics typing — most lines hit, one misses.
        let mut toggle = false;
        b.iter(|| {
            toggle = !toggle;
            if toggle {
                pipe.render_frame(&edited_refs)
            } else {
                pipe.render_frame(&refs)
            }
        });
    });
}

criterion_group!(
    parley_pipeline,
    bench_shape_cold,
    bench_shape_warm,
    bench_frame_cold_24,
    bench_frame_warm_24,
    bench_frame_one_line_changed_24,
);
criterion_main!(parley_pipeline);
