use super::text_pipeline::{Cache, Resolution, Viewport};
use crate::animation::CursorRenderState;
use kasane_core::config::FontConfig;
use kasane_core::render::scene::line_display_width_str;
use kasane_core::render::{CellSize, CursorStyle, DrawCommand, PixelRect};
use wgpu::MultisampleState;
use winit::dpi::PhysicalSize;

mod draw_commands;

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
    viewport: Viewport,
    quad: QuadPipeline,
    image: ImagePipeline,
    texture_cache: TextureCache,
    metrics: CellMetrics,

    /// Glyph-accurate primary cursor position and width set by
    /// [`process_render_paragraph`]. Overrides the cell-based cursor
    /// in render_cursor() when set.
    paragraph_cursor: Option<(f32, f32)>,

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

    // ADR-031: text stack. See `text/mod.rs` for the module
    // decomposition.
    /// Parley shaping + font context (FontContext + LayoutContext).
    text: super::text::ParleyText,
    /// L1: per-line shaped Parley layouts. Cache hits on cursor-only
    /// frames skip the dominant CPU cost (whole-line shaping). Wired
    /// into `process_render_paragraph_parley`.
    layout_cache: super::text::layout_cache::LayoutCache,
    /// swash::ScaleContext owner — reused across frames.
    glyph_rasterizer: super::text::glyph_rasterizer::GlyphRasterizer,
    /// L2 glyph bitmap cache. Owns the LRU; the atlases live below.
    raster_cache: super::text::raster_cache::GlyphRasterCache,
    /// L3 GPU mask atlas (R8Unorm).
    mask_atlas: super::text::gpu_atlas::GpuAtlasShelf,
    /// L3 GPU colour atlas (Rgba8Unorm) — emoji and colour outlines.
    color_atlas: super::text::gpu_atlas::GpuAtlasShelf,
    /// wgpu glue: vertex buffer + pipeline + bind groups.
    text_renderer: super::text::text_renderer::TextRenderer,
    /// Shared shader / bind-group-layout cache (owns the wgpu pipeline
    /// state). Used by `text_renderer`.
    cache: Cache,
    /// Per-frame drawable accumulator. Populated by `emit_text`
    /// during the layer's DrawCommand walk and drained by
    /// `finalize_text_frame` before the render pass executes.
    drawables: Vec<super::text::frame_builder::DrawableGlyph>,

    /// Inline-box paint sub-commands queued during paragraph painting.
    /// `process_render_paragraph_parley` pushes translated copies of the
    /// host-pre-painted plugin content here when it encounters a
    /// `PositionedLayoutItem::InlineBox`; the surrounding
    /// `process_draw_command` `RenderParagraph` arm drains and recurses
    /// once the paragraph is done, so sub-commands compose on top of the
    /// paragraph layout. ADR-031 Phase 10 Step 2-renderer (Step A.2b).
    pub(super) deferred_inline_box_cmds: Vec<DrawCommand>,
}

/// Diagnostic kill-switch for the Parley L2 raster cache.
/// Set `KASANE_PARLEY_NO_CACHE=1` to invalidate the cache + clear both
/// atlases at the start of every frame. Useful for atlas / eviction
/// debugging; harmless otherwise. Cached on first read.
pub(crate) fn parley_cache_disabled() -> bool {
    static DISABLED: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *DISABLED.get_or_init(|| {
        std::env::var("KASANE_PARLEY_NO_CACHE")
            .map(|v| !v.is_empty() && v != "0")
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
        if !font_config.fallback_list.is_empty() {
            tracing::info!(
                "font fallback list: {:?} (parley resolves fallback via fontique)",
                font_config.fallback_list
            );
        }

        let font_size = font_config.size * scale_factor as f32;
        let line_height = font_size * font_config.line_height;

        let mut text = super::text::ParleyText::new(font_config);
        let metrics = super::text::metrics::calculate_with_parley(
            &mut text,
            font_config,
            scale_factor,
            window_size,
        );
        tracing::info!(
            target: "kasane::parley",
            cell_width = metrics.cell_width,
            cell_height = metrics.cell_height,
            baseline = metrics.baseline,
            cols = metrics.cols,
            rows = metrics.rows,
            "Parley CellMetrics computed"
        );

        let surface_format = gpu.config.format;

        let cache = Cache::new(&gpu.device);
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

        let layout_cache = super::text::layout_cache::LayoutCache::new();
        let glyph_rasterizer = super::text::glyph_rasterizer::GlyphRasterizer::new();
        let raster_cache = super::text::raster_cache::GlyphRasterCache::default_sized();
        let mask_atlas = super::text::gpu_atlas::GpuAtlasShelf::default_for(
            &gpu.device,
            super::text::gpu_atlas::Kind::Mask,
        );
        let color_atlas = super::text::gpu_atlas::GpuAtlasShelf::default_for(
            &gpu.device,
            super::text::gpu_atlas::Kind::Color,
        );
        let text_renderer = super::text::text_renderer::TextRenderer::new(
            &gpu.device,
            &cache,
            surface_format,
            MultisampleState::default(),
            Some(super::depth_stencil::pipeline_depth_stencil()),
        );

        SceneRenderer {
            viewport,
            quad,
            image,
            texture_cache,
            metrics,
            paragraph_cursor: None,
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
            text,
            layout_cache,
            glyph_rasterizer,
            raster_cache,
            mask_atlas,
            color_atlas,
            text_renderer,
            cache,
            drawables: Vec::with_capacity(2048),
            deferred_inline_box_cmds: Vec::new(),
        }
    }

    pub fn metrics(&self) -> &CellMetrics {
        &self.metrics
    }

    /// Read access to the Parley state. Currently only used by the
    /// `Brush` smoke tests; production paths share `&mut self` and
    /// reach the field directly.
    #[allow(dead_code)]
    pub(crate) fn text(&self) -> &super::text::ParleyText {
        &self.text
    }

    /// Mutable access to the Parley state. See [`Self::text`] note.
    #[allow(dead_code)]
    pub(crate) fn text_mut(&mut self) -> &mut super::text::ParleyText {
        &mut self.text
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
    pub(crate) fn emit_atoms(
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
                self.emit_text(&atom.contents, &atom.face(), x, py, color_resolver);
            }
            x += atom_w;
        }
    }

    pub(crate) fn emit_text(
        &mut self,
        text: &str,
        face: &kasane_core::protocol::Face,
        px: f32,
        py: f32,
        color_resolver: &ColorResolver,
    ) {
        use super::text::Brush as PBrush;
        use super::text::frame_builder::DrawableGlyph;
        use super::text::glyph_rasterizer::SubpixelX;
        use super::text::styled_line::StyledLine;
        use kasane_core::protocol::{Atom, Style};
        use parley::PositionedLayoutItem;

        if text.is_empty() {
            return;
        }

        let atoms = vec![Atom::with_style(text, Style::from_face(face))];
        let line = StyledLine::from_atoms(
            &atoms,
            &Style::default(),
            PBrush::opaque(255, 255, 255),
            self.font_size,
            None,
        );
        let parley_layout = self.text.shape(&line);

        let (visual_fg, _bg, _needs_bg) = color_resolver.resolve_face_colors_linear(face);
        let brush = PBrush::rgba(
            (visual_fg[0].clamp(0.0, 1.0) * 255.0).round() as u8,
            (visual_fg[1].clamp(0.0, 1.0) * 255.0).round() as u8,
            (visual_fg[2].clamp(0.0, 1.0) * 255.0).round() as u8,
            (visual_fg[3].clamp(0.0, 1.0) * 255.0).round() as u8,
        );

        // ADR-031 Phase 9b — Parley's `positioned_glyphs()` already
        // returns each glyph's `y` in layout-relative coordinates that
        // include the run's baseline (`y = run.baseline() + glyph.y` per
        // parley v0.9). So `py + glyph.y` already places the glyph at the
        // intended baseline — adding our own `line_baseline` on top would
        // double-count the baseline and shift every glyph down by ~ascent
        // (precisely the bug that put status bar text below its bg).
        //
        // We still need a per-line cell-grid leading offset because
        // Parley's intrinsic `line_height` (no LineHeight pushed) is
        // smaller than `cell_h = font_size × 1.2`. Without that shift
        // glyphs sit `leading/2` too high in the cell. Apply the leading
        // adjustment to the layout origin and let `glyph.y` carry the
        // intra-line baseline offset.
        let cell_h = self.metrics.cell_height;
        // Split borrows so the L2 cache + rasterizer + atlases all
        // mutate independently inside the per-glyph loop.
        let rasterizer = &mut self.glyph_rasterizer;
        let cache = &mut self.raster_cache;
        let mut atlases = super::text::raster_cache_glue::ParleyAtlasPair {
            mask: &mut self.mask_atlas,
            color: &mut self.color_atlas,
        };
        let drawables = &mut self.drawables;
        for layout_line in parley_layout.layout.lines() {
            let lm = layout_line.metrics();
            let leading = (cell_h - lm.line_height).max(0.0);
            let layout_origin_y = py + leading * 0.5;

            for item in layout_line.items() {
                let PositionedLayoutItem::GlyphRun(run) = item else {
                    continue;
                };
                let parley_run = run.run();
                let font = parley_run.font();
                let font_id = super::text::font_id::font_id_from_data(font);
                let var_hash =
                    super::text::font_id::var_hash_from_coords(parley_run.normalized_coords());
                let font_size = parley_run.font_size();
                let size_q = (font_size * 64.0).round().clamp(0.0, u16::MAX as f32) as u16;
                let Some(font_ref) =
                    swash::FontRef::from_index(font.data.data(), font.index as usize)
                else {
                    continue;
                };
                for glyph in run.positioned_glyphs() {
                    let abs_x = px + glyph.x;
                    let abs_y = layout_origin_y + glyph.y;
                    let subpx = SubpixelX::from_fract(abs_x);
                    let glyph_id = glyph.id as u16;
                    let key = super::text::raster_cache::GlyphRasterKey {
                        font_id,
                        glyph_id,
                        size_q,
                        subpx_x: subpx.0,
                        var_hash,
                        hint: true,
                    };

                    let entry = cache.get_or_insert(key, &mut atlases, || {
                        rasterizer.rasterize(font_ref, glyph_id, font_size, subpx, true)
                    });
                    let Some(entry) = entry else {
                        continue;
                    };

                    drawables.push(DrawableGlyph {
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
                }
            }
        }
    }

    /// Reset Parley per-frame state. Called once at the start of each
    /// frame.
    ///
    /// Phase 9b Step 4c: atlases are no longer cleared per frame. The
    /// L2 [`GlyphRasterCache`](super::text::raster_cache::GlyphRasterCache)
    /// owns the atlas slots and only releases them on LRU / atlas-full
    /// eviction. Per-frame clearing was a workaround for the previous
    /// double-allocator architecture and meant we re-rasterised every
    /// glyph every frame.
    ///
    /// Diagnostic: `KASANE_PARLEY_NO_CACHE=1` reverts to the pre-Step-4c
    /// behaviour by invalidating the cache + clearing both atlases here.
    /// Used to confirm whether a bug originates in the cache layer.
    fn text_frame_start(&mut self) {
        self.drawables.clear();
        // Bump the L2 cache's frame epoch so eviction can distinguish
        // entries already drawable-pushed in this frame from older
        // ones (Phase 9b Step 4c follow-up — fixes the "info popup
        // glyphs appear scrambled" bug caused by mid-frame slot reuse).
        self.raster_cache.bump_epoch();
        if parley_cache_disabled() {
            self.raster_cache.invalidate_all();
            self.mask_atlas.clear();
            self.color_atlas.clear();
        }
    }

    /// Hand drawables accumulated since the last call to the Parley
    /// renderer. Called once per layer (so each layer's overlay text
    /// reaches the GPU before that layer's render pass begins). The
    /// drawables vector is then cleared so the next layer accumulates
    /// fresh entries; atlases keep their slots across layers within the
    /// frame.
    fn finalize_text_frame(&mut self, device: &wgpu::Device, queue: &wgpu::Queue) {
        tracing::trace!(
            target: "kasane::parley::frame",
            drawables = self.drawables.len() as u32,
            mask_pending = self.mask_atlas.pending_uploads().len() as u32,
            color_pending = self.color_atlas.pending_uploads().len() as u32,
            "parley finalize"
        );
        self.text_renderer.prepare(
            device,
            queue,
            &self.cache,
            &mut self.mask_atlas,
            &mut self.color_atlas,
            &self.drawables,
        );
        self.drawables.clear();
    }

    /// Cell-grid mouse hit test.
    ///
    /// Returns `(col, row)` on the display grid. Kasane is a Kakoune
    /// frontend and Kakoune is cell-based, so cell-grid resolution is
    /// the right answer for keyboard / mouse → editor coordinate
    /// translation; clicks on multi-cell glyphs (CJK, ligatures) land
    /// on the leftmost cell of the cluster, which matches Kakoune's
    /// own input model.
    ///
    /// `super::text::hit_test::hit_byte` is the byte-precise
    /// alternative used by paragraph-internal cursor placement
    /// (`draw_commands.rs::byte_to_advance`). Mouse → byte mapping is
    /// only needed when the renderer runs a proportional font; until
    /// that happens the cell-grid path stays as the production hit test.
    pub fn hit_test(&self, px: f64, py: f64) -> (u16, u16) {
        let cell_h = self.metrics.cell_height;
        let cell_w = self.metrics.cell_width;
        let row = (py as f32 / cell_h).floor().max(0.0) as u16;
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
        // Rebuild the cached default family so subsequent shapes pick up
        // the new fallback list. Required before metrics recomputation
        // because `calculate_with_parley` itself shapes a probe line.
        self.text.set_default_family(font_config);
        self.metrics = super::text::metrics::calculate_with_parley(
            &mut self.text,
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
        // Font / scale changed → all three cache tiers drop in lockstep.
        self.layout_cache.invalidate_all();
        self.raster_cache.invalidate_all();
        self.mask_atlas.clear();
        self.color_atlas.clear();
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
        self.clip_stack.clear();
        self.paragraph_cursor = None;
        // Bump frame epoch + reset per-frame Parley state.
        self.text_frame_start();
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

            // Process this layer's DrawCommands
            for cmd in &commands[range_start..range_end] {
                self.process_draw_command(cmd, gpu, color_resolver, cell_w, cell_h);
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

            // Phase 11 — finalize the Parley draw for this layer.
            // process_draw_* has accumulated DrawableGlyphs into
            // self.drawables during this layer's processing.
            // finalize_text_frame uploads atlas pixels + writes the
            // vertex buffer + clears the accumulator for the next layer.
            self.finalize_text_frame(&gpu.device, &gpu.queue);

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
                    // Standard path: Parley text directly into the main
                    // render pass. finalize_text_frame above already
                    // wrote the vertex buffer for this layer.
                    self.text_renderer.render(&self.viewport, &mut render_pass);
                }

                self.image.upload_and_draw(gpu, &mut render_pass);

                if text_effects_active {
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
                    self.text_renderer.render(&self.viewport, &mut text_pass);
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

// ADR-032: GpuBackend implementation. Pure pass-through to the inherent
// `pub fn` surface. The call site in `crate::app::render::submit_render`
// continues to use the inherent methods; this impl exists so the
// `kasane-vello-spike` crate can target the same trait surface.
impl super::backend::GpuBackend for SceneRenderer {
    fn render_with_cursor(
        &mut self,
        gpu: &super::GpuState,
        commands: &[DrawCommand],
        color_resolver: &ColorResolver,
        cursor_style: CursorStyle,
        cursor_state: &CursorRenderState,
        cursor_color: kasane_core::protocol::Color,
        overlay_opacities: &[f32],
        visual_hints: &kasane_core::render::VisualHints,
    ) -> Result<(), super::backend::BackendError> {
        SceneRenderer::render_with_cursor(
            self,
            gpu,
            commands,
            color_resolver,
            cursor_style,
            cursor_state,
            cursor_color,
            overlay_opacities,
            visual_hints,
        )
        .map_err(super::backend::BackendError::from)
    }

    fn resize(
        &mut self,
        gpu: &super::GpuState,
        font_config: &FontConfig,
        scale_factor: f64,
        window_size: PhysicalSize<u32>,
    ) {
        SceneRenderer::resize(self, gpu, font_config, scale_factor, window_size);
    }

    fn capabilities(&self) -> super::backend::BackendCapabilities {
        super::backend::BackendCapabilities {
            supports_paths: false,
            supports_compute: false,
            atlas_kind: super::backend::AtlasKind::EtagereShelf,
        }
    }
}
