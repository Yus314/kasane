use wgpu::MultisampleState;

use super::pipeline_common::{InstanceBuffer, ScreenUniforms};
use kasane_core::element::ImageFit;

/// Initial capacity for image instance buffer.
const INITIAL_IMAGE_CAPACITY: usize = 16;

/// Per-instance data: 9 floats (rect x,y,w,h + uv u0,v0,u1,v1 + opacity), 36 bytes.
const FLOATS_PER_INSTANCE: usize = 9;
const BYTES_PER_INSTANCE: u64 = (FLOATS_PER_INSTANCE * 4) as u64;

/// A single draw batch: one texture bound, multiple instances.
pub struct ImageDrawCall {
    pub bind_group: wgpu::BindGroup,
    pub instance_start: usize,
    pub instance_count: usize,
}

/// Textured quad rendering pipeline for image display.
///
/// Textures are managed externally by `TextureCache`; this pipeline only
/// holds the render pipeline, uniforms, instance buffer, and per-frame draw calls.
pub struct ImagePipeline {
    pipeline: wgpu::RenderPipeline,
    uniforms: ScreenUniforms,
    instance_buf: InstanceBuffer,
    pub instances: Vec<f32>,
    pub draw_calls: Vec<ImageDrawCall>,
}

impl ImagePipeline {
    /// Create the image pipeline. `texture_bind_group_layout` is provided by
    /// `TextureCache` so that bind groups are compatible across both.
    pub fn new(
        gpu: &super::GpuState,
        surface_format: wgpu::TextureFormat,
        texture_bind_group_layout: &wgpu::BindGroupLayout,
    ) -> Self {
        let shader = gpu
            .device
            .create_shader_module(wgpu::ShaderModuleDescriptor {
                label: Some("image_shader"),
                source: wgpu::ShaderSource::Wgsl(include_str!("image.wgsl").into()),
            });

        let uniforms = ScreenUniforms::new(&gpu.device, "image_uniforms");

        let pipeline_layout = gpu
            .device
            .create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                label: Some("image_pipeline_layout"),
                bind_group_layouts: &[&uniforms.bind_group_layout, texture_bind_group_layout],
                immediate_size: 0,
            });

        let pipeline = gpu
            .device
            .create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                label: Some("image_pipeline"),
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
                            // uv_rect: vec4<f32> (u0, v0, u1, v1)
                            wgpu::VertexAttribute {
                                offset: 16,
                                shader_location: 1,
                                format: wgpu::VertexFormat::Float32x4,
                            },
                            // opacity: f32
                            wgpu::VertexAttribute {
                                offset: 32,
                                shader_location: 2,
                                format: wgpu::VertexFormat::Float32,
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
            INITIAL_IMAGE_CAPACITY,
            BYTES_PER_INSTANCE,
            "image_instances",
        );

        ImagePipeline {
            pipeline,
            uniforms,
            instance_buf,
            instances: Vec::with_capacity(INITIAL_IMAGE_CAPACITY * FLOATS_PER_INSTANCE),
            draw_calls: Vec::new(),
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

    /// Push a textured quad with an externally-created bind group.
    #[allow(clippy::too_many_arguments)]
    pub fn push_textured_quad(
        &mut self,
        bind_group: wgpu::BindGroup,
        tex_w: f32,
        tex_h: f32,
        rect_x: f32,
        rect_y: f32,
        rect_w: f32,
        rect_h: f32,
        fit: ImageFit,
        opacity: f32,
    ) {
        let (u0, v0, u1, v1) = compute_uv(tex_w, tex_h, rect_w, rect_h, fit);

        let instance_start = self.instances.len() / FLOATS_PER_INSTANCE;
        self.instances
            .extend_from_slice(&[rect_x, rect_y, rect_w, rect_h, u0, v0, u1, v1, opacity]);

        self.draw_calls.push(ImageDrawCall {
            bind_group,
            instance_start,
            instance_count: 1,
        });
    }

    /// Upload instance data and issue draw calls. Returns the number of instances drawn.
    pub fn upload_and_draw<'a>(
        &'a self,
        gpu: &super::GpuState,
        render_pass: &mut wgpu::RenderPass<'a>,
    ) -> u32 {
        let total_instances = self.instances.len() / FLOATS_PER_INSTANCE;
        if total_instances == 0 {
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

        let mut drawn = 0u32;
        for call in &self.draw_calls {
            render_pass.set_bind_group(1, &call.bind_group, &[]);
            let start = call.instance_start as u32;
            let count = call.instance_count as u32;
            render_pass.draw(0..4, start..start + count);
            drawn += count;
        }
        drawn
    }

    /// Clear per-frame state.
    pub fn clear_frame(&mut self) {
        self.instances.clear();
        self.draw_calls.clear();
    }
}

/// Compute UV coordinates based on ImageFit mode.
/// Returns (u0, v0, u1, v1).
fn compute_uv(
    tex_w: f32,
    tex_h: f32,
    rect_w: f32,
    rect_h: f32,
    fit: ImageFit,
) -> (f32, f32, f32, f32) {
    match fit {
        ImageFit::Fill | ImageFit::Contain => (0.0, 0.0, 1.0, 1.0),
        ImageFit::Cover => {
            let tex_aspect = tex_w / tex_h;
            let rect_aspect = rect_w / rect_h;
            if tex_aspect > rect_aspect {
                // Texture is wider: crop sides
                let visible_frac = rect_aspect / tex_aspect;
                let offset = (1.0 - visible_frac) / 2.0;
                (offset, 0.0, offset + visible_frac, 1.0)
            } else {
                // Texture is taller: crop top/bottom
                let visible_frac = tex_aspect / rect_aspect;
                let offset = (1.0 - visible_frac) / 2.0;
                (0.0, offset, 1.0, offset + visible_frac)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_compute_uv_fill() {
        let (u0, v0, u1, v1) = compute_uv(100.0, 50.0, 200.0, 100.0, ImageFit::Fill);
        assert_eq!((u0, v0, u1, v1), (0.0, 0.0, 1.0, 1.0));
    }

    #[test]
    fn test_compute_uv_contain() {
        let (u0, v0, u1, v1) = compute_uv(100.0, 50.0, 200.0, 100.0, ImageFit::Contain);
        assert_eq!((u0, v0, u1, v1), (0.0, 0.0, 1.0, 1.0));
    }

    #[test]
    fn test_compute_uv_cover_wider_texture() {
        let (u0, v0, u1, v1) = compute_uv(200.0, 100.0, 100.0, 100.0, ImageFit::Cover);
        assert!((u0 - 0.25).abs() < 0.001);
        assert_eq!(v0, 0.0);
        assert!((u1 - 0.75).abs() < 0.001);
        assert_eq!(v1, 1.0);
    }

    #[test]
    fn test_compute_uv_cover_taller_texture() {
        let (u0, v0, u1, v1) = compute_uv(100.0, 200.0, 100.0, 100.0, ImageFit::Cover);
        assert_eq!(u0, 0.0);
        assert!((v0 - 0.25).abs() < 0.001);
        assert_eq!(u1, 1.0);
        assert!((v1 - 0.75).abs() < 0.001);
    }
}
