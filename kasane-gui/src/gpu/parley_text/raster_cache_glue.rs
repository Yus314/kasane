//! Glue between [`raster_cache::AtlasOps`] and the wgpu-backed
//! [`GpuAtlasShelf`] pair (mask + color).
//!
//! ADR-031 Phase 9b Step 4c. The L2 cache no longer owns its atlases —
//! the production path holds two `GpuAtlasShelf`s on the SceneRenderer,
//! and this wrapper exposes them as a single `&mut dyn AtlasOps` that
//! the cache can call into. Tests use a CPU-only adapter that lives
//! alongside their tests; only the production path needs wgpu.
//!
//! Why a `Vec<u8>` clone in `allocate`: `GpuAtlasShelf::allocate_and_queue`
//! needs an owned bitmap to keep the queue stable across the renderer's
//! `flush_uploads` call. The cache stores its own `Vec<u8>` for re-upload
//! after device loss / atlas growth, so we accept one clone per glyph
//! *miss* — cache hits skip the rasteriser and the upload entirely.

use super::atlas::AtlasSlot;
use super::glyph_rasterizer::ContentKind;
use super::gpu_atlas::GpuAtlasShelf;
use super::raster_cache::AtlasOps;

/// Routes `(content, w, h, &data)` allocations into the matching mask
/// or color GpuAtlasShelf, and `deallocate` back to the same pair.
pub struct ParleyAtlasPair<'a> {
    pub mask: &'a mut GpuAtlasShelf,
    pub color: &'a mut GpuAtlasShelf,
}

impl AtlasOps for ParleyAtlasPair<'_> {
    fn allocate(
        &mut self,
        content: ContentKind,
        width: u16,
        height: u16,
        data: &[u8],
    ) -> Option<AtlasSlot> {
        let atlas = match content {
            ContentKind::Mask => &mut *self.mask,
            ContentKind::Color => &mut *self.color,
        };
        atlas.allocate_and_queue(width, height, data.to_vec())
    }

    fn deallocate(&mut self, content: ContentKind, slot: &AtlasSlot) {
        let atlas = match content {
            ContentKind::Mask => &mut *self.mask,
            ContentKind::Color => &mut *self.color,
        };
        atlas.deallocate(slot);
    }
}
