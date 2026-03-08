use std::hash::{Hash, Hasher};

use glyphon::{
    Attrs, Buffer as GlyphonBuffer, Cache, Color as GlyphonColor, FontSystem, Metrics, Resolution,
    Shaping, SwashCache, TextArea, TextAtlas, TextBounds, TextRenderer, Viewport,
};
use kasane_core::config::FontConfig;
use kasane_core::render::{CellGrid, CursorStyle};
use wgpu::MultisampleState;
use winit::dpi::PhysicalSize;

use super::CellMetrics;
use crate::colors::ColorResolver;

/// Initial capacity for bg instance buffer (enough for 256x64 grid + cursor)
const INITIAL_BG_CAPACITY: usize = 256 * 64 + 8;

/// Width of the cursor bar (CursorStyle::Bar) in pixels.
const CURSOR_BAR_WIDTH: f32 = 2.0;
/// Height of the cursor underline (CursorStyle::Underline) in pixels.
const CURSOR_UNDERLINE_HEIGHT: f32 = 2.0;
/// Thickness of the cursor outline (CursorStyle::Outline) border in pixels.
const CURSOR_OUTLINE_THICKNESS: f32 = 1.0;

/// Background quad rendering pipeline — owns the GPU pipeline, uniform buffer,
/// bind group, instance buffer, and CPU-side scratch vector.
struct BgPipeline {
    pipeline: wgpu::RenderPipeline,
    uniform_buffer: wgpu::Buffer,
    uniform_bind_group: wgpu::BindGroup,
    instance_buffer: wgpu::Buffer,
    instance_capacity: usize,
    instances: Vec<f32>,
}

impl BgPipeline {
    /// Create the background pipeline, uniform buffer, bind group, and instance buffer.
    fn new(gpu: &super::GpuState, surface_format: wgpu::TextureFormat) -> Self {
        let bg_shader = gpu
            .device
            .create_shader_module(wgpu::ShaderModuleDescriptor {
                label: Some("bg_shader"),
                source: wgpu::ShaderSource::Wgsl(include_str!("bg.wgsl").into()),
            });

        let uniform_buffer = gpu.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("bg_uniforms"),
            size: 8, // vec2<f32> screen_size
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        let bg_bind_group_layout =
            gpu.device
                .create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                    label: Some("bg_bind_group_layout"),
                    entries: &[wgpu::BindGroupLayoutEntry {
                        binding: 0,
                        visibility: wgpu::ShaderStages::VERTEX,
                        ty: wgpu::BindingType::Buffer {
                            ty: wgpu::BufferBindingType::Uniform,
                            has_dynamic_offset: false,
                            min_binding_size: None,
                        },
                        count: None,
                    }],
                });

        let uniform_bind_group = gpu.device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("bg_bind_group"),
            layout: &bg_bind_group_layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: uniform_buffer.as_entire_binding(),
            }],
        });

        let bg_pipeline_layout =
            gpu.device
                .create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                    label: Some("bg_pipeline_layout"),
                    bind_group_layouts: &[&bg_bind_group_layout],
                    immediate_size: 0,
                });

        let pipeline = gpu
            .device
            .create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                label: Some("bg_pipeline"),
                layout: Some(&bg_pipeline_layout),
                vertex: wgpu::VertexState {
                    module: &bg_shader,
                    entry_point: Some("vs_main"),
                    buffers: &[wgpu::VertexBufferLayout {
                        array_stride: 32, // 4 floats rect + 4 floats color = 32 bytes
                        step_mode: wgpu::VertexStepMode::Instance,
                        attributes: &[
                            // rect: vec4<f32> (x, y, w, h)
                            wgpu::VertexAttribute {
                                offset: 0,
                                shader_location: 0,
                                format: wgpu::VertexFormat::Float32x4,
                            },
                            // color: vec4<f32>
                            wgpu::VertexAttribute {
                                offset: 16,
                                shader_location: 1,
                                format: wgpu::VertexFormat::Float32x4,
                            },
                        ],
                    }],
                    compilation_options: Default::default(),
                },
                fragment: Some(wgpu::FragmentState {
                    module: &bg_shader,
                    entry_point: Some("fs_main"),
                    targets: &[Some(wgpu::ColorTargetState {
                        format: surface_format,
                        blend: Some(wgpu::BlendState::ALPHA_BLENDING),
                        write_mask: wgpu::ColorWrites::ALL,
                    })],
                    compilation_options: Default::default(),
                }),
                primitive: wgpu::PrimitiveState {
                    topology: wgpu::PrimitiveTopology::TriangleStrip,
                    strip_index_format: None,
                    ..Default::default()
                },
                depth_stencil: None,
                multisample: MultisampleState::default(),
                multiview_mask: None,
                cache: None,
            });

        let instance_buffer = gpu.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("bg_instances"),
            size: (INITIAL_BG_CAPACITY * 32) as u64, // 32 bytes per instance
            usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        BgPipeline {
            pipeline,
            uniform_buffer,
            uniform_bind_group,
            instance_buffer,
            instance_capacity: INITIAL_BG_CAPACITY,
            instances: Vec::with_capacity(INITIAL_BG_CAPACITY * 8),
        }
    }

    /// Append a background rectangle instance (8 floats: x, y, w, h, r, g, b, a).
    fn push_rect(&mut self, x: f32, y: f32, w: f32, h: f32, color: [f32; 4]) {
        self.instances
            .extend_from_slice(&[x, y, w, h, color[0], color[1], color[2], color[3]]);
    }

    /// Ensure the persistent instance buffer is large enough.
    fn ensure_buffer(&mut self, gpu: &super::GpuState, needed: usize) {
        if needed <= self.instance_capacity {
            return;
        }
        // Grow by 2x or to needed, whichever is larger
        let new_cap = (self.instance_capacity * 2).max(needed);
        self.instance_buffer = gpu.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("bg_instances"),
            size: (new_cap * 32) as u64,
            usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        self.instance_capacity = new_cap;
    }
}

/// Renders a CellGrid onto a GPU surface using glyphon for text and a custom
/// pipeline for background rectangles.
pub struct CellRenderer {
    font_system: FontSystem,
    swash_cache: SwashCache,
    viewport: Viewport,
    text_atlas: TextAtlas,
    text_renderer: TextRenderer,
    metrics: CellMetrics,
    font_size: f32,
    line_height: f32,

    // Background quad pipeline
    bg: BgPipeline,

    // Reusable CPU-side scratch buffers
    row_text: String,
    span_ranges: Vec<(usize, usize, [f32; 4])>,

    // Cached glyphon text buffers (one per row, persistent across frames)
    text_buffers: Vec<GlyphonBuffer>,

    // Row-level dirty tracking: cached hash of each row's content+colors
    row_hashes: Vec<u64>,

    // Font family name from config (owned so we can lend &str to glyphon)
    font_family: String,
}

impl CellRenderer {
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

        // Background quad pipeline
        let bg = BgPipeline::new(gpu, surface_format);

        // Pre-create text buffers for each row
        let rows = metrics.rows as usize;
        let screen_w = window_size.width.max(1) as f32;
        let text_buffers = Self::create_text_buffers(
            &mut font_system,
            rows,
            font_size,
            line_height,
            screen_w,
            metrics.cell_height,
        );

        CellRenderer {
            font_system,
            swash_cache,
            viewport,
            text_atlas,
            text_renderer,
            font_size,
            line_height,
            bg,
            row_text: String::with_capacity(512),
            span_ranges: Vec::with_capacity(256),
            text_buffers,
            row_hashes: vec![0; rows],
            metrics,
            font_family: font_config.family.clone(),
        }
    }

    pub fn metrics(&self) -> &CellMetrics {
        &self.metrics
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

        // Rebuild text buffers for new row count
        let rows = self.metrics.rows as usize;
        let screen_w = window_size.width.max(1) as f32;
        self.text_buffers = Self::create_text_buffers(
            &mut self.font_system,
            rows,
            self.font_size,
            self.line_height,
            screen_w,
            self.metrics.cell_height,
        );
        self.row_hashes = vec![0; rows];
    }

    fn create_text_buffers(
        font_system: &mut FontSystem,
        rows: usize,
        font_size: f32,
        line_height: f32,
        screen_w: f32,
        cell_height: f32,
    ) -> Vec<GlyphonBuffer> {
        let glyph_metrics = Metrics::new(font_size, line_height);
        let mut buffers = Vec::with_capacity(rows);
        for _ in 0..rows {
            let mut buffer = GlyphonBuffer::new(font_system, glyph_metrics);
            buffer.set_size(font_system, Some(screen_w), Some(cell_height));
            buffers.push(buffer);
        }
        buffers
    }

    /// Render one frame: backgrounds, text, and cursor.
    pub fn render(
        &mut self,
        gpu: &super::GpuState,
        grid: &CellGrid,
        color_resolver: &ColorResolver,
        cursor: Option<(u16, u16, CursorStyle)>,
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

        // Update screen size uniform
        gpu.queue.write_buffer(
            &self.bg.uniform_buffer,
            0,
            bytemuck::cast_slice(&[screen_w, screen_h]),
        );

        // --- Build background instance data (reuse Vec) ---
        self.bg.instances.clear();
        let cell_w = self.metrics.cell_width;
        let cell_h = self.metrics.cell_height;

        for row in 0..grid.height {
            let y = row as f32 * cell_h;
            for col in 0..grid.width {
                let cell = grid.get(col, row).unwrap();
                let bg = color_resolver.resolve(cell.face.bg, false);
                let x = col as f32 * cell_w;
                self.bg.push_rect(x, y, cell_w, cell_h, bg);
            }
        }

        // Add cursor overlay
        if let Some((cx, cy, style)) = cursor {
            let x = cx as f32 * cell_w;
            let y = cy as f32 * cell_h;
            let cc = color_resolver.resolve(kasane_core::protocol::Color::Default, true);
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
                    self.bg.push_rect(x, y, cell_w, t, cc); // Top
                    self.bg.push_rect(x, y + cell_h - t, cell_w, t, cc); // Bottom
                    self.bg.push_rect(x, y, t, cell_h, cc); // Left
                    self.bg.push_rect(x + cell_w - t, y, t, cell_h, cc); // Right
                }
            }
        }

        let instance_count = self.bg.instances.len() / 8;

        // Upload bg instances to persistent GPU buffer
        self.bg.ensure_buffer(gpu, instance_count);
        gpu.queue.write_buffer(
            &self.bg.instance_buffer,
            0,
            bytemuck::cast_slice(&self.bg.instances),
        );

        // --- Update text buffers (reuse cached buffers, Basic shaping) ---
        let rows = grid.height as usize;
        // Ensure we have the right number of buffers
        let glyph_metrics = Metrics::new(self.font_size, self.line_height);
        while self.text_buffers.len() < rows {
            let mut buffer = GlyphonBuffer::new(&mut self.font_system, glyph_metrics);
            buffer.set_size(
                &mut self.font_system,
                Some(screen_w),
                Some(self.metrics.cell_height),
            );
            self.text_buffers.push(buffer);
        }
        self.text_buffers.truncate(rows);

        let default_attrs = Attrs::new().family(super::to_family(&self.font_family));

        // Ensure row_hashes is the right size
        self.row_hashes.resize(rows, 0);

        for row in 0..rows {
            // Compute a hash of this row's content + fg colors to detect changes
            let mut hasher = std::collections::hash_map::DefaultHasher::new();
            for col in 0..grid.width {
                let cell = grid.get(col, row as u16).unwrap();
                cell.grapheme.hash(&mut hasher);
                // Hash fg color discriminant and values
                std::mem::discriminant(&cell.face.fg).hash(&mut hasher);
                let fg_bits = color_resolver.resolve(cell.face.fg, true);
                fg_bits[0].to_bits().hash(&mut hasher);
                fg_bits[1].to_bits().hash(&mut hasher);
                fg_bits[2].to_bits().hash(&mut hasher);
            }
            let row_hash = hasher.finish();

            // Skip reshaping if this row hasn't changed
            if self.row_hashes[row] == row_hash {
                continue;
            }
            self.row_hashes[row] = row_hash;

            // Reuse scratch buffers
            self.row_text.clear();
            self.span_ranges.clear();

            for col in 0..grid.width {
                let cell = grid.get(col, row as u16).unwrap();
                if cell.width == 0 {
                    continue;
                }
                let start = self.row_text.len();
                let grapheme = if cell.grapheme.is_empty() {
                    " "
                } else {
                    &cell.grapheme
                };
                self.row_text.push_str(grapheme);
                let fg = color_resolver.resolve(cell.face.fg, true);
                self.span_ranges.push((start, self.row_text.len(), fg));
            }

            // Build rich text spans from scratch buffers
            let rich_text_iter = self.span_ranges.iter().map(|(start, end, fg)| {
                let text = &self.row_text[*start..*end];
                let color = GlyphonColor::rgba(
                    (fg[0] * 255.0) as u8,
                    (fg[1] * 255.0) as u8,
                    (fg[2] * 255.0) as u8,
                    255,
                );
                (text, default_attrs.clone().color(color))
            });

            let buffer = &mut self.text_buffers[row];
            buffer.set_rich_text(
                &mut self.font_system,
                rich_text_iter,
                &default_attrs,
                Shaping::Basic,
                None,
            );
            buffer.shape_until_scroll(&mut self.font_system, false);
        }

        // Prepare text areas
        let text_areas: Vec<TextArea> = self
            .text_buffers
            .iter()
            .enumerate()
            .map(|(row, buffer)| TextArea {
                buffer,
                left: 0.0,
                top: row as f32 * cell_h,
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

        // Update viewport
        self.viewport.update(
            &gpu.queue,
            Resolution {
                width: gpu.config.width,
                height: gpu.config.height,
            },
        );

        // Prepare glyphon
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

        // Render
        let mut encoder = gpu
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("frame_encoder"),
            });

        {
            let default_bg = color_resolver.default_bg();
            let mut render_pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("main_pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &view,
                    depth_slice: None,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color {
                            r: default_bg[0] as f64,
                            g: default_bg[1] as f64,
                            b: default_bg[2] as f64,
                            a: 1.0,
                        }),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
                multiview_mask: None,
            });

            // Pass 1: background quads
            render_pass.set_pipeline(&self.bg.pipeline);
            render_pass.set_bind_group(0, &self.bg.uniform_bind_group, &[]);
            render_pass.set_vertex_buffer(0, self.bg.instance_buffer.slice(..));
            render_pass.draw(0..4, 0..instance_count as u32);

            // Pass 2: text
            self.text_renderer
                .render(&self.text_atlas, &self.viewport, &mut render_pass)
                .map_err(|e| anyhow::anyhow!("glyphon render failed: {e}"))?;
        }

        gpu.queue.submit(std::iter::once(encoder.finish()));
        output.present();

        self.text_atlas.trim();

        Ok(())
    }
}
