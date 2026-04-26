//! wgpu-aware wrapper around [`AtlasShelf`] (ADR-031, Phase 9b Step 1).
//!
//! Pairs the CPU-side [`AtlasShelf`] allocator with a `wgpu::Texture`, so
//! the L2 [`GlyphRasterCache`](super::raster_cache::GlyphRasterCache) can
//! place a glyph bitmap on the GPU without involving every render-loop
//! site in wgpu mechanics.
//!
//! ## Two-phase upload
//!
//! Glyph rasterisation can happen anywhere in the frame (often inside the
//! L2 cache miss closure), but `wgpu::Queue::write_texture` is best
//! batched. [`GpuAtlasShelf`] separates the two concerns:
//!
//! 1. [`allocate`](Self::allocate) reserves a slot through the CPU
//!    allocator and queues the bitmap into `pending_uploads` (no GPU
//!    interaction).
//! 2. [`flush_uploads`](Self::flush_uploads) walks the queued bitmaps and
//!    issues `Queue::write_texture` calls. The renderer calls this once
//!    per frame, before the text render pass begins.
//!
//! This separation also makes the type fully unit-testable for the CPU
//! side: tests do not need a `wgpu::Device` to verify allocation +
//! pending-upload bookkeeping.
//!
//! ## Format selection
//!
//! - `Kind::Mask` → `R8Unorm` (one byte per pixel; the shader samples the
//!   alpha channel and tints with the per-glyph brush).
//! - `Kind::Color` → `Rgba8Unorm` (four bytes per pixel; the bitmap data
//!   already contains premultiplied alpha and is sampled as-is).
//!
//! ## Lifecycle
//!
//! - `clear()` releases every CPU allocation but leaves the GPU texture
//!   untouched. Callers that need to invalidate the GPU side (font /
//!   hint change) re-create the `GpuAtlasShelf`.
//! - `grow()` recreates the GPU texture at a larger size and tells the
//!   caller; the CPU allocator grows in-place. Glyph re-upload after a
//!   grow is the caller's responsibility (re-walk the L2 cache).

use wgpu::{
    Device, Extent3d, Queue, TexelCopyBufferLayout, TexelCopyTextureInfo, Texture, TextureAspect,
    TextureDescriptor, TextureDimension, TextureFormat, TextureUsages, TextureView,
    TextureViewDescriptor,
};

use super::atlas::{AtlasShelf, AtlasSlot, DEFAULT_ATLAS_SIZE};

/// Which channel layout the atlas stores. Mask atlases hold 8-bit alpha,
/// colour atlases hold 32-bit RGBA. The renderer uses two atlases (one of
/// each kind) so a single sampler can serve both.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    Mask,
    Color,
}

impl Kind {
    /// Wgpu texel format for this atlas kind.
    pub fn texture_format(self) -> TextureFormat {
        match self {
            Kind::Mask => TextureFormat::R8Unorm,
            // Linear (not sRGB): the parley_text render path passes
            // already-linear colours through a non-sRGB framebuffer view to
            // match the cosmic-text path's `ColorMode::Web` choice.
            Kind::Color => TextureFormat::Rgba8Unorm,
        }
    }

    /// Bytes per pixel.
    pub fn bytes_per_pixel(self) -> u32 {
        match self {
            Kind::Mask => 1,
            Kind::Color => 4,
        }
    }
}

/// One queued upload waiting for [`GpuAtlasShelf::flush_uploads`].
///
/// The bitmap is owned (not borrowed) so the queue survives the
/// rasteriser's stack frame. A [`GpuAtlasShelf`] caps the queue length
/// indirectly through atlas allocation: a full atlas refuses to allocate.
#[derive(Debug, Clone)]
pub struct PendingUpload {
    pub slot: AtlasSlot,
    pub data: Vec<u8>,
}

/// Atlas with a backing `wgpu::Texture`.
pub struct GpuAtlasShelf {
    cpu: AtlasShelf,
    kind: Kind,
    texture: Texture,
    view: TextureView,
    pending_uploads: Vec<PendingUpload>,
}

impl GpuAtlasShelf {
    /// Construct an atlas of the given square side length, allocating both
    /// the CPU shelf and the wgpu texture.
    pub fn new(device: &Device, kind: Kind, side: u16) -> Self {
        let cpu = AtlasShelf::new(side);
        let actual_side = cpu.width();
        let (texture, view) = create_texture(device, kind, actual_side);
        Self {
            cpu,
            kind,
            texture,
            view,
            pending_uploads: Vec::new(),
        }
    }

    /// Convenience constructor with the default atlas side
    /// ([`DEFAULT_ATLAS_SIZE`]).
    pub fn default_for(device: &Device, kind: Kind) -> Self {
        Self::new(device, kind, DEFAULT_ATLAS_SIZE)
    }

    /// Allocate `width × height` pixels and queue the bitmap for upload on
    /// the next [`flush_uploads`](Self::flush_uploads) call. Returns
    /// `None` when the atlas has no room (caller may grow or evict).
    pub fn allocate_and_queue(
        &mut self,
        width: u16,
        height: u16,
        data: Vec<u8>,
    ) -> Option<AtlasSlot> {
        let expected_len =
            usize::from(width) * usize::from(height) * self.kind.bytes_per_pixel() as usize;
        debug_assert_eq!(
            data.len(),
            expected_len,
            "GpuAtlasShelf({:?}): data len {} != width*height*bpp = {}",
            self.kind,
            data.len(),
            expected_len
        );
        let slot = self.cpu.allocate(width, height)?;
        self.pending_uploads.push(PendingUpload { slot, data });
        Some(slot)
    }

    /// Allocate a slot without queuing data. Used by callers that prefer
    /// to manage uploads externally.
    pub fn allocate(&mut self, width: u16, height: u16) -> Option<AtlasSlot> {
        self.cpu.allocate(width, height)
    }

    /// Manually queue a pre-allocated slot's data for the next flush.
    pub fn queue_upload(&mut self, slot: AtlasSlot, data: Vec<u8>) {
        self.pending_uploads.push(PendingUpload { slot, data });
    }

    /// Release the CPU allocation. Has no effect on already-uploaded GPU
    /// pixels; subsequent allocations may overwrite them.
    pub fn deallocate(&mut self, slot: &AtlasSlot) {
        self.cpu.deallocate(slot);
    }

    /// Issue queued `Queue::write_texture` calls and clear the queue.
    /// Returns the number of glyph bitmaps written.
    pub fn flush_uploads(&mut self, queue: &Queue) -> usize {
        let bytes_per_pixel = self.kind.bytes_per_pixel();
        let count = self.pending_uploads.len();
        for upload in self.pending_uploads.drain(..) {
            queue.write_texture(
                TexelCopyTextureInfo {
                    texture: &self.texture,
                    mip_level: 0,
                    origin: wgpu::Origin3d {
                        x: u32::from(upload.slot.x),
                        y: u32::from(upload.slot.y),
                        z: 0,
                    },
                    aspect: TextureAspect::All,
                },
                &upload.data,
                TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(u32::from(upload.slot.width) * bytes_per_pixel),
                    rows_per_image: Some(u32::from(upload.slot.height)),
                },
                Extent3d {
                    width: u32::from(upload.slot.width),
                    height: u32::from(upload.slot.height),
                    depth_or_array_layers: 1,
                },
            );
        }
        count
    }

    /// Grow the atlas to `new_side`. Recreates the wgpu texture; existing
    /// glyph bitmaps are *not* preserved (the L2 cache must re-rasterise
    /// or re-upload). Returns `false` when the request is below current
    /// size.
    pub fn grow(&mut self, device: &Device, new_side: u16) -> bool {
        if !self.cpu.grow(new_side) {
            return false;
        }
        let (texture, view) = create_texture(device, self.kind, self.cpu.width());
        self.texture = texture;
        self.view = view;
        // Pending uploads referencing the old texture are now invalid; drop.
        self.pending_uploads.clear();
        true
    }

    /// Drop every CPU allocation. The GPU texture pixels remain (callers
    /// that need a clean GPU surface should rebuild via [`new`](Self::new)).
    pub fn clear(&mut self) {
        self.cpu.clear();
        self.pending_uploads.clear();
    }

    pub fn texture(&self) -> &Texture {
        &self.texture
    }

    pub fn view(&self) -> &TextureView {
        &self.view
    }

    pub fn kind(&self) -> Kind {
        self.kind
    }

    pub fn width(&self) -> u16 {
        self.cpu.width()
    }

    pub fn height(&self) -> u16 {
        self.cpu.height()
    }

    pub fn pending_uploads(&self) -> &[PendingUpload] {
        &self.pending_uploads
    }

    pub fn is_empty(&self) -> bool {
        self.cpu.is_empty()
    }
}

fn create_texture(device: &Device, kind: Kind, side: u16) -> (Texture, TextureView) {
    let texture = device.create_texture(&TextureDescriptor {
        label: Some(match kind {
            Kind::Mask => "kasane parley_text mask atlas",
            Kind::Color => "kasane parley_text color atlas",
        }),
        size: Extent3d {
            width: u32::from(side),
            height: u32::from(side),
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: TextureDimension::D2,
        format: kind.texture_format(),
        usage: TextureUsages::TEXTURE_BINDING | TextureUsages::COPY_DST,
        view_formats: &[],
    });
    let view = texture.create_view(&TextureViewDescriptor::default());
    (texture, view)
}

#[cfg(test)]
mod tests {
    //! CPU-side tests only. The wgpu integration is exercised through the
    //! SceneRenderer end-to-end smoke (Phase 9b Step 4); building a
    //! headless wgpu device here would force every test runner to find a
    //! Vulkan / Metal / DX adapter, which is too costly for a unit test.
    use super::*;

    /// Helper: build a synthetic AtlasSlot for queue-only tests.
    /// We obtain a real AllocId by going through AtlasShelf since
    /// `etagere::AllocId` lacks a public mint constructor.
    fn fake_slot(x: u16, y: u16, w: u16, h: u16) -> AtlasSlot {
        let mut shelf = AtlasShelf::new(super::super::atlas::MIN_ATLAS_SIZE);
        let real = shelf.allocate(w, h).expect("allocate");
        AtlasSlot {
            alloc_id: real.alloc_id,
            x,
            y,
            width: w,
            height: h,
        }
    }

    // The Kind enum is wgpu-free; we exercise it directly.
    #[test]
    fn kind_format_mapping() {
        assert_eq!(Kind::Mask.texture_format(), TextureFormat::R8Unorm);
        assert_eq!(Kind::Color.texture_format(), TextureFormat::Rgba8Unorm);
    }

    #[test]
    fn kind_bpp() {
        assert_eq!(Kind::Mask.bytes_per_pixel(), 1);
        assert_eq!(Kind::Color.bytes_per_pixel(), 4);
    }

    #[test]
    fn pending_upload_carries_slot_and_data() {
        // Drive PendingUpload through public construction so the field
        // semantics are pinned. We do not need a wgpu device for this.
        let slot = fake_slot(8, 16, 4, 4);
        let upload = PendingUpload {
            slot,
            data: vec![1, 2, 3, 4],
        };
        assert_eq!(upload.slot.width, 4);
        assert_eq!(upload.data.len(), 4);
    }
}
