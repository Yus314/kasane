use super::text_pipeline::{Cache, ColorMode, Resolution, TextAtlas, TextRenderer, Viewport};
use crate::animation::CursorRenderState;
use cosmic_text::{Buffer as GlyphonBuffer, FontSystem, SwashCache};
use kasane_core::config::FontConfig;
use kasane_core::render::scene::line_display_width_str;
use kasane_core::render::{CellSize, CursorStyle, DrawCommand, PixelRect};
use wgpu::MultisampleState;
use winit::dpi::PhysicalSize;

mod draw_commands;
mod line_cache;

use line_cache::LineShapingCache;

use super::CellMetrics;
use super::compositor::{BlitPipeline, BlurPipeline, RenderTarget};
use super::depth_stencil::DepthStencilState;
use super::image_pipeline::ImagePipeline;
use super::quad_pipeline::QuadPipeline;
use super::text_effects::TextEffects;
use super::texture_cache::TextureCache;
use super::timing::GpuTimingState;
use crate::colors::ColorResolver;

/// Scene-based GPU renderer that processes DrawCommands directly,
/// bypassing the CellGrid intermediate representation.
pub struct SceneRenderer {
    font_system: FontSystem,
    swash_cache: SwashCache,
    viewport: Viewport,
    text_atlas: TextAtlas,
    text_renderer: TextRenderer,
    quad: QuadPipeline,
    image: ImagePipeline,
    texture_cache: TextureCache,
    metrics: CellMetrics,

    // Reusable text buffers (growable pool). Slots are addressed by index;
    // unused slots may exist between live ones because the line cache returns
    // arbitrary slots on hit.
    text_buffers: Vec<GlyphonBuffer>,
    /// Per-emission render record: (left, top, buffer_idx). One entry per
    /// DrawAtoms / RenderParagraph / DrawText / DrawPaddingRow processed.
    text_draws: Vec<(f32, f32, usize)>,
    /// Clip bounds (left, top, right, bottom) parallel to `text_draws`.
    text_clip_bounds: Vec<(i32, i32, i32, i32)>,

    /// Per-line cosmic-text shaping cache. Cache hits skip the dominant
    /// CPU cost (shape_until_scroll, ~100 µs/line) for unchanged lines on
    /// cursor-only frames.
    line_cache: LineShapingCache,

    // Scratch buffers
    row_text: String,
    span_ranges: Vec<(usize, usize, [f32; 4])>,
    /// Per-frame scratch: byte offsets where each atom starts in `row_text`.
    atom_byte_boundaries: Vec<usize>,
    /// Per-frame scratch: face per atom (cloned from input).
    atom_faces: Vec<kasane_core::protocol::Face>,
    /// Per-frame scratch: glyph-derived per-atom min X.
    atom_x_min: Vec<f32>,
    /// Per-frame scratch: glyph-derived per-atom max X.
    atom_x_max: Vec<f32>,
    /// Per-frame scratch: cell-aligned per-atom X (fallback when glyph extents missing).
    atom_estimated_x: Vec<f32>,
    /// Per-frame scratch: cell-aligned per-atom width (fallback).
    atom_estimated_w: Vec<f32>,

    /// Glyph-accurate primary cursor position and width from RenderParagraph.
    /// Overrides the cell-based cursor position/width in render_cursor().
    paragraph_cursor: Option<(f32, f32)>,

    /// Per-paragraph hit test data from the current frame.
    /// Each entry is (origin_x, origin_y, buffer_idx) for a RenderParagraph.
    paragraph_hit_data: Vec<(f32, f32, usize)>,

    // Font config
    font_family: String,
    font_size: f32,
    line_height: f32,

    // Clipping state
    clip_stack: Vec<PixelRect>,
    /// Current frame screen dimensions (set at start of render_inner).
    frame_screen_w: f32,
    frame_screen_h: f32,

    /// Event loop proxy for dispatching async image load completions.
    event_proxy: winit::event_loop::EventLoopProxy<crate::GuiEvent>,

    /// GPU effects configuration.
    effects: kasane_core::config::EffectsConfig,

    // Compositor for render-to-texture effects (backdrop blur)
    blit: BlitPipeline,
    blur: BlurPipeline,
    base_target: Option<RenderTarget>,

    /// GPU pass timing measurement.
    timing: GpuTimingState,

    /// Depth/stencil buffer for clip stencil and z-order.
    depth_stencil: DepthStencilState,

    /// Text post-processing effects (shadow, glow).
    text_effects: TextEffects,

    /// ADR-031 Parley text stack — Phase 9 scaffold integration.
    ///
    /// Currently parallel to the cosmic-text path: the field is initialised
    /// in [`SceneRenderer::new`] and exercised by hit-test/metrics helpers,
    /// but the production rendering path still runs through cosmic-text. The
    /// `KASANE_TEXT_BACKEND=parley` environment variable opts into the
    /// Parley path where it has been implemented (Phase 11 retires the
    /// legacy renderer entirely).
    parley_text: super::parley_text::ParleyText,
    /// Cached Parley-side `CellMetrics`. Computed alongside the cosmic-text
    /// version so Phase 11 can swap by deleting the legacy field. Currently
    /// only consulted when `KASANE_TEXT_BACKEND=parley`.
    parley_metrics: CellMetrics,

    // ADR-031 Phase 9b Step 4a — Parley render pipeline integration.
    //
    // These fields hold the GPU-side Parley state (renderer, atlases, the
    // L1/L2 caches, the rasteriser). Phase 9b Step 4b will branch the
    // process_draw_* handlers into a Parley path that drives them.
    /// L1: per-line shaped Parley layouts. Hit on cursor-only frames.
    /// Smoke bypasses this; production text routing (Phase 9b Step 4c+)
    /// will route through it once the L2/L3 architecture issue is fixed.
    #[allow(dead_code)]
    parley_layout_cache: super::parley_text::layout_cache::LayoutCache,
    /// swash::ScaleContext owner — reused across frames.
    parley_glyph_rasterizer: super::parley_text::glyph_rasterizer::GlyphRasterizer,
    /// L2 + L3: glyph bitmap cache + atlas-slot bookkeeping.
    /// Currently unused: the L2's CPU-only `AtlasShelf` produces slot
    /// coordinates that do not match the GPU `parley_*_atlas` layouts.
    /// Phase 9b Step 4c will fix the architecture so L2 owns the GPU
    /// atlases directly.
    #[allow(dead_code)]
    parley_raster_cache: super::parley_text::raster_cache::GlyphRasterCache,
    /// L3 GPU mask atlas (R8Unorm). Pairs with the CPU side inside
    /// `parley_raster_cache`. Phase 9b Step 4b uploads to this.
    parley_mask_atlas: super::parley_text::gpu_atlas::GpuAtlasShelf,
    /// L3 GPU colour atlas (Rgba8Unorm). Used for emoji glyphs.
    parley_color_atlas: super::parley_text::gpu_atlas::GpuAtlasShelf,
    /// wgpu glue: vertex buffer + pipeline + bind groups.
    parley_renderer: super::parley_text::parley_text_renderer::ParleyTextRenderer,
    /// Shared shader / bind-group-layout cache (owns the wgpu pipeline
    /// state). Reused by both `text_renderer` (cosmic-text) and
    /// `parley_renderer` so the two renderers issue compatible draw calls
    /// against the same pipeline.
    cache: Cache,
    /// Per-frame Parley drawable accumulator. Populated by
    /// `parley_emit_text` (called from `process_draw_text` when the env
    /// var is on) and drained by `parley_finalize_frame` before the
    /// render pass starts. Currently only DrawText is wired into Parley;
    /// DrawAtoms / RenderParagraph / DrawPaddingRow continue on the
    /// cosmic-text path until later steps.
    parley_drawables: Vec<super::parley_text::frame_builder::DrawableGlyph>,
}

/// Returns true when the user opted into the Parley text backend through
/// `KASANE_TEXT_BACKEND=parley`. Any other value (including unset) returns
/// false. Cached at process start through std::env, so dynamic toggling is
/// not supported — restart the editor to switch backends.
pub(crate) fn parley_backend_requested() -> bool {
    static REQUESTED: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *REQUESTED.get_or_init(|| {
        std::env::var("KASANE_TEXT_BACKEND")
            .map(|v| v.eq_ignore_ascii_case("parley"))
            .unwrap_or(false)
    })
}

impl SceneRenderer {
    pub(crate) fn new(
        gpu: &super::GpuState,
        font_config: &FontConfig,
        scale_factor: f64,
        window_size: PhysicalSize<u32>,
        event_proxy: winit::event_loop::EventLoopProxy<crate::GuiEvent>,
    ) -> Self {
        let mut font_system = FontSystem::new();
        if !font_config.fallback_list.is_empty() {
            tracing::info!(
                "font fallback list: {:?} (cosmic-text handles fallback via system fontconfig)",
                font_config.fallback_list
            );
        }
        let swash_cache = SwashCache::new();

        let font_size = font_config.size * scale_factor as f32;
        let line_height = font_size * font_config.line_height;

        let metrics =
            CellMetrics::calculate(&mut font_system, font_config, scale_factor, window_size);

        // ADR-031 Phase 9: instantiate the Parley text stack alongside the
        // cosmic-text path. Computing parley_metrics here also serves as a
        // smoke test of the Parley shaper at startup.
        let mut parley_text = super::parley_text::ParleyText::new(font_config);
        let parley_metrics = super::parley_text::metrics::calculate_with_parley(
            &mut parley_text,
            font_config,
            scale_factor,
            window_size,
        );
        if parley_backend_requested() {
            tracing::info!(
                target: "kasane::parley",
                cell_width = parley_metrics.cell_width,
                cell_height = parley_metrics.cell_height,
                baseline = parley_metrics.baseline,
                cols = parley_metrics.cols,
                rows = parley_metrics.rows,
                "KASANE_TEXT_BACKEND=parley detected; Parley metrics will be used"
            );
        }

        let surface_format = gpu.config.format;

        let cache = Cache::new(&gpu.device);
        // ColorMode::Web: vertex colors are passed through to fragment shader
        // without sRGB→linear conversion. We pass linear-space colors via
        // resolve_face_colors_linear() and the shader outputs them as-is to
        // the linear framebuffer. ColorMode::Accurate would apply srgb_to_linear()
        // a second time, producing dramatically darker text (the original bug).
        // Note: Color glyphs (emoji) sampled from the linear-format atlas may
        // appear slightly washed out compared to ColorMode::Accurate; tracked
        // separately for follow-up.
        let mut text_atlas = TextAtlas::with_color_mode(
            &gpu.device,
            &gpu.queue,
            &cache,
            surface_format,
            ColorMode::Web,
        );
        let text_renderer = TextRenderer::new(
            &mut text_atlas,
            &gpu.device,
            MultisampleState::default(),
            Some(super::depth_stencil::pipeline_depth_stencil()),
        );
        let mut viewport = Viewport::new(&gpu.device, &cache);
        viewport.update(
            &gpu.queue,
            Resolution {
                width: window_size.width.max(1),
                height: window_size.height.max(1),
            },
        );

        let timing = GpuTimingState::new(&gpu.device, &gpu.queue);

        let depth_stencil = DepthStencilState::new(
            &gpu.device,
            window_size.width.max(1),
            window_size.height.max(1),
        );

        let text_effects = TextEffects::new(&gpu.device, surface_format);

        let quad = QuadPipeline::new(gpu, surface_format);
        let blit = BlitPipeline::new(&gpu.device, surface_format);
        let blur = BlurPipeline::new(&gpu.device, surface_format);
        let texture_cache = TextureCache::new(&gpu.device, 128 * 1024 * 1024); // 128 MB budget
        let image = ImagePipeline::new(gpu, surface_format, texture_cache.bind_group_layout());

        // ADR-031 Phase 9b Step 4a — Parley render pipeline state.
        // Wired through Cache (shared with TextRenderer) so the new
        // pipeline reuses the existing shader / vertex layout / bind
        // layouts, which lets Phase 11's removal of cosmic-text be a
        // pure subtraction rather than a wgpu re-derivation.
        let parley_layout_cache = super::parley_text::layout_cache::LayoutCache::new();
        let parley_glyph_rasterizer = super::parley_text::glyph_rasterizer::GlyphRasterizer::new();
        let parley_raster_cache =
            super::parley_text::raster_cache::GlyphRasterCache::default_sized();
        let parley_mask_atlas = super::parley_text::gpu_atlas::GpuAtlasShelf::default_for(
            &gpu.device,
            super::parley_text::gpu_atlas::Kind::Mask,
        );
        let parley_color_atlas = super::parley_text::gpu_atlas::GpuAtlasShelf::default_for(
            &gpu.device,
            super::parley_text::gpu_atlas::Kind::Color,
        );
        let parley_renderer = super::parley_text::parley_text_renderer::ParleyTextRenderer::new(
            &gpu.device,
            &cache,
            surface_format,
            MultisampleState::default(),
            Some(super::depth_stencil::pipeline_depth_stencil()),
        );

        SceneRenderer {
            font_system,
            swash_cache,
            viewport,
            text_atlas,
            text_renderer,
            quad,
            image,
            texture_cache,
            metrics,
            text_buffers: Vec::with_capacity(128),
            text_draws: Vec::with_capacity(128),
            text_clip_bounds: Vec::with_capacity(128),
            line_cache: LineShapingCache::new(),
            row_text: String::with_capacity(512),
            span_ranges: Vec::with_capacity(256),
            atom_byte_boundaries: Vec::with_capacity(64),
            atom_faces: Vec::with_capacity(64),
            atom_x_min: Vec::with_capacity(64),
            atom_x_max: Vec::with_capacity(64),
            atom_estimated_x: Vec::with_capacity(64),
            atom_estimated_w: Vec::with_capacity(64),
            paragraph_cursor: None,
            paragraph_hit_data: Vec::with_capacity(64),
            font_family: font_config.family.clone(),
            font_size,
            line_height,
            clip_stack: Vec::new(),
            frame_screen_w: 0.0,
            frame_screen_h: 0.0,
            event_proxy,
            effects: kasane_core::config::EffectsConfig::default(),
            blit,
            blur,
            base_target: None,
            timing,
            depth_stencil,
            text_effects,
            parley_text,
            parley_metrics,
            parley_layout_cache,
            parley_glyph_rasterizer,
            parley_raster_cache,
            parley_mask_atlas,
            parley_color_atlas,
            parley_renderer,
            cache,
            parley_drawables: Vec::with_capacity(2048),
        }
    }

    pub fn metrics(&self) -> &CellMetrics {
        if parley_backend_requested() {
            &self.parley_metrics
        } else {
            &self.metrics
        }
    }

    /// Read access to the Parley state — used by future phases that need
    /// to invoke `parley_text::shaper::shape_line` directly.
    #[allow(dead_code)]
    pub(crate) fn parley_text(&self) -> &super::parley_text::ParleyText {
        &self.parley_text
    }

    /// Mutable access to the Parley state — used by Phase 9b's draw-command
    /// path migration.
    #[allow(dead_code)]
    pub(crate) fn parley_text_mut(&mut self) -> &mut super::parley_text::ParleyText {
        &mut self.parley_text
    }

    /// ADR-031 Phase 9b Step 4b smoke test — render a hard-coded "PARLEY"
    /// string at top-left through the Parley pipeline.
    ///
    /// **L2 cache bypass**: this smoke deliberately bypasses the
    /// `parley_raster_cache`. The L2 cache currently owns its own
    /// CPU-only `AtlasShelf` allocator, but the GPU bind group reads
    /// from `parley_mask_atlas` / `parley_color_atlas` (separate
    /// `GpuAtlasShelf` allocators). Routing through L2 would produce
    /// atlas slot coordinates that point at empty regions of the GPU
    /// texture. The architectural fix (raster_cache should own the
    /// `GpuAtlasShelf`s) is staged separately; for the visual smoke we
    /// rasterise + allocate + queue + render directly.
    ///
    /// Atlases are cleared each frame to keep memory bounded — every
    /// "PARLEY" frame re-allocates and re-uploads. Per-frame waste is
    /// fine for a smoke test of fixed-size content.
    ///
    /// No-op unless `KASANE_TEXT_BACKEND=parley` is set.
    /// Emit a multi-atom run through Parley by shaping each atom
    /// independently and snapping each to its cell-grid x. Mirrors the
    /// cosmic-text DrawAtoms layout strategy (atom_estimated_x =
    /// cumulative cell_w × unicode_width), so per-atom positions match
    /// the cell-grid backgrounds + decorations emitted in
    /// `process_draw_atoms_parley`.
    ///
    /// We deliberately do NOT shape the whole `[atom]` slice as one
    /// Parley `StyledLine`: a single Parley layout would lay glyphs out
    /// at Parley's continuous advance, drifting from the cell-grid
    /// coordinates the rest of the renderer assumes. Per-atom shaping
    /// loses cross-atom shaping (ligatures across atom boundaries) but
    /// that boundary is rare in Kakoune UI atoms (which differ in
    /// face/colour and so already break ligatures naturally).
    pub(crate) fn parley_emit_atoms(
        &mut self,
        atoms: &[kasane_core::render::ResolvedAtom],
        px: f32,
        py: f32,
        color_resolver: &ColorResolver,
    ) {
        if atoms.is_empty() {
            return;
        }
        let cell_w = self.metrics.cell_width;
        let mut x = px;
        for atom in atoms {
            let atom_w = line_display_width_str(&atom.contents) as f32 * cell_w;
            if !atom.contents.is_empty() {
                self.parley_emit_text(&atom.contents, &atom.face, x, py, color_resolver);
            }
            x += atom_w;
        }
    }

    pub(crate) fn parley_emit_text(
        &mut self,
        text: &str,
        face: &kasane_core::protocol::Face,
        px: f32,
        py: f32,
        color_resolver: &ColorResolver,
    ) {
        use super::parley_text::Brush as PBrush;
        use super::parley_text::frame_builder::DrawableGlyph;
        use super::parley_text::glyph_rasterizer::{ContentKind, SubpixelX};
        use super::parley_text::shaper::shape_line_with_default_family;
        use super::parley_text::styled_line::StyledLine;
        use kasane_core::protocol::{Atom, Style};
        use parley::PositionedLayoutItem;

        if text.is_empty() {
            return;
        }

        let atoms = vec![Atom {
            face: *face,
            contents: text.into(),
        }];
        let line = StyledLine::from_atoms(
            &atoms,
            &Style::default(),
            PBrush::opaque(255, 255, 255),
            self.font_size,
            None,
        );
        let parley_layout = shape_line_with_default_family(&mut self.parley_text, &line);

        let (visual_fg, _bg, _needs_bg) = color_resolver.resolve_face_colors_linear(face);
        let brush = PBrush::rgba(
            (visual_fg[0].clamp(0.0, 1.0) * 255.0).round() as u8,
            (visual_fg[1].clamp(0.0, 1.0) * 255.0).round() as u8,
            (visual_fg[2].clamp(0.0, 1.0) * 255.0).round() as u8,
            (visual_fg[3].clamp(0.0, 1.0) * 255.0).round() as u8,
        );

        // ADR-031 Phase 9b — use the *cosmic-derived* baseline so the
        // Parley path lines up with the rest of the renderer's
        // expectations (background quads, cursor positioning, status
        // bar geometry). Parley's own LineMetrics::baseline depends on
        // the LineHeight property which we have not pushed yet, so it
        // defaults to the font-intrinsic value (~ascent), about 1-2 px
        // above the cell-grid baseline cosmic computes from
        // `font_size × 1.2`. That tiny offset is enough to lift status
        // bar text off its background.
        let cell_baseline = self.metrics.baseline;
        for layout_line in parley_layout.layout.lines() {
            let line_baseline = py + cell_baseline;

            for item in layout_line.items() {
                let PositionedLayoutItem::GlyphRun(run) = item else {
                    continue;
                };
                let parley_run = run.run();
                let font = parley_run.font();
                let font_size = parley_run.font_size();
                let Some(font_ref) =
                    swash::FontRef::from_index(font.data.data(), font.index as usize)
                else {
                    continue;
                };

                for glyph in run.positioned_glyphs() {
                    let abs_x = px + glyph.x;
                    let abs_y = line_baseline + glyph.y;
                    let subpx = SubpixelX::from_fract(abs_x);
                    let glyph_id = glyph.id as u16;

                    let raster = match self
                        .parley_glyph_rasterizer
                        .rasterize(font_ref, glyph_id, font_size, subpx, true)
                    {
                        Some(r) => r,
                        None => continue,
                    };

                    let atlas = match raster.content {
                        ContentKind::Mask => &mut self.parley_mask_atlas,
                        ContentKind::Color => &mut self.parley_color_atlas,
                    };
                    let raster_w = raster.width;
                    let raster_h = raster.height;
                    let raster_left = raster.left;
                    let raster_top = raster.top;
                    let raster_content = raster.content;
                    let Some(slot) = atlas.allocate_and_queue(raster_w, raster_h, raster.data)
                    else {
                        continue;
                    };

                    self.parley_drawables.push(DrawableGlyph {
                        px: abs_x,
                        py: abs_y,
                        width: raster_w,
                        height: raster_h,
                        left: raster_left,
                        top: raster_top,
                        content: raster_content,
                        atlas_slot: slot,
                        brush,
                    });
                }
            }
        }
    }

    /// Reset Parley per-frame state. Called once at the start of each
    /// frame.
    fn parley_frame_start(&mut self) {
        self.parley_drawables.clear();
        self.parley_mask_atlas.clear();
        self.parley_color_atlas.clear();
    }

    /// Hand drawables accumulated since the last call to the Parley
    /// renderer. Called once per layer (so each layer's overlay text
    /// reaches the GPU before that layer's render pass begins). The
    /// drawables vector is then cleared so the next layer accumulates
    /// fresh entries; atlases keep their slots across layers within the
    /// frame.
    fn parley_finalize_frame(&mut self, device: &wgpu::Device, queue: &wgpu::Queue) {
        tracing::trace!(
            target: "kasane::parley::frame",
            drawables = self.parley_drawables.len() as u32,
            mask_pending = self.parley_mask_atlas.pending_uploads().len() as u32,
            color_pending = self.parley_color_atlas.pending_uploads().len() as u32,
            "parley finalize"
        );
        self.parley_renderer.prepare(
            device,
            queue,
            &self.cache,
            &mut self.parley_mask_atlas,
            &mut self.parley_color_atlas,
            &self.parley_drawables,
        );
        self.parley_drawables.clear();
    }

    /// Proportional-aware mouse hit test.
    ///
    /// Uses the shaped paragraph buffers from the last frame to convert pixel
    /// coordinates into a (display_col, row) pair. Falls back to cell-based
    /// division for areas outside paragraph regions (status bar, menus, etc.).
    pub fn hit_test(&self, px: f64, py: f64) -> (u16, u16) {
        let cell_h = self.metrics.cell_height;
        let cell_w = self.metrics.cell_width;
        let row = (py as f32 / cell_h).floor().max(0.0) as u16;

        // Find the paragraph buffer whose y range covers this pixel.
        for &(origin_x, origin_y, buf_idx) in &self.paragraph_hit_data {
            let rel_y = py as f32 - origin_y;
            if rel_y < 0.0 || rel_y >= cell_h {
                continue;
            }
            let rel_x = px as f32 - origin_x;
            if rel_x < 0.0 {
                break; // x is before the paragraph origin
            }
            let buffer = &self.text_buffers[buf_idx];
            if let Some(cursor) = buffer.hit(rel_x, rel_y) {
                // cursor.index is a byte offset into the shaped text.
                // Convert to display column by measuring unicode widths
                // using the run text (single-line buffers have one run).
                if let Some(run) = buffer.layout_runs().next() {
                    let col = byte_offset_to_display_col(run.text, cursor.index);
                    return (
                        col.min(self.metrics.cols.saturating_sub(1)),
                        row.min(self.metrics.rows.saturating_sub(1)),
                    );
                }
            }
        }

        // Fallback: cell-based grid division
        let col = (px as f32 / cell_w).floor().max(0.0) as u16;
        (
            col.min(self.metrics.cols.saturating_sub(1)),
            row.min(self.metrics.rows.saturating_sub(1)),
        )
    }

    /// Get the latest GPU timing data.
    pub fn gpu_timings(&self) -> Option<&super::timing::GpuFrameTimings> {
        self.timing.latest_timings()
    }

    pub fn cell_size(&self) -> CellSize {
        CellSize {
            width: self.metrics.cell_width,
            height: self.metrics.cell_height,
        }
    }

    /// Update GPU effects configuration.
    pub fn set_effects(&mut self, effects: kasane_core::config::EffectsConfig) {
        self.effects = effects;
    }

    /// Returns true if a bg color matches the default bg and gradient is active,
    /// meaning the fill should be skipped to let the gradient show through.
    fn should_skip_default_bg(&self, bg: &[f32; 4], color_resolver: &ColorResolver) -> bool {
        if self.effects.gradient_start.is_none() {
            return false;
        }
        let dbg = color_resolver.default_bg_linear();
        (bg[0] - dbg[0]).abs() < 0.002
            && (bg[1] - dbg[1]).abs() < 0.002
            && (bg[2] - dbg[2]).abs() < 0.002
    }

    /// Draw semi-transparent overlays on non-focused pane areas.
    fn render_pane_dim(&mut self, visual_hints: &kasane_core::render::VisualHints) {
        let Some(ref pane) = visual_hints.focused_pane else {
            return;
        };
        let dim_color = [0.0_f32, 0.0, 0.0, 0.25];
        let sw = self.frame_screen_w;
        let sh = self.frame_screen_h;

        // Top strip (above focused pane)
        if pane.y > 0.0 {
            self.quad.push_solid(0.0, 0.0, sw, pane.y, dim_color);
        }
        // Bottom strip (below focused pane)
        let bottom = pane.y + pane.h;
        if bottom < sh {
            self.quad
                .push_solid(0.0, bottom, sw, sh - bottom, dim_color);
        }
        // Left strip (within pane row)
        if pane.x > 0.0 {
            self.quad.push_solid(0.0, pane.y, pane.x, pane.h, dim_color);
        }
        // Right strip (within pane row)
        let right = pane.x + pane.w;
        if right < sw {
            self.quad
                .push_solid(right, pane.y, sw - right, pane.h, dim_color);
        }
    }

    /// Finalize an async image load. Returns `true` if the texture was inserted.
    pub fn finalize_image_load(
        &mut self,
        key: super::texture_cache::TextureKey,
        result: Result<super::texture_cache::DecodedImage, String>,
        gpu: &super::GpuState,
    ) -> bool {
        self.texture_cache
            .finalize_load(key, result, &gpu.device, &gpu.queue)
    }

    /// Recalculate metrics after resize or scale factor change.
    pub fn resize(
        &mut self,
        gpu: &super::GpuState,
        font_config: &FontConfig,
        scale_factor: f64,
        window_size: PhysicalSize<u32>,
    ) {
        self.font_size = font_config.size * scale_factor as f32;
        self.line_height = self.font_size * font_config.line_height;
        self.font_family.clone_from(&font_config.family);
        self.metrics = CellMetrics::calculate(
            &mut self.font_system,
            font_config,
            scale_factor,
            window_size,
        );
        self.viewport.update(
            &gpu.queue,
            Resolution {
                width: window_size.width.max(1),
                height: window_size.height.max(1),
            },
        );
        self.depth_stencil.resize(
            &gpu.device,
            window_size.width.max(1),
            window_size.height.max(1),
        );
        self.text_buffers.clear();
        self.text_draws.clear();
        // Cached shaping is keyed on font_size; the text_buffers pool was
        // also wiped above, so all cached buffer_idx values are invalid.
        self.line_cache.invalidate_all();
    }

    /// Render with animated cursor state.
    #[allow(clippy::too_many_arguments)]
    pub fn render_with_cursor(
        &mut self,
        gpu: &super::GpuState,
        commands: &[DrawCommand],
        color_resolver: &ColorResolver,
        cursor_style: CursorStyle,
        cursor_state: &CursorRenderState,
        cursor_color: kasane_core::protocol::Color,
        overlay_opacities: &[f32],
        visual_hints: &kasane_core::render::VisualHints,
    ) -> anyhow::Result<()> {
        self.render_inner(
            gpu,
            commands,
            color_resolver,
            Some((
                cursor_state.x,
                cursor_state.y,
                cursor_state.opacity,
                cursor_style,
                cursor_color,
            )),
            overlay_opacities,
            visual_hints,
        )
    }

    /// Render a frame from DrawCommands (non-animated cursor).
    pub fn render(
        &mut self,
        gpu: &super::GpuState,
        commands: &[DrawCommand],
        color_resolver: &ColorResolver,
        cursor: Option<(u16, u16, CursorStyle)>,
    ) -> anyhow::Result<()> {
        let animated = cursor.map(|(cx, cy, style)| {
            let cell_w = self.metrics.cell_width;
            let cell_h = self.metrics.cell_height;
            (
                cx as f32 * cell_w,
                cy as f32 * cell_h,
                1.0f32,
                style,
                kasane_core::protocol::Color::Default,
            )
        });
        self.render_inner(
            gpu,
            commands,
            color_resolver,
            animated,
            &[],
            &Default::default(),
        )
    }

    /// Core render implementation.
    ///
    /// Renders in layers: base layer first, then each overlay layer.  Within
    /// each layer the draw order is background → borders → text, so overlay
    /// backgrounds correctly occlude base-layer text.
    #[allow(clippy::too_many_arguments)]
    fn render_inner(
        &mut self,
        gpu: &super::GpuState,
        commands: &[DrawCommand],
        color_resolver: &ColorResolver,
        cursor: Option<(f32, f32, f32, CursorStyle, kasane_core::protocol::Color)>,
        overlay_opacities: &[f32],
        visual_hints: &kasane_core::render::VisualHints,
    ) -> anyhow::Result<()> {
        let _frame_span = tracing::info_span!("gpu_frame", commands = commands.len()).entered();
        let output = match gpu.surface.get_current_texture() {
            wgpu::CurrentSurfaceTexture::Success(frame) => frame,
            wgpu::CurrentSurfaceTexture::Suboptimal(frame) => {
                gpu.surface.configure(&gpu.device, &gpu.config);
                frame
            }
            wgpu::CurrentSurfaceTexture::Outdated | wgpu::CurrentSurfaceTexture::Lost => {
                gpu.surface.configure(&gpu.device, &gpu.config);
                match gpu.surface.get_current_texture() {
                    wgpu::CurrentSurfaceTexture::Success(frame)
                    | wgpu::CurrentSurfaceTexture::Suboptimal(frame) => frame,
                    other => {
                        anyhow::bail!(
                            "failed to acquire surface texture after reconfigure: {other:?}"
                        )
                    }
                }
            }
            wgpu::CurrentSurfaceTexture::Timeout | wgpu::CurrentSurfaceTexture::Occluded => {
                return Ok(());
            }
            wgpu::CurrentSurfaceTexture::Validation => {
                anyhow::bail!("surface texture validation error");
            }
        };
        let view = output.texture.create_view(&Default::default());

        let screen_w = gpu.config.width as f32;
        let screen_h = gpu.config.height as f32;
        self.frame_screen_w = screen_w;
        self.frame_screen_h = screen_h;

        // Update screen size uniforms
        let screen_size = [screen_w, screen_h];
        let screen_size_data = bytemuck::cast_slice(&screen_size);
        for buffer in [self.quad.uniform_buffer(), self.image.uniform_buffer()] {
            gpu.queue.write_buffer(buffer, 0, screen_size_data);
        }

        // Reset per-frame state
        self.texture_cache.frame_tick();
        self.text_draws.clear();
        self.text_clip_bounds.clear();
        self.clip_stack.clear();
        self.paragraph_cursor = None;
        self.paragraph_hit_data.clear();
        // ADR-031 Phase 9b Step 4d — clear Parley accumulator + atlases.
        // Per-frame re-rasterisation while the L2 cache architecture
        // (Step 4c) is still in flight.
        if parley_backend_requested() {
            self.parley_frame_start();
        }
        // Emit last frame's hit/miss tally before clearing for the new frame.
        // Filter to `kasane::line_cache=debug` to see per-frame summaries
        // without the noisy per-line `trace` events.
        let prev_stats = self.line_cache.take_stats();
        if prev_stats.hits + prev_stats.misses + prev_stats.bypass > 0 {
            tracing::debug!(
                target: "kasane::line_cache",
                hits = prev_stats.hits,
                misses = prev_stats.misses,
                bypass = prev_stats.bypass,
                "frame summary",
            );
        }
        // Reset which buffer pool slots are claimed; cache entries are kept
        // so subsequent frames can hit them.
        self.line_cache.frame_start(self.text_buffers.len());
        self.timing.begin_frame();

        // Split commands into layers at BeginOverlay boundaries.
        // layer_ranges[i] = (start, end) index into `commands`.
        let _split_span = tracing::info_span!("layer_split").entered();
        let mut layer_ranges: Vec<(usize, usize)> = Vec::new();
        let mut layer_start = 0;
        for (i, cmd) in commands.iter().enumerate() {
            if matches!(cmd, DrawCommand::BeginOverlay) {
                layer_ranges.push((layer_start, i));
                layer_start = i + 1; // skip the marker itself
            }
        }
        layer_ranges.push((layer_start, commands.len()));

        drop(_split_span);

        let cell_w = self.metrics.cell_width;
        let cell_h = self.metrics.cell_height;

        // Update viewport (shared across layers)
        self.viewport.update(
            &gpu.queue,
            Resolution {
                width: gpu.config.width,
                height: gpu.config.height,
            },
        );

        // Determine if compositor path is active (backdrop blur with overlays)
        let has_overlays = layer_ranges.len() > 1;
        let use_compositor = self.effects.backdrop_blur && has_overlays;

        // Ensure base render target exists at correct size
        if use_compositor {
            let w = gpu.config.width;
            let h = gpu.config.height;
            let format = gpu.config.format;
            if let Some(ref mut target) = self.base_target {
                target.resize(&gpu.device, w, h, format, "base_target");
            } else {
                self.base_target =
                    Some(RenderTarget::new(&gpu.device, w, h, format, "base_target"));
            }
        }

        // Each layer gets its own encoder + submit so that queue.write_buffer
        // data is flushed before the next layer overwrites shared GPU buffers.
        for (layer_idx, &(range_start, range_end)) in layer_ranges.iter().enumerate() {
            // Reset per-layer pipeline instances
            self.quad.instances.clear();
            self.image.clear_frame();
            let layer_text_start = self.text_draws.len();

            // Process this layer's DrawCommands
            for cmd in &commands[range_start..range_end] {
                self.process_draw_command(cmd, gpu, color_resolver, cell_w, cell_h, screen_w);
            }

            // Cursor belongs to the base layer (layer 0)
            if layer_idx == 0 {
                // Gradient background (drawn before bg, after clear)
                if let (Some(start), Some(end)) =
                    (self.effects.gradient_start, self.effects.gradient_end)
                {
                    self.quad.push_gradient(
                        0.0,
                        0.0,
                        screen_w,
                        screen_h,
                        crate::colors::srgb_color_to_linear(start),
                        crate::colors::srgb_color_to_linear(end),
                    );
                }

                // Cursor line highlight
                if let Some((_cx, cy, _opacity, _style, _color)) = cursor
                    && self.effects.cursor_line_highlight
                        != kasane_core::config::CursorLineHighlightMode::Off
                {
                    let fg =
                        color_resolver.resolve_linear(kasane_core::protocol::Color::Default, true);
                    let highlight_color = [fg[0], fg[1], fg[2], 0.03];
                    let line_y = (cy / cell_h).floor() * cell_h;
                    self.quad
                        .push_solid(0.0, line_y, screen_w, cell_h, highlight_color);
                }

                self.render_cursor(cursor, color_resolver, cell_w, cell_h);

                // Dim non-focused panes in multi-pane mode
                self.render_pane_dim(visual_hints);
            }

            // Apply per-layer opacity for overlay fade transitions.
            // Overlay layers are layer_idx > 0; their opacity comes from
            // overlay_opacities[layer_idx - 1] (0.0 = invisible, 1.0 = full).
            if layer_idx > 0 {
                let opacity = overlay_opacities.get(layer_idx - 1).copied().unwrap_or(1.0);
                if opacity < 1.0 {
                    // Unified quad stride=20: fill alpha at 7, border alpha at 11, extra alpha at 19
                    for chunk in self.quad.instances.chunks_exact_mut(20) {
                        chunk[7] *= opacity; // fill_color.a
                        chunk[11] *= opacity; // border_color.a
                        chunk[19] *= opacity; // extra.a (gradient end_color)
                    }
                }
            }

            // Ensure GPU buffers are large enough for this layer
            self.quad.ensure_buffer(gpu, self.quad.instance_count());

            let image_count = self.image.instances.len() / 9;
            self.image.ensure_buffer(gpu, image_count);

            // Build TextAreas for this layer's text emissions. Each emission
            // carries its own buffer pool index (cache hits return arbitrary
            // indices), so we look up buffers indirectly rather than slicing.
            let layer_text_end = self.text_draws.len();
            let layer_clips = &self.text_clip_bounds[layer_text_start..layer_text_end];
            let has_clips = layer_clips.iter().any(|&(l, t, r, b)| {
                l != 0 || t != 0 || r != screen_w as i32 || b != screen_h as i32
            });
            let text_areas = super::text_helpers::prepare_text_areas(
                &self.text_draws[layer_text_start..layer_text_end],
                &self.text_buffers,
                screen_w,
                screen_h,
                if has_clips { Some(layer_clips) } else { None },
            );

            // Prepare this layer's text
            let _text_span = tracing::info_span!("text_prepare", layer = layer_idx).entered();
            self.text_renderer
                .prepare(
                    &gpu.device,
                    &gpu.queue,
                    &mut self.font_system,
                    &mut self.text_atlas,
                    &self.viewport,
                    text_areas,
                    &mut self.swash_cache,
                )
                .map_err(|e| anyhow::anyhow!("glyphon prepare failed: {e}"))?;
            drop(_text_span);

            // ADR-031 Phase 9b Step 4e — finalize the Parley draw for
            // this layer. process_draw_* has accumulated DrawableGlyphs
            // into self.parley_drawables during this layer's
            // processing. parley_finalize_frame uploads atlas pixels +
            // writes vertex buffer + clears the accumulator so the next
            // layer starts fresh.
            if parley_backend_requested() {
                self.parley_finalize_frame(&gpu.device, &gpu.queue);
            }

            // Draw this layer: bg → border → text.
            // Each layer needs its own encoder + submit so that
            // queue.write_buffer data is committed before the next
            // layer overwrites the shared instance buffers.
            let mut encoder = gpu
                .device
                .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                    label: Some("scene_layer_encoder"),
                });

            {
                let load_op = if layer_idx == 0 {
                    let default_bg = color_resolver.default_bg_linear();
                    wgpu::LoadOp::Clear(wgpu::Color {
                        r: default_bg[0] as f64,
                        g: default_bg[1] as f64,
                        b: default_bg[2] as f64,
                        a: 1.0,
                    })
                } else {
                    wgpu::LoadOp::Load
                };

                // When compositor is active, layer 0 renders to base_target;
                // overlay layers render directly to the swapchain.
                let target_view = if use_compositor && layer_idx == 0 {
                    &self.base_target.as_ref().unwrap().view
                } else {
                    &view
                };

                let pass_label = if layer_idx == 0 { "base" } else { "overlay" };
                let ts_writes = self.timing.timestamp_writes(pass_label);

                let ds_attachment = self.depth_stencil.attachment(layer_idx == 0);
                let mut render_pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                    label: Some("scene_layer_pass"),
                    color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                        view: target_view,
                        depth_slice: None,
                        resolve_target: None,
                        ops: wgpu::Operations {
                            load: load_op,
                            store: wgpu::StoreOp::Store,
                        },
                    })],
                    depth_stencil_attachment: Some(ds_attachment),
                    timestamp_writes: ts_writes,
                    occlusion_query_set: None,
                    multiview_mask: None,
                });

                // Stencil reference 0 matches the cleared stencil buffer.
                // PushClip/PopClip modify the clip_stack for software clipping;
                // hardware stencil-based clipping can be layered on top later.
                render_pass.set_stencil_reference(0);

                self.quad.upload_and_draw(gpu, &mut render_pass);

                let text_effects_active = self.effects.text_effects.is_active();
                if !text_effects_active {
                    // Standard path: text directly to the main render target
                    self.text_renderer
                        .render(&self.text_atlas, &self.viewport, &mut render_pass)
                        .map_err(|e| anyhow::anyhow!("glyphon render failed: {e}"))?;
                    // ADR-031 Phase 9b Step 4e — Parley draw for this
                    // layer. parley_finalize_frame above wrote the vertex
                    // buffer with this layer's accumulated drawables.
                    if parley_backend_requested() {
                        self.parley_renderer
                            .render(&self.viewport, &mut render_pass);
                    }
                }

                self.image.upload_and_draw(gpu, &mut render_pass);

                if text_effects_active {
                    // Apply text effects: shadow/glow from intermediate RT
                    self.text_effects.apply(
                        &gpu.device,
                        &gpu.queue,
                        &mut render_pass,
                        &self.effects.text_effects,
                        screen_w,
                        screen_h,
                    );
                }
            }

            // When text effects are active, render text to intermediate RT
            // in a separate pass, then blit it to the main target.
            if self.effects.text_effects.is_active() {
                self.text_effects.ensure_target(
                    &gpu.device,
                    gpu.config.width,
                    gpu.config.height,
                    gpu.config.format,
                );
                let text_target_view = &self.text_effects.text_target.as_ref().unwrap().view;

                // Render text to intermediate RT
                {
                    let mut text_pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                        label: Some("text_effects_text_pass"),
                        color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                            view: text_target_view,
                            depth_slice: None,
                            resolve_target: None,
                            ops: wgpu::Operations {
                                load: wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT),
                                store: wgpu::StoreOp::Store,
                            },
                        })],
                        depth_stencil_attachment: None,
                        timestamp_writes: None,
                        occlusion_query_set: None,
                        multiview_mask: None,
                    });
                    self.text_renderer
                        .render(&self.text_atlas, &self.viewport, &mut text_pass)
                        .map_err(|e| anyhow::anyhow!("glyphon render failed: {e}"))?;
                }

                // Blit sharp text to main target
                let target_view = if use_compositor && layer_idx == 0 {
                    &self.base_target.as_ref().unwrap().view
                } else {
                    &view
                };
                let text_blit_bg = self
                    .blit
                    .create_texture_bind_group(&gpu.device, text_target_view);
                {
                    let mut blit_pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                        label: Some("text_effects_blit_pass"),
                        color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                            view: target_view,
                            depth_slice: None,
                            resolve_target: None,
                            ops: wgpu::Operations {
                                load: wgpu::LoadOp::Load,
                                store: wgpu::StoreOp::Store,
                            },
                        })],
                        depth_stencil_attachment: None,
                        timestamp_writes: None,
                        occlusion_query_set: None,
                        multiview_mask: None,
                    });
                    self.blit
                        .draw(&gpu.queue, &mut blit_pass, &text_blit_bg, 1.0);
                }
            }

            let _submit_span = tracing::info_span!("encoder_submit", layer = layer_idx).entered();
            gpu.queue.submit(std::iter::once(encoder.finish()));

            // After layer 0: apply blur and blit base_target to swapchain
            if use_compositor && layer_idx == 0 {
                let base = self.base_target.as_ref().unwrap();
                let format = gpu.config.format;

                // Blur the base target
                let mut blur_encoder =
                    gpu.device
                        .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                            label: Some("blur_encoder"),
                        });
                let blurred_view =
                    self.blur
                        .apply(&gpu.device, &gpu.queue, &mut blur_encoder, base, format);

                // Blit blurred result to swapchain
                let blit_bg = self
                    .blit
                    .create_texture_bind_group(&gpu.device, blurred_view);
                {
                    let mut blit_pass =
                        blur_encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                            label: Some("blur_blit_pass"),
                            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                                view: &view,
                                depth_slice: None,
                                resolve_target: None,
                                ops: wgpu::Operations {
                                    load: wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                                    store: wgpu::StoreOp::Store,
                                },
                            })],
                            depth_stencil_attachment: None,
                            timestamp_writes: None,
                            occlusion_query_set: None,
                            multiview_mask: None,
                        });
                    self.blit.draw(&gpu.queue, &mut blit_pass, &blit_bg, 1.0);
                }

                // Also blit the sharp (unblurred) base on top with slight opacity
                // so text remains readable. This creates the frosted glass effect.
                let sharp_bg = self.blit.create_texture_bind_group(&gpu.device, &base.view);
                {
                    let mut sharp_pass =
                        blur_encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                            label: Some("sharp_blit_pass"),
                            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                                view: &view,
                                depth_slice: None,
                                resolve_target: None,
                                ops: wgpu::Operations {
                                    load: wgpu::LoadOp::Load,
                                    store: wgpu::StoreOp::Store,
                                },
                            })],
                            depth_stencil_attachment: None,
                            timestamp_writes: None,
                            occlusion_query_set: None,
                            multiview_mask: None,
                        });
                    // Blend sharp base at ~70% over blurred base for readability
                    self.blit.draw(&gpu.queue, &mut sharp_pass, &sharp_bg, 0.7);
                }

                gpu.queue.submit(std::iter::once(blur_encoder.finish()));
            }
        }

        // Resolve GPU timing queries
        if self.timing.is_enabled() {
            let mut timing_encoder =
                gpu.device
                    .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                        label: Some("timing_resolve_encoder"),
                    });
            self.timing.resolve(&mut timing_encoder);
            gpu.queue.submit(std::iter::once(timing_encoder.finish()));
            self.timing.readback(&gpu.device);
            if let Some(timings) = self.timing.latest_timings() {
                tracing::info!("{timings}");
            }
        }

        output.present();

        self.texture_cache.evict_to_budget();
        self.text_atlas.trim();

        Ok(())
    }

    /// Get the current clip rect, or `None` if no clip is active.
    fn current_clip(&self) -> Option<&PixelRect> {
        self.clip_stack.last()
    }

    /// Intersect a rectangle with the current clip. Returns `None` if fully clipped.
    fn clip_rect(&self, x: f32, y: f32, w: f32, h: f32) -> Option<(f32, f32, f32, f32)> {
        let Some(clip) = self.current_clip() else {
            return Some((x, y, w, h));
        };
        let x1 = x.max(clip.x);
        let y1 = y.max(clip.y);
        let x2 = (x + w).min(clip.x + clip.w);
        let y2 = (y + h).min(clip.y + clip.h);
        if x2 <= x1 || y2 <= y1 {
            None
        } else {
            Some((x1, y1, x2 - x1, y2 - y1))
        }
    }
}

/// Convert a byte offset into text to a display column (unicode width).
fn byte_offset_to_display_col(text: &str, byte_offset: usize) -> u16 {
    let prefix = &text[..byte_offset.min(text.len())];
    line_display_width_str(prefix) as u16
}
