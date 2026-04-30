//! L3 atlas — etagere-backed CPU-side allocation tracker.
//!
//! Decides *where* each glyph bitmap lives. The wgpu-aware
//! [`GpuAtlasShelf`](super::gpu_atlas::GpuAtlasShelf) wraps this with a
//! GPU texture; here we keep just the allocator so unit tests can run
//! without a GPU adapter.
//!
//! Two atlases coexist in production: one for [`ContentKind::Mask`]
//! (single-channel `R8`) and one for [`ContentKind::Color`] (`RGBA8`).
//! Both use the same allocator; the
//! [`GlyphRasterCache`](super::raster_cache::GlyphRasterCache) picks the
//! atlas to allocate into based on the rasterised glyph's content kind.
//!
//! Eviction protocol: this atlas does not have an LRU of its own. When
//! the L2 cache evicts a glyph it calls [`AtlasShelf::deallocate`] to
//! release the slot. When the atlas runs out of space, the L2 cache
//! either grows the atlas via [`AtlasShelf::grow`] or falls back to
//! "rasterise without caching" for that frame.

use etagere::euclid::Size2D;
use etagere::{AllocId, BucketedAtlasAllocator, size2};

/// A region within an atlas, returned by [`AtlasShelf::allocate`] and
/// surrendered to [`AtlasShelf::deallocate`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AtlasSlot {
    pub alloc_id: AllocId,
    pub x: u16,
    pub y: u16,
    pub width: u16,
    pub height: u16,
}

/// Minimum atlas dimension. Smaller atlases waste fixed bookkeeping
/// overhead and almost always need to grow on the first emoji.
pub const MIN_ATLAS_SIZE: u16 = 256;

/// Default starting atlas dimension. Sized to comfortably fit a full
/// frame's worth of glyphs at HiDPI plus headroom for the command
/// palette and status bar.
///
/// Budget at 40 px font (HiDPI mid-size): ~24×40 ≈ 960 px² per glyph.
/// 2048² = 4 194 304 px² → ~4 350 glyph slots. Smaller defaults
/// (e.g. 512²) cause silent skip when the menu opens: only narrow
/// glyphs (`-`, `.`, `_`) fit the leftover slot pattern, producing
/// the "menu items collapse to a row of dashes" symptom.
pub const DEFAULT_ATLAS_SIZE: u16 = 2048;

/// Maximum atlas dimension. wgpu requires textures ≤ device max (typically
/// 8192 or 16384); we cap at 4096 to stay below the smaller WebGPU
/// `maxTextureDimension2D` baseline.
pub const MAX_ATLAS_SIZE: u16 = 4096;

/// One CPU-side atlas: an [`etagere::BucketedAtlasAllocator`] plus its
/// dimensions. The GPU texture lives in
/// [`GpuAtlasShelf`](super::gpu_atlas::GpuAtlasShelf), which wraps this.
pub struct AtlasShelf {
    allocator: BucketedAtlasAllocator,
    width: u16,
    height: u16,
}

impl AtlasShelf {
    /// Create an atlas of the given square size. The size is clamped into
    /// `[MIN_ATLAS_SIZE, MAX_ATLAS_SIZE]`.
    pub fn new(side: u16) -> Self {
        let side = side.clamp(MIN_ATLAS_SIZE, MAX_ATLAS_SIZE);
        Self {
            allocator: BucketedAtlasAllocator::new(size2(i32::from(side), i32::from(side))),
            width: side,
            height: side,
        }
    }

    /// Try to allocate a `width × height` region. Returns `None` when the
    /// atlas is too full; the caller may then grow or evict.
    pub fn allocate(&mut self, width: u16, height: u16) -> Option<AtlasSlot> {
        if width == 0 || height == 0 {
            return None;
        }
        let alloc = self
            .allocator
            .allocate(size2(i32::from(width), i32::from(height)))?;
        Some(AtlasSlot {
            alloc_id: alloc.id,
            x: alloc.rectangle.min.x as u16,
            y: alloc.rectangle.min.y as u16,
            width,
            height,
        })
    }

    /// Release a slot allocated through [`Self::allocate`].
    pub fn deallocate(&mut self, slot: &AtlasSlot) {
        self.allocator.deallocate(slot.alloc_id);
    }

    /// Grow the atlas to the given side length. Returns `false` when the
    /// caller asked for a smaller-or-equal size; clamps to `MAX_ATLAS_SIZE`
    /// silently otherwise.
    pub fn grow(&mut self, new_side: u16) -> bool {
        let new_side = new_side.clamp(MIN_ATLAS_SIZE, MAX_ATLAS_SIZE);
        if new_side <= self.width {
            return false;
        }
        let new_size: Size2D<i32, _> = size2(i32::from(new_side), i32::from(new_side));
        self.allocator.grow(new_size);
        self.width = new_side;
        self.height = new_side;
        true
    }

    /// Drop every allocation. Used on font/metrics changes that invalidate
    /// every cached glyph bitmap.
    pub fn clear(&mut self) {
        self.allocator.clear();
    }

    /// Whether any slot is currently allocated.
    pub fn is_empty(&self) -> bool {
        self.allocator.is_empty()
    }

    pub fn width(&self) -> u16 {
        self.width
    }

    pub fn height(&self) -> u16 {
        self.height
    }
}

impl Default for AtlasShelf {
    fn default() -> Self {
        Self::new(DEFAULT_ATLAS_SIZE)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn allocates_then_deallocates() {
        let mut atlas = AtlasShelf::new(256);
        let slot = atlas.allocate(16, 32).expect("allocate");
        assert_eq!(slot.width, 16);
        assert_eq!(slot.height, 32);
        assert!(slot.x as u32 + slot.width as u32 <= atlas.width as u32);
        assert!(slot.y as u32 + slot.height as u32 <= atlas.height as u32);
        assert!(!atlas.is_empty());

        atlas.deallocate(&slot);
        // After deallocation the bookkeeping reports empty (single allocation).
        assert!(atlas.is_empty());
    }

    #[test]
    fn rejects_zero_dimension() {
        let mut atlas = AtlasShelf::new(256);
        assert!(atlas.allocate(0, 16).is_none());
        assert!(atlas.allocate(16, 0).is_none());
    }

    #[test]
    fn fills_to_capacity_then_returns_none() {
        let mut atlas = AtlasShelf::new(MIN_ATLAS_SIZE);
        // Allocate 16x16 glyphs until we run out. 256/16 = 16 per row,
        // 16 rows = 256 max; any reasonable count short of that should fit.
        let mut slots = Vec::new();
        for _ in 0..200 {
            if let Some(s) = atlas.allocate(16, 16) {
                slots.push(s);
            }
        }
        assert!(!slots.is_empty(), "expected to allocate at least some");
        // Atlas should refuse a giant allocation.
        assert!(atlas.allocate(MIN_ATLAS_SIZE, MIN_ATLAS_SIZE).is_none());
    }

    #[test]
    fn grow_extends_capacity() {
        let mut atlas = AtlasShelf::new(MIN_ATLAS_SIZE);
        assert_eq!(atlas.width(), MIN_ATLAS_SIZE);
        assert!(atlas.grow(512), "grow should succeed");
        assert_eq!(atlas.width(), 512);
        assert_eq!(atlas.height(), 512);
        // Grow rejects shrinking.
        assert!(!atlas.grow(256));
    }

    #[test]
    fn clear_drops_allocations() {
        let mut atlas = AtlasShelf::new(256);
        let _s1 = atlas.allocate(8, 8).unwrap();
        let _s2 = atlas.allocate(8, 8).unwrap();
        assert!(!atlas.is_empty());
        atlas.clear();
        assert!(atlas.is_empty());
    }

    #[test]
    fn size_clamping() {
        let too_small = AtlasShelf::new(16);
        assert_eq!(too_small.width(), MIN_ATLAS_SIZE);
        // Max is 4096; create one at 4096 and verify it sticks.
        let max = AtlasShelf::new(MAX_ATLAS_SIZE);
        assert_eq!(max.width(), MAX_ATLAS_SIZE);
    }
}
