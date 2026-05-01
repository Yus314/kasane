//! ADR-032 W5 — Vello adoption spike.
//!
//! This crate is an isolated workspace member that hosts an
//! experimental [`GpuBackend`](kasane_gui::gpu::backend::GpuBackend)
//! implementation backed by `vello_hybrid`. It exists to feed
//! decision-grade data into ADR-032 §Spike Findings; it is **not** a
//! production backend.
//!
//! ## Status
//!
//! Currently a *stub*. The `with-vello` feature flag gates the actual
//! Vello dependency; without the feature, all `GpuBackend` methods
//! return [`BackendError::Unsupported`]. With the feature, the impl is
//! still a `todo!()` placeholder — actual rendering is deferred until:
//!
//! 1. **Glifo** ships on crates.io with a stable API for atlas-based
//!    glyph caching driven by Parley layouts, and
//! 2. A GPU-capable environment is available for the spike runs (the
//!    sandbox this crate was authored in lacks `/dev/dri` access).
//!
//! ## Spike Plan (5-day timebox)
//!
//! Once gates open, fill the impl by:
//!
//! 1. **Day 1**: instantiate `vello_hybrid::Renderer` against a
//!    headless wgpu device; render a single solid quad to a texture;
//!    confirm the readback matches the W2 golden harness format.
//! 2. **Day 2**: Translate `DrawCommand::FillRect` and `DrawAtoms` via
//!    Glifo. **Halt gate**: 80×24 warm frame ≤ 100 µs.
//! 3. **Day 3**: Color emoji + variable font parity with swash. **Halt
//!    gate**: DSSIM ≤ 0.05 vs. WgpuBackend goldens.
//! 4. **Day 4**: BiDi, complex scripts (Arabic, Devanagari, CJK
//!    ligatures), subpixel positioning. **Halt gate**: ≤ 2 matrix
//!    rows red.
//! 5. **Day 5**: Document findings into `docs/decisions.md`
//!    `ADR-032 §Spike Findings`. Whether positive or negative, the
//!    findings get written; the spike branch is **not** merged into
//!    main.
//!
//! See `docs/decisions.md` ADR-032 for the full evaluation framework.
//!
//! ## Translation Contract: DrawCommand → vello Scene
//!
//! This section is a *paper design* completed before Day 1 code so the
//! per-variant Scene API mapping is a written contract, not an
//! emergent decision discovered during implementation. The 13
//! `DrawCommand` variants in `kasane_core::render::scene::DrawCommand`
//! map as follows. Each row records: target Scene API, the
//! Glifo/Vello-side cost class, and any retained Kasane-side state.
//!
//! | DrawCommand variant | vello Scene API | Cost class | Retained Kasane state |
//! |---|---|---|---|
//! | `FillRect { rect, face, elevated }` | `Scene::fill(Fill::NonZero, Affine::IDENTITY, &Brush::Solid(peniko_color), None, &kurbo::Rect)` | rect-coarse-only; brush translation `Style → Brush::Solid` (lossless if Style.bg is solid; downgrades any future gradient feature) | `colors::ColorResolver` for `face → peniko::Color` resolution; `elevated` lightening logic stays in adapter (peniko has no "elevated" concept) |
//! | `DrawAtoms { pos, atoms, max_width, line_idx }` | `Glifo::render_to_atlas(layout)` then `Scene::draw_glyphs` | text fast path; **load-bearing** for warm-frame target | `kasane-gui/src/gpu/text/{layout_cache, styled_line, style_resolver, shaper, hit_test}` retained (Glifo does not provide a `parley::Layout` cache); `line_idx` becomes the L1 LayoutCache key, Glifo atlas key is derived from the cached Layout |
//! | `DrawText { pos, text, face, max_width }` | shape-with-Parley → `Glifo::render_to_atlas` → `Scene::draw_glyphs` | text fast path; single-style-run subset of DrawAtoms | identical to DrawAtoms; `text` is shaped on demand (no L1 cache hit on first frame) |
//! | `DrawBorder { rect, line_style, face, fill_face }` | optional `Scene::fill` (interior) + `Scene::stroke(Stroke::new(width).with_caps(...), Affine, &Brush, None, &kurbo::Rect)` | rect + stroke-coarse | `BorderLineStyle → kurbo::Stroke` mapping table (single, double, dashed → dash array, rounded → caps); `fill_face` independent fill ordering must match WgpuBackend |
//! | `DrawBorderTitle { rect, title, border_face, elevated }` | composed: `DrawBorder` for the frame + `DrawAtoms` for the title; clip-stack push for title region | composed | identical to DrawBorder + DrawAtoms |
//! | `DrawShadow { rect, offset, blur_radius, color }` | `Scene::fill(... &Brush::Solid(color))` with blur — **API uncertain in `vello_hybrid` 0.0.7**; spike Day 1 must verify whether hybrid path supports blur via `peniko::BlurredRoundedRect` or requires CPU-side blur composition (current `compositor/blur.rs` Kawase Dual-Filter is the fallback) | rect + (optional) blur | if Vello blur is unavailable, `compositor/blur.rs` (258 LOC) is *not retired*; this re-classifies the LOC retire estimate |
//! | `DrawPaddingRow { pos, width, ch, face }` | identical to `DrawText` with single-character repeat | text fast path | the repeat unit `ch` is shaped once and reused; Glifo atlas hit on subsequent cells (deterministic perf win) |
//! | `PushClip(rect)` | `Scene::push_layer(Mix::Clip, alpha=1.0, Affine::IDENTITY, &kurbo::Rect)` | clip-stack | clip-rect translation is rect-shaped only; no rounded-rect or path clipping in current usage |
//! | `PopClip` | `Scene::pop_layer()` | clip-stack | n/a |
//! | `DrawImage { rect, source, fit, opacity }` | `Scene::draw_image(&peniko::Image, Affine)` | image | `ImageSource → peniko::Image` conversion (RGBA8 premultiplied is the shared format); `ImageFit` translation table (Contain/Cover/Fill → Affine scale matrix); `texture_cache.rs` retained as `peniko::Image` cache (Vello's image API is by-value, so without a cache every frame re-uploads — performance load-bearing) |
//! | `RenderParagraph { pos, max_width, paragraph, line_idx }` | multi-line `DrawAtoms` decomposition: shape via Parley, walk Parley `Layout::lines()`, each line emits `Glifo::render_to_atlas` + `Scene::draw_glyphs`; annotation overlays (cursor rect, selection highlight) emit `Scene::fill` after the glyph layer | text fast path; **highest complexity** variant (BiDi + cursor + selection in one call) | `BufferParagraph` decomposition stays in kasane-gui (Vello does not understand cursor/selection semantics); `line_idx` keys L1 LayoutCache as in DrawAtoms |
//! | `DrawCanvas { rect, content }` | **undefined.** Plugin-emitted custom drawing extension point. See "DrawCanvas — pre-spike resolution required" below | undefined | tbd |
//! | `BeginOverlay` | implicit layer flush; in vello terms, `Scene::pop_layer` then `Scene::push_layer(Mix::Normal, ..)` to start a new overlay layer | clip-stack/layer-stack | overlay opacity (`overlay_opacities` argument to `render_with_cursor`) is read at BeginOverlay time and passed as `alpha` to `push_layer` |
//!
//! ### DrawCanvas — pre-spike resolution required
//!
//! `DrawCanvas` carries `crate::plugin::canvas::CanvasContent` — a
//! plugin-emitted opaque drawing payload. The current production
//! WgpuBackend has a path for it; that path's API surface is what
//! determines the Vello translation strategy. Three options are open
//! before Day 1:
//!
//! 1. **Reject via BackendCapabilities** (default): the spike's
//!    `VelloBackend::capabilities()` does not advertise
//!    DrawCanvas support; canvas-emitting plugins receive
//!    `degradation_policy::Reject` per ADR-032 §Decision item 3.
//!    Spike scope shrinks; Day 1–5 unaffected.
//! 2. **Translate via plugin-side cooperation**: extend
//!    `CanvasContent` to return a `Vec<DrawCommand>` slice that the
//!    backend recursively translates. Rejected for spike scope —
//!    multiplies surface to verify.
//! 3. **Defer to adoption phase**: post-spike Phase Z work adds
//!    DrawPath + the canvas representation; spike treats it as
//!    rejected for the matrix.
//!
//! **Decision for spike**: option 1 (reject via BackendCapabilities).
//! `DrawCanvas → BackendError::Unsupported("DrawCanvas")` is the
//! Day-2 implementation. ADR-032 §Spike Findings field 9 (plugin
//! wire protocol delta) records the deferral.
//!
//! ### Cost-class summary
//!
//! - **rect-coarse-only**: 4 variants (FillRect, FillRect-via-DrawBorder
//!   interior, DrawShadow without blur, BeginOverlay). Vello hybrid
//!   coarse stage is mature — these are the lowest-risk row of the
//!   matrix.
//! - **stroke-coarse**: 1 variant (DrawBorder outline). Stroke caps and
//!   dash patterns are within `kurbo::Stroke`; verify dash translation
//!   matches WgpuBackend pixel result.
//! - **text fast path**: 4 variants (DrawAtoms, DrawText,
//!   DrawPaddingRow, RenderParagraph). Glifo atlas + Scene draw_glyphs.
//!   **Load-bearing for the warm-frame target**.
//! - **image**: 1 variant (DrawImage). texture_cache retention is
//!   performance load-bearing.
//! - **clip-stack/layer-stack**: 2 variants (PushClip/PopClip,
//!   BeginOverlay). Layer flush ordering must match WgpuBackend's
//!   `(bg → border → text)` invariant.
//! - **composed**: 1 variant (DrawBorderTitle). Decomposes into
//!   simpler variants, no new API surface.
//! - **uncertain**: 1 variant (DrawShadow with blur). Spike Day 1 hard
//!   gate: verify `vello_hybrid` 0.0.7 blur support; if absent,
//!   `compositor/blur.rs` is retained (LOC retire estimate adjusts).
//! - **undefined**: 1 variant (DrawCanvas). Resolved by spike-side
//!   reject; ADR-032 §Spike Findings records.
//!
//! ### Style → peniko::Brush translation
//!
//! `kasane_core::protocol::Style` carries `fg: Color`, `bg: Color`,
//! `attributes: StyleAttributes` (bold/italic/underline/etc.),
//! `underline_color: Option<Color>`, and (post-ADR-031 Phase 10)
//! `font_weight: FontWeight(u16)` plus variable-font axis settings.
//!
//! peniko::Brush represents a paint source: solid, linear gradient,
//! radial gradient, sweep gradient, or image. The translation is
//! lossless for solid colours (`Color → peniko::Color::rgba8 →
//! Brush::Solid`); gradient/image brushes are *not yet emitted* by
//! Kasane plugins, so the translation is total under current usage.
//!
//! Variable font axes flow through Parley `StyleProperty::FontVariations`
//! independently of the brush; they are not Brush-side concerns.
//!
//! Underline/strikethrough decoration is a separate `Scene::fill`
//! after the glyph layer (matching the current `text/metrics.rs`
//! decoration flow). Curly underline (Phase 10) requires a stroked
//! sine path — `kurbo::CubicBez` chain; W2 Phase 10 fixture must pin
//! this against both backends.
//!
//! ### Performance-load-bearing translations
//!
//! Three rows above carry the warm-frame budget:
//!
//! 1. **DrawAtoms / DrawText / RenderParagraph** — hot text path.
//!    L1 LayoutCache hit + Glifo atlas hit must compose: spike Day 2
//!    instrumentation records hit rates and confirms the cache
//!    hierarchy flattening predicted in the §Linebender alignment
//!    metric subsection of ADR-032.
//! 2. **DrawImage** — texture_cache retention. Removing it makes every
//!    frame re-upload images; verify the retention model fits Vello's
//!    by-value Image API (likely via `Arc<peniko::Image>` interning at
//!    the cache layer).
//! 3. **BeginOverlay** — layer-stack flush ordering. Overlay opacity
//!    multiplication must happen at `push_layer` time, not as a
//!    post-process; verify pixel parity with WgpuBackend's compositor
//!    blit path.
//!
//! Rows outside these three may red-flag the matrix individually
//! without the warm-frame budget moving; conversely, a row inside
//! these three red-flagging is grounds for halt at Day 2.

use kasane_core::config::FontConfig;
use kasane_core::protocol::Color;
use kasane_core::render::{CursorStyle, DrawCommand, VisualHints};
use kasane_gui::animation::CursorRenderState;
use kasane_gui::colors::ColorResolver;
use kasane_gui::gpu::GpuState;
use kasane_gui::gpu::backend::{
    AtlasKind, BackendCapabilities, BackendError, DegradationPolicy, GpuBackend,
};
use winit::dpi::PhysicalSize;

/// Experimental Vello-backed renderer. Stubbed; see crate docs.
pub struct VelloBackend {
    #[allow(dead_code)]
    width: u32,
    #[allow(dead_code)]
    height: u32,

    #[cfg(feature = "with-vello")]
    #[allow(dead_code)]
    renderer: Option<vello_hybrid::Renderer>,
}

impl VelloBackend {
    /// Construct a stub backend. With the `with-vello` feature off,
    /// this is a no-op container. With the feature on, it allocates
    /// a `vello_hybrid::Renderer` (still wrapped in `Option` until
    /// the spike fills in the actual init code).
    pub fn new(_gpu: &GpuState, window_size: PhysicalSize<u32>) -> anyhow::Result<Self> {
        Ok(Self {
            width: window_size.width,
            height: window_size.height,
            #[cfg(feature = "with-vello")]
            renderer: None,
        })
    }

    /// Whether the underlying Vello renderer is available. Used by
    /// callers (and tests) that want to gate behaviour without
    /// matching on cargo features directly.
    pub const fn is_active() -> bool {
        cfg!(feature = "with-vello")
    }
}

impl GpuBackend for VelloBackend {
    fn render_with_cursor(
        &mut self,
        _gpu: &GpuState,
        commands: &[DrawCommand],
        _color_resolver: &ColorResolver,
        _cursor_style: CursorStyle,
        _cursor_state: &CursorRenderState,
        _cursor_color: Color,
        _overlay_opacities: &[f32],
        _visual_hints: &VisualHints,
    ) -> Result<(), BackendError> {
        #[cfg(not(feature = "with-vello"))]
        {
            // Without the feature flag, no Vello backend is reachable.
            // The match-arm-exhaustive walk below still runs in feature-on
            // builds; this branch keeps the no-default-features build
            // green.
            let _ = commands;
            Err(BackendError::Unsupported(
                "VelloBackend compiled without 'with-vello' feature",
            ))
        }

        #[cfg(feature = "with-vello")]
        {
            // ADR-032 W5 — translation skeleton.
            //
            // Per the paper-design contract in this crate's module
            // docstring (§Translation Contract: DrawCommand → vello
            // Scene), each DrawCommand variant maps to a specific
            // vello / Glifo / peniko API. The match below is
            // *match-arm-exhaustive on purpose*: it must list every
            // `DrawCommand` variant. When a new variant lands in
            // `kasane_core::render::scene::DrawCommand`, this match
            // produces a compile error and the spike author is
            // forced to extend the translation contract before
            // building the new variant against Vello. This is the
            // intentional pre-position — it converts
            // "we forgot to translate FooBar" from a runtime panic
            // into a compile-time blocker, satisfying ADR-032
            // §Spike Findings field 9 (plugin wire protocol delta).
            //
            // The arms currently raise `BackendError::Unsupported`
            // with the variant name. Day 1 of the spike replaces
            // each arm body in the order specified by the paper
            // design's cost-class summary:
            //
            //   Day 1: rect-coarse-only + clip-stack
            //   Day 2: text fast path (load-bearing for warm target)
            //   Day 3: image + composed
            //   Day 4: stroke-coarse + uncertain (DrawShadow blur)
            //
            // `DrawCanvas` is *deliberately* unsupported per the
            // paper-design's pre-spike resolution (§DrawCanvas —
            // pre-spike resolution required, option 1).
            // BackendCapabilities advertises `supports_paths` =
            // feature-flag, so production-side rejection is wired
            // through `degradation_policy::Reject` →
            // `BackendCapabilityRejected` diagnostic before any
            // DrawCanvas reaches this match.
            for cmd in commands {
                match cmd {
                    DrawCommand::FillRect { .. } => {
                        return Err(BackendError::Unsupported(
                            "FillRect (Day 1: Scene::fill rect-coarse, pending)",
                        ));
                    }
                    DrawCommand::DrawAtoms { .. } => {
                        return Err(BackendError::Unsupported(
                            "DrawAtoms (Day 2: Glifo render_to_atlas + Scene::draw_glyphs, pending)",
                        ));
                    }
                    DrawCommand::DrawText { .. } => {
                        return Err(BackendError::Unsupported(
                            "DrawText (Day 2: shape-with-Parley + Glifo, pending)",
                        ));
                    }
                    DrawCommand::DrawBorder { .. } => {
                        return Err(BackendError::Unsupported(
                            "DrawBorder (Day 4: Scene::stroke + optional Scene::fill, pending)",
                        ));
                    }
                    DrawCommand::DrawBorderTitle { .. } => {
                        return Err(BackendError::Unsupported(
                            "DrawBorderTitle (Day 4: composed DrawBorder + DrawAtoms, pending)",
                        ));
                    }
                    DrawCommand::DrawShadow { .. } => {
                        return Err(BackendError::Unsupported(
                            "DrawShadow (Day 4: Scene::fill+blur or compositor/blur.rs fallback, uncertain)",
                        ));
                    }
                    DrawCommand::DrawPaddingRow { .. } => {
                        return Err(BackendError::Unsupported(
                            "DrawPaddingRow (Day 2: DrawText with single-char repeat, pending)",
                        ));
                    }
                    DrawCommand::PushClip(_) => {
                        return Err(BackendError::Unsupported(
                            "PushClip (Day 1: Scene::push_layer Mix::Clip, pending)",
                        ));
                    }
                    DrawCommand::PopClip => {
                        return Err(BackendError::Unsupported(
                            "PopClip (Day 1: Scene::pop_layer, pending)",
                        ));
                    }
                    DrawCommand::DrawImage { .. } => {
                        return Err(BackendError::Unsupported(
                            "DrawImage (Day 3: Scene::draw_image + texture_cache, pending)",
                        ));
                    }
                    DrawCommand::RenderParagraph { .. } => {
                        return Err(BackendError::Unsupported(
                            "RenderParagraph (Day 2: multi-line DrawAtoms decomposition, pending)",
                        ));
                    }
                    DrawCommand::DrawCanvas { .. } => {
                        // Deliberately unsupported per paper-design
                        // §DrawCanvas — pre-spike resolution required.
                        // The production path emits
                        // `BackendCapabilityRejected` diagnostic via
                        // `degradation_policy::Reject` before reaching
                        // here; this arm exists so the match remains
                        // exhaustive against future variant additions.
                        return Err(BackendError::Unsupported("DrawCanvas"));
                    }
                    DrawCommand::BeginOverlay => {
                        return Err(BackendError::Unsupported(
                            "BeginOverlay (Day 1: Scene::pop_layer + push_layer Mix::Normal, pending)",
                        ));
                    }
                }
            }
            // Empty command list — unreachable in production but
            // valid type-wise. Spike fixtures may pass an empty
            // slice; treat as a no-op render.
            Ok(())
        }
    }

    fn resize(
        &mut self,
        _gpu: &GpuState,
        _font_config: &FontConfig,
        _scale_factor: f64,
        window_size: PhysicalSize<u32>,
    ) {
        self.width = window_size.width;
        self.height = window_size.height;
    }

    fn capabilities(&self) -> BackendCapabilities {
        BackendCapabilities {
            // Vello supports vector paths, but gated on the feature
            // flag — the stub-only build exposes no path support.
            supports_paths: cfg!(feature = "with-vello"),
            // vello_hybrid is the GPU/CPU mixed path — ostensibly
            // does not require pure compute shaders. The full Vello
            // (compute) path would set this to true; we deliberately
            // chose the hybrid variant per ADR-032 §Decision.
            supports_compute: false,
            atlas_kind: AtlasKind::Glifo,
            // Default per ADR-032 §Decision item 3. The translation
            // skeleton in `render_with_cursor` raises
            // `BackendError::Unsupported("DrawCanvas")` for the one
            // currently-undefined variant; the per-frame diagnostic
            // emission lives in the SceneRenderer adapter at the
            // production boundary, not in the spike itself.
            degradation_policy: DegradationPolicy::Reject,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Smoke test: the stub backend must construct and report
    /// reasonable capabilities even with `--no-default-features`.
    #[test]
    fn stub_backend_constructs_and_reports_capabilities() {
        // We cannot construct GpuState in this test (requires winit),
        // so we exercise the capability-only path.
        let backend = VelloBackend {
            width: 800,
            height: 600,
            #[cfg(feature = "with-vello")]
            renderer: None,
        };
        let caps = backend.capabilities();
        assert_eq!(caps.atlas_kind, AtlasKind::Glifo);
        assert!(!caps.supports_compute);
        assert_eq!(caps.supports_paths, cfg!(feature = "with-vello"));
        // ADR-032 §Decision item 3: default policy is `Reject`.
        assert_eq!(caps.degradation_policy, DegradationPolicy::Reject);
    }

    #[test]
    fn is_active_matches_feature_flag() {
        assert_eq!(VelloBackend::is_active(), cfg!(feature = "with-vello"));
    }
}
