//! Line-level shaping cache for the GPU text pipeline.
//!
//! Shaping a single buffer line through cosmic-text costs roughly 100 µs.
//! Across an 80-line viewport that adds up to ~8 ms per frame — wasted on
//! cursor-only frames where line content has not changed. This cache stores
//! the byte-level identity of each line's atom stream and maps it to the
//! `text_buffers` pool slot that already holds the shaped result. On a hit,
//! the renderer can skip `set_rich_text` + `shape_until_scroll` entirely
//! and walk the existing buffer's `layout_runs()` for glyph metrics.
//!
//! The cache is keyed by the `line_idx` value emitted from
//! `ScenePaintVisitor` (display line for buffer rows, descending counter
//! for status / menu / gutter rows) and is invalidated wholesale when font
//! configuration changes.
//!
//! `frame_start()` resets the per-frame "in use" tracking that drives buffer
//! pool eviction; existing entries are preserved so subsequent frames can
//! hit them again.

use kasane_core::protocol::{Color, Face};
use kasane_core::render::ResolvedAtom;
use rustc_hash::FxHashMap;
use std::hash::{Hash, Hasher};

/// Per-line cache entry tracking the shaping inputs that produced
/// `text_buffers[buffer_idx]`.
#[derive(Clone, Copy)]
struct CacheEntry {
    content_hash: u64,
    buffer_idx: usize,
    /// `f32::to_bits` so we can compare exactly without floating-point gotchas.
    max_width_bits: u32,
    font_size_bits: u32,
}

pub(super) struct LineShapingCache {
    /// Stable identity → cached buffer slot.
    entries: FxHashMap<u32, CacheEntry>,
    /// Per-frame: which `text_buffers` slots have been touched (hit or miss).
    /// Used by `alloc_text_buffer` to find a slot free for new shaping work.
    in_use: Vec<bool>,
}

impl LineShapingCache {
    pub fn new() -> Self {
        Self {
            entries: FxHashMap::default(),
            in_use: Vec::with_capacity(128),
        }
    }

    /// Reset per-frame "in use" tracking. Cache entries are preserved so the
    /// next frame can hit them; each entry's `buffer_idx` will be re-marked
    /// in_use when the corresponding line hits.
    pub fn frame_start(&mut self, buffer_pool_size: usize) {
        self.in_use.clear();
        self.in_use.resize(buffer_pool_size, false);
    }

    /// Notify the cache that the buffer pool has grown — extend the in_use
    /// tracking to match.
    pub fn note_pool_size(&mut self, buffer_pool_size: usize) {
        if self.in_use.len() < buffer_pool_size {
            self.in_use.resize(buffer_pool_size, false);
        }
    }

    /// Drop all cache entries (font/metrics changed → all shapings stale).
    pub fn invalidate_all(&mut self) {
        self.entries.clear();
    }

    /// Look up a cached buffer for `line_idx`. Returns the buffer pool index
    /// if and only if every shaping input matches.
    pub fn lookup(
        &mut self,
        line_idx: u32,
        content_hash: u64,
        max_width: f32,
        font_size: f32,
    ) -> Option<usize> {
        if line_idx == u32::MAX {
            return None; // Sentinel: caller opted out of caching.
        }
        let entry = *self.entries.get(&line_idx)?;
        if entry.content_hash == content_hash
            && entry.max_width_bits == max_width.to_bits()
            && entry.font_size_bits == font_size.to_bits()
        {
            self.mark_in_use(entry.buffer_idx);
            Some(entry.buffer_idx)
        } else {
            None
        }
    }

    /// Record a freshly shaped line. Overwrites any prior entry for `line_idx`.
    pub fn insert(
        &mut self,
        line_idx: u32,
        content_hash: u64,
        buffer_idx: usize,
        max_width: f32,
        font_size: f32,
    ) {
        if line_idx == u32::MAX {
            self.mark_in_use(buffer_idx);
            return;
        }
        // If we displaced an entry pointing at the same buffer_idx, the
        // displaced entry is silently invalidated (overwritten value).
        self.entries.insert(
            line_idx,
            CacheEntry {
                content_hash,
                buffer_idx,
                max_width_bits: max_width.to_bits(),
                font_size_bits: font_size.to_bits(),
            },
        );
        self.mark_in_use(buffer_idx);
    }

    pub fn mark_in_use(&mut self, buffer_idx: usize) {
        if buffer_idx >= self.in_use.len() {
            self.in_use.resize(buffer_idx + 1, false);
        }
        self.in_use[buffer_idx] = true;
    }

    #[cfg(test)]
    pub fn is_in_use(&self, buffer_idx: usize) -> bool {
        self.in_use.get(buffer_idx).copied().unwrap_or(false)
    }

    /// Find the lowest buffer pool index that has not been claimed this frame.
    /// Returns `None` if every slot is taken (caller must extend the pool).
    pub fn find_free_slot(&self) -> Option<usize> {
        self.in_use.iter().position(|&used| !used)
    }
}

/// Hash the shaping-relevant identity of a buffer paragraph.
///
/// This must include everything that influences the cosmic-text shaping
/// output: the byte stream, every per-atom face (fg/bg/attrs), and the
/// optional base face used for face stripping. Annotations (cursor byte
/// offsets) are *not* hashed because they are overlaid after shaping and
/// do not affect glyph layout.
pub(super) fn hash_paragraph(atoms: &[ResolvedAtom], base_face: Option<&Face>) -> u64 {
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    if let Some(face) = base_face {
        hash_face(&mut hasher, face);
    } else {
        // Distinguish "no base_face" from "base_face = default".
        0u8.hash(&mut hasher);
    }
    atoms.len().hash(&mut hasher);
    for atom in atoms {
        atom.contents.as_bytes().hash(&mut hasher);
        hash_face(&mut hasher, &atom.face);
    }
    hasher.finish()
}

fn hash_face<H: Hasher>(hasher: &mut H, face: &Face) {
    hash_color(hasher, face.fg);
    hash_color(hasher, face.bg);
    hash_color(hasher, face.underline);
    face.attributes.bits().hash(hasher);
}

fn hash_color<H: Hasher>(hasher: &mut H, color: Color) {
    match color {
        Color::Default => 0u8.hash(hasher),
        Color::Named(n) => {
            1u8.hash(hasher);
            (n as u8).hash(hasher);
        }
        Color::Rgb { r, g, b } => {
            2u8.hash(hasher);
            r.hash(hasher);
            g.hash(hasher);
            b.hash(hasher);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use kasane_core::protocol::{Color, Face};

    fn atom(text: &str, face: Face) -> ResolvedAtom {
        ResolvedAtom {
            contents: text.into(),
            face,
        }
    }

    #[test]
    fn hash_is_stable_for_identical_input() {
        let atoms = vec![atom("hello", Face::default())];
        let h1 = hash_paragraph(&atoms, Some(&Face::default()));
        let h2 = hash_paragraph(&atoms, Some(&Face::default()));
        assert_eq!(h1, h2);
    }

    #[test]
    fn hash_changes_on_text_change() {
        let h1 = hash_paragraph(&[atom("hello", Face::default())], None);
        let h2 = hash_paragraph(&[atom("hellp", Face::default())], None);
        assert_ne!(h1, h2);
    }

    #[test]
    fn hash_changes_on_color_change() {
        let mut face = Face::default();
        face.fg = Color::Rgb { r: 255, g: 0, b: 0 };
        let h1 = hash_paragraph(&[atom("x", face)], None);
        face.fg = Color::Rgb { r: 0, g: 255, b: 0 };
        let h2 = hash_paragraph(&[atom("x", face)], None);
        assert_ne!(h1, h2);
    }

    #[test]
    fn lookup_returns_buffer_on_hit() {
        let mut cache = LineShapingCache::new();
        cache.frame_start(8);
        cache.insert(0, 12345, 3, 100.0, 14.0);
        assert_eq!(cache.lookup(0, 12345, 100.0, 14.0), Some(3));
    }

    #[test]
    fn lookup_misses_on_hash_mismatch() {
        let mut cache = LineShapingCache::new();
        cache.frame_start(8);
        cache.insert(0, 12345, 3, 100.0, 14.0);
        assert_eq!(cache.lookup(0, 99999, 100.0, 14.0), None);
    }

    #[test]
    fn lookup_misses_on_max_width_change() {
        let mut cache = LineShapingCache::new();
        cache.frame_start(8);
        cache.insert(0, 12345, 3, 100.0, 14.0);
        assert_eq!(cache.lookup(0, 12345, 200.0, 14.0), None);
    }

    #[test]
    fn lookup_misses_on_font_size_change() {
        let mut cache = LineShapingCache::new();
        cache.frame_start(8);
        cache.insert(0, 12345, 3, 100.0, 14.0);
        assert_eq!(cache.lookup(0, 12345, 100.0, 16.0), None);
    }

    #[test]
    fn invalidate_all_drops_entries() {
        let mut cache = LineShapingCache::new();
        cache.frame_start(8);
        cache.insert(0, 12345, 3, 100.0, 14.0);
        cache.invalidate_all();
        assert_eq!(cache.lookup(0, 12345, 100.0, 14.0), None);
    }

    #[test]
    fn sentinel_line_idx_bypasses_cache() {
        let mut cache = LineShapingCache::new();
        cache.frame_start(8);
        cache.insert(u32::MAX, 12345, 3, 100.0, 14.0);
        assert_eq!(cache.lookup(u32::MAX, 12345, 100.0, 14.0), None);
    }

    #[test]
    fn find_free_slot_skips_in_use() {
        let mut cache = LineShapingCache::new();
        cache.frame_start(4);
        cache.mark_in_use(0);
        cache.mark_in_use(1);
        assert_eq!(cache.find_free_slot(), Some(2));
    }

    #[test]
    fn find_free_slot_returns_none_when_all_taken() {
        let mut cache = LineShapingCache::new();
        cache.frame_start(2);
        cache.mark_in_use(0);
        cache.mark_in_use(1);
        assert_eq!(cache.find_free_slot(), None);
    }
}
