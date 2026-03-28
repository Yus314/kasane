use wgpu::MultisampleState;

use super::pipeline_common::{InstanceBuffer, ScreenUniforms};

/// Initial capacity for decoration instance buffer.
const INITIAL_DECO_CAPACITY: usize = 512;

/// Per-instance data: 10 floats, 40 bytes.
/// [0..4] rect: x, y, w, h (pixels)
/// [4..8] color: r, g, b, a (sRGB normalized)
/// [8]    decoration type: 0=solid, 1=curly, 2=double
/// [9]    stroke thickness (reserved for future use)
const FLOATS_PER_INSTANCE: usize = 10;
const BYTES_PER_INSTANCE: u64 = (FLOATS_PER_INSTANCE * 4) as u64;

/// Decoration type constants passed to the shader.
pub const DECO_SOLID: f32 = 0.0;
pub const DECO_CURLY: f32 = 1.0;
pub const DECO_DOUBLE: f32 = 2.0;

/// Text decoration rendering pipeline — renders underlines, curly underlines,
/// double underlines, and strikethrough lines after the text pass.
pub struct DecorationPipeline {
    pipeline: wgpu::RenderPipeline,
    uniforms: ScreenUniforms,
    instance_buf: InstanceBuffer,
    pub instances: Vec<f32>,
}

impl DecorationPipeline {
    pub fn new(gpu: &super::GpuState, surface_format: wgpu::TextureFormat) -> Self {
        let shader = gpu
            .device
            .create_shader_module(wgpu::ShaderModuleDescriptor {
                label: Some("decoration_shader"),
                source: wgpu::ShaderSource::Wgsl(include_str!("decoration.wgsl").into()),
            });

        let uniforms = ScreenUniforms::new(&gpu.device, "decoration_uniforms");

        let pipeline_layout = gpu
            .device
            .create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                label: Some("decoration_pipeline_layout"),
                bind_group_layouts: &[Some(&uniforms.bind_group_layout)],
                immediate_size: 0,
            });

        let pipeline = gpu
            .device
            .create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                label: Some("decoration_pipeline"),
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
                            // color: vec4<f32>
                            wgpu::VertexAttribute {
                                offset: 16,
                                shader_location: 1,
                                format: wgpu::VertexFormat::Float32x4,
                            },
                            // params: vec2<f32> (type, thickness)
                            wgpu::VertexAttribute {
                                offset: 32,
                                shader_location: 2,
                                format: wgpu::VertexFormat::Float32x2,
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
                depth_stencil: None,
                multisample: MultisampleState::default(),
                multiview_mask: None,
                cache: None,
            });

        let instance_buf = InstanceBuffer::new(
            &gpu.device,
            INITIAL_DECO_CAPACITY,
            BYTES_PER_INSTANCE,
            "decoration_instances",
        );

        DecorationPipeline {
            pipeline,
            uniforms,
            instance_buf,
            instances: Vec::with_capacity(INITIAL_DECO_CAPACITY * FLOATS_PER_INSTANCE),
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

    /// Push a decoration instance (10 floats).
    pub fn push(&mut self, x: f32, y: f32, w: f32, h: f32, color: [f32; 4], deco_type: f32) {
        self.instances.extend_from_slice(&[
            x, y, w, h, color[0], color[1], color[2], color[3], deco_type, 0.0,
        ]);
    }

    /// Upload instance data and draw. Returns the number of instances drawn.
    pub fn upload_and_draw<'a>(
        &'a self,
        gpu: &super::GpuState,
        render_pass: &mut wgpu::RenderPass<'a>,
    ) -> u32 {
        let instance_count = self.instances.len() / FLOATS_PER_INSTANCE;
        if instance_count == 0 {
            return 0;
        }
        gpu.queue.write_buffer(
            self.instance_buf.buffer(),
            0,
            bytemuck::cast_slice(&self.instances),
        );
        render_pass.set_pipeline(&self.pipeline);
        render_pass.set_bind_group(0, &self.uniforms.bind_group, &[]);
        render_pass.set_vertex_buffer(0, self.instance_buf.buffer().slice(..));
        render_pass.draw(0..4, 0..instance_count as u32);
        instance_count as u32
    }
}
