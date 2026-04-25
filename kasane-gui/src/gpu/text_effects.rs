//! Text post-processing effects: shadow, glow.
//!
//! When active, text is rendered to an intermediate render target. Shadow and/or
//! glow passes read that target's alpha channel to produce effect layers, which
//! are composited before the sharp text.

use kasane_core::config::TextEffectsConfig;

use super::compositor::RenderTarget;

/// Manages text post-processing pipelines (shadow, glow).
pub struct TextEffects {
    shadow_pipeline: wgpu::RenderPipeline,
    glow_pipeline: wgpu::RenderPipeline,
    sampler: wgpu::Sampler,
    texture_bind_group_layout: wgpu::BindGroupLayout,
    params_buffer: wgpu::Buffer,
    params_bind_group: wgpu::BindGroup,
    /// Intermediate render target for text (lazily sized).
    pub text_target: Option<RenderTarget>,
}

/// Uniform data for the shadow/glow shaders (32 bytes = 2x vec4).
#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct EffectParams {
    /// xy: offset (UV space), z: blur radius (UV space), w: unused
    offset_blur: [f32; 4],
    /// Shadow/glow color (linear RGBA)
    color: [f32; 4],
}

impl TextEffects {
    pub fn new(device: &wgpu::Device, surface_format: wgpu::TextureFormat) -> Self {
        let shadow_shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("text_shadow_shader"),
            source: wgpu::ShaderSource::Wgsl(include_str!("text_shadow.wgsl").into()),
        });
        let glow_shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("text_glow_shader"),
            source: wgpu::ShaderSource::Wgsl(include_str!("text_glow.wgsl").into()),
        });

        let texture_bind_group_layout =
            device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: Some("text_effects_texture_layout"),
                entries: &[
                    wgpu::BindGroupLayoutEntry {
                        binding: 0,
                        visibility: wgpu::ShaderStages::FRAGMENT,
                        ty: wgpu::BindingType::Texture {
                            multisampled: false,
                            view_dimension: wgpu::TextureViewDimension::D2,
                            sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        },
                        count: None,
                    },
                    wgpu::BindGroupLayoutEntry {
                        binding: 1,
                        visibility: wgpu::ShaderStages::FRAGMENT,
                        ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                        count: None,
                    },
                ],
            });

        let params_bind_group_layout =
            device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: Some("text_effects_params_layout"),
                entries: &[wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                }],
            });

        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("text_effects_pipeline_layout"),
            bind_group_layouts: &[
                Some(&texture_bind_group_layout),
                Some(&params_bind_group_layout),
            ],
            immediate_size: 0,
        });

        let make_pipeline = |shader: &wgpu::ShaderModule, label: &str| -> wgpu::RenderPipeline {
            device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                label: Some(label),
                layout: Some(&pipeline_layout),
                vertex: wgpu::VertexState {
                    module: shader,
                    entry_point: Some("vs_main"),
                    buffers: &[],
                    compilation_options: Default::default(),
                },
                fragment: Some(wgpu::FragmentState {
                    module: shader,
                    entry_point: Some("fs_main"),
                    targets: &[Some(wgpu::ColorTargetState {
                        format: surface_format,
                        blend: Some(wgpu::BlendState::ALPHA_BLENDING),
                        write_mask: wgpu::ColorWrites::ALL,
                    })],
                    compilation_options: Default::default(),
                }),
                primitive: wgpu::PrimitiveState {
                    topology: wgpu::PrimitiveTopology::TriangleList,
                    ..Default::default()
                },
                depth_stencil: None,
                multisample: wgpu::MultisampleState::default(),
                multiview_mask: None,
                cache: None,
            })
        };

        let shadow_pipeline = make_pipeline(&shadow_shader, "text_shadow_pipeline");
        let glow_pipeline = make_pipeline(&glow_shader, "text_glow_pipeline");

        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("text_effects_sampler"),
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            ..Default::default()
        });

        let params_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("text_effects_params"),
            size: std::mem::size_of::<EffectParams>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        let params_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("text_effects_params_bind_group"),
            layout: &params_bind_group_layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: params_buffer.as_entire_binding(),
            }],
        });

        Self {
            shadow_pipeline,
            glow_pipeline,
            sampler,
            texture_bind_group_layout,
            params_buffer,
            params_bind_group,
            text_target: None,
        }
    }

    /// Ensure the text render target exists at the correct size.
    pub fn ensure_target(
        &mut self,
        device: &wgpu::Device,
        width: u32,
        height: u32,
        format: wgpu::TextureFormat,
    ) {
        let w = width.max(1);
        let h = height.max(1);
        if let Some(ref mut target) = self.text_target {
            target.resize(device, w, h, format, "text_effects_target");
        } else {
            self.text_target = Some(RenderTarget::new(
                device,
                w,
                h,
                format,
                "text_effects_target",
            ));
        }
    }

    /// Apply text effects (shadow and/or glow) to the main render pass.
    ///
    /// The text has already been rendered to `self.text_target`. This method
    /// draws effect passes (shadow, glow) into the given render pass, then
    /// the caller should blit the sharp text on top.
    pub fn apply(
        &self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        render_pass: &mut wgpu::RenderPass<'_>,
        config: &TextEffectsConfig,
        screen_w: f32,
        screen_h: f32,
    ) {
        let Some(ref target) = self.text_target else {
            return;
        };

        let source_bg = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("text_effects_source_bg"),
            layout: &self.texture_bind_group_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(&target.view),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::Sampler(&self.sampler),
                },
            ],
        });

        // Shadow pass
        if let Some((dx, dy)) = config.shadow_offset {
            let params = EffectParams {
                offset_blur: [
                    dx / screen_w,
                    dy / screen_h,
                    config.shadow_blur / screen_w.max(screen_h),
                    0.0,
                ],
                color: config.shadow_color,
            };
            queue.write_buffer(&self.params_buffer, 0, bytemuck::cast_slice(&[params]));
            render_pass.set_pipeline(&self.shadow_pipeline);
            render_pass.set_bind_group(0, &source_bg, &[]);
            render_pass.set_bind_group(1, &self.params_bind_group, &[]);
            render_pass.draw(0..3, 0..1);
        }

        // Glow pass
        if config.glow_radius > 0.0 {
            let params = EffectParams {
                offset_blur: [config.glow_radius / screen_w.max(screen_h), 0.0, 0.0, 0.0],
                color: config.glow_color,
            };
            queue.write_buffer(&self.params_buffer, 0, bytemuck::cast_slice(&[params]));
            render_pass.set_pipeline(&self.glow_pipeline);
            render_pass.set_bind_group(0, &source_bg, &[]);
            render_pass.set_bind_group(1, &self.params_bind_group, &[]);
            render_pass.draw(0..3, 0..1);
        }
    }
}
