use crate::animation::CursorRenderState;
use glyphon::{
    Attrs, Buffer as GlyphonBuffer, Cache, Color as GlyphonColor, FontSystem, Metrics, Resolution,
    Shaping, SwashCache, TextArea, TextAtlas, TextBounds, TextRenderer, Viewport,
    cosmic_text::{FeatureTag, FontFeatures},
};
use kasane_core::config::FontConfig;
use kasane_core::render::scene::line_display_width_str;
use kasane_core::render::{CellSize, CursorStyle, DrawCommand};
use wgpu::MultisampleState;
use winit::dpi::PhysicalSize;

use kasane_core::element::BorderLineStyle;

use super::bg_pipeline::BgPipeline;
use super::border_pipeline::BorderPipeline;
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
    metrics: CellMetrics,

    // Reusable text buffers (growable pool)
    text_buffers: Vec<GlyphonBuffer>,
    /// Position (left, top) for each text buffer allocated this frame.
    text_positions: Vec<(f32, f32)>,
    text_buffer_count: usize,

    // Scratch buffers
    row_text: String,
    span_ranges: Vec<(usize, usize, [f32; 4])>,

    // Font config
    font_family: String,
    font_size: f32,
    line_height: f32,
}

impl SceneRenderer {
    pub fn new(
        gpu: &super::GpuState,
        font_config: &FontConfig,
        scale_factor: f64,
        window_size: PhysicalSize<u32>,
    ) -> Self {
        let mut font_system = FontSystem::new();
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

        SceneRenderer {
            font_system,
            swash_cache,
            viewport,
            text_atlas,
            text_renderer,
            shadow,
            bg,
            border,
            metrics,
            text_buffers: Vec::with_capacity(128),
            text_positions: Vec::with_capacity(128),
            text_buffer_count: 0,
            row_text: String::with_capacity(512),
            span_ranges: Vec::with_capacity(256),
            font_family: font_config.family.clone(),
            font_size,
            line_height,
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
            (cx as f32 * cell_w, cy as f32 * cell_h, 1.0f32, style)
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
        cursor: Option<(f32, f32, f32, CursorStyle)>,
    ) -> anyhow::Result<()> {
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

        // Update screen size uniforms
        let screen_size_data = [screen_w, screen_h];
        gpu.queue.write_buffer(
            self.shadow.uniform_buffer(),
            0,
            bytemuck::cast_slice(&screen_size_data),
        );
        gpu.queue.write_buffer(
            self.bg.uniform_buffer(),
            0,
            bytemuck::cast_slice(&screen_size_data),
        );
        gpu.queue.write_buffer(
            self.border.uniform_buffer(),
            0,
            bytemuck::cast_slice(&screen_size_data),
        );

        // Reset per-frame state
        self.text_buffer_count = 0;
        self.text_positions.clear();

        // Split commands into layers at BeginOverlay boundaries.
        // layer_ranges[i] = (start, end) index into `commands`.
        let mut layer_ranges: Vec<(usize, usize)> = Vec::new();
        let mut layer_start = 0;
        for (i, cmd) in commands.iter().enumerate() {
            if matches!(cmd, DrawCommand::BeginOverlay) {
                layer_ranges.push((layer_start, i));
                layer_start = i + 1; // skip the marker itself
            }
        }
        layer_ranges.push((layer_start, commands.len()));

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
            let layer_text_start = self.text_buffer_count;

            // Process this layer's DrawCommands
            for cmd in &commands[range_start..range_end] {
                match cmd {
                    DrawCommand::FillRect {
                        rect,
                        face,
                        elevated,
                    } => {
                        let mut bg = color_resolver.resolve(face.bg, false);
                        if *elevated {
                            // Lighten popup background significantly.
                            // In the dark sRGB range, small additive steps are
                            // imperceptible.  Use a large lift (+0.25 ≈ +64 in
                            // 0-255) to make floating panels clearly distinct.
                            bg[0] = (bg[0] + 0.25).min(1.0);
                            bg[1] = (bg[1] + 0.25).min(1.0);
                            bg[2] = (bg[2] + 0.25).min(1.0);
                            tracing::debug!(
                                "elevated FillRect: bg=[{:.3},{:.3},{:.3}] rect=({:.0},{:.0},{:.0},{:.0})",
                                bg[0],
                                bg[1],
                                bg[2],
                                rect.x,
                                rect.y,
                                rect.w,
                                rect.h,
                            );
                        }
                        self.bg.push_rect(rect.x, rect.y, rect.w, rect.h, bg);
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
                        self.process_draw_text(
                            pos.x,
                            pos.y,
                            text,
                            face,
                            *max_width,
                            color_resolver,
                        );
                    }
                    DrawCommand::DrawPaddingRow {
                        pos,
                        width: _,
                        ch,
                        face,
                    } => {
                        let fg = color_resolver.resolve(face.fg, true);
                        let buf_idx = self.alloc_text_buffer(screen_w);
                        self.text_positions.push((pos.x, pos.y));
                        let attrs = default_attrs(&self.font_family);
                        let color = to_glyphon_color(fg);
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
                        let border_color = color_resolver.resolve(face.fg, true);
                        let (corner_radius, border_width) =
                            border_style_params(line_style.clone(), cell_h);
                        let fill = match fill_face {
                            Some(ff) => color_resolver.resolve(ff.bg, false),
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
                        // Compute the title width in pixels
                        let title_w: f32 = title
                            .iter()
                            .map(|a| line_display_width_str(&a.contents) as f32 * cell_w)
                            .sum();
                        // Horizontal padding around the title text so it doesn't
                        // touch the border line.
                        let pad_x = cell_w * 0.5;

                        // Center the title on the top edge of the border rect,
                        // vertically aligned so text sits on the border line.
                        let title_x = rect.x + (rect.w - title_w) / 2.0;
                        let title_y = rect.y - cell_h * 0.35;

                        // Match the container's background color.
                        // Elevated containers (shadow=true) add +0.25.
                        let mut title_bg = color_resolver.resolve(border_face.bg, false);
                        if *elevated {
                            title_bg[0] = (title_bg[0] + 0.25).min(1.0);
                            title_bg[1] = (title_bg[1] + 0.25).min(1.0);
                            title_bg[2] = (title_bg[2] + 0.25).min(1.0);
                        }

                        // Push bg into the border pipeline so it renders AFTER the
                        // border line, covering the line segment behind the title.
                        // The bg rect is wider than the text by pad_x on each side.
                        self.border.push_rounded_rect(
                            title_x - pad_x,
                            title_y,
                            title_w + pad_x * 2.0,
                            cell_h,
                            0.0,
                            0.0, // no corner radius, no border
                            title_bg,
                            [0.0, 0.0, 0.0, 0.0],
                        );

                        // Draw the title text
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
                    DrawCommand::PushClip(_) | DrawCommand::PopClip => {
                        // Will be implemented with scissor rects
                    }
                    DrawCommand::BeginOverlay => {} // handled by layer splitting
                }
            }

            // Cursor belongs to the base layer (layer 0)
            if layer_idx == 0
                && let Some((x, y, opacity, style)) = cursor
            {
                let mut cc = color_resolver.resolve(kasane_core::protocol::Color::Default, true);
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

            // Ensure GPU buffers are large enough for this layer
            let shadow_count = self.shadow.instances.len() / 14;
            self.shadow.ensure_buffer(gpu, shadow_count);

            let bg_count = self.bg.instances.len() / 8;
            self.bg.ensure_buffer(gpu, bg_count);

            let border_count = self.border.instances.len() / 14;
            self.border.ensure_buffer(gpu, border_count);

            // Build TextAreas for this layer's text buffers only
            let layer_text_end = self.text_buffer_count;
            let text_areas: Vec<TextArea> = self.text_positions[layer_text_start..layer_text_end]
                .iter()
                .zip(self.text_buffers[layer_text_start..layer_text_end].iter())
                .map(|(&(left, top), buffer)| TextArea {
                    buffer,
                    left,
                    top,
                    scale: 1.0,
                    bounds: TextBounds {
                        left: 0,
                        top: 0,
                        right: screen_w as i32,
                        bottom: screen_h as i32,
                    },
                    default_color: GlyphonColor::rgb(255, 255, 255),
                    custom_glyphs: &[],
                })
                .collect();

            // Prepare this layer's text
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
            }

            gpu.queue.submit(std::iter::once(encoder.finish()));
        }

        output.present();

        self.text_atlas.trim();

        Ok(())
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

            // Background rectangle — skip for Color::Default so the parent
            // element's background (e.g. an elevated Container) shows through.
            let actual_w = atom_display_w.min(remaining);
            if actual_w > 0.0 && atom.face.bg != kasane_core::protocol::Color::Default {
                let bg = color_resolver.resolve(atom.face.bg, false);
                self.bg.push_rect(x, py, actual_w, cell_h, bg);
            }

            // Text span — merge with previous span if same fg color
            let fg = color_resolver.resolve(atom.face.fg, true);
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
        insert_word_joiners(&mut self.row_text, &mut self.span_ranges);

        let buf_idx = self.alloc_text_buffer(max_width);
        self.text_positions.push((px, py));
        let default_attrs = default_attrs(&self.font_family);

        let rich_text_iter = self.span_ranges.iter().map(|(start, end, fg)| {
            let text = &self.row_text[*start..*end];
            let color = to_glyphon_color(*fg);
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

        // Background — skip for Color::Default (parent bg shows through)
        let text_w = line_display_width_str(text) as f32 * self.metrics.cell_width;
        let actual_w = text_w.min(max_width);
        if actual_w > 0.0 && face.bg != kasane_core::protocol::Color::Default {
            let bg = color_resolver.resolve(face.bg, false);
            self.bg
                .push_rect(px, py, actual_w, self.metrics.cell_height, bg);
        }

        let fg = color_resolver.resolve(face.fg, true);
        let buf_idx = self.alloc_text_buffer(max_width);
        self.text_positions.push((px, py));
        let default_attrs = default_attrs(&self.font_family);
        let color = to_glyphon_color(fg);

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

/// Map BorderLineStyle to (corner_radius, border_width).
fn border_style_params(style: BorderLineStyle, cell_height: f32) -> (f32, f32) {
    // Scale border width with cell size so it looks proportional on any DPI.
    let base = (cell_height * 0.08).max(1.5);
    match style {
        BorderLineStyle::Single => (0.0, base),
        BorderLineStyle::Rounded => (cell_height * 0.3, base),
        BorderLineStyle::Double => (0.0, base),
        BorderLineStyle::Heavy => (0.0, base * 2.0),
        BorderLineStyle::Ascii => (0.0, base),
        BorderLineStyle::Custom(_) => (0.0, base),
    }
}

/// Build default `Attrs` with font family and discretionary ligatures enabled.
fn default_attrs(font_family: &str) -> Attrs<'_> {
    let mut features = FontFeatures::new();
    features.enable(FeatureTag::DISCRETIONARY_LIGATURES);
    Attrs::new()
        .family(super::to_family(font_family))
        .font_features(features)
}

/// Insert Word Joiners (U+2060) after characters whose Unicode line-break
/// class allows a break, preventing `cosmic-text` from splitting operator
/// sequences (e.g. `->`, `!=`, `|>`, `/*`) into separate shaping words.
///
/// Without this, `unicode_linebreak` treats `-` (HY), `!` (EX), `/` (SY),
/// `|` (BA), and `+` (PR) as break opportunities, which splits them from
/// the following character and prevents ligature formation in harfrust.
fn insert_word_joiners(text: &mut String, spans: &mut [(usize, usize, [f32; 4])]) {
    const WJ: &str = "\u{2060}";
    const WJ_LEN: usize = 3; // UTF-8 byte length of U+2060

    let bytes = text.as_bytes();
    let mut insert_positions = Vec::new();

    for i in 0..bytes.len().saturating_sub(1) {
        if matches!(bytes[i], b'-' | b'!' | b'/' | b'|' | b'+') {
            // Only insert WJ if the next byte is a printable ASCII non-space
            // character (potential ligature partner).
            let next = bytes[i + 1];
            if next > b' ' && next < 0x7F {
                insert_positions.push(i + 1);
            }
        }
    }

    if insert_positions.is_empty() {
        return;
    }

    // Build new string with WJs inserted at each position.
    let mut new_text = String::with_capacity(text.len() + insert_positions.len() * WJ_LEN);
    let mut last = 0;
    for &pos in &insert_positions {
        new_text.push_str(&text[last..pos]);
        new_text.push_str(WJ);
        last = pos;
    }
    new_text.push_str(&text[last..]);

    // Adjust span byte ranges: each WJ inserted at position p shifts all
    // offsets after p by WJ_LEN bytes.
    for span in spans.iter_mut() {
        let start_shift = insert_positions.partition_point(|&p| p <= span.0) * WJ_LEN;
        let end_shift = insert_positions.partition_point(|&p| p <= span.1) * WJ_LEN;
        span.0 += start_shift;
        span.1 += end_shift;
    }

    *text = new_text;
}

fn to_glyphon_color(c: [f32; 4]) -> GlyphonColor {
    GlyphonColor::rgba(
        (c[0] * 255.0) as u8,
        (c[1] * 255.0) as u8,
        (c[2] * 255.0) as u8,
        255,
    )
}
