//! wgpu-side renderer for the Parley pipeline (ADR-031, Phase 9b Step 3).
//!
//! Mirrors [`text_pipeline::TextRenderer`](crate::gpu::text_pipeline::TextRenderer)
//! but consumes [`DrawableGlyph`]s from [`super::frame_builder`] and writes
//! [`ParleyGlyphVertex`]s built by [`super::vertex_builder`]. The shader
//! (`text_pipeline/shader.wgsl`), vertex layout, atlas bind-group layout,
//! and uniforms layout are all shared with the cosmic-text renderer
//! through a single [`text_pipeline::Cache`] instance.
//!
//! ## Two-call lifecycle
//!
//! ```text
//! prepare(&Device, &Queue, drawables, mask_atlas, color_atlas, viewport)
//!   ├─ flush mask + color atlas pending uploads
//!   ├─ rebuild atlas bind group (mask + color textures + sampler)
//!   ├─ rebuild vertex buffer (grows on demand)
//!   └─ stash bind group + draw count for the upcoming render() call
//!
//! render(&mut RenderPass)
//!   ├─ set_pipeline / set_bind_group / set_vertex_buffer
//!   └─ pass.draw(0..4, 0..count)   // instanced triangle strips
//! ```
//!
//! `prepare` requires `&Device`/`&Queue` (uploads + buffer growth), `render`
//! borrows nothing beyond the previously-prepared state. The split mirrors
//! the existing TextRenderer so SceneRenderer's frame loop layout does not
//! change.
//!
//! ## Buffer growth
//!
//! The vertex buffer doubles via `next_copy_buffer_size` (next power of two,
//! aligned to `COPY_BUFFER_ALIGNMENT`). A frame that exceeds the previous
//! buffer's capacity triggers re-allocation and wgpu will internally release
//! the old buffer once the GPU is done with it.

use wgpu::{
    BindGroup, Buffer, BufferDescriptor, BufferUsages, COPY_BUFFER_ALIGNMENT, DepthStencilState,
    Device, MultisampleState, Queue, RenderPass, RenderPipeline,
};

use crate::gpu::text_pipeline::{Cache, Viewport};

use super::frame_builder::DrawableGlyph;
use super::gpu_atlas::GpuAtlasShelf;
use super::vertex_builder::build_vertices;

/// Renders accumulated [`DrawableGlyph`]s into the wgpu vertex buffer.
///
/// Owns the vertex buffer, the render pipeline, and the per-frame
/// atlas bind group. `prepare` builds the vertex data from the
/// drawables list; `render` issues the draw call.
pub struct TextRenderer {
    vertex_buffer: Buffer,
    vertex_buffer_size: u64,
    pipeline: RenderPipeline,
    /// Set on each `prepare` call from the current mask + color atlas
    /// `TextureView`s, so the renderer is self-contained at render time.
    atlas_bind_group: Option<BindGroup>,
    /// Number of glyph instances staged on the most recent `prepare`.
    glyph_count: u32,
}

impl TextRenderer {
    /// Build a new renderer. Reuses the supplied [`Cache`]'s shader / bind
    /// layouts / pipeline cache, which means the parley renderer and the
    /// cosmic-text renderer share their wgpu pipeline state machine.
    pub fn new(
        device: &Device,
        cache: &Cache,
        target_format: wgpu::TextureFormat,
        multisample: MultisampleState,
        depth_stencil: Option<DepthStencilState>,
    ) -> Self {
        let vertex_buffer_size = next_copy_buffer_size(4096);
        let vertex_buffer = device.create_buffer(&BufferDescriptor {
            label: Some("kasane parley_text vertex buffer"),
            size: vertex_buffer_size,
            usage: BufferUsages::VERTEX | BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        let pipeline =
            cache.get_or_create_pipeline(device, target_format, multisample, depth_stencil);

        Self {
            vertex_buffer,
            vertex_buffer_size,
            pipeline,
            atlas_bind_group: None,
            glyph_count: 0,
        }
    }

    /// Upload all queued atlas pixels, build vertex data, and stage a
    /// fresh atlas bind group. Empty `drawables` is a valid input — the
    /// next [`render`](Self::render) becomes a no-op.
    #[allow(clippy::too_many_arguments)]
    pub fn prepare(
        &mut self,
        device: &Device,
        queue: &Queue,
        cache: &Cache,
        mask_atlas: &mut GpuAtlasShelf,
        color_atlas: &mut GpuAtlasShelf,
        drawables: &[DrawableGlyph],
    ) {
        // Phase 1 — drain pending uploads. Idempotent if already empty.
        mask_atlas.flush_uploads(queue);
        color_atlas.flush_uploads(queue);

        // Phase 2 — convert drawables to wire-format vertices.
        let vertices = build_vertices(drawables);
        self.glyph_count = vertices.len() as u32;

        if !vertices.is_empty() {
            let bytes: &[u8] = bytemuck::cast_slice(&vertices);
            let needed = next_copy_buffer_size(bytes.len() as u64);
            if needed > self.vertex_buffer_size {
                // Grow: allocate a larger buffer.
                self.vertex_buffer = device.create_buffer(&BufferDescriptor {
                    label: Some("kasane parley_text vertex buffer (grown)"),
                    size: needed,
                    usage: BufferUsages::VERTEX | BufferUsages::COPY_DST,
                    mapped_at_creation: false,
                });
                self.vertex_buffer_size = needed;
            }
            queue.write_buffer(&self.vertex_buffer, 0, bytes);
        }

        // Phase 3 — atlas bind group. Cache::create_atlas_bind_group wants
        // (color, mask) order to match the shader's binding indices.
        self.atlas_bind_group =
            Some(cache.create_atlas_bind_group(device, color_atlas.view(), mask_atlas.view()));
    }

    /// Issue draw calls for the previously-prepared frame.
    ///
    /// No-op when `prepare` was never called or the frame had no glyphs.
    pub fn render(&self, viewport: &Viewport, pass: &mut RenderPass<'_>) {
        let Some(atlas_bg) = &self.atlas_bind_group else {
            return;
        };
        if self.glyph_count == 0 {
            return;
        }

        pass.set_pipeline(&self.pipeline);
        pass.set_bind_group(0, atlas_bg, &[]);
        pass.set_bind_group(1, &viewport.bind_group, &[]);
        pass.set_vertex_buffer(0, self.vertex_buffer.slice(..));
        pass.draw(0..4, 0..self.glyph_count);
    }

    /// Number of glyph instances staged on the most recent `prepare`.
    pub fn glyph_count(&self) -> u32 {
        self.glyph_count
    }
}

/// Round `size` up to the next power-of-two `COPY_BUFFER_ALIGNMENT`-aligned
/// buffer size — matches the helper in `text_pipeline::text_render`.
fn next_copy_buffer_size(size: u64) -> u64 {
    let align_mask = COPY_BUFFER_ALIGNMENT - 1;
    ((size.next_power_of_two() + align_mask) & !align_mask).max(COPY_BUFFER_ALIGNMENT)
}

#[cfg(test)]
mod tests {
    //! Vertex packing + buffer sizing are unit-tested without wgpu. The
    //! TextRenderer construction itself is exercised through the
    //! SceneRenderer end-to-end smoke (Phase 9b Step 4) — building a
    //! headless wgpu device here would force every CI runner to find a
    //! Vulkan / Metal / DX adapter.
    use super::*;

    #[test]
    fn buffer_size_aligns_and_powers_of_two() {
        // COPY_BUFFER_ALIGNMENT is 4 bytes on every backend.
        assert!(next_copy_buffer_size(1) >= COPY_BUFFER_ALIGNMENT);
        // Power-of-two growth.
        let small = next_copy_buffer_size(28); // one ParleyGlyphVertex
        let larger = next_copy_buffer_size(280); // ten of them
        assert!(larger > small);
        assert!(larger.is_power_of_two());
    }

    #[test]
    fn alignment_invariant() {
        for size in [1u64, 28, 1024, 65535, 1_048_576] {
            let s = next_copy_buffer_size(size);
            assert_eq!(s % COPY_BUFFER_ALIGNMENT, 0, "size {size} → {s}");
            assert!(s >= size, "must not shrink");
        }
    }
}
