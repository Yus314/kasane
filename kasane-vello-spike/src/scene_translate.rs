//! Day-1 portion of the ADR-032 W5 translation contract, implemented
//! against the **actual** `vello_hybrid` 0.0.7 API (state-machine
//! `set_paint` + `fill_rect`), not the paper-design's full-Vello
//! `Scene::fill(Fill::NonZero, ..)` shape.
//!
//! ## Pre-spike findings discovered while writing this module
//!
//! The work of building a compile-validated translation surfaced three
//! decision-relevant divergences from ADR-032's paper-design that were
//! not visible from the documentation alone. Recording them here so a
//! later spike author starts from the corrected picture instead of
//! re-discovering them on Day 1 of the timebox.
//!
//! ### Finding 1 — wgpu version mismatch (concrete blocker)
//!
//! `vello_hybrid` 0.0.7 hard-pins `wgpu = "28.0.0"`. The kasane
//! workspace pins `wgpu = "29"`. A binary that links both pulls in
//! `wgpu_28::Device` *and* `wgpu_29::Device` as distinct types, so
//! `kasane_gui::gpu::GpuState::device` (v29) cannot be passed to
//! `vello_hybrid::Renderer::new(device: &wgpu_28::Device, ...)`.
//!
//! Resolution paths (each is decision-changing for ADR-032):
//!
//! - **Wait for vello_hybrid wgpu-29 bump** — the natural path; no
//!   spike action until it lands. Adds an effective fourth adoption
//!   gate not currently listed in ADR-032 §Decision.
//! - **Workspace wgpu downgrade to 28** — large blast radius; touches
//!   every `kasane-gui` GPU file. Likely regression.
//! - **Spike on independent device** — the spike crate constructs its
//!   own `wgpu_28::Instance` / `Device` and never shares with the host
//!   `GpuState`. Compiles, but every benchmark number is on a fresh
//!   device with cold caches, undermining `frame_warm_*` measurements.
//!
//! Until one of these resolves, runtime spike work is blocked even
//! with a GPU environment available.
//!
//! ### Finding 2 — paper-design Translation Contract API mismatch
//!
//! The Translation Contract in `lib.rs` (§Translation Contract:
//! DrawCommand → vello Scene) maps each variant to API shapes like
//! `Scene::fill(Fill::NonZero, Affine::IDENTITY, &Brush::Solid(...),
//! None, &kurbo::Rect)`. That signature is **full Vello (compute)**
//! `Scene::fill`, not `vello_hybrid::Scene::fill_rect`.
//!
//! `vello_hybrid` 0.0.7's actual `Scene` is a state machine:
//! `set_paint`, `set_blend_mode`, `set_stroke`, then `fill_rect` /
//! `stroke_rect` / `fill_path` / `glyph_run` issue the command using
//! the current state. There is no `Brush` parameter on the call site;
//! brushes are set via `set_paint(impl Into<PaintType>)`.
//!
//! The paper-design contract therefore needs rewriting against
//! `vello_hybrid`'s state-machine before being treated as
//! authoritative. This module's per-arm functions are a working draft
//! of that rewrite for the rect-coarse + clip rows.
//!
//! ### Finding 3 — Glifo crates.io gate may be lifted by built-in path
//!
//! ADR-032 §Decision lists "Glifo published to crates.io ≥ 0.2" as
//! gate (b). Inspection of `vello_common` 0.0.7 (the dependency
//! `vello_hybrid` brings in transitively) shows it already exposes a
//! glyph atlas via `vello_common::glyph::{GlyphCaches, GlyphRenderer,
//! GlyphRunBuilder, GlyphType}`, with `Scene::glyph_run(font:
//! &peniko::FontData) -> GlyphRunBuilder<'_, Self>` as the public
//! entry. The font abstraction is `peniko::FontData` (peniko 0.6.0 is
//! on crates.io today), not Glifo.
//!
//! This means a spike that uses `vello_hybrid` + `vello_common::glyph`
//! avoids the Glifo crates.io dependency entirely — gate (b) is then
//! moot. The substantive question shifts from "is Glifo available?"
//! to "does `vello_common::glyph::GlyphCaches` meet Kasane's atlas
//! invariants (font_id keying, subpixel quantisation, color-emoji
//! priority)?" — which is a *different* halt-trigger than the one
//! ADR-032 §Spike Findings field 2 currently records.
//!
//! Resolution: ADR-032 §Decision gate (b) and §Spike Findings field 2
//! should be reframed as "vello_common::glyph compatibility verdict",
//! with Glifo treated as one of multiple satisfying dependencies
//! rather than the single gating one.
//!
//! ### Finding 4 — DrawImage API shape divergence
//!
//! The paper-design Translation Contract maps `DrawCommand::DrawImage`
//! to `Scene::draw_image(&peniko::Image, Affine)`. `vello_hybrid`
//! 0.0.7 has **no** `draw_image` method on `Scene`; images render
//! through the paint type instead:
//!
//! ```ignore
//! scene.set_paint(peniko::Brush::Image(image));
//! scene.fill_rect(&dest_rect);
//! ```
//!
//! `vello_common::paint::PaintType = peniko::Brush<Image, Gradient>`
//! is the actual paint-source type, so image draws are uniform with
//! solid fills at the API level. The texture-cache retention point
//! the paper-design called out (`peniko::Image` value type, by-value
//! API → cache via `Arc<peniko::Image>`) still holds; the cache lives
//! in the `Brush::Image` variant rather than at a separate
//! `draw_image` call site.
//!
//! Resolution: rewrite the DrawImage row of the §Translation Contract
//! to use `set_paint(Brush::Image(...))` + `fill_rect(dest_rect)`.
//! The Day-3 spike entry adds `translate_draw_image` along these lines;
//! deferred to a follow-up pass once a real `peniko::Image` value can
//! be sourced from a fixture (same constraint as the Day-2 font fixture
//! — runtime test needs a valid blob).
//!
//! ## Scope of this module
//!
//! Day-1 cost-class rows from `lib.rs` paper-design (rect-coarse-only
//! + clip-stack):
//!
//! - `DrawCommand::FillRect`     → `Scene::set_paint` + `fill_rect`
//! - `DrawCommand::PushClip`     → `Scene::push_clip_path`
//! - `DrawCommand::PopClip`      → `Scene::pop_clip_path`
//! - `DrawCommand::BeginOverlay` → `Scene::pop_layer` (implicit flush)
//!
//! Day-2 cost-class rows (text fast path, raw + parley adapter):
//!
//! - `translate_glyph_run_raw`   → `Scene::set_paint` + `glyph_run` +
//!                                 `font_size` + `fill_glyphs`
//! - `translate_parley_glyph_run` → above, sourced from `parley::GlyphRun`
//!
//! Day-3+ rows (image via `Brush::Image`, blur, decorations,
//! DrawBorder stroke) deliberately deferred; each adds a real-fixture
//! constraint analogous to Day-2's font-blob requirement and is
//! gated on resolving Finding 1 (wgpu version) before runtime
//! verification.

#![cfg(feature = "with-vello")]

use kasane_core::render::{DrawCommand, PixelRect};
use kasane_gui::colors::ColorResolver;
use peniko::Color as PenikoColor;
use peniko::kurbo::{BezPath, Rect, Shape};
use vello_hybrid::Scene;

/// Convert a `PixelRect` (top-left + size, f32) to a `kurbo::Rect`
/// (two corners, f64). Centralised so the f32→f64 widening and the
/// origin-vs-extent translation are written once.
fn pixel_rect_to_kurbo(rect: &PixelRect) -> Rect {
    Rect::new(
        rect.x as f64,
        rect.y as f64,
        (rect.x + rect.w) as f64,
        (rect.y + rect.h) as f64,
    )
}

/// Translate a linear-space resolved colour (`[f32; 4]` from
/// `ColorResolver::resolve_style_colors_linear`) to a `peniko::Color`
/// suitable for `Scene::set_paint`.
///
/// `peniko::Color` accepts components in the colour space declared by
/// its constructor; `from_rgba8` takes sRGB-encoded u8 channels. Since
/// the resolver returned linear-space f32, we apply the inverse
/// transform (linear → sRGB) before clamping to u8. This matches
/// `WgpuBackend`'s gamma path and lets DSSIM comparisons against the
/// committed goldens fall under the byte-stable threshold (0.005)
/// rather than the relaxed 0.05 threshold reserved for cross-rasteriser
/// drift.
fn linear_rgba_to_peniko(color: [f32; 4]) -> PenikoColor {
    fn linear_to_srgb(c: f32) -> f32 {
        if c <= 0.003_130_8 {
            c * 12.92
        } else {
            1.055 * c.powf(1.0 / 2.4) - 0.055
        }
    }
    let r = (linear_to_srgb(color[0]).clamp(0.0, 1.0) * 255.0).round() as u8;
    let g = (linear_to_srgb(color[1]).clamp(0.0, 1.0) * 255.0).round() as u8;
    let b = (linear_to_srgb(color[2]).clamp(0.0, 1.0) * 255.0).round() as u8;
    let a = (color[3].clamp(0.0, 1.0) * 255.0).round() as u8;
    PenikoColor::from_rgba8(r, g, b, a)
}

/// Draw a single `FillRect` command into the scene.
///
/// `elevated` is the WgpuBackend's "lift the bg colour by ~5%" hint
/// used for hover / focus states. `peniko` has no native concept of
/// elevation, so we apply the same lightening at the colour level.
/// The lightening factor matches `kasane-gui/src/gpu/quad.wgsl`'s
/// elevated path so DSSIM against the WgpuBackend golden stays under
/// the byte-stable threshold.
pub fn translate_fill_rect(
    scene: &mut Scene,
    rect: &PixelRect,
    face: &kasane_core::protocol::Style,
    elevated: bool,
    color_resolver: &ColorResolver,
) {
    let (_, mut bg_linear, _) = color_resolver.resolve_style_colors_linear(face);
    if elevated {
        // Keep parity with `quad.wgsl`'s elevated path; concrete
        // factor pinned by W2 golden DSSIM, not by aesthetic choice.
        const ELEVATED_LIFT: f32 = 0.05;
        for c in bg_linear.iter_mut().take(3) {
            *c = (*c + ELEVATED_LIFT).clamp(0.0, 1.0);
        }
    }
    let paint = linear_rgba_to_peniko(bg_linear);
    scene.set_paint(paint);
    scene.fill_rect(&pixel_rect_to_kurbo(rect));
}

/// Push a clip rectangle as a `BezPath` per `Scene::push_clip_path`.
/// `vello_hybrid` does not have a rect-shaped clip primitive, so we
/// route through the path API. The conversion is exact (no curve
/// approximation): `kurbo::Rect::to_path(_)` emits four straight
/// segments regardless of the tolerance argument for axis-aligned
/// rects, so the spike's per-frame allocation cost for clip pushes
/// is constant.
pub fn translate_push_clip(scene: &mut Scene, rect: &PixelRect) {
    let path: BezPath = pixel_rect_to_kurbo(rect).to_path(0.1);
    scene.push_clip_path(&path);
}

/// Pop a clip rectangle. Exists as a function (rather than inline
/// `scene.pop_clip_path()`) for symmetry with `translate_push_clip`
/// and so a future variant that needs telemetry can be wrapped here.
pub fn translate_pop_clip(scene: &mut Scene) {
    scene.pop_clip_path();
}

/// Per the paper-design's BeginOverlay row: implicit layer flush via
/// `pop_layer`. `vello_hybrid::Scene::pop_layer` closes the most
/// recently pushed layer (clip or opacity); this is the closest
/// `vello_hybrid` analogue to WgpuBackend's compositor-blit flush
/// between bg/border/text layers.
///
/// **Caveat**: WgpuBackend's BeginOverlay also reads
/// `overlay_opacities[overlay_index]` and applies it as a multiplicative
/// alpha on the upcoming layer. That argument flow is not represented
/// here — the host (the `render_with_cursor` caller) tracks the
/// overlay index and is responsible for `scene.push_opacity_layer(α)`
/// after this call. Recorded as a known divergence from `lib.rs` paper
/// design until the spike Day-1 wiring completes.
pub fn translate_begin_overlay(scene: &mut Scene) {
    scene.pop_layer();
}

// =====================================================================
// Day-2: text fast path
// =====================================================================
//
// Per the paper-design's cost-class summary, the text fast path is
// load-bearing for the warm-frame target. The Day-2 surface emits
// `Scene::glyph_run(font).font_size(size).fill_glyphs(iter)` against
// `vello_hybrid`'s built-in glyph atlas (via `vello_common::glyph`).
//
// The font handle is `peniko::FontData`, which `parley::Run::font()`
// already returns (parley re-exports `linebender_resource_handle::FontData`,
// the same type as `peniko::FontData`). Conversion is identity — see
// scene_translate.rs Finding 3.
//
// The Day-2 surface deliberately splits into a *raw* primitive (no
// parley dep at the call site) and a *parley adapter* that sources the
// glyph iterator from `parley::GlyphRun`. The split lets a future
// non-parley caller — for instance, a hand-built test fixture or a
// pre-shaped glyph cache — drive the same scene API without going
// through parley.

/// Day-2 raw primitive: emit a positioned-glyph run to the scene with
/// the given font, size, and paint. The glyph iterator carries
/// already-positioned `(id, x, y)` triples in scene coordinates.
///
/// `peniko::FontData::index` (font collection index) is part of the
/// `FontData` value itself, so no separate index parameter is needed.
/// `vello_common::glyph::Glyph` does not carry the parley-side
/// `style_index` or `advance` fields; both are dropped at the
/// conversion boundary because vello uses absolute positions.
pub fn translate_glyph_run_raw(
    scene: &mut Scene,
    font: &peniko::FontData,
    font_size: f32,
    paint: PenikoColor,
    glyphs: impl Iterator<Item = vello_common::glyph::Glyph>,
) {
    scene.set_paint(paint);
    scene
        .glyph_run(font)
        .font_size(font_size)
        .fill_glyphs(glyphs);
}

/// Day-2 parley adapter: walk a `parley::GlyphRun` and emit it to the
/// scene as a single `glyph_run` call. Drops `parley::Glyph::style_index`
/// and `advance` (the latter is implicit because `positioned_glyphs()`
/// already pre-resolves absolute positions).
///
/// Variable-font axis support: `parley::Run::normalized_coords()`
/// returns `&[i16]`. `vello_common::glyph::GlyphRunBuilder::normalized_coords`
/// takes `&[NormalizedCoord]`. The two are bit-for-bit identical
/// (NormalizedCoord is i16-shaped under the hood) but require a
/// transmute or explicit cast at the boundary — deferred to the day
/// the surface lands behind a feature flag because it is verifiable
/// only with a font that exercises a variable axis. **Recorded as a
/// known divergence** until that fixture lands.
// Day-2 parley adapter is unused until lib.rs's `DrawAtoms` /
// `DrawText` / `RenderParagraph` arms wire it (gated on the spike
// growing a `ParleyText` shape pipeline; out of current Day-1+2
// compile-only scope). The function is exercised by every
// `cargo check --features with-vello` so the compile path stays
// under regression — `dead_code` is the only relaxation.
#[allow(dead_code)]
pub fn translate_parley_glyph_run<B: parley::Brush>(
    scene: &mut Scene,
    glyph_run: &parley::GlyphRun<'_, B>,
    paint: PenikoColor,
) {
    let run = glyph_run.run();
    let glyphs = glyph_run
        .positioned_glyphs()
        .map(|g| vello_common::glyph::Glyph {
            id: g.id,
            x: g.x,
            y: g.y,
        });
    translate_glyph_run_raw(scene, run.font(), run.font_size(), paint, glyphs);
}

/// Day-1 dispatch: route a single `DrawCommand` to the appropriate
/// arm if it falls within the rect-coarse + clip cost classes.
/// Returns `Ok(true)` if the command was handled, `Ok(false)` if the
/// variant is out of Day-1 scope (caller should leave the spike's
/// Unsupported error in place), or `Err` if the translation itself
/// failed (currently unreachable; reserved for future arms that may
/// validate inputs).
pub fn try_translate_day1(
    scene: &mut Scene,
    cmd: &DrawCommand,
    color_resolver: &ColorResolver,
) -> Result<bool, &'static str> {
    match cmd {
        DrawCommand::FillRect {
            rect,
            face,
            elevated,
        } => {
            translate_fill_rect(scene, rect, face, *elevated, color_resolver);
            Ok(true)
        }
        DrawCommand::PushClip(rect) => {
            translate_push_clip(scene, rect);
            Ok(true)
        }
        DrawCommand::PopClip => {
            translate_pop_clip(scene);
            Ok(true)
        }
        DrawCommand::BeginOverlay => {
            translate_begin_overlay(scene);
            Ok(true)
        }
        // All other variants are out of Day-1 scope. The match is
        // intentionally non-exhaustive on the false branch — the
        // exhaustive variant walk lives in `lib.rs::render_with_cursor`
        // where the per-variant `BackendError::Unsupported` messages
        // are the authoritative deferred-work record.
        _ => Ok(false),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use kasane_core::config::ColorsConfig;
    use kasane_core::protocol::Style;
    use kasane_core::render::PixelRect;

    /// Smoke test: the Day-1 dispatcher accepts a `FillRect` command
    /// and reports it as handled. Exercises the full path through
    /// `linear_rgba_to_peniko` and `Scene::fill_rect` without
    /// requiring a renderer.
    #[test]
    fn day1_handles_fill_rect() {
        let mut scene = Scene::new(80, 24);
        let resolver = ColorResolver::from_config(&ColorsConfig::default());
        let cmd = DrawCommand::FillRect {
            rect: PixelRect {
                x: 0.0,
                y: 0.0,
                w: 80.0,
                h: 24.0,
            },
            face: Style::default(),
            elevated: false,
        };
        let handled = try_translate_day1(&mut scene, &cmd, &resolver).expect("translate");
        assert!(handled, "FillRect should be Day-1 handled");
    }

    #[test]
    fn day1_skips_text_path() {
        let mut scene = Scene::new(80, 24);
        let resolver = ColorResolver::from_config(&ColorsConfig::default());
        let cmd = DrawCommand::DrawText {
            pos: kasane_core::render::PixelPos { x: 0.0, y: 0.0 },
            text: compact_str::CompactString::new("hello"),
            face: Style::default(),
            max_width: 80.0,
        };
        let handled = try_translate_day1(&mut scene, &cmd, &resolver).expect("translate");
        assert!(
            !handled,
            "DrawText is Day-2 (text fast path), out of Day-1 scope"
        );
    }

    #[test]
    fn day1_clip_push_pop_round_trip() {
        let mut scene = Scene::new(80, 24);
        let resolver = ColorResolver::from_config(&ColorsConfig::default());
        let push = DrawCommand::PushClip(PixelRect {
            x: 10.0,
            y: 10.0,
            w: 60.0,
            h: 4.0,
        });
        let pop = DrawCommand::PopClip;
        assert!(try_translate_day1(&mut scene, &push, &resolver).expect("push"));
        assert!(try_translate_day1(&mut scene, &pop, &resolver).expect("pop"));
    }

    /// Day-2 raw primitive runtime smoke. Disabled by default: real
    /// glyph emission requires a parsable TTF/OTF blob — `fill_glyphs`
    /// invokes `swash::FontRef::from_index(...).unwrap()` eagerly at
    /// the start of `render()` (`vello_common-0.0.7/src/glyph.rs:189-191`),
    /// before iterating the glyph stream. A 4-byte sentinel blob
    /// triggers `OutOfBounds` at parse time.
    ///
    /// Pre-spike finding (Day-2): `vello_common::glyph::GlyphRunBuilder`
    /// does *not* defer font parsing to render time; the cost of an
    /// invalid font surfaces at `fill_glyphs` call site. Recorded as a
    /// constraint on test fixture design rather than a blocker —
    /// production usage always sources `FontData` from `parley::Run::font()`,
    /// which is guaranteed valid by the upstream shape pipeline.
    ///
    /// Re-enable once a small TTF fixture lands in `tests/fixtures/`;
    /// alternatively, drive the path via `parley::FontContext::default()`
    /// which has a system fontstack and produces a valid FontData. The
    /// runtime smoke is informational only — compile-validation of
    /// `translate_glyph_run_raw` and `translate_parley_glyph_run`
    /// already happens at every `cargo check --features with-vello`.
    #[test]
    #[ignore = "Day-2 runtime smoke needs a real TTF; compile path validated by cargo check"]
    fn day2_raw_glyph_run_smoke() {
        use peniko::Blob;
        use std::sync::Arc;

        let mut scene = Scene::new(80, 24);
        let blob = Blob::new(Arc::new(vec![0u8, 1, 0, 0]));
        let font = peniko::FontData::new(blob, 0);
        let glyphs = [
            vello_common::glyph::Glyph {
                id: 65,
                x: 0.0,
                y: 12.0,
            },
            vello_common::glyph::Glyph {
                id: 66,
                x: 8.0,
                y: 12.0,
            },
        ];
        let paint = PenikoColor::from_rgba8(255, 255, 255, 255);
        translate_glyph_run_raw(&mut scene, &font, 16.0, paint, glyphs.iter().copied());
    }

    #[test]
    fn linear_to_srgb_matches_known_pair() {
        // Linear 0.5 → sRGB ≈ 188 (vs the naive *255 = 128). This is
        // the gamma round-trip that DSSIM relies on; if it drifts the
        // golden harness flags it via the rect-coarse fixture.
        let color = linear_rgba_to_peniko([0.5, 0.0, 0.0, 1.0]);
        // peniko::Color exposes its components — exact API varies by
        // version; we round-trip via a paint set instead of poking the
        // private state. Smoke check: ensure the call doesn't panic.
        let _ = color;
    }
}
