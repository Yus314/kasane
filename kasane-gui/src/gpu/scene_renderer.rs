use super::text_pipeline::{Cache, Resolution, TextAtlas, TextRenderer, Viewport};
use crate::animation::CursorRenderState;
use cosmic_text::{Buffer as GlyphonBuffer, FontSystem, Metrics, Shaping, SwashCache};
use kasane_core::config::FontConfig;
use kasane_core::render::scene::line_display_width_str;
use kasane_core::render::{
    CellSize, CursorStyle, DrawCommand, PixelRect,
    scene::{BufferParagraph, ParagraphAnnotation},
};
use wgpu::MultisampleState;
use winit::dpi::PhysicalSize;

use kasane_core::element::BorderLineStyle;

use kasane_core::protocol::Attributes;

use super::bg_pipeline::BgPipeline;
use super::border_pipeline::BorderPipeline;
use super::compositor::{BlitPipeline, BlurPipeline, RenderTarget};
use super::decoration_pipeline::{self, DecorationPipeline};
use super::gradient_pipeline::GradientPipeline;
use super::image_pipeline::ImagePipeline;
use super::texture_cache::{LoadState, TextureCache, TextureKey};
use super::{CURSOR_BAR_WIDTH, CURSOR_OUTLINE_THICKNESS, CURSOR_UNDERLINE_HEIGHT, CellMetrics};
use crate::colors::ColorResolver;

/// Scene-based GPU renderer that processes DrawCommands directly,
/// bypassing the CellGrid intermediate representation.
pub struct SceneRenderer {
    font_system: FontSystem,
    swash_cache: SwashCache,
    viewport: Viewport,
    text_atlas: TextAtlas,
    text_renderer: TextRenderer,
    shadow: BorderPipeline,
    gradient: GradientPipeline,
    bg: BgPipeline,
    border: BorderPipeline,
    decoration: DecorationPipeline,
    image: ImagePipeline,
    texture_cache: TextureCache,
    metrics: CellMetrics,

    // Reusable text buffers (growable pool)
    text_buffers: Vec<GlyphonBuffer>,
    /// Position (left, top) for each text buffer allocated this frame.
    text_positions: Vec<(f32, f32)>,
    /// Clip bounds (left, top, right, bottom) for each text buffer.
    text_clip_bounds: Vec<(i32, i32, i32, i32)>,
    text_buffer_count: usize,

    // Scratch buffers
    row_text: String,
    span_ranges: Vec<(usize, usize, [f32; 4])>,

    /// Glyph-accurate primary cursor position and width from RenderParagraph.
    /// Overrides the cell-based cursor position/width in render_cursor().
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

        let surface_format = gpu.config.format;

        let cache = Cache::new(&gpu.device);
        let mut text_atlas = TextAtlas::new(&gpu.device, &gpu.queue, &cache, surface_format);
        let text_renderer = TextRenderer::new(
            &mut text_atlas,
            &gpu.device,
            MultisampleState::default(),
            None,
        );
        let mut viewport = Viewport::new(&gpu.device, &cache);
        viewport.update(
            &gpu.queue,
            Resolution {
                width: window_size.width.max(1),
                height: window_size.height.max(1),
            },
        );

        let shadow = BorderPipeline::new(gpu, surface_format);
        let gradient = GradientPipeline::new(gpu, surface_format);
        let blit = BlitPipeline::new(&gpu.device, surface_format);
        let blur = BlurPipeline::new(&gpu.device, surface_format);
        let bg = BgPipeline::new(gpu, surface_format);
        let border = BorderPipeline::new(gpu, surface_format);
        let decoration = DecorationPipeline::new(gpu, surface_format);
        let texture_cache = TextureCache::new(&gpu.device, 128 * 1024 * 1024); // 128 MB budget
        let image = ImagePipeline::new(gpu, surface_format, texture_cache.bind_group_layout());

        SceneRenderer {
            font_system,
            swash_cache,
            viewport,
            text_atlas,
            text_renderer,
            shadow,
            gradient,
            bg,
            border,
            decoration,
            image,
            texture_cache,
            metrics,
            text_buffers: Vec::with_capacity(128),
            text_positions: Vec::with_capacity(128),
            text_clip_bounds: Vec::with_capacity(128),
            text_buffer_count: 0,
            row_text: String::with_capacity(512),
            span_ranges: Vec::with_capacity(256),
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
        }
    }

    pub fn metrics(&self) -> &CellMetrics {
        &self.metrics
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
        let dbg = color_resolver.default_bg();
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
            self.bg.push_rect(0.0, 0.0, sw, pane.y, dim_color);
        }
        // Bottom strip (below focused pane)
        let bottom = pane.y + pane.h;
        if bottom < sh {
            self.bg.push_rect(0.0, bottom, sw, sh - bottom, dim_color);
        }
        // Left strip (within pane row)
        if pane.x > 0.0 {
            self.bg.push_rect(0.0, pane.y, pane.x, pane.h, dim_color);
        }
        // Right strip (within pane row)
        let right = pane.x + pane.w;
        if right < sw {
            self.bg
                .push_rect(right, pane.y, sw - right, pane.h, dim_color);
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
        self.text_buffers.clear();
        self.text_positions.clear();
        self.text_buffer_count = 0;
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
        for buffer in [
            self.shadow.uniform_buffer(),
            self.gradient.uniform_buffer(),
            self.bg.uniform_buffer(),
            self.border.uniform_buffer(),
            self.decoration.uniform_buffer(),
            self.image.uniform_buffer(),
        ] {
            gpu.queue.write_buffer(buffer, 0, screen_size_data);
        }

        // Reset per-frame state
        self.texture_cache.frame_tick();
        self.text_buffer_count = 0;
        self.text_positions.clear();
        self.text_clip_bounds.clear();
        self.clip_stack.clear();
        self.paragraph_cursor = None;

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
            self.shadow.instances.clear();
            self.gradient.instances.clear();
            self.bg.instances.clear();
            self.border.instances.clear();
            self.decoration.instances.clear();
            self.image.clear_frame();
            let layer_text_start = self.text_buffer_count;

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
                    self.gradient.push(0.0, 0.0, screen_w, screen_h, start, end);
                }

                // Cursor line highlight
                if let Some((_cx, cy, _opacity, _style, _color)) = cursor
                    && self.effects.cursor_line_highlight
                        != kasane_core::config::CursorLineHighlightMode::Off
                {
                    let fg = color_resolver.resolve(kasane_core::protocol::Color::Default, true);
                    let highlight_color = [fg[0], fg[1], fg[2], 0.03];
                    let line_y = (cy / cell_h).floor() * cell_h;
                    self.bg
                        .push_rect(0.0, line_y, screen_w, cell_h, highlight_color);
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
                    // Multiply alpha channel of all bg instances (stride=8, alpha at offset 7)
                    for chunk in self.bg.instances.chunks_exact_mut(8) {
                        chunk[7] *= opacity;
                    }
                    // Border instances (stride=14, fill alpha at offset 9, border alpha at offset 13)
                    for chunk in self.border.instances.chunks_exact_mut(14) {
                        chunk[9] *= opacity;
                        chunk[13] *= opacity;
                    }
                    // Decoration instances (stride=10, alpha at offset 7)
                    for chunk in self.decoration.instances.chunks_exact_mut(10) {
                        chunk[7] *= opacity;
                    }
                    // Shadow instances (stride=14, fill alpha at offset 9)
                    for chunk in self.shadow.instances.chunks_exact_mut(14) {
                        chunk[9] *= opacity;
                    }
                }
            }

            // Ensure GPU buffers are large enough for this layer
            let shadow_count = self.shadow.instances.len() / 14;
            self.shadow.ensure_buffer(gpu, shadow_count);

            let gradient_count = self.gradient.instances.len() / 12;
            self.gradient.ensure_buffer(gpu, gradient_count);

            let bg_count = self.bg.instances.len() / 8;
            self.bg.ensure_buffer(gpu, bg_count);

            let border_count = self.border.instances.len() / 14;
            self.border.ensure_buffer(gpu, border_count);

            let deco_count = self.decoration.instances.len() / 10;
            self.decoration.ensure_buffer(gpu, deco_count);

            let image_count = self.image.instances.len() / 9;
            self.image.ensure_buffer(gpu, image_count);

            // Build TextAreas for this layer's text buffers only
            let layer_text_end = self.text_buffer_count;
            let layer_clips = &self.text_clip_bounds[layer_text_start..layer_text_end];
            let has_clips = layer_clips.iter().any(|&(l, t, r, b)| {
                l != 0 || t != 0 || r != screen_w as i32 || b != screen_h as i32
            });
            let text_areas = super::text_helpers::prepare_text_areas(
                &self.text_positions[layer_text_start..layer_text_end],
                &self.text_buffers[layer_text_start..layer_text_end],
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
                    let default_bg = color_resolver.default_bg();
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
                    depth_stencil_attachment: None,
                    timestamp_writes: None,
                    occlusion_query_set: None,
                    multiview_mask: None,
                });

                self.shadow.upload_and_draw(gpu, &mut render_pass);
                self.gradient.upload_and_draw(gpu, &mut render_pass);
                self.bg.upload_and_draw(gpu, &mut render_pass);
                self.border.upload_and_draw(gpu, &mut render_pass);
                self.text_renderer
                    .render(&self.text_atlas, &self.viewport, &mut render_pass)
                    .map_err(|e| anyhow::anyhow!("glyphon render failed: {e}"))?;
                self.image.upload_and_draw(gpu, &mut render_pass);
                self.decoration.upload_and_draw(gpu, &mut render_pass);
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

    /// Process a single DrawCommand, dispatching to the appropriate pipeline.
    fn process_draw_command(
        &mut self,
        cmd: &DrawCommand,
        gpu: &super::GpuState,
        color_resolver: &ColorResolver,
        cell_w: f32,
        cell_h: f32,
        screen_w: f32,
    ) {
        match cmd {
            DrawCommand::FillRect {
                rect,
                face,
                elevated,
            } => {
                let Some((cx, cy, cw, ch)) = self.clip_rect(rect.x, rect.y, rect.w, rect.h) else {
                    return;
                };
                let (_, mut bg, _) = color_resolver.resolve_face_colors(face);

                // When gradient is active, skip fills matching default bg
                // so the gradient shows through.
                if !*elevated && self.should_skip_default_bg(&bg, color_resolver) {
                    return;
                }
                if *elevated {
                    // Subtle elevation: ~10/255 in sRGB ≈ VS Code's floating window offset
                    bg[0] = (bg[0] + 0.04).min(1.0);
                    bg[1] = (bg[1] + 0.04).min(1.0);
                    bg[2] = (bg[2] + 0.04).min(1.0);
                    tracing::debug!(
                        "elevated FillRect: bg=[{:.3},{:.3},{:.3}] rect=({:.0},{:.0},{:.0},{:.0})",
                        bg[0],
                        bg[1],
                        bg[2],
                        cx,
                        cy,
                        cw,
                        ch,
                    );
                }
                self.bg.push_rect(cx, cy, cw, ch, bg);
            }
            DrawCommand::DrawAtoms {
                pos,
                atoms,
                max_width,
            } => {
                self.process_draw_atoms(pos.x, pos.y, atoms, *max_width, color_resolver);
            }
            DrawCommand::DrawText {
                pos,
                text,
                face,
                max_width,
            } => {
                self.process_draw_text(pos.x, pos.y, text, face, *max_width, color_resolver);
            }
            DrawCommand::DrawPaddingRow {
                pos,
                width: _,
                ch,
                face,
            } => {
                let (visual_fg, _, _) = color_resolver.resolve_face_colors(face);
                let buf_idx = self.alloc_text_buffer(screen_w);
                self.text_positions.push((pos.x, pos.y));
                self.push_text_clip_bounds();
                let attrs = super::text_helpers::default_attrs(&self.font_family);
                let color = super::text_helpers::to_glyphon_color(visual_fg);
                let buffer = &mut self.text_buffers[buf_idx];
                buffer.set_rich_text(
                    &mut self.font_system,
                    [(ch.as_str(), attrs.clone().color(color))],
                    &attrs,
                    Shaping::Advanced,
                    None,
                );
                buffer.shape_until_scroll(&mut self.font_system, false);
            }
            DrawCommand::DrawBorder {
                rect,
                line_style,
                face,
                fill_face,
            } => {
                let (visual_fg, _, _) = color_resolver.resolve_face_colors(face);
                let border_color = visual_fg;
                let (corner_radius, border_width) =
                    super::text_helpers::border_style_params(line_style.clone(), cell_h);
                let fill = match fill_face {
                    Some(ff) => {
                        let (_, ff_bg, _) = color_resolver.resolve_face_colors(ff);
                        ff_bg
                    }
                    None => [0.0, 0.0, 0.0, 0.0],
                };
                if *line_style == BorderLineStyle::Double {
                    self.border.push_rounded_rect(
                        rect.x,
                        rect.y,
                        rect.w,
                        rect.h,
                        corner_radius,
                        border_width,
                        fill,
                        border_color,
                    );
                    let inset = border_width + 1.0;
                    if rect.w > inset * 2.0 && rect.h > inset * 2.0 {
                        self.border.push_rounded_rect(
                            rect.x + inset,
                            rect.y + inset,
                            rect.w - inset * 2.0,
                            rect.h - inset * 2.0,
                            corner_radius,
                            border_width,
                            fill,
                            border_color,
                        );
                    }
                } else {
                    self.border.push_rounded_rect(
                        rect.x,
                        rect.y,
                        rect.w,
                        rect.h,
                        corner_radius,
                        border_width,
                        fill,
                        border_color,
                    );
                }
            }
            DrawCommand::DrawBorderTitle {
                rect,
                title,
                border_face,
                elevated,
            } => {
                let title_w: f32 = title
                    .iter()
                    .map(|a| line_display_width_str(&a.contents) as f32 * cell_w)
                    .sum();
                let pad_x = cell_w * 0.5;
                let title_x = rect.x + (rect.w - title_w) / 2.0;
                let title_y = rect.y - cell_h * 0.35;

                let (_, mut title_bg, _) = color_resolver.resolve_face_colors(border_face);
                if *elevated {
                    // Subtle elevation: ~10/255 in sRGB ≈ VS Code's floating window offset
                    title_bg[0] = (title_bg[0] + 0.04).min(1.0);
                    title_bg[1] = (title_bg[1] + 0.04).min(1.0);
                    title_bg[2] = (title_bg[2] + 0.04).min(1.0);
                }

                self.border.push_rounded_rect(
                    title_x - pad_x,
                    title_y,
                    title_w + pad_x * 2.0,
                    cell_h,
                    0.0,
                    0.0,
                    title_bg,
                    [0.0, 0.0, 0.0, 0.0],
                );

                self.process_draw_atoms(title_x, title_y, title, title_w, color_resolver);
            }
            DrawCommand::DrawShadow {
                rect,
                offset,
                blur_radius,
                color,
            } => {
                let expand = *blur_radius;
                self.shadow.push_rounded_rect(
                    rect.x + offset.0 - expand,
                    rect.y + offset.1 - expand,
                    rect.w + expand * 2.0,
                    rect.h + expand * 2.0,
                    expand,
                    0.0,
                    *color,
                    [0.0, 0.0, 0.0, 0.0],
                );
            }
            DrawCommand::PushClip(rect) => {
                // Intersect with current clip (if any) to handle nested clips
                let new_clip = if let Some(cur) = self.current_clip() {
                    let x1 = rect.x.max(cur.x);
                    let y1 = rect.y.max(cur.y);
                    let x2 = (rect.x + rect.w).min(cur.x + cur.w);
                    let y2 = (rect.y + rect.h).min(cur.y + cur.h);
                    PixelRect {
                        x: x1,
                        y: y1,
                        w: (x2 - x1).max(0.0),
                        h: (y2 - y1).max(0.0),
                    }
                } else {
                    rect.clone()
                };
                self.clip_stack.push(new_clip);
            }
            DrawCommand::PopClip => {
                self.clip_stack.pop();
            }
            DrawCommand::DrawImage {
                rect,
                source,
                fit,
                opacity,
            } => {
                let Some((cx, cy, cw, ch)) = self.clip_rect(rect.x, rect.y, rect.w, rect.h) else {
                    return;
                };
                let key = match source {
                    kasane_core::element::ImageSource::FilePath(path) => {
                        TextureKey::FilePath(path.clone())
                    }
                    kasane_core::element::ImageSource::Rgba {
                        data,
                        width,
                        height,
                    } => {
                        let k = TextureKey::inline_from_data(data, *width, *height);
                        // Ensure inline data is in the cache (synchronous for inline)
                        if !self.texture_cache.insert_rgba(
                            k.clone(),
                            data,
                            *width,
                            *height,
                            &gpu.device,
                            &gpu.queue,
                        ) {
                            return;
                        }
                        k
                    }
                    kasane_core::element::ImageSource::SvgData { data } => {
                        let k = TextureKey::inline_from_svg_data(data);
                        // Rasterize inline SVG synchronously (like RGBA inline data)
                        if self.texture_cache.get_view(&k).is_none() {
                            match kasane_core::render::svg::render_svg_to_rgba_intrinsic(data, 8192)
                            {
                                Ok(r) => {
                                    if !self.texture_cache.insert_rgba(
                                        k.clone(),
                                        &r.data,
                                        r.width,
                                        r.height,
                                        &gpu.device,
                                        &gpu.queue,
                                    ) {
                                        return;
                                    }
                                }
                                Err(e) => {
                                    tracing::warn!("SVG render failed: {e}");
                                    self.bg.push_rect(cx, cy, cw, ch, [0.2, 0.2, 0.2, 1.0]);
                                    return;
                                }
                            }
                        }
                        k
                    }
                };
                // Look up or dispatch async load
                match self.texture_cache.get_or_load(&key, &self.event_proxy) {
                    LoadState::Ready(tex_w, tex_h) => {
                        let view = self.texture_cache.get_view(&key).unwrap();
                        let bind_group = self.texture_cache.create_bind_group(&gpu.device, view);
                        self.image.push_textured_quad(
                            bind_group,
                            tex_w as f32,
                            tex_h as f32,
                            cx,
                            cy,
                            cw,
                            ch,
                            *fit,
                            *opacity,
                        );
                    }
                    LoadState::Pending => {
                        // Loading in progress — draw semi-transparent placeholder
                        self.bg.push_rect(cx, cy, cw, ch, [0.15, 0.15, 0.15, 0.6]);
                    }
                    LoadState::Failed => {
                        // Failed — draw grey placeholder
                        self.bg.push_rect(cx, cy, cw, ch, [0.2, 0.2, 0.2, 1.0]);
                    }
                }
            }
            DrawCommand::RenderParagraph {
                pos,
                max_width,
                paragraph,
            } => {
                self.process_render_paragraph(pos.x, pos.y, *max_width, paragraph, color_resolver);
            }
            DrawCommand::BeginOverlay => {} // handled by layer splitting
        }
    }

    /// Render the cursor into the bg pipeline.
    ///
    /// When `paragraph_cursor` is set (from RenderParagraph shaping),
    /// the glyph-accurate x position and width are used instead of the
    /// cell-based values. This ensures correct cursor placement and size
    /// with proportional fonts, CJK characters, and RTL text.
    fn render_cursor(
        &mut self,
        cursor: Option<(f32, f32, f32, CursorStyle, kasane_core::protocol::Color)>,
        color_resolver: &ColorResolver,
        cell_w: f32,
        cell_h: f32,
    ) {
        let Some((cell_x, y, opacity, style, cursor_color)) = cursor else {
            return;
        };
        let (x, w) = self
            .paragraph_cursor
            .map(|(gx, gw)| (gx, gw.max(CURSOR_BAR_WIDTH)))
            .unwrap_or((cell_x, cell_w));
        let mut cc = color_resolver.resolve(cursor_color, true);
        cc[3] = opacity;
        match style {
            CursorStyle::Block => {
                self.bg.push_rect(x, y, w, cell_h, cc);
            }
            CursorStyle::Bar => {
                self.bg.push_rect(x, y, CURSOR_BAR_WIDTH, cell_h, cc);
            }
            CursorStyle::Underline => {
                self.bg.push_rect(
                    x,
                    y + cell_h - CURSOR_UNDERLINE_HEIGHT,
                    w,
                    CURSOR_UNDERLINE_HEIGHT,
                    cc,
                );
            }
            CursorStyle::Outline => {
                let t = CURSOR_OUTLINE_THICKNESS;
                self.bg.push_rect(x, y, w, t, cc);
                self.bg.push_rect(x, y + cell_h - t, w, t, cc);
                self.bg.push_rect(x, y, t, cell_h, cc);
                self.bg.push_rect(x + w - t, y, t, cell_h, cc);
            }
        }
    }

    /// Emit decoration instances for a face's text attributes.
    fn emit_decorations(
        &mut self,
        x: f32,
        py: f32,
        w: f32,
        face: &kasane_core::protocol::Face,
        fg: [f32; 4],
        color_resolver: &ColorResolver,
    ) {
        let attrs = face.attributes;
        if !attrs.intersects(
            Attributes::UNDERLINE
                | Attributes::CURLY_UNDERLINE
                | Attributes::DOUBLE_UNDERLINE
                | Attributes::DOTTED_UNDERLINE
                | Attributes::DASHED_UNDERLINE
                | Attributes::STRIKETHROUGH,
        ) {
            return;
        }

        let baseline = self.metrics.baseline;
        let cell_h = self.metrics.cell_height;
        let thickness = (cell_h * 0.06).max(1.0);

        // Underline color: use face.underline if set, otherwise fallback to fg
        let ul_color = if face.underline != kasane_core::protocol::Color::Default {
            color_resolver.resolve(face.underline, true)
        } else {
            fg
        };

        if attrs.contains(Attributes::UNDERLINE) {
            let y = py + baseline + thickness;
            self.decoration.push(
                x,
                y,
                w,
                thickness,
                ul_color,
                decoration_pipeline::DECO_SOLID,
            );
        }
        if attrs.contains(Attributes::CURLY_UNDERLINE) {
            // Curly needs more height for the wave amplitude
            let wave_h = (cell_h * 0.2).max(4.0);
            let y = py + baseline + thickness - wave_h * 0.25;
            self.decoration
                .push(x, y, w, wave_h, ul_color, decoration_pipeline::DECO_CURLY);
        }
        if attrs.contains(Attributes::DOUBLE_UNDERLINE) {
            let double_h = (cell_h * 0.15).max(4.0);
            let y = py + baseline + thickness - double_h * 0.1;
            self.decoration.push(
                x,
                y,
                w,
                double_h,
                ul_color,
                decoration_pipeline::DECO_DOUBLE,
            );
        }
        if attrs.contains(Attributes::DOTTED_UNDERLINE) {
            let dot_h = (cell_h * 0.15).max(4.0);
            let y = py + baseline + thickness - dot_h * 0.1;
            self.decoration
                .push(x, y, w, dot_h, ul_color, decoration_pipeline::DECO_DOTTED);
        }
        if attrs.contains(Attributes::DASHED_UNDERLINE) {
            let y = py + baseline + thickness;
            let dash_h = (cell_h * 0.08).max(2.0);
            self.decoration
                .push(x, y, w, dash_h, ul_color, decoration_pipeline::DECO_DASHED);
        }
        if attrs.contains(Attributes::STRIKETHROUGH) {
            // Strikethrough at approximately the x-height center
            let y = py + baseline * 0.55;
            self.decoration
                .push(x, y, w, thickness, fg, decoration_pipeline::DECO_SOLID);
        }
    }

    /// Process DrawAtoms: shaping-first approach for proportional font support.
    ///
    /// Adjacent atoms with the same foreground color are merged into a single
    /// shaping span so that ligatures (e.g. `->`, `!=`, `=>`) can form across
    /// atom boundaries. Background rectangles and decorations are computed from
    /// glyph metrics after shaping, so they adapt to proportional fonts.
    fn process_draw_atoms(
        &mut self,
        px: f32,
        py: f32,
        atoms: &[kasane_core::render::ResolvedAtom],
        max_width: f32,
        color_resolver: &ColorResolver,
    ) {
        let cell_h = self.metrics.cell_height;

        // === Step 1: Concatenate text + track atom byte boundaries + build spans ===
        self.row_text.clear();
        self.span_ranges.clear();
        let mut atom_byte_boundaries: Vec<usize> = vec![0];
        let mut atom_faces: Vec<&kasane_core::protocol::Face> = Vec::new();

        for atom in atoms {
            let (visual_fg, _, _) = color_resolver.resolve_face_colors(&atom.face);
            let fg = visual_fg;

            // Text span — merge with previous span if same fg color
            if let Some(last) = self.span_ranges.last_mut() {
                if last.2 == fg {
                    self.row_text.push_str(&atom.contents);
                    last.1 = self.row_text.len();
                } else {
                    let start = self.row_text.len();
                    self.row_text.push_str(&atom.contents);
                    self.span_ranges.push((start, self.row_text.len(), fg));
                }
            } else {
                let start = self.row_text.len();
                self.row_text.push_str(&atom.contents);
                self.span_ranges.push((start, self.row_text.len(), fg));
            }

            atom_byte_boundaries.push(self.row_text.len());
            atom_faces.push(&atom.face);
        }

        if self.row_text.is_empty() {
            return;
        }

        // === Step 2: Shape text with cosmic-text ===
        let buf_idx = self.alloc_text_buffer(max_width);
        let default_attrs = super::text_helpers::default_attrs(&self.font_family);

        let rich_text_iter = self.span_ranges.iter().map(|(start, end, fg)| {
            let text = &self.row_text[*start..*end];
            let color = super::text_helpers::to_glyphon_color(*fg);
            (text, default_attrs.clone().color(color))
        });

        let buffer = &mut self.text_buffers[buf_idx];
        buffer.set_rich_text(
            &mut self.font_system,
            rich_text_iter,
            &default_attrs,
            Shaping::Advanced,
            None,
        );
        buffer.shape_until_scroll(&mut self.font_system, false);

        // === Step 3: Compute per-atom pixel extents from glyph metrics ===
        let atom_count = atom_faces.len();
        let mut atom_x_min = vec![f32::MAX; atom_count];
        let mut atom_x_max = vec![f32::MIN; atom_count];

        let buffer = &self.text_buffers[buf_idx];
        for run in buffer.layout_runs() {
            for glyph in run.glyphs.iter() {
                let idx = atom_byte_boundaries
                    .partition_point(|&b| b <= glyph.start)
                    .saturating_sub(1);
                if idx < atom_count {
                    atom_x_min[idx] = atom_x_min[idx].min(glyph.x);
                    atom_x_max[idx] = atom_x_max[idx].max(glyph.x + glyph.w);
                }
            }
        }

        // === Step 4: Per-atom background rectangles + decorations ===
        for i in 0..atom_count {
            if atom_x_min[i] > atom_x_max[i] {
                continue; // No glyphs for this atom (empty)
            }
            let w = (atom_x_max[i] - atom_x_min[i]).min(max_width);
            let x = px + atom_x_min[i];
            let (visual_fg, visual_bg, needs_bg) =
                color_resolver.resolve_face_colors(atom_faces[i]);

            if w > 0.0 && needs_bg && !self.should_skip_default_bg(&visual_bg, color_resolver) {
                self.bg.push_rect(x, py, w, cell_h, visual_bg);
            }
            if w > 0.0 {
                self.emit_decorations(x, py, w, atom_faces[i], visual_fg, color_resolver);
            }
        }

        // === Step 5: Register text position ===
        self.text_positions.push((px, py));
        self.push_text_clip_bounds();
    }

    /// Process RenderParagraph: buffer line with semantic annotations.
    ///
    /// Shapes text first, then draws per-atom backgrounds and decorations from
    /// glyph metrics, and finally resolves annotation positions (cursors) from
    /// the shaping result.
    fn process_render_paragraph(
        &mut self,
        px: f32,
        py: f32,
        max_width: f32,
        para: &BufferParagraph,
        color_resolver: &ColorResolver,
    ) {
        let cell_h = self.metrics.cell_height;
        let cell_w = self.metrics.cell_width;

        // 1. Line-wide background fill (always drawn, matching old FillRect behavior).
        // Only skip when gradient is active and bg matches default.
        let (_, base_bg, _) = color_resolver.resolve_face_colors(&para.base_face);
        if !self.should_skip_default_bg(&base_bg, color_resolver) {
            self.bg.push_rect(px, py, max_width, cell_h, base_bg);
        }

        if para.atoms.is_empty() {
            return;
        }

        // 2. Identify primary cursor atom index for face stripping.
        // Always strip the cursor face for ALL styles: render_cursor() is the
        // single authoritative cursor rendering, using glyph-accurate position
        // and width. Keeping the REVERSE face would create a second cursor block
        // (at glyph width) that conflicts with render_cursor's block (which now
        // also uses glyph width), and for CJK the old cell_w mismatch caused
        // a visible "split cursor".
        let mut clear_cursor_atom_idx: Option<usize> = None;
        for ann in &para.annotations {
            if let ParagraphAnnotation::PrimaryCursor { byte_offset, .. } = ann {
                let mut accum = 0usize;
                for (i, atom) in para.atoms.iter().enumerate() {
                    let atom_end = accum + atom.contents.len();
                    if *byte_offset >= accum && *byte_offset < atom_end {
                        clear_cursor_atom_idx = Some(i);
                        break;
                    }
                    accum = atom_end;
                }
            }
        }

        // 3. Concatenate text + track atom byte boundaries + build color spans
        self.row_text.clear();
        self.span_ranges.clear();
        let mut atom_byte_boundaries: Vec<usize> = vec![0];
        let mut atom_faces: Vec<kasane_core::protocol::Face> = Vec::new();

        for (i, atom) in para.atoms.iter().enumerate() {
            // Strip cursor face: render_cursor() handles all cursor drawing
            // with correct glyph width. The REVERSE bg would conflict.
            let face = if clear_cursor_atom_idx == Some(i) {
                para.base_face
            } else {
                atom.face
            };

            let (visual_fg, _, _) = color_resolver.resolve_face_colors(&face);
            let fg = visual_fg;

            if let Some(last) = self.span_ranges.last_mut() {
                if last.2 == fg {
                    self.row_text.push_str(&atom.contents);
                    last.1 = self.row_text.len();
                } else {
                    let start = self.row_text.len();
                    self.row_text.push_str(&atom.contents);
                    self.span_ranges.push((start, self.row_text.len(), fg));
                }
            } else {
                let start = self.row_text.len();
                self.row_text.push_str(&atom.contents);
                self.span_ranges.push((start, self.row_text.len(), fg));
            }

            atom_byte_boundaries.push(self.row_text.len());
            atom_faces.push(face);
        }

        if self.row_text.is_empty() {
            return;
        }

        // 4. Shape text with cosmic-text
        let buf_idx = self.alloc_text_buffer(max_width);
        let default_attrs = super::text_helpers::default_attrs(&self.font_family);

        let rich_text_iter = self.span_ranges.iter().map(|(start, end, fg)| {
            let text = &self.row_text[*start..*end];
            let color = super::text_helpers::to_glyphon_color(*fg);
            (text, default_attrs.clone().color(color))
        });

        let buffer = &mut self.text_buffers[buf_idx];
        buffer.set_rich_text(
            &mut self.font_system,
            rich_text_iter,
            &default_attrs,
            Shaping::Advanced,
            None,
        );
        buffer.shape_until_scroll(&mut self.font_system, false);

        // 5. Per-atom background rectangles + decorations from glyph metrics
        let atom_count = atom_faces.len();
        let mut atom_x_min = vec![f32::MAX; atom_count];
        let mut atom_x_max = vec![f32::MIN; atom_count];

        let buffer = &self.text_buffers[buf_idx];
        for run in buffer.layout_runs() {
            for glyph in run.glyphs.iter() {
                let idx = atom_byte_boundaries
                    .partition_point(|&b| b <= glyph.start)
                    .saturating_sub(1);
                if idx < atom_count {
                    atom_x_min[idx] = atom_x_min[idx].min(glyph.x);
                    atom_x_max[idx] = atom_x_max[idx].max(glyph.x + glyph.w);
                }
            }
        }

        for i in 0..atom_count {
            if atom_x_min[i] > atom_x_max[i] {
                continue;
            }
            let w = (atom_x_max[i] - atom_x_min[i]).min(max_width);
            let x = px + atom_x_min[i];
            let (visual_fg, visual_bg, needs_bg) =
                color_resolver.resolve_face_colors(&atom_faces[i]);

            if w > 0.0 && needs_bg && !self.should_skip_default_bg(&visual_bg, color_resolver) {
                self.bg.push_rect(x, py, w, cell_h, visual_bg);
            }
            if w > 0.0 {
                self.emit_decorations(x, py, w, &atom_faces[i], visual_fg, color_resolver);
            }
        }

        // 6. Resolve annotation positions from glyph metrics
        let buffer = &self.text_buffers[buf_idx];
        for ann in &para.annotations {
            match ann {
                ParagraphAnnotation::PrimaryCursor { byte_offset, .. } => {
                    // Store glyph-accurate cursor position and width for render_cursor()
                    if let Some((gx, gw)) = find_glyph_at_byte_offset(buffer, *byte_offset) {
                        self.paragraph_cursor = Some((px + gx, gw));
                    }
                }
                ParagraphAnnotation::SecondaryCursor {
                    byte_offset,
                    blend_ratio,
                } => {
                    if let Some((gx, gw)) = find_glyph_at_byte_offset(buffer, *byte_offset) {
                        let x = px + gx;
                        let w = gw.max(cell_w);
                        let cursor_color = [1.0_f32, 1.0, 1.0, 1.0];
                        let bg_color = base_bg;
                        let blended = [
                            cursor_color[0] * blend_ratio + bg_color[0] * (1.0 - blend_ratio),
                            cursor_color[1] * blend_ratio + bg_color[1] * (1.0 - blend_ratio),
                            cursor_color[2] * blend_ratio + bg_color[2] * (1.0 - blend_ratio),
                            1.0,
                        ];
                        self.bg.push_rect(x, py, w, cell_h, blended);
                    }
                }
            }
        }

        // 7. Register text position for rendering
        self.text_positions.push((px, py));
        self.push_text_clip_bounds();
    }

    /// Process DrawText: simple single-face text.
    fn process_draw_text(
        &mut self,
        px: f32,
        py: f32,
        text: &str,
        face: &kasane_core::protocol::Face,
        max_width: f32,
        color_resolver: &ColorResolver,
    ) {
        if text.is_empty() {
            return;
        }

        let text_w = line_display_width_str(text) as f32 * self.metrics.cell_width;
        let actual_w = text_w.min(max_width);
        let (visual_fg, visual_bg, needs_bg) = color_resolver.resolve_face_colors(face);

        // Background — skip when not needed (parent bg shows through)
        if actual_w > 0.0 && needs_bg && !self.should_skip_default_bg(&visual_bg, color_resolver) {
            self.bg
                .push_rect(px, py, actual_w, self.metrics.cell_height, visual_bg);
        }

        // Text decorations
        if actual_w > 0.0 {
            self.emit_decorations(px, py, actual_w, face, visual_fg, color_resolver);
        }
        let buf_idx = self.alloc_text_buffer(max_width);
        self.text_positions.push((px, py));
        self.push_text_clip_bounds();
        let default_attrs = super::text_helpers::default_attrs(&self.font_family);
        let color = super::text_helpers::to_glyphon_color(visual_fg);

        let buffer = &mut self.text_buffers[buf_idx];
        buffer.set_rich_text(
            &mut self.font_system,
            [(text, default_attrs.clone().color(color))],
            &default_attrs,
            Shaping::Advanced,
            None,
        );
        buffer.shape_until_scroll(&mut self.font_system, false);
    }

    /// Push the current clip bounds for the most recently allocated text buffer.
    fn push_text_clip_bounds(&mut self) {
        let bounds = if let Some(clip) = self.current_clip() {
            (
                clip.x as i32,
                clip.y as i32,
                (clip.x + clip.w) as i32,
                (clip.y + clip.h) as i32,
            )
        } else {
            (0, 0, self.frame_screen_w as i32, self.frame_screen_h as i32)
        };
        self.text_clip_bounds.push(bounds);
    }

    /// Allocate (or reuse) a text buffer. Returns the index.
    fn alloc_text_buffer(&mut self, max_width: f32) -> usize {
        let idx = self.text_buffer_count;
        self.text_buffer_count += 1;

        let glyph_metrics = Metrics::new(self.font_size, self.line_height);
        if idx >= self.text_buffers.len() {
            let mut buffer = GlyphonBuffer::new(&mut self.font_system, glyph_metrics);
            buffer.set_hinting(&mut self.font_system, cosmic_text::Hinting::Enabled);
            buffer.set_wrap(&mut self.font_system, cosmic_text::Wrap::None);
            buffer.set_size(
                &mut self.font_system,
                Some(max_width),
                Some(self.metrics.cell_height),
            );
            self.text_buffers.push(buffer);
        } else {
            self.text_buffers[idx].set_metrics(&mut self.font_system, glyph_metrics);
            self.text_buffers[idx].set_wrap(&mut self.font_system, cosmic_text::Wrap::None);
            self.text_buffers[idx].set_size(
                &mut self.font_system,
                Some(max_width),
                Some(self.metrics.cell_height),
            );
        }
        idx
    }
}

/// Find the glyph covering the given byte offset in a shaped buffer.
/// Returns `(glyph_x, glyph_w)` if found.
fn find_glyph_at_byte_offset(buffer: &GlyphonBuffer, byte_offset: usize) -> Option<(f32, f32)> {
    for run in buffer.layout_runs() {
        for glyph in run.glyphs.iter() {
            if byte_offset >= glyph.start && byte_offset < glyph.end {
                return Some((glyph.x, glyph.w));
            }
        }
    }
    None
}
