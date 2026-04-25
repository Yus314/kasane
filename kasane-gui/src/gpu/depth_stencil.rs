//! Depth/stencil buffer management for clipping and z-order control.
//!
//! Provides a `Depth24PlusStencil8` texture that is attached to every render pass.
//! Stencil operations are used for rounded-rect clipping (PushClip/PopClip);
//! the depth channel is reserved for future z-order control.

/// Manages the depth/stencil texture and provides helpers for stencil operations.
pub struct DepthStencilState {
    texture: wgpu::Texture,
    pub view: wgpu::TextureView,
    width: u32,
    height: u32,
}

/// The format used for all depth/stencil attachments.
pub const DS_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Depth24PlusStencil8;

/// Build the `DepthStencilState` descriptor used when creating render pipelines.
/// All draw pipelines use this: stencil compare `Equal`, depth disabled.
///
/// `depth_write_enabled` / `depth_compare` must be `Some(_)` for any format
/// with a depth aspect (wgpu 29 validation). We use `Some(false)` + `Always`
/// to make depth effectively pass-through while satisfying the validator;
/// only the stencil channel carries meaning here.
pub fn pipeline_depth_stencil() -> wgpu::DepthStencilState {
    wgpu::DepthStencilState {
        format: DS_FORMAT,
        depth_write_enabled: Some(false),
        depth_compare: Some(wgpu::CompareFunction::Always),
        stencil: wgpu::StencilState {
            front: wgpu::StencilFaceState {
                compare: wgpu::CompareFunction::Equal,
                fail_op: wgpu::StencilOperation::Keep,
                depth_fail_op: wgpu::StencilOperation::Keep,
                pass_op: wgpu::StencilOperation::Keep,
            },
            back: wgpu::StencilFaceState {
                compare: wgpu::CompareFunction::Equal,
                fail_op: wgpu::StencilOperation::Keep,
                depth_fail_op: wgpu::StencilOperation::Keep,
                pass_op: wgpu::StencilOperation::Keep,
            },
            read_mask: 0xFF,
            write_mask: 0x00,
        },
        bias: wgpu::DepthBiasState::default(),
    }
}

/// Build a `DepthStencilState` for the stencil-write pass (PushClip).
/// Increments the stencil value where the clip shape is drawn.
pub fn stencil_write_increment() -> wgpu::DepthStencilState {
    wgpu::DepthStencilState {
        format: DS_FORMAT,
        depth_write_enabled: Some(false),
        depth_compare: Some(wgpu::CompareFunction::Always),
        stencil: wgpu::StencilState {
            front: wgpu::StencilFaceState {
                compare: wgpu::CompareFunction::Always,
                fail_op: wgpu::StencilOperation::Keep,
                depth_fail_op: wgpu::StencilOperation::Keep,
                pass_op: wgpu::StencilOperation::IncrementClamp,
            },
            back: wgpu::StencilFaceState {
                compare: wgpu::CompareFunction::Always,
                fail_op: wgpu::StencilOperation::Keep,
                depth_fail_op: wgpu::StencilOperation::Keep,
                pass_op: wgpu::StencilOperation::IncrementClamp,
            },
            read_mask: 0xFF,
            write_mask: 0xFF,
        },
        bias: wgpu::DepthBiasState::default(),
    }
}

/// Build a `DepthStencilState` for the stencil-restore pass (PopClip).
/// Decrements the stencil value where the clip shape is drawn.
pub fn stencil_write_decrement() -> wgpu::DepthStencilState {
    wgpu::DepthStencilState {
        format: DS_FORMAT,
        depth_write_enabled: Some(false),
        depth_compare: Some(wgpu::CompareFunction::Always),
        stencil: wgpu::StencilState {
            front: wgpu::StencilFaceState {
                compare: wgpu::CompareFunction::Always,
                fail_op: wgpu::StencilOperation::Keep,
                depth_fail_op: wgpu::StencilOperation::Keep,
                pass_op: wgpu::StencilOperation::DecrementClamp,
            },
            back: wgpu::StencilFaceState {
                compare: wgpu::CompareFunction::Always,
                fail_op: wgpu::StencilOperation::Keep,
                depth_fail_op: wgpu::StencilOperation::Keep,
                pass_op: wgpu::StencilOperation::DecrementClamp,
            },
            read_mask: 0xFF,
            write_mask: 0xFF,
        },
        bias: wgpu::DepthBiasState::default(),
    }
}

impl DepthStencilState {
    /// Create a new depth/stencil texture at the given dimensions.
    pub fn new(device: &wgpu::Device, width: u32, height: u32) -> Self {
        let (texture, view) = Self::create_texture(device, width, height);
        Self {
            texture,
            view,
            width: width.max(1),
            height: height.max(1),
        }
    }

    fn create_texture(
        device: &wgpu::Device,
        width: u32,
        height: u32,
    ) -> (wgpu::Texture, wgpu::TextureView) {
        let w = width.max(1);
        let h = height.max(1);
        let texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("depth_stencil"),
            size: wgpu::Extent3d {
                width: w,
                height: h,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: DS_FORMAT,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            view_formats: &[],
        });
        let view = texture.create_view(&Default::default());
        (texture, view)
    }

    /// Resize the texture if the dimensions changed. Returns `true` if recreated.
    pub fn resize(&mut self, device: &wgpu::Device, width: u32, height: u32) -> bool {
        let w = width.max(1);
        let h = height.max(1);
        if w == self.width && h == self.height {
            return false;
        }
        let (texture, view) = Self::create_texture(device, w, h);
        self.texture = texture;
        self.view = view;
        self.width = w;
        self.height = h;
        true
    }

    /// Build the `RenderPassDepthStencilAttachment` for a render pass.
    /// Clears both depth and stencil on the first layer; loads on subsequent layers.
    pub fn attachment(&self, clear: bool) -> wgpu::RenderPassDepthStencilAttachment<'_> {
        wgpu::RenderPassDepthStencilAttachment {
            view: &self.view,
            depth_ops: Some(wgpu::Operations {
                load: if clear {
                    wgpu::LoadOp::Clear(1.0)
                } else {
                    wgpu::LoadOp::Load
                },
                store: wgpu::StoreOp::Store,
            }),
            stencil_ops: Some(wgpu::Operations {
                load: if clear {
                    wgpu::LoadOp::Clear(0)
                } else {
                    wgpu::LoadOp::Load
                },
                store: wgpu::StoreOp::Store,
            }),
        }
    }
}
