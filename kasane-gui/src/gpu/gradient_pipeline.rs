use wgpu::MultisampleState;

use super::pipeline_common::{InstanceBuffer, ScreenUniforms};

/// Initial capacity for gradient instance buffer.
const INITIAL_GRADIENT_CAPACITY: usize = 4;

/// Per-instance data: 12 floats, 48 bytes.
/// [0..4]  rect: x, y, w, h (pixels)
/// [4..8]  start_color: r, g, b, a (sRGB normalized)
/// [8..12] end_color: r, g, b, a (sRGB normalized)
const FLOATS_PER_INSTANCE: usize = 12;
const BYTES_PER_INSTANCE: u64 = (FLOATS_PER_INSTANCE * 4) as u64;

/// Gradient background rendering pipeline — renders vertical gradients with dithering.
pub struct GradientPipeline {
    pipeline: wgpu::RenderPipeline,
    uniforms: ScreenUniforms,
    instance_buf: InstanceBuffer,
    pub instances: Vec<f32>,
}

impl GradientPipeline {
    pub fn new(gpu: &super::GpuState, surface_format: wgpu::TextureFormat) -> Self {
        let shader = gpu
            .device
            .create_shader_module(wgpu::ShaderModuleDescriptor {
                label: Some("gradient_shader"),
                source: wgpu::ShaderSource::Wgsl(include_str!("gradient.wgsl").into()),
            });

        let uniforms = ScreenUniforms::new(&gpu.device, "gradient_uniforms");

        let pipeline_layout = gpu
            .device
            .create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                label: Some("gradient_pipeline_layout"),
                bind_group_layouts: &[Some(&uniforms.bind_group_layout)],
                immediate_size: 0,
            });

        let pipeline = gpu
            .device
            .create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                label: Some("gradient_pipeline"),
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
                            // start_color: vec4<f32>
                            wgpu::VertexAttribute {
                                offset: 16,
                                shader_location: 1,
                                format: wgpu::VertexFormat::Float32x4,
                            },
                            // end_color: vec4<f32>
                            wgpu::VertexAttribute {
                                offset: 32,
                                shader_location: 2,
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
                depth_stencil: None,
                multisample: MultisampleState::default(),
                multiview_mask: None,
                cache: None,
            });

        let instance_buf = InstanceBuffer::new(
            &gpu.device,
            INITIAL_GRADIENT_CAPACITY,
            BYTES_PER_INSTANCE,
            "gradient_instances",
        );

        GradientPipeline {
            pipeline,
            uniforms,
            instance_buf,
            instances: Vec::with_capacity(INITIAL_GRADIENT_CAPACITY * FLOATS_PER_INSTANCE),
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

    /// Push a gradient instance (12 floats: rect + start_color + end_color).
    pub fn push(
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
            end_color[0],
            end_color[1],
            end_color[2],
            end_color[3],
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
