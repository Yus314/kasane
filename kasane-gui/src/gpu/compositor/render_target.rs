//! Render target: a texture + view pair for off-screen rendering.

/// A render target backed by a wgpu texture.
pub struct RenderTarget {
    pub texture: wgpu::Texture,
    pub view: wgpu::TextureView,
    pub width: u32,
    pub height: u32,
}

impl RenderTarget {
    pub fn new(
        device: &wgpu::Device,
        width: u32,
        height: u32,
        format: wgpu::TextureFormat,
        label: &str,
    ) -> Self {
        let texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some(label),
            size: wgpu::Extent3d {
                width: width.max(1),
                height: height.max(1),
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING,
            view_formats: &[],
        });
        let view = texture.create_view(&Default::default());
        Self {
            texture,
            view,
            width: width.max(1),
            height: height.max(1),
        }
    }

    /// Resize the render target if dimensions changed. Returns true if recreated.
    pub fn resize(
        &mut self,
        device: &wgpu::Device,
        width: u32,
        height: u32,
        format: wgpu::TextureFormat,
        label: &str,
    ) -> bool {
        let w = width.max(1);
        let h = height.max(1);
        if w == self.width && h == self.height {
            return false;
        }
        *self = Self::new(device, w, h, format, label);
        true
    }
}
