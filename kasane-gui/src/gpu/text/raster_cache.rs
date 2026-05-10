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
//! Pressure relief protocol when an atlas allocation fails:
//!
//! 1. **Grow first** — call [`AtlasOps::try_grow`] to double the atlas
//!    side up to its backend maximum. CPU slot coordinates survive a
//!    grow, so the cache walks its existing entries and re-queues their
//!    bitmap data via [`AtlasOps::reupload`]; cached glyphs reach the
//!    new GPU texture without re-rasterising.
//! 2. **Then evict** — once the atlas is at its maximum size, fall back
//!    to LRU eviction, restricted to entries whose `last_used_epoch <
//!    current_epoch` so a same-frame entry's drawable is never
//!    invalidated mid-frame.
//! 3. **Drop** — if neither grow nor cross-frame eviction is possible,
//!    return `None`. The caller renders the glyph uncached or skips it.
//!    The drop is recorded under [`CacheStats::dropped`] and (rate-
//!    limited) traced via the `kasane_gui::glyph_atlas` target so
//!    operators can size atlases / LRU caps from the log alone.
//!
//! Wholesale invalidation: [`GlyphRasterCache::invalidate_all`] empties
//! the LRU and clears both atlases. Used on font / scale-factor / hint
//! changes.

use std::num::NonZeroUsize;
use std::time::{Duration, Instant};

use lru::LruCache;
use rustc_hash::FxBuildHasher;

use super::atlas::MAX_ATLAS_SIZE;

/// Minimum interval between glyph-atlas pressure warnings emitted from a
/// single [`GlyphRasterCache`]. The warning carries a "suppressed since
/// last warning" count so high-frequency drops still surface, just at a
/// rate that doesn't flood the log.
const PRESSURE_WARN_MIN_INTERVAL: Duration = Duration::from_secs(5);

/// Rate-limit tracker for atlas-pressure log lines. Replaces the prior
/// `stats.dropped == 0` gate, which gave at most one warning per cache
/// instance — and since `stats` is not reset in production code, recurring
/// pressure was effectively invisible. This gate emits at most once per
/// [`PRESSURE_WARN_MIN_INTERVAL`] and reports how many drops were
/// suppressed during the silent window so operators can size atlases /
/// LRU caps from the trace alone.
#[derive(Debug, Default)]
struct PressureWarnGate {
    last_warn: Option<Instant>,
    suppressed_since_last: u32,
}

impl PressureWarnGate {
    /// Returns `Some(suppressed_count)` when the caller should emit a
    /// warning; `suppressed_count` is the number of additional drops
    /// that occurred since the last emitted warning (0 on the first
    /// drop). Returns `None` when the rate limit applies; the caller
    /// must treat the drop as silent (the suppressed count is updated
    /// internally).
    fn admit(&mut self, now: Instant) -> Option<u32> {
        let due = self
            .last_warn
            .is_none_or(|t| now.duration_since(t) >= PRESSURE_WARN_MIN_INTERVAL);
        if due {
            let n = self.suppressed_since_last;
            self.suppressed_since_last = 0;
            self.last_warn = Some(now);
            Some(n)
        } else {
            self.suppressed_since_last = self.suppressed_since_last.saturating_add(1);
            None
        }
    }
}

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
    /// Times an atlas was grown to relieve pressure. A non-trivial
    /// number means glyph workload exceeded the default atlas size; if
    /// it stays high after the workload stabilises, consider raising
    /// the default size in `atlas.rs::DEFAULT_ATLAS_SIZE`.
    pub atlas_grows: u32,
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

    /// Attempt to grow the atlas backing `content`. Returns the new side
    /// length on success, or `None` when the atlas is already at maximum
    /// size or the implementation does not support growing.
    ///
    /// On success, **all previously allocated slots remain
    /// coordinate-valid** (the underlying CPU allocator preserves
    /// layout), but the GPU pixel data is invalidated — the caller is
    /// expected to walk every cached entry of the same `content` kind
    /// and call [`AtlasOps::reupload`] so the new GPU texture has their
    /// bitmap data again.
    fn try_grow(&mut self, content: ContentKind) -> Option<u16>;

    /// Re-queue an existing slot's bitmap for upload. Used after
    /// [`AtlasOps::try_grow`] to repopulate the freshly-allocated GPU
    /// texture with cached glyph data.
    fn reupload(&mut self, content: ContentKind, slot: AtlasSlot, data: &[u8]);
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
    pressure_warn_gate: PressureWarnGate,
}

impl GlyphRasterCache {
    /// Build a cache with the given LRU capacity.
    pub fn new(capacity: NonZeroUsize) -> Self {
        Self {
            entries: LruCache::with_hasher(capacity, FxBuildHasher),
            stats: CacheStats::default(),
            current_epoch: 1,
            pressure_warn_gate: PressureWarnGate::default(),
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

        // Oversize guard: a glyph larger than the backend's maximum atlas
        // dimension can never fit, even after a full grow + evict cycle.
        // Without this guard the cache would loop forever between grow and
        // drop (allocating a slot that never succeeds), spamming the
        // pressure warning at frame rate. Log once with full identifying
        // info and return `None`; the caller renders the glyph uncached
        // (or skips it). The error is rate-limited via the same gate so
        // a single recurring oversized glyph (e.g. an emoji at extreme
        // size) does not flood the log either.
        if width > MAX_ATLAS_SIZE || height > MAX_ATLAS_SIZE {
            if let Some(suppressed) = self.pressure_warn_gate.admit(Instant::now()) {
                tracing::error!(
                    target: "kasane_gui::glyph_atlas",
                    glyph_w = width,
                    glyph_h = height,
                    max_atlas = MAX_ATLAS_SIZE,
                    font_id = key.font_id,
                    glyph_id = key.glyph_id,
                    size_q = key.size_q,
                    content = ?content,
                    suppressed_since_last = suppressed,
                    "rasterized glyph exceeds MAX_ATLAS_SIZE in at least one \
                     dimension and cannot be cached. Likely cause: a font \
                     with disproportionate bbox metrics (some Nerd Font \
                     icons) or a color-emoji bitmap strike at extreme \
                     scale. Glyph dropped.",
                );
            }
            self.stats.dropped += 1;
            return None;
        }

        // Atlas allocation with grow-then-evict. Eviction is restricted
        // to entries from earlier frames because same-frame entries
        // already have drawables pointing at their slots; deallocating +
        // reusing those slots would let the queued upload corrupt them
        // mid-frame. Grow is tried *before* eviction: it costs one
        // texture recreation + an O(N) re-upload of cached data
        // (preserved exactly for this case), but avoids discarding
        // entries that are likely to be re-hit. Once the atlas hits its
        // backend-imposed maximum, [`AtlasOps::try_grow`] returns `None`
        // and the loop falls through to eviction / drop.
        let slot = loop {
            if let Some(slot) = atlases.allocate(content, width, height, &data) {
                break slot;
            }
            if atlases.try_grow(content).is_some() {
                // Re-upload every cached entry of this content kind so
                // the freshly-allocated GPU texture has their bitmap
                // data again. CPU slot coordinates are preserved by
                // AtlasShelf::grow, so the cache state stays valid.
                for (_key, e) in self.entries.iter() {
                    if e.content == content {
                        atlases.reupload(content, e.atlas_slot, &e.data);
                    }
                }
                self.stats.atlas_grows += 1;
                continue;
            }
            let evictable = self
                .entries
                .peek_lru()
                .map(|(_, v)| v.last_used_epoch < epoch)
                .unwrap_or(false);
            if !evictable {
                // No safe candidate to evict — accept the loss for
                // this glyph rather than corrupt an in-flight one.
                if let Some(suppressed) = self.pressure_warn_gate.admit(Instant::now()) {
                    tracing::warn!(
                        target: "kasane_gui::glyph_atlas",
                        cache_len = self.entries.len(),
                        cache_cap = self.entries.cap().get(),
                        epoch,
                        suppressed_since_last = suppressed,
                        glyph_w = width,
                        glyph_h = height,
                        font_id = key.font_id,
                        glyph_id = key.glyph_id,
                        size_q = key.size_q,
                        content = ?content,
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
            if let Some(suppressed) = self.pressure_warn_gate.admit(Instant::now()) {
                tracing::warn!(
                    target: "kasane_gui::glyph_atlas",
                    cache_len = self.entries.len(),
                    cache_cap = self.entries.cap().get(),
                    epoch,
                    suppressed_since_last = suppressed,
                    glyph_w = width,
                    glyph_h = height,
                    font_id = key.font_id,
                    glyph_id = key.glyph_id,
                    size_q = key.size_q,
                    content = ?content,
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

    #[test]
    fn pressure_warn_gate_first_call_admits_with_zero_suppressed() {
        let mut gate = PressureWarnGate::default();
        let now = Instant::now();
        assert_eq!(gate.admit(now), Some(0));
    }

    #[test]
    fn pressure_warn_gate_suppresses_within_interval_then_emits_count() {
        let mut gate = PressureWarnGate::default();
        let t0 = Instant::now();
        // First admit: emit (0 suppressed).
        assert_eq!(gate.admit(t0), Some(0));
        // Within the interval: suppressed.
        assert_eq!(gate.admit(t0 + Duration::from_millis(100)), None);
        assert_eq!(gate.admit(t0 + Duration::from_millis(200)), None);
        assert_eq!(gate.admit(t0 + Duration::from_millis(300)), None);
        // After the interval: emit with the suppressed count.
        let after = t0 + PRESSURE_WARN_MIN_INTERVAL + Duration::from_millis(1);
        assert_eq!(gate.admit(after), Some(3));
        // Counter resets after emission.
        assert_eq!(
            gate.admit(after + PRESSURE_WARN_MIN_INTERVAL + Duration::from_millis(1)),
            Some(0)
        );
    }

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
    /// semantics that the cache cares about, plus a `try_grow` that
    /// doubles the side up to a test-configured cap so the grow loop in
    /// [`GlyphRasterCache::get_or_insert`] can be exercised without
    /// wgpu. `reupload` is a no-op since these atlases hold no GPU
    /// state.
    struct TestAtlases {
        mask: AtlasShelf,
        color: AtlasShelf,
        max_side: u16,
        reupload_calls: u32,
    }

    impl TestAtlases {
        fn new(side: u16) -> Self {
            Self {
                mask: AtlasShelf::new(side),
                color: AtlasShelf::new(side),
                max_side: side, // grow disabled by default: max == initial
                reupload_calls: 0,
            }
        }

        fn with_grow(side: u16, max_side: u16) -> Self {
            Self {
                mask: AtlasShelf::new(side),
                color: AtlasShelf::new(side),
                max_side,
                reupload_calls: 0,
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

        fn try_grow(&mut self, content: ContentKind) -> Option<u16> {
            let max = self.max_side;
            let atlas = match content {
                ContentKind::Mask => &mut self.mask,
                ContentKind::Color => &mut self.color,
            };
            let current = atlas.width();
            if current >= max {
                return None;
            }
            let next = u16::try_from(u32::from(current) * 2)
                .unwrap_or(max)
                .min(max);
            if atlas.grow(next) { Some(next) } else { None }
        }

        fn reupload(&mut self, _content: ContentKind, _slot: AtlasSlot, _data: &[u8]) {
            self.reupload_calls += 1;
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

    #[test]
    fn grow_relieves_atlas_pressure_in_same_frame() {
        // Same-frame inserts that would hit the eviction-protection
        // path now succeed because the atlas grows before the cache
        // gives up. Without grow this is the `same_frame_eviction_refused`
        // scenario; with grow we expect `atlas_grows > 0` and `dropped == 0`
        // for a workload that fits in the grown atlas.
        let mut cache = GlyphRasterCache::new(NonZeroUsize::new(1024).unwrap());
        // Initial 256 fits ~16 64x64 glyphs; grow up to 1024 fits ~256.
        let mut at = TestAtlases::with_grow(256, 1024);
        let mut all_succeeded = true;
        for i in 0..30u16 {
            let ok = insert(&mut cache, &mut at, key(1, i), mask_raster(64, 64));
            all_succeeded &= ok;
        }
        let stats = cache.take_stats();
        assert!(
            stats.atlas_grows > 0,
            "expected atlas to grow, stats={stats:?}"
        );
        assert_eq!(
            stats.atlas_evictions, 0,
            "grow path should preempt eviction, stats={stats:?}"
        );
        assert!(all_succeeded, "all 30 glyphs should fit after grow");
        assert!(at.reupload_calls > 0, "reupload must repopulate slots");
    }

    #[test]
    fn oversize_glyph_is_dropped_without_loop() {
        // A glyph wider than MAX_ATLAS_SIZE can never fit, even after a
        // full grow + evict cycle. The oversize guard must short-circuit
        // before entering the allocate/grow/evict loop, returning None
        // and incrementing `dropped`. Without the guard the loop would
        // spin (alloc fails → grow fails or wastes a grow → evict empty
        // LRU → drop), eating CPU and flooding the warning rate-limit.
        let mut cache = GlyphRasterCache::new(NonZeroUsize::new(16).unwrap());
        let mut at = TestAtlases::with_grow(256, MAX_ATLAS_SIZE);
        let oversize_w = MAX_ATLAS_SIZE + 1;
        let oversize_h = 16;
        let raster = RasterizedGlyph {
            width: oversize_w,
            height: oversize_h,
            top: 0,
            left: 0,
            content: ContentKind::Mask,
            data: vec![0u8; usize::from(oversize_w) * usize::from(oversize_h)],
        };
        let result = cache.get_or_insert(key(1, 1), &mut at, || Some(raster));
        assert!(result.is_none(), "oversize glyph must be dropped");
        let stats = cache.take_stats();
        assert_eq!(stats.dropped, 1);
        assert_eq!(
            stats.atlas_grows, 0,
            "guard must trip before any grow attempt, stats={stats:?}"
        );
        assert_eq!(at.reupload_calls, 0);
        assert!(cache.is_empty());
    }

    #[test]
    fn grow_capped_falls_back_to_drop_path() {
        // When the atlas reaches its max size, `try_grow` returns
        // `None` and the cache must take the same-frame drop path.
        let mut cache = GlyphRasterCache::new(NonZeroUsize::new(1024).unwrap());
        // max_side == initial_side == 256: grow disabled.
        let mut at = TestAtlases::new(256);
        let mut first_failure = None;
        for i in 0..30u16 {
            let ok = insert(&mut cache, &mut at, key(1, i), mask_raster(64, 64));
            if !ok && first_failure.is_none() {
                first_failure = Some(i);
            }
        }
        let stats = cache.take_stats();
        assert_eq!(
            stats.atlas_grows, 0,
            "grow disabled in this fixture, stats={stats:?}"
        );
        assert!(
            stats.dropped > 0,
            "without grow the same-frame drop path must run, stats={stats:?}"
        );
        assert!(first_failure.is_some());
    }
}
