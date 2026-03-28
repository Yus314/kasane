use wgpu::MultisampleState;

use super::pipeline_common::{InstanceBuffer, ScreenUniforms};

/// Initial capacity for border instance buffer.
const INITIAL_BORDER_CAPACITY: usize = 64;

/// Per-instance data: 14 floats.
/// [0..4]   rect: x, y, w, h
/// [4..6]   params: corner_radius, border_width
/// [6..10]  fill_color: r, g, b, a
/// [10..14] border_color: r, g, b, a
const FLOATS_PER_INSTANCE: usize = 14;
const BYTES_PER_INSTANCE: u64 = (FLOATS_PER_INSTANCE * 4) as u64;

/// SDF-based rounded rectangle pipeline for borders and shadows.
pub struct BorderPipeline {
    pipeline: wgpu::RenderPipeline,
    uniforms: ScreenUniforms,
    instance_buf: InstanceBuffer,
    pub instances: Vec<f32>,
}

impl BorderPipeline {
    pub fn new(gpu: &super::GpuState, surface_format: wgpu::TextureFormat) -> Self {
        let shader = gpu
            .device
            .create_shader_module(wgpu::ShaderModuleDescriptor {
                label: Some("rounded_rect_shader"),
                source: wgpu::ShaderSource::Wgsl(include_str!("rounded_rect.wgsl").into()),
            });

        let uniforms = ScreenUniforms::new(&gpu.device, "border_uniforms");

        let pipeline_layout = gpu
            .device
            .create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                label: Some("border_pipeline_layout"),
                bind_group_layouts: &[Some(&uniforms.bind_group_layout)],
                immediate_size: 0,
            });

        let pipeline = gpu
            .device
            .create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                label: Some("border_pipeline"),
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
                            // params: vec2<f32> (corner_radius, border_width)
                            wgpu::VertexAttribute {
                                offset: 16,
                                shader_location: 1,
                                format: wgpu::VertexFormat::Float32x2,
                            },
                            // fill_color: vec4<f32>
                            wgpu::VertexAttribute {
                                offset: 24,
                                shader_location: 2,
                                format: wgpu::VertexFormat::Float32x4,
                            },
                            // border_color: vec4<f32>
                            wgpu::VertexAttribute {
                                offset: 40,
                                shader_location: 3,
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
            INITIAL_BORDER_CAPACITY,
            BYTES_PER_INSTANCE,
            "border_instances",
        );

        BorderPipeline {
            pipeline,
            uniforms,
            instance_buf,
            instances: Vec::with_capacity(INITIAL_BORDER_CAPACITY * FLOATS_PER_INSTANCE),
        }
    }

    /// Access the uniform buffer (for writing screen_size data).
    pub fn uniform_buffer(&self) -> &wgpu::Buffer {
        &self.uniforms.buffer
    }

    /// Push a rounded rect instance.
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
            corner_radius,
            border_width,
            fill_color[0],
            fill_color[1],
            fill_color[2],
            fill_color[3],
            border_color[0],
            border_color[1],
            border_color[2],
            border_color[3],
        ]);
    }

    /// Ensure the instance buffer is large enough.
    pub fn ensure_buffer(&mut self, gpu: &super::GpuState, needed: usize) {
        self.instance_buf.ensure_capacity(&gpu.device, needed);
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
