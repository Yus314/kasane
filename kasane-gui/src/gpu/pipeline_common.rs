//! Shared GPU infrastructure for instanced rendering pipelines.
//!
//! Both `BgPipeline` and `BorderPipeline` use the same uniform buffer layout
//! (8-byte `vec2<f32>` screen size) and the same instance buffer growth strategy.
//! This module extracts that common code.

/// 8-byte uniform buffer (`vec2<f32>` screen_size) + bind group.
pub(crate) struct ScreenUniforms {
    pub buffer: wgpu::Buffer,
    pub bind_group: wgpu::BindGroup,
    pub bind_group_layout: wgpu::BindGroupLayout,
}

impl ScreenUniforms {
    /// Create the uniform buffer, bind group layout, and bind group.
    pub fn new(device: &wgpu::Device, label: &str) -> Self {
        let buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some(label),
            size: 8, // vec2<f32> screen_size
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        let bind_group_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some(&format!("{label}_bind_group_layout")),
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

        let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some(&format!("{label}_bind_group")),
            layout: &bind_group_layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: buffer.as_entire_binding(),
            }],
        });

        ScreenUniforms {
            buffer,
            bind_group,
            bind_group_layout,
        }
    }
}

/// Dynamically resizable instance buffer for instanced rendering.
pub(crate) struct InstanceBuffer {
    buffer: wgpu::Buffer,
    capacity: usize,
    bytes_per_instance: u64,
    label: &'static str,
}

impl InstanceBuffer {
    /// Create an instance buffer with the given initial capacity.
    pub fn new(
        device: &wgpu::Device,
        capacity: usize,
        bytes_per_instance: u64,
        label: &'static str,
    ) -> Self {
        let buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some(label),
            size: capacity as u64 * bytes_per_instance,
            usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        InstanceBuffer {
            buffer,
            capacity,
            bytes_per_instance,
            label,
        }
    }

    /// Grow the buffer if `needed` exceeds current capacity (doubling strategy).
    pub fn ensure_capacity(&mut self, device: &wgpu::Device, needed: usize) {
        if needed <= self.capacity {
            return;
        }
        let new_cap = (self.capacity * 2).max(needed);
        self.buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some(self.label),
            size: new_cap as u64 * self.bytes_per_instance,
            usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        self.capacity = new_cap;
    }

    /// Access the underlying `wgpu::Buffer`.
    pub fn buffer(&self) -> &wgpu::Buffer {
        &self.buffer
    }
}
