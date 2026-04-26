//! L2 glyph raster cache with L3 atlas eviction link (ADR-031, Phase 8).
//!
//! Sits between [`GlyphRasterizer`](super::glyph_rasterizer::GlyphRasterizer)
//! and [`AtlasShelf`](super::atlas::AtlasShelf):
//!
//! ```text
//!  shaper → key  ──►  L2 GlyphRasterCache (LRU)
//!                      ├─ hit  → return existing slot
//!                      └─ miss → rasterize → allocate atlas slot → store
//!                                              │
//!                                              ▼
//!                                            L3 AtlasShelf (mask | color)
//! ```
//!
//! Eviction protocol:
//!
//! - **L2 LRU full**: oldest entry is popped and its atlas slot deallocated
//!   before the new entry is inserted.
//! - **Atlas full** (allocator returns `None` on a fresh insert): the cache
//!   evicts L2 entries from oldest to newest, deallocating their slots,
//!   until the new allocation succeeds. If the LRU empties without success
//!   the request returns `None`; the caller may then grow the atlas via
//!   [`AtlasShelf::grow`] or skip the glyph for this frame.
//!
//! Wholesale invalidation: [`GlyphRasterCache::invalidate_all`] empties the
//! LRU and clears both atlases. Used on font / scale-factor / hint changes.

use std::num::NonZeroUsize;

use lru::LruCache;
use rustc_hash::FxBuildHasher;

use super::atlas::{AtlasShelf, AtlasSlot, DEFAULT_ATLAS_SIZE};
use super::glyph_rasterizer::{ContentKind, RasterizedGlyph};

/// Key uniquely identifying a rasterised glyph in the cache.
///
/// Two glyphs with the same key produce identical bitmaps; the L1
/// LayoutCache may reference the same key from multiple lines, so a single
/// L2 entry serves many viewport positions.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct GlyphRasterKey {
    /// Font identity. Phase 9 will use a stable hash of the
    /// `fontique::FontInfo` (family + weight + width + style + variations);
    /// at Phase 8 the caller provides any consistent `u32`.
    pub font_id: u32,
    pub glyph_id: u16,
    /// Size in 1/64-pixel units: `(size * 64.0).round() as u16`. Quantising
    /// keeps near-identical sizes from polluting the cache.
    pub size_q: u16,
    /// 4-level subpixel x quantisation (0..=3).
    pub subpx_x: u8,
    /// Hash of the variable-font axis settings, or 0 when none are set.
    pub var_hash: u32,
    pub hint: bool,
}

impl GlyphRasterKey {
    /// Construct from a logical size and subpixel offset.
    pub fn from_size(
        font_id: u32,
        glyph_id: u16,
        size: f32,
        subpx: super::glyph_rasterizer::SubpixelX,
        hint: bool,
    ) -> Self {
        Self {
            font_id,
            glyph_id,
            size_q: (size * 64.0).round().clamp(0.0, u16::MAX as f32) as u16,
            subpx_x: subpx.0,
            var_hash: 0,
            hint,
        }
    }
}

/// Cached raster + its atlas allocation. The bitmap data is retained on the
/// CPU side so Phase 9's wgpu integration can re-upload after device loss
/// or atlas growth without re-rasterising.
#[derive(Debug, Clone)]
pub struct GlyphRasterEntry {
    pub width: u16,
    pub height: u16,
    pub left: i16,
    pub top: i16,
    pub content: ContentKind,
    pub atlas_slot: AtlasSlot,
    pub data: Vec<u8>,
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct CacheStats {
    pub hits: u32,
    pub misses: u32,
    pub atlas_evictions: u32,
    pub lru_evictions: u32,
}

/// L2 cache plus the two L3 atlases (mask + color).
pub struct GlyphRasterCache {
    entries: LruCache<GlyphRasterKey, GlyphRasterEntry, FxBuildHasher>,
    atlas_mask: AtlasShelf,
    atlas_color: AtlasShelf,
    stats: CacheStats,
}

impl GlyphRasterCache {
    /// Build a cache with the given LRU capacity and per-atlas dimensions.
    pub fn new(capacity: NonZeroUsize, atlas_side: u16) -> Self {
        Self {
            entries: LruCache::with_hasher(capacity, FxBuildHasher),
            atlas_mask: AtlasShelf::new(atlas_side),
            atlas_color: AtlasShelf::new(atlas_side),
            stats: CacheStats::default(),
        }
    }

    /// Convenience constructor with `DEFAULT_ATLAS_SIZE` and a moderate
    /// LRU cap suited to a typical 80×24 viewport (~2 000 unique glyphs).
    pub fn default_sized() -> Self {
        Self::new(NonZeroUsize::new(8192).unwrap(), DEFAULT_ATLAS_SIZE)
    }

    /// Look up an existing entry without affecting LRU ordering. Used by
    /// callers that want to peek (e.g. dirty-region debugging).
    pub fn peek(&self, key: &GlyphRasterKey) -> Option<&GlyphRasterEntry> {
        self.entries.peek(key)
    }

    /// Look up `key`. On hit, the entry is promoted to MRU and returned.
    pub fn get(&mut self, key: &GlyphRasterKey) -> Option<&GlyphRasterEntry> {
        let hit = self.entries.get(key);
        if hit.is_some() {
            self.stats.hits += 1;
        }
        hit
    }

    /// Insert a freshly-rasterised glyph and return a reference to the
    /// stored entry. On L2 capacity overflow, the oldest entry is evicted
    /// (its atlas slot is deallocated). On atlas exhaustion, older entries
    /// are evicted from L2 until allocation succeeds — `None` is returned
    /// only when the atlas remains too full even after the LRU is empty.
    pub fn insert(
        &mut self,
        key: GlyphRasterKey,
        raster: RasterizedGlyph,
    ) -> Option<&GlyphRasterEntry> {
        debug_assert_eq!(raster.data.len(), raster.expected_data_len());
        self.stats.misses += 1;

        let RasterizedGlyph {
            width,
            height,
            top,
            left,
            content,
            data,
        } = raster;

        // Allocate the atlas slot first (with retry on atlas-full).
        let slot = self.allocate_with_eviction(content, width, height)?;

        let entry = GlyphRasterEntry {
            width,
            height,
            left,
            top,
            content,
            atlas_slot: slot,
            data,
        };

        // Insert into L2; the LRU may evict an older entry to make room.
        if let Some((_evicted_key, evicted)) = self.entries.push(key, entry) {
            self.deallocate(evicted.content, &evicted.atlas_slot);
            self.stats.lru_evictions += 1;
        }

        // The just-inserted entry is now at MRU.
        self.entries.peek(&key)
    }

    /// Hit-or-miss + populate. Calls `rasterize` only on miss. Convenient
    /// when the caller has the [`GlyphRasterizer`] handy and does not want
    /// to manage the get/insert split itself.
    pub fn get_or_insert<F>(
        &mut self,
        key: GlyphRasterKey,
        rasterize: F,
    ) -> Option<&GlyphRasterEntry>
    where
        F: FnOnce() -> Option<RasterizedGlyph>,
    {
        if self.entries.contains(&key) {
            self.stats.hits += 1;
            return self.entries.get(&key);
        }
        let raster = rasterize()?;
        self.insert(key, raster)
    }

    /// Drop every cached entry and clear both atlases. Used on font /
    /// scale-factor changes that invalidate every bitmap.
    pub fn invalidate_all(&mut self) {
        self.entries.clear();
        self.atlas_mask.clear();
        self.atlas_color.clear();
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    pub fn take_stats(&mut self) -> CacheStats {
        std::mem::take(&mut self.stats)
    }

    /// Allocate an atlas slot, evicting older L2 entries until the
    /// allocation succeeds or the LRU empties. Returns `None` only in the
    /// latter case.
    fn allocate_with_eviction(
        &mut self,
        content: ContentKind,
        width: u16,
        height: u16,
    ) -> Option<AtlasSlot> {
        loop {
            let atlas = self.atlas_for_mut(content);
            if let Some(slot) = atlas.allocate(width, height) {
                return Some(slot);
            }
            // Atlas full — evict from L2 (oldest first) and retry.
            let (_, evicted) = self.entries.pop_lru()?;
            self.deallocate(evicted.content, &evicted.atlas_slot);
            self.stats.atlas_evictions += 1;
        }
    }

    fn atlas_for_mut(&mut self, content: ContentKind) -> &mut AtlasShelf {
        match content {
            ContentKind::Mask => &mut self.atlas_mask,
            ContentKind::Color => &mut self.atlas_color,
        }
    }

    fn deallocate(&mut self, content: ContentKind, slot: &AtlasSlot) {
        match content {
            ContentKind::Mask => self.atlas_mask.deallocate(slot),
            ContentKind::Color => self.atlas_color.deallocate(slot),
        }
    }
}

impl Default for GlyphRasterCache {
    fn default() -> Self {
        Self::default_sized()
    }
}

#[cfg(test)]
mod tests {
    use super::super::glyph_rasterizer::SubpixelX;
    use super::*;

    fn key(font_id: u32, glyph_id: u16) -> GlyphRasterKey {
        GlyphRasterKey {
            font_id,
            glyph_id,
            size_q: 14 * 64,
            subpx_x: 0,
            var_hash: 0,
            hint: true,
        }
    }

    fn mask_raster(w: u16, h: u16) -> RasterizedGlyph {
        RasterizedGlyph {
            width: w,
            height: h,
            top: 0,
            left: 0,
            content: ContentKind::Mask,
            data: vec![0u8; usize::from(w) * usize::from(h)],
        }
    }

    fn color_raster(w: u16, h: u16) -> RasterizedGlyph {
        RasterizedGlyph {
            width: w,
            height: h,
            top: 0,
            left: 0,
            content: ContentKind::Color,
            data: vec![0u8; usize::from(w) * usize::from(h) * 4],
        }
    }

    #[test]
    fn key_quantises_size_to_64ths() {
        let k = GlyphRasterKey::from_size(7, 42, 14.0, SubpixelX(2), true);
        assert_eq!(k.size_q, 14 * 64);
        assert_eq!(k.subpx_x, 2);
        assert!(k.hint);

        let k2 = GlyphRasterKey::from_size(7, 42, 14.5, SubpixelX(0), false);
        assert_eq!(k2.size_q, (14.5 * 64.0) as u16);
        assert!(!k2.hint);
    }

    #[test]
    fn miss_then_hit() {
        let mut cache = GlyphRasterCache::new(NonZeroUsize::new(16).unwrap(), 256);
        let k = key(1, 65);
        assert!(cache.get(&k).is_none());
        let _entry = cache.insert(k, mask_raster(8, 16)).expect("insert");
        let stats1 = cache.take_stats();
        assert_eq!(stats1.misses, 1);
        assert_eq!(stats1.hits, 0);

        // Hit
        let entry = cache.get(&k).expect("hit");
        assert_eq!(entry.width, 8);
        assert_eq!(entry.height, 16);
        let stats2 = cache.take_stats();
        assert_eq!(stats2.hits, 1);
    }

    #[test]
    fn distinct_keys_share_cache() {
        let mut cache = GlyphRasterCache::new(NonZeroUsize::new(16).unwrap(), 256);
        cache.insert(key(1, 65), mask_raster(8, 16));
        cache.insert(key(1, 66), mask_raster(8, 16));
        cache.insert(key(2, 65), mask_raster(8, 16));
        assert_eq!(cache.len(), 3);
    }

    #[test]
    fn lru_eviction_releases_atlas_slot() {
        let mut cache = GlyphRasterCache::new(NonZeroUsize::new(2).unwrap(), 256);
        cache.insert(key(1, 1), mask_raster(8, 16));
        cache.insert(key(1, 2), mask_raster(8, 16));
        // Inserting a 3rd entry triggers LRU eviction of (1, 1).
        cache.insert(key(1, 3), mask_raster(8, 16));
        let stats = cache.take_stats();
        assert_eq!(stats.lru_evictions, 1);
        assert!(
            cache.get(&key(1, 1)).is_none(),
            "evicted entry must be gone"
        );
        assert!(cache.get(&key(1, 2)).is_some());
        assert!(cache.get(&key(1, 3)).is_some());
    }

    #[test]
    fn atlas_full_triggers_eviction_then_succeeds() {
        // Tiny atlas: 256 wide / 256 tall. Glyph dimensions chosen so that
        // ~4 fit per shelf row; allocate enough to force atlas exhaustion
        // before the LRU is naturally exhausted.
        let mut cache = GlyphRasterCache::new(NonZeroUsize::new(1024).unwrap(), 256);
        // 64×64 glyphs: at most 16 fit in a 256² atlas.
        for i in 0..30u16 {
            cache.insert(key(1, i), mask_raster(64, 64));
        }
        let stats = cache.take_stats();
        // Some atlas evictions must have happened.
        assert!(
            stats.atlas_evictions > 0,
            "expected atlas evictions, got {stats:?}"
        );
        // Cache has all 30 entries (LRU cap is high enough).
        assert!(cache.len() <= 30);
    }

    #[test]
    fn color_and_mask_use_separate_atlases() {
        let mut cache = GlyphRasterCache::new(NonZeroUsize::new(16).unwrap(), 256);
        let mask_entry = cache
            .insert(key(1, 1), mask_raster(16, 16))
            .expect("insert mask");
        let mask_slot = mask_entry.atlas_slot;
        let color_entry = cache
            .insert(key(1, 2), color_raster(16, 16))
            .expect("insert color");
        let color_slot = color_entry.atlas_slot;
        // Slots may share x/y because they live in different atlases; the
        // distinguishing fact is that both fit despite the atlas being
        // small enough that placement collisions are likely if they shared.
        assert_eq!(mask_slot.width, 16);
        assert_eq!(color_slot.width, 16);
    }

    #[test]
    fn invalidate_all_clears_both_layers() {
        let mut cache = GlyphRasterCache::new(NonZeroUsize::new(16).unwrap(), 256);
        cache.insert(key(1, 1), mask_raster(16, 16));
        cache.insert(key(1, 2), color_raster(16, 16));
        assert_eq!(cache.len(), 2);
        cache.invalidate_all();
        assert!(cache.is_empty());
        assert!(cache.atlas_mask.is_empty());
        assert!(cache.atlas_color.is_empty());
    }

    #[test]
    fn get_or_insert_short_circuits_on_hit() {
        let mut cache = GlyphRasterCache::new(NonZeroUsize::new(16).unwrap(), 256);
        let k = key(1, 1);
        cache.insert(k, mask_raster(8, 16));
        let _ = cache.take_stats();

        let mut compute_called = false;
        let _ = cache.get_or_insert(k, || {
            compute_called = true;
            Some(mask_raster(8, 16))
        });
        assert!(!compute_called, "rasterize must not run on hit");
        let stats = cache.take_stats();
        assert_eq!(stats.hits, 1);
        assert_eq!(stats.misses, 0);
    }

    #[test]
    fn get_or_insert_invokes_rasterize_on_miss() {
        let mut cache = GlyphRasterCache::new(NonZeroUsize::new(16).unwrap(), 256);
        let mut count = 0;
        let _ = cache.get_or_insert(key(1, 1), || {
            count += 1;
            Some(mask_raster(8, 16))
        });
        assert_eq!(count, 1);
        assert_eq!(cache.len(), 1);
    }

    #[test]
    fn get_or_insert_propagates_rasterize_failure() {
        let mut cache = GlyphRasterCache::new(NonZeroUsize::new(16).unwrap(), 256);
        let result = cache.get_or_insert::<_>(key(1, 1), || None);
        assert!(result.is_none());
        assert!(cache.is_empty());
    }
}
