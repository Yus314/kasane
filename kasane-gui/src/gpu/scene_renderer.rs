use crate::animation::CursorRenderState;
use glyphon::{
    Buffer as GlyphonBuffer, Cache, FontSystem, Metrics, Resolution, Shaping, SwashCache,
    TextAtlas, TextRenderer, Viewport,
};
use kasane_core::config::FontConfig;
use kasane_core::render::scene::line_display_width_str;
use kasane_core::render::{CellSize, CursorStyle, DrawCommand, PixelRect};
use wgpu::MultisampleState;
use winit::dpi::PhysicalSize;

use kasane_core::element::BorderLineStyle;

use kasane_core::protocol::Attributes;

use super::bg_pipeline::BgPipeline;
use super::border_pipeline::BorderPipeline;
use super::decoration_pipeline::{self, DecorationPipeline};
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
            font_family: font_config.family.clone(),
            font_size,
            line_height,
            clip_stack: Vec::new(),
            frame_screen_w: 0.0,
            frame_screen_h: 0.0,
            event_proxy,
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
    pub fn render_with_cursor(
        &mut self,
        gpu: &super::GpuState,
        commands: &[DrawCommand],
        color_resolver: &ColorResolver,
        cursor_style: CursorStyle,
        cursor_state: &CursorRenderState,
        cursor_color: kasane_core::protocol::Color,
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
        self.render_inner(gpu, commands, color_resolver, animated)
    }

    /// Core render implementation.
    ///
    /// Renders in layers: base layer first, then each overlay layer.  Within
    /// each layer the draw order is background → borders → text, so overlay
    /// backgrounds correctly occlude base-layer text.
    fn render_inner(
        &mut self,
        gpu: &super::GpuState,
        commands: &[DrawCommand],
        color_resolver: &ColorResolver,
        cursor: Option<(f32, f32, f32, CursorStyle, kasane_core::protocol::Color)>,
    ) -> anyhow::Result<()> {
        let _frame_span = tracing::info_span!("gpu_frame", commands = commands.len()).entered();
        let output = match gpu.surface.get_current_texture() {
            Ok(output) => output,
            Err(wgpu::SurfaceError::Outdated | wgpu::SurfaceError::Lost) => {
                gpu.surface.configure(&gpu.device, &gpu.config);
                gpu.surface.get_current_texture()?
            }
            Err(e) => return Err(e.into()),
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

        // Each layer gets its own encoder + submit so that queue.write_buffer
        // data is flushed before the next layer overwrites shared GPU buffers.
        for (layer_idx, &(range_start, range_end)) in layer_ranges.iter().enumerate() {
            // Reset per-layer pipeline instances
            self.shadow.instances.clear();
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
                self.render_cursor(cursor, color_resolver, cell_w, cell_h);
            }

            // Ensure GPU buffers are large enough for this layer
            let shadow_count = self.shadow.instances.len() / 14;
            self.shadow.ensure_buffer(gpu, shadow_count);

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

                let mut render_pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                    label: Some("scene_layer_pass"),
                    color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                        view: &view,
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
            DrawCommand::BeginOverlay => {} // handled by layer splitting
        }
    }

    /// Render the cursor into the bg pipeline.
    fn render_cursor(
        &mut self,
        cursor: Option<(f32, f32, f32, CursorStyle, kasane_core::protocol::Color)>,
        color_resolver: &ColorResolver,
        cell_w: f32,
        cell_h: f32,
    ) {
        let Some((x, y, opacity, style, cursor_color)) = cursor else {
            return;
        };
        let mut cc = color_resolver.resolve(cursor_color, true);
        cc[3] = opacity;
        match style {
            CursorStyle::Block => {
                self.bg.push_rect(x, y, cell_w, cell_h, cc);
            }
            CursorStyle::Bar => {
                self.bg.push_rect(x, y, CURSOR_BAR_WIDTH, cell_h, cc);
            }
            CursorStyle::Underline => {
                self.bg.push_rect(
                    x,
                    y + cell_h - CURSOR_UNDERLINE_HEIGHT,
                    cell_w,
                    CURSOR_UNDERLINE_HEIGHT,
                    cc,
                );
            }
            CursorStyle::Outline => {
                let t = CURSOR_OUTLINE_THICKNESS;
                self.bg.push_rect(x, y, cell_w, t, cc);
                self.bg.push_rect(x, y + cell_h - t, cell_w, t, cc);
                self.bg.push_rect(x, y, t, cell_h, cc);
                self.bg.push_rect(x + cell_w - t, y, t, cell_h, cc);
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
        if attrs.contains(Attributes::STRIKETHROUGH) {
            // Strikethrough at approximately the x-height center
            let y = py + baseline * 0.55;
            self.decoration
                .push(x, y, w, thickness, fg, decoration_pipeline::DECO_SOLID);
        }
    }

    /// Process DrawAtoms: build atom-level spans for ligature support.
    ///
    /// Adjacent atoms with the same foreground color are merged into a single
    /// shaping span so that ligatures (e.g. `->`, `!=`, `=>`) can form across
    /// atom boundaries.
    fn process_draw_atoms(
        &mut self,
        px: f32,
        py: f32,
        atoms: &[kasane_core::render::ResolvedAtom],
        max_width: f32,
        color_resolver: &ColorResolver,
    ) {
        let cell_w = self.metrics.cell_width;
        let cell_h = self.metrics.cell_height;

        let mut x = px;
        self.row_text.clear();
        self.span_ranges.clear();

        for atom in atoms {
            let atom_display_w = line_display_width_str(&atom.contents) as f32 * cell_w;
            let remaining = max_width - (x - px);
            if remaining <= 0.0 {
                break;
            }

            let actual_w = atom_display_w.min(remaining);
            let (visual_fg, visual_bg, needs_bg) = color_resolver.resolve_face_colors(&atom.face);

            // Background rectangle — skip when not needed so the parent
            // element's background (e.g. an elevated Container) shows through.
            if actual_w > 0.0 && needs_bg {
                self.bg.push_rect(x, py, actual_w, cell_h, visual_bg);
            }

            // Text decorations (underline, strikethrough, etc.)
            if actual_w > 0.0 {
                self.emit_decorations(x, py, actual_w, &atom.face, visual_fg, color_resolver);
            }

            // Text span — merge with previous span if same fg color
            let fg = visual_fg;
            if let Some(last) = self.span_ranges.last_mut() {
                if last.2 == fg {
                    // Extend previous span
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

            x += atom_display_w;
        }

        if self.row_text.is_empty() {
            return;
        }

        // Insert Word Joiners to prevent cosmic-text from splitting operator
        // sequences (like `->`, `!=`) into separate shaping runs.
        super::text_helpers::insert_word_joiners(&mut self.row_text, &mut self.span_ranges);

        let buf_idx = self.alloc_text_buffer(max_width);
        self.text_positions.push((px, py));
        self.push_text_clip_bounds();
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
        if actual_w > 0.0 && needs_bg {
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
            buffer.set_size(
                &mut self.font_system,
                Some(max_width),
                Some(self.metrics.cell_height),
            );
            self.text_buffers.push(buffer);
        } else {
            self.text_buffers[idx].set_metrics(&mut self.font_system, glyph_metrics);
            self.text_buffers[idx].set_size(
                &mut self.font_system,
                Some(max_width),
                Some(self.metrics.cell_height),
            );
        }
        idx
    }
}
