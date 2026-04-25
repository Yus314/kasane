use wgpu::MultisampleState;

use super::pipeline_common::{InstanceBuffer, ScreenUniforms};
use super::retained_scene::RetainedScene;

/// Initial capacity for the unified quad instance buffer.
const INITIAL_QUAD_CAPACITY: usize = 256 * 64 + 128;

/// Per-instance data: 20 floats, 80 bytes.
/// [0..4]   rect: x, y, w, h (pixels)
/// [4..8]   fill_color: r, g, b, a (linear)
/// [8..12]  border_color: r, g, b, a (linear) — or gradient end_color
/// [12..16] params: corner_radius, border_width, quad_type, deco_type
/// [16..20] extra: end_color for gradient (or reserved)
const FLOATS_PER_INSTANCE: usize = 20;
const BYTES_PER_INSTANCE: u64 = (FLOATS_PER_INSTANCE * 4) as u64;

/// Quad type constants passed via params.z.
const QUAD_TYPE_SOLID: f32 = 0.0;
const QUAD_TYPE_ROUNDED_RECT: f32 = 1.0;
const QUAD_TYPE_DECORATION: f32 = 2.0;
const QUAD_TYPE_GRADIENT: f32 = 3.0;

/// Decoration type constants passed via params.w.
pub const DECO_SOLID: f32 = 0.0;
pub const DECO_CURLY: f32 = 1.0;
pub const DECO_DOUBLE: f32 = 2.0;
pub const DECO_DOTTED: f32 = 3.0;
pub const DECO_DASHED: f32 = 4.0;

/// Unified quad rendering pipeline — handles solid backgrounds, rounded rect
/// borders/shadows, decorations, and gradients in a single draw call.
pub struct QuadPipeline {
    pipeline: wgpu::RenderPipeline,
    uniforms: ScreenUniforms,
    instance_buf: InstanceBuffer,
    pub instances: Vec<f32>,
    retained: RetainedScene,
}

impl QuadPipeline {
    pub fn new(gpu: &super::GpuState, surface_format: wgpu::TextureFormat) -> Self {
        let shader = gpu
            .device
            .create_shader_module(wgpu::ShaderModuleDescriptor {
                label: Some("quad_shader"),
                source: wgpu::ShaderSource::Wgsl(include_str!("quad.wgsl").into()),
            });

        let uniforms = ScreenUniforms::new(&gpu.device, "quad_uniforms");

        let pipeline_layout = gpu
            .device
            .create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                label: Some("quad_pipeline_layout"),
                bind_group_layouts: &[Some(&uniforms.bind_group_layout)],
                immediate_size: 0,
            });

        let pipeline = gpu
            .device
            .create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                label: Some("quad_pipeline"),
                layout: Some(&pipeline_layout),
                vertex: wgpu::VertexState {
                    module: &shader,
                    entry_point: Some("vs_main"),
                    buffers: &[wgpu::VertexBufferLayout {
                        array_stride: BYTES_PER_INSTANCE,
                        step_mode: wgpu::VertexStepMode::Instance,
                        attributes: &[
                            // rect: vec4<f32> (x, y, w, h)
                            wgpu::VertexAttribute {
                                offset: 0,
                                shader_location: 0,
                                format: wgpu::VertexFormat::Float32x4,
                            },
                            // fill_color: vec4<f32>
                            wgpu::VertexAttribute {
                                offset: 16,
                                shader_location: 1,
                                format: wgpu::VertexFormat::Float32x4,
                            },
                            // border_color: vec4<f32>
                            wgpu::VertexAttribute {
                                offset: 32,
                                shader_location: 2,
                                format: wgpu::VertexFormat::Float32x4,
                            },
                            // params: vec4<f32> (corner_radius, border_width, quad_type, deco_type)
                            wgpu::VertexAttribute {
                                offset: 48,
                                shader_location: 3,
                                format: wgpu::VertexFormat::Float32x4,
                            },
                            // extra: vec4<f32> (gradient end_color or reserved)
                            wgpu::VertexAttribute {
                                offset: 64,
                                shader_location: 4,
                                format: wgpu::VertexFormat::Float32x4,
                            },
                        ],
                    }],
                    compilation_options: Default::default(),
                },
                fragment: Some(wgpu::FragmentState {
                    module: &shader,
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
                depth_stencil: Some(super::depth_stencil::pipeline_depth_stencil()),
                multisample: MultisampleState::default(),
                multiview_mask: None,
                cache: gpu.pipeline_cache.as_ref(),
            });

        let instance_buf = InstanceBuffer::new(
            &gpu.device,
            INITIAL_QUAD_CAPACITY,
            BYTES_PER_INSTANCE,
            "quad_instances",
        );

        QuadPipeline {
            pipeline,
            uniforms,
            instance_buf,
            instances: Vec::with_capacity(INITIAL_QUAD_CAPACITY * FLOATS_PER_INSTANCE),
            retained: RetainedScene::new(FLOATS_PER_INSTANCE),
        }
    }

    /// Access the uniform buffer (for writing screen_size data).
    pub fn uniform_buffer(&self) -> &wgpu::Buffer {
        &self.uniforms.buffer
    }

    /// Ensure the persistent instance buffer is large enough.
    pub fn ensure_buffer(&mut self, gpu: &super::GpuState, needed: usize) {
        self.instance_buf.ensure_capacity(&gpu.device, needed);
    }

    /// Number of instances currently queued.
    pub fn instance_count(&self) -> usize {
        self.instances.len() / FLOATS_PER_INSTANCE
    }

    /// Push a solid background rectangle (type 0).
    pub fn push_solid(&mut self, x: f32, y: f32, w: f32, h: f32, color: [f32; 4]) {
        self.instances.extend_from_slice(&[
            x,
            y,
            w,
            h,
            color[0],
            color[1],
            color[2],
            color[3],
            0.0,
            0.0,
            0.0,
            0.0, // border_color (unused)
            0.0,
            0.0,
            QUAD_TYPE_SOLID,
            0.0, // params
            0.0,
            0.0,
            0.0,
            0.0, // extra (unused)
        ]);
    }

    /// Push a rounded rect instance (type 1) for borders and shadows.
    #[allow(clippy::too_many_arguments)]
    pub fn push_rounded_rect(
        &mut self,
        x: f32,
        y: f32,
        w: f32,
        h: f32,
        corner_radius: f32,
        border_width: f32,
        fill_color: [f32; 4],
        border_color: [f32; 4],
    ) {
        self.instances.extend_from_slice(&[
            x,
            y,
            w,
            h,
            fill_color[0],
            fill_color[1],
            fill_color[2],
            fill_color[3],
            border_color[0],
            border_color[1],
            border_color[2],
            border_color[3],
            corner_radius,
            border_width,
            QUAD_TYPE_ROUNDED_RECT,
            0.0, // params
            0.0,
            0.0,
            0.0,
            0.0, // extra (unused)
        ]);
    }

    /// Push a decoration instance (type 2).
    pub fn push_decoration(
        &mut self,
        x: f32,
        y: f32,
        w: f32,
        h: f32,
        color: [f32; 4],
        deco_type: f32,
    ) {
        self.instances.extend_from_slice(&[
            x,
            y,
            w,
            h,
            color[0],
            color[1],
            color[2],
            color[3],
            0.0,
            0.0,
            0.0,
            0.0, // border_color (unused)
            0.0,
            0.0,
            QUAD_TYPE_DECORATION,
            deco_type, // params
            0.0,
            0.0,
            0.0,
            0.0, // extra (unused)
        ]);
    }

    /// Push a gradient instance (type 3).
    pub fn push_gradient(
        &mut self,
        x: f32,
        y: f32,
        w: f32,
        h: f32,
        start_color: [f32; 4],
        end_color: [f32; 4],
    ) {
        self.instances.extend_from_slice(&[
            x,
            y,
            w,
            h,
            start_color[0],
            start_color[1],
            start_color[2],
            start_color[3],
            0.0,
            0.0,
            0.0,
            0.0, // border_color (unused for gradient)
            0.0,
            0.0,
            QUAD_TYPE_GRADIENT,
            0.0, // params
            end_color[0],
            end_color[1],
            end_color[2],
            end_color[3], // extra = end_color
        ]);
    }

    /// Upload instance data (with retained-mode diffing) and draw.
    /// Returns the number of instances drawn.
    pub fn upload_and_draw<'a>(
        &'a mut self,
        gpu: &super::GpuState,
        render_pass: &mut wgpu::RenderPass<'a>,
    ) -> u32 {
        let instance_count = self.instance_count();
        if instance_count == 0 {
            return 0;
        }

        let (diff_ops, full_upload) = self.retained.diff(&self.instances);
        if full_upload {
            gpu.queue.write_buffer(
                self.instance_buf.buffer(),
                0,
                bytemuck::cast_slice(&self.instances),
            );
        } else {
            for op in &diff_ops {
                self.instance_buf.write_range(
                    &gpu.queue,
                    op.offset,
                    &self.instances[op.offset..op.offset + op.len],
                );
            }
        }

        render_pass.set_pipeline(&self.pipeline);
        render_pass.set_bind_group(0, &self.uniforms.bind_group, &[]);
        render_pass.set_vertex_buffer(0, self.instance_buf.buffer().slice(..));
        render_pass.draw(0..4, 0..instance_count as u32);
        instance_count as u32
    }

    /// Invalidate the retained scene (forces full upload next frame).
    pub fn invalidate_retained(&mut self) {
        self.retained.invalidate();
    }
}
