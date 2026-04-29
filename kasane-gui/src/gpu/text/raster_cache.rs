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

use super::atlas::AtlasSlot;
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
    /// Last frame epoch in which this entry was hit or freshly inserted.
    /// Eviction protocol refuses to drop entries with `last_used_epoch ==
    /// current_epoch` because doing so would corrupt drawables already
    /// pushed into the per-layer accumulator in the same frame: the
    /// queued upload would overwrite the slot under their feet. See
    /// [`GlyphRasterCache::bump_epoch`].
    pub last_used_epoch: u64,
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct CacheStats {
    pub hits: u32,
    pub misses: u32,
    pub atlas_evictions: u32,
    pub lru_evictions: u32,
    /// Glyphs that could not be cached because the atlas was full and the
    /// LRU eviction candidate was protected by same-frame use. The caller
    /// receives `None` from `get_or_insert`; the glyph for that frame is
    /// either rendered uncached or dropped, depending on the caller's
    /// fall-back strategy. Non-zero values indicate atlas pressure and
    /// should appear as a Service-Level Objective in
    /// [docs/performance.md](../../../../../../../docs/performance.md).
    pub dropped: u32,
}

/// Atlas operations the cache needs. Production wires this to a pair of
/// `GpuAtlasShelf`s (see `scene_renderer`). The unit tests below
/// implement it on a CPU-only `AtlasShelf` pair so they keep running
/// without wgpu. The single-trait shape (rather than two closures) keeps
/// borrow-checker bookkeeping simple inside `get_or_insert`.
pub trait AtlasOps {
    fn allocate(
        &mut self,
        content: ContentKind,
        width: u16,
        height: u16,
        data: &[u8],
    ) -> Option<AtlasSlot>;

    fn deallocate(&mut self, content: ContentKind, slot: &AtlasSlot);
}

/// L2 cache. Atlases are owned externally (by SceneRenderer) since they
/// must be `GpuAtlasShelf` instances backed by a wgpu texture. Phase 9b
/// Step 4c moved the atlases out so the cache and the GPU atlas now share
/// the same allocator state — a previous mismatch had cached slots
/// pointing into garbage GPU regions.
pub struct GlyphRasterCache {
    entries: LruCache<GlyphRasterKey, GlyphRasterEntry, FxBuildHasher>,
    stats: CacheStats,
    /// Monotonic frame counter. Bumped via [`Self::bump_epoch`] at the
    /// start of every frame; entries record this on hit/insert so the
    /// eviction loop can refuse to drop slots whose drawable is already
    /// queued in the current frame's vertex buffer.
    current_epoch: u64,
}

impl GlyphRasterCache {
    /// Build a cache with the given LRU capacity.
    pub fn new(capacity: NonZeroUsize) -> Self {
        Self {
            entries: LruCache::with_hasher(capacity, FxBuildHasher),
            stats: CacheStats::default(),
            current_epoch: 1,
        }
    }

    /// Bump the frame epoch. Call once at the start of each frame
    /// before any `get_or_insert` calls. Entries inserted or hit in
    /// the new frame become non-evictable (within the frame); entries
    /// from previous frames remain candidates for atlas-full eviction.
    pub fn bump_epoch(&mut self) {
        self.current_epoch = self.current_epoch.wrapping_add(1);
    }

    /// Current frame epoch. Exposed for diagnostics and tests.
    pub fn current_epoch(&self) -> u64 {
        self.current_epoch
    }

    /// Convenience constructor sized for a typical 80×24 viewport
    /// (~2 000 unique glyphs leaves headroom for status bar + menus).
    pub fn default_sized() -> Self {
        Self::new(NonZeroUsize::new(8192).unwrap())
    }

    /// Look up an existing entry without affecting LRU ordering. Used by
    /// callers that want to peek (e.g. dirty-region debugging).
    pub fn peek(&self, key: &GlyphRasterKey) -> Option<&GlyphRasterEntry> {
        self.entries.peek(key)
    }

    /// Look up `key`. On hit, the entry is promoted to MRU, its
    /// `last_used_epoch` is updated to the current frame, and a
    /// reference is returned.
    pub fn get(&mut self, key: &GlyphRasterKey) -> Option<&GlyphRasterEntry> {
        let epoch = self.current_epoch;
        let hit = self.entries.get_mut(key);
        if let Some(entry) = hit {
            self.stats.hits += 1;
            entry.last_used_epoch = epoch;
            return Some(&*entry);
        }
        None
    }

    /// Hit-or-miss with on-demand rasterisation.
    ///
    /// On miss: `rasterize` produces the bitmap, then `allocate` is called
    /// to obtain an atlas slot. If `allocate` returns `None` (atlas full),
    /// the LRU's oldest entry is evicted via `deallocate` and `allocate`
    /// is retried, repeating until the LRU is empty (returns `None`) or
    /// the slot is acquired.
    ///
    /// `allocate` receives `(content, width, height, &data)` and is
    /// expected to call `GpuAtlasShelf::allocate_and_queue` (or the
    /// equivalent test stub). `deallocate` mirrors the reverse.
    pub fn get_or_insert<R>(
        &mut self,
        key: GlyphRasterKey,
        atlases: &mut dyn AtlasOps,
        rasterize: R,
    ) -> Option<&GlyphRasterEntry>
    where
        R: FnOnce() -> Option<RasterizedGlyph>,
    {
        let epoch = self.current_epoch;
        // Hit path is two LRU lookups: `get_mut` to promote-to-MRU and
        // tag `last_used_epoch` (so eviction-loops below leave this slot
        // alone), then `peek` to obtain the `&V` we return. Going to a
        // single `get_mut` and reborrowing for the return value would be
        // ideal, but the current borrow checker (NLL, no Polonius) can't
        // prove the borrow ends at the early return so the eviction
        // branches below cannot reuse `self.entries`. Two lookups is a
        // 33 % reduction from the previous shape (`contains` / `get_mut`
        // / `peek`); revisit when Polonius lands.
        let hit = self
            .entries
            .get_mut(&key)
            .map(|entry| entry.last_used_epoch = epoch)
            .is_some();
        if hit {
            self.stats.hits += 1;
            return self.entries.peek(&key);
        }
        let raster = rasterize()?;
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

        // Atlas allocation with eviction restricted to entries from
        // earlier frames. Same-frame entries already have drawables
        // pointing at their slots; deallocating + reusing those slots
        // would let the queued upload corrupt them mid-frame.
        let slot = loop {
            if let Some(slot) = atlases.allocate(content, width, height, &data) {
                break slot;
            }
            let evictable = self
                .entries
                .peek_lru()
                .map(|(_, v)| v.last_used_epoch < epoch)
                .unwrap_or(false);
            if !evictable {
                // No safe candidate to evict — accept the loss for
                // this glyph rather than corrupt an in-flight one.
                if self.stats.dropped == 0 {
                    tracing::warn!(
                        target: "kasane_gui::glyph_atlas",
                        cache_len = self.entries.len(),
                        cache_cap = self.entries.cap().get(),
                        epoch,
                        "GPU glyph atlas pressure: cannot allocate slot and \
                         no LRU entry is evictable (all candidates from this \
                         frame). Glyph dropped this frame; increase atlas \
                         dimensions or LRU capacity if this recurs.",
                    );
                }
                self.stats.dropped += 1;
                return None;
            }
            let (_, evicted) = self.entries.pop_lru().expect("peek succeeded");
            atlases.deallocate(evicted.content, &evicted.atlas_slot);
            self.stats.atlas_evictions += 1;
        };

        let entry = GlyphRasterEntry {
            width,
            height,
            left,
            top,
            content,
            atlas_slot: slot,
            data,
            last_used_epoch: epoch,
        };

        // LRU push may evict the oldest entry; same-frame protection
        // also applies here. If the oldest is from this frame, skip
        // the insertion (the caller still gets the drawable for *this*
        // glyph — we just don't keep it cached).
        let oldest_in_frame = self
            .entries
            .peek_lru()
            .map(|(_, v)| v.last_used_epoch == epoch)
            .unwrap_or(false);
        if self.entries.len() >= self.entries.cap().get() && oldest_in_frame {
            // Cache full of in-frame entries: deallocate the slot we
            // just allocated and return None so the caller skips the
            // glyph (rather than corrupting a sibling).
            atlases.deallocate(content, &slot);
            if self.stats.dropped == 0 {
                tracing::warn!(
                    target: "kasane_gui::glyph_atlas",
                    cache_len = self.entries.len(),
                    cache_cap = self.entries.cap().get(),
                    epoch,
                    "GPU glyph atlas pressure: LRU is saturated with \
                     entries from the current frame and cannot accept a \
                     new insertion. Glyph dropped this frame; increase \
                     LRU capacity if this recurs.",
                );
            }
            self.stats.dropped += 1;
            return None;
        }
        if let Some((_evicted_key, evicted)) = self.entries.push(key, entry) {
            atlases.deallocate(evicted.content, &evicted.atlas_slot);
            self.stats.lru_evictions += 1;
        }

        self.entries.peek(&key)
    }

    /// Drop every cached entry. Caller is responsible for clearing the
    /// atlases separately (matching the cache's slot ownership model).
    pub fn invalidate_all(&mut self) {
        self.entries.clear();
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
}

impl Default for GlyphRasterCache {
    fn default() -> Self {
        Self::default_sized()
    }
}

#[cfg(test)]
mod tests {
    use super::super::atlas::AtlasShelf;
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

    /// CPU-only atlas pair used by these tests in lieu of the wgpu-backed
    /// `GpuAtlasShelf`. Mirrors the production allocate/deallocate
    /// semantics that the cache cares about.
    struct TestAtlases {
        mask: AtlasShelf,
        color: AtlasShelf,
    }

    impl TestAtlases {
        fn new(side: u16) -> Self {
            Self {
                mask: AtlasShelf::new(side),
                color: AtlasShelf::new(side),
            }
        }
    }

    impl AtlasOps for TestAtlases {
        fn allocate(
            &mut self,
            content: ContentKind,
            w: u16,
            h: u16,
            _data: &[u8],
        ) -> Option<AtlasSlot> {
            let atlas = match content {
                ContentKind::Mask => &mut self.mask,
                ContentKind::Color => &mut self.color,
            };
            atlas.allocate(w, h)
        }

        fn deallocate(&mut self, content: ContentKind, slot: &AtlasSlot) {
            let atlas = match content {
                ContentKind::Mask => &mut self.mask,
                ContentKind::Color => &mut self.color,
            };
            atlas.deallocate(slot);
        }
    }

    fn insert(
        cache: &mut GlyphRasterCache,
        atlases: &mut TestAtlases,
        k: GlyphRasterKey,
        raster: RasterizedGlyph,
    ) -> bool {
        cache.get_or_insert(k, atlases, || Some(raster)).is_some()
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
        let mut cache = GlyphRasterCache::new(NonZeroUsize::new(16).unwrap());
        let mut at = TestAtlases::new(256);
        let k = key(1, 65);
        assert!(cache.get(&k).is_none());
        assert!(insert(&mut cache, &mut at, k, mask_raster(8, 16)));
        let stats1 = cache.take_stats();
        assert_eq!(stats1.misses, 1);
        assert_eq!(stats1.hits, 0);

        let entry = cache.get(&k).expect("hit");
        assert_eq!(entry.width, 8);
        assert_eq!(entry.height, 16);
        let stats2 = cache.take_stats();
        assert_eq!(stats2.hits, 1);
    }

    #[test]
    fn distinct_keys_share_cache() {
        let mut cache = GlyphRasterCache::new(NonZeroUsize::new(16).unwrap());
        let mut at = TestAtlases::new(256);
        insert(&mut cache, &mut at, key(1, 65), mask_raster(8, 16));
        insert(&mut cache, &mut at, key(1, 66), mask_raster(8, 16));
        insert(&mut cache, &mut at, key(2, 65), mask_raster(8, 16));
        assert_eq!(cache.len(), 3);
    }

    #[test]
    fn lru_eviction_releases_atlas_slot() {
        let mut cache = GlyphRasterCache::new(NonZeroUsize::new(2).unwrap());
        let mut at = TestAtlases::new(256);
        // Same-frame eviction is disallowed by design (the entry's
        // drawable would be rendered with the new bitmap). Bump the
        // epoch between inserts to simulate cross-frame evictions —
        // the only kind the production cache performs.
        insert(&mut cache, &mut at, key(1, 1), mask_raster(8, 16));
        cache.bump_epoch();
        insert(&mut cache, &mut at, key(1, 2), mask_raster(8, 16));
        cache.bump_epoch();
        insert(&mut cache, &mut at, key(1, 3), mask_raster(8, 16));
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
        let mut cache = GlyphRasterCache::new(NonZeroUsize::new(1024).unwrap());
        let mut at = TestAtlases::new(256);
        // Each glyph is in its own "frame" so atlas-full eviction can
        // run; otherwise the same-frame guard refuses to evict and
        // simply skips the new glyph.
        for i in 0..30u16 {
            insert(&mut cache, &mut at, key(1, i), mask_raster(64, 64));
            cache.bump_epoch();
        }
        let stats = cache.take_stats();
        assert!(
            stats.atlas_evictions > 0,
            "expected atlas evictions, got {stats:?}"
        );
        assert!(cache.len() <= 30);
    }

    #[test]
    fn same_frame_eviction_refused() {
        // All inserts share the same epoch (no bump_epoch between
        // them). Once the atlas runs out, the new insertion should be
        // skipped rather than evict an in-flight entry.
        let mut cache = GlyphRasterCache::new(NonZeroUsize::new(1024).unwrap());
        let mut at = TestAtlases::new(256);
        let mut first_failure = None;
        for i in 0..30u16 {
            let ok = insert(&mut cache, &mut at, key(1, i), mask_raster(64, 64));
            if !ok && first_failure.is_none() {
                first_failure = Some(i);
            }
        }
        assert!(
            first_failure.is_some(),
            "atlas should fill before all 30 glyphs are inserted"
        );
        let stats = cache.take_stats();
        assert_eq!(
            stats.atlas_evictions, 0,
            "same-frame eviction must be refused, got {stats:?}"
        );
        assert!(
            stats.dropped > 0,
            "refused-eviction path must increment dropped counter, got {stats:?}"
        );
    }

    #[test]
    fn dropped_counter_zero_on_steady_state() {
        // Exercises the happy path: a small cache with plenty of atlas
        // headroom should never report dropped glyphs.
        let mut cache = GlyphRasterCache::new(NonZeroUsize::new(16).unwrap());
        let mut at = TestAtlases::new(256);
        for i in 0..8u16 {
            cache.bump_epoch();
            assert!(insert(&mut cache, &mut at, key(1, i), mask_raster(16, 16)));
        }
        let stats = cache.take_stats();
        assert_eq!(
            stats.dropped, 0,
            "no atlas pressure expected, got {stats:?}"
        );
    }

    #[test]
    fn color_and_mask_use_separate_atlases() {
        let mut cache = GlyphRasterCache::new(NonZeroUsize::new(16).unwrap());
        let mut at = TestAtlases::new(256);
        assert!(insert(&mut cache, &mut at, key(1, 1), mask_raster(16, 16)));
        assert!(insert(&mut cache, &mut at, key(1, 2), color_raster(16, 16)));
        assert_eq!(cache.len(), 2);
    }

    #[test]
    fn invalidate_all_clears_entries() {
        let mut cache = GlyphRasterCache::new(NonZeroUsize::new(16).unwrap());
        let mut at = TestAtlases::new(256);
        insert(&mut cache, &mut at, key(1, 1), mask_raster(16, 16));
        insert(&mut cache, &mut at, key(1, 2), color_raster(16, 16));
        assert_eq!(cache.len(), 2);
        cache.invalidate_all();
        assert!(cache.is_empty());
    }

    #[test]
    fn get_or_insert_short_circuits_on_hit() {
        let mut cache = GlyphRasterCache::new(NonZeroUsize::new(16).unwrap());
        let mut at = TestAtlases::new(256);
        let k = key(1, 1);
        insert(&mut cache, &mut at, k, mask_raster(8, 16));
        let _ = cache.take_stats();

        let mut compute_called = false;
        let _ = cache.get_or_insert(k, &mut at, || {
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
        let mut cache = GlyphRasterCache::new(NonZeroUsize::new(16).unwrap());
        let mut at = TestAtlases::new(256);
        let mut count = 0;
        let _ = cache.get_or_insert(key(1, 1), &mut at, || {
            count += 1;
            Some(mask_raster(8, 16))
        });
        assert_eq!(count, 1);
        assert_eq!(cache.len(), 1);
    }

    #[test]
    fn get_or_insert_propagates_rasterize_failure() {
        let mut cache = GlyphRasterCache::new(NonZeroUsize::new(16).unwrap());
        let mut at = TestAtlases::new(256);
        let result = cache.get_or_insert(key(1, 1), &mut at, || None);
        assert!(result.is_none());
        assert!(cache.is_empty());
    }
}
