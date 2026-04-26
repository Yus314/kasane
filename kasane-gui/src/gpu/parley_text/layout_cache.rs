//! L1 LayoutCache — per-line cache of shaped Parley layouts (ADR-031, Phase 7).
//!
//! Replaces `kasane-gui/src/gpu/scene_renderer/line_cache.rs`. Key differences:
//!
//! - **Value type**: `Arc<ParleyLayout>` instead of an opaque buffer slot
//!   index. Sharing is cheap; no in-flight buffer-pool bookkeeping.
//! - **Style key**: hashes the resolved style spans, not just the atom
//!   contents. This lets the cache distinguish between two lines that have
//!   the same text but different colours.
//! - **Generation counter**: `font_size` / metrics changes are handled by
//!   bumping `context_gen`, which invalidates every entry without touching
//!   the map. This keeps `invalidate_all` to O(1).
//!
//! Cache structure: `FxHashMap<line_idx, CacheEntry>`. The invariant is that
//! a hit requires every shaping input to match — content, style, max width,
//! font size, and the context generation.

use std::hash::{Hash, Hasher};
use std::sync::Arc;

use rustc_hash::{FxHashMap, FxHasher};

use super::layout::ParleyLayout;
use super::styled_line::StyledLine;

/// Per-frame cache statistics emitted via tracing for performance monitoring.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct FrameStats {
    pub hits: u32,
    pub misses: u32,
    pub bypass: u32,
}

impl FrameStats {
    pub fn lookups(self) -> u32 {
        self.hits + self.misses + self.bypass
    }
}

#[derive(Clone)]
struct CacheEntry {
    content_hash: u64,
    style_hash: u64,
    max_width_bits: u32,
    font_size_bits: u32,
    context_gen: u64,
    layout: Arc<ParleyLayout>,
}

/// Per-line shaped layout cache.
pub struct LayoutCache {
    entries: FxHashMap<u32, CacheEntry>,
    stats: FrameStats,
    /// Bumped by [`invalidate_all`]. Entries whose stored `context_gen`
    /// disagrees with the current value are stale and force a miss.
    context_gen: u64,
}

impl Default for LayoutCache {
    fn default() -> Self {
        Self::new()
    }
}

impl LayoutCache {
    pub fn new() -> Self {
        Self {
            entries: FxHashMap::default(),
            stats: FrameStats::default(),
            context_gen: 0,
        }
    }

    /// Drop all entries. Called when font / metrics change at a coarse grain
    /// (config reload, scale-factor update, theme change).
    pub fn invalidate_all(&mut self) {
        // Bump the generation so any in-flight `Arc<ParleyLayout>` references
        // a caller may still hold remain valid — they just won't hit again.
        self.context_gen = self.context_gen.wrapping_add(1);
        self.entries.clear();
    }

    /// Take and reset per-frame stats. Caller emits via tracing.
    pub fn take_stats(&mut self) -> FrameStats {
        std::mem::take(&mut self.stats)
    }

    /// Look up or compute the [`ParleyLayout`] for `line`.
    ///
    /// `line_idx == u32::MAX` opts out of caching unconditionally (matches
    /// the legacy `LineShapingCache` bypass convention; used for ephemeral
    /// content like padding rows).
    ///
    /// On miss, `compute` is invoked to produce the layout. The resulting
    /// `Arc` is stashed in the map and returned to the caller.
    pub fn get_or_compute(
        &mut self,
        line_idx: u32,
        line: &StyledLine,
        compute: impl FnOnce(&StyledLine) -> ParleyLayout,
    ) -> Arc<ParleyLayout> {
        if line_idx == u32::MAX {
            self.stats.bypass += 1;
            return Arc::new(compute(line));
        }

        let content_hash = hash_content(line);
        let style_hash = hash_style(line);
        let max_width_bits = line.max_width.map(f32::to_bits).unwrap_or(u32::MAX);
        let font_size_bits = line.font_size.to_bits();

        if let Some(entry) = self.entries.get(&line_idx)
            && entry.content_hash == content_hash
            && entry.style_hash == style_hash
            && entry.max_width_bits == max_width_bits
            && entry.font_size_bits == font_size_bits
            && entry.context_gen == self.context_gen
        {
            self.stats.hits += 1;
            return Arc::clone(&entry.layout);
        }

        self.stats.misses += 1;
        let layout = Arc::new(compute(line));
        self.entries.insert(
            line_idx,
            CacheEntry {
                content_hash,
                style_hash,
                max_width_bits,
                font_size_bits,
                context_gen: self.context_gen,
                layout: Arc::clone(&layout),
            },
        );
        layout
    }

    /// Number of cached entries. Mostly for diagnostics and tests.
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
}

/// Hash the textual content of a styled line. Two lines with identical
/// `text` and atom boundaries share the same content hash regardless of
/// styling.
fn hash_content(line: &StyledLine) -> u64 {
    let mut h = FxHasher::default();
    line.text.hash(&mut h);
    line.atom_boundaries.hash(&mut h);
    h.finish()
}

/// Hash the resolved style runs and base style. Captures everything that
/// affects the shaped output but is not in `content_hash`.
fn hash_style(line: &StyledLine) -> u64 {
    let mut h = FxHasher::default();
    for run in &line.runs {
        run.byte_range.hash(&mut h);
        run.resolved.fg.hash(&mut h);
        // ResolvedParleyStyle uses f32 for weight / letter_spacing — hash by
        // bit pattern so two equal styles produce the same hash.
        run.resolved.weight.to_bits().hash(&mut h);
        run.resolved.letter_spacing.to_bits().hash(&mut h);
        run.resolved.italic.hash(&mut h);
        run.resolved.oblique.hash(&mut h);
    }
    h.finish()
}

#[cfg(test)]
mod tests {
    use super::*;
    use kasane_core::config::FontConfig;
    use kasane_core::protocol::{Atom, Color, Face, NamedColor, Style};

    use super::super::shaper::shape_line_with_default_family;
    use super::super::styled_line::StyledLine;
    use super::super::{Brush, ParleyText};

    fn ascii_atoms(s: &str) -> Vec<Atom> {
        vec![Atom {
            face: Face::default(),
            contents: s.into(),
        }]
    }

    fn default_line(s: &str) -> StyledLine {
        StyledLine::from_atoms(
            &ascii_atoms(s),
            &Style::default(),
            Brush::opaque(255, 255, 255),
            14.0,
            None,
        )
    }

    #[test]
    fn cache_miss_then_hit() {
        let mut text = ParleyText::new(&FontConfig::default());
        let mut cache = LayoutCache::new();
        let line = default_line("hello");
        let _l1 = cache.get_or_compute(0, &line, |l| shape_line_with_default_family(&mut text, l));
        let stats1 = cache.take_stats();
        assert_eq!(stats1.misses, 1);
        assert_eq!(stats1.hits, 0);

        let _l2 = cache.get_or_compute(0, &line, |l| shape_line_with_default_family(&mut text, l));
        let stats2 = cache.take_stats();
        assert_eq!(stats2.misses, 0);
        assert_eq!(stats2.hits, 1);
    }

    #[test]
    fn cache_returns_same_arc_on_hit() {
        let mut text = ParleyText::new(&FontConfig::default());
        let mut cache = LayoutCache::new();
        let line = default_line("hello");
        let l1 = cache.get_or_compute(0, &line, |l| shape_line_with_default_family(&mut text, l));
        let l2 = cache.get_or_compute(0, &line, |l| shape_line_with_default_family(&mut text, l));
        assert!(Arc::ptr_eq(&l1, &l2), "hit should return the cached Arc");
    }

    #[test]
    fn content_change_misses() {
        let mut text = ParleyText::new(&FontConfig::default());
        let mut cache = LayoutCache::new();
        let _ = cache.get_or_compute(0, &default_line("hello"), |l| {
            shape_line_with_default_family(&mut text, l)
        });
        let _ = cache.get_or_compute(0, &default_line("world"), |l| {
            shape_line_with_default_family(&mut text, l)
        });
        let stats = cache.take_stats();
        assert_eq!(stats.misses, 2);
        assert_eq!(stats.hits, 0);
    }

    #[test]
    fn style_change_misses() {
        let mut text = ParleyText::new(&FontConfig::default());
        let mut cache = LayoutCache::new();

        let plain = default_line("hello");
        let red_face = Face {
            fg: Color::Named(NamedColor::Red),
            ..Face::default()
        };
        let red_atoms = vec![Atom {
            face: red_face,
            contents: "hello".into(),
        }];
        let red_line = StyledLine::from_atoms(
            &red_atoms,
            &Style::default(),
            Brush::opaque(255, 255, 255),
            14.0,
            None,
        );

        let _ = cache.get_or_compute(0, &plain, |l| shape_line_with_default_family(&mut text, l));
        let _ = cache.get_or_compute(0, &red_line, |l| {
            shape_line_with_default_family(&mut text, l)
        });
        let stats = cache.take_stats();
        assert_eq!(stats.misses, 2);
    }

    #[test]
    fn font_size_change_misses() {
        let mut text = ParleyText::new(&FontConfig::default());
        let mut cache = LayoutCache::new();
        let line_a = StyledLine::from_atoms(
            &ascii_atoms("hi"),
            &Style::default(),
            Brush::default(),
            14.0,
            None,
        );
        let line_b = StyledLine::from_atoms(
            &ascii_atoms("hi"),
            &Style::default(),
            Brush::default(),
            16.0,
            None,
        );
        let _ = cache.get_or_compute(0, &line_a, |l| shape_line_with_default_family(&mut text, l));
        let _ = cache.get_or_compute(0, &line_b, |l| shape_line_with_default_family(&mut text, l));
        let stats = cache.take_stats();
        assert_eq!(stats.misses, 2);
    }

    #[test]
    fn invalidate_all_clears_entries() {
        let mut text = ParleyText::new(&FontConfig::default());
        let mut cache = LayoutCache::new();
        let line = default_line("hello");
        let _ = cache.get_or_compute(0, &line, |l| shape_line_with_default_family(&mut text, l));
        assert_eq!(cache.len(), 1);
        let _ = cache.take_stats(); // discard the populating miss
        cache.invalidate_all();
        assert_eq!(cache.len(), 0);
        let _ = cache.get_or_compute(0, &line, |l| shape_line_with_default_family(&mut text, l));
        let stats = cache.take_stats();
        assert_eq!(stats.misses, 1, "should miss after invalidate_all");
        assert_eq!(stats.hits, 0);
    }

    #[test]
    fn line_idx_max_bypasses_cache() {
        let mut text = ParleyText::new(&FontConfig::default());
        let mut cache = LayoutCache::new();
        let line = default_line("hello");
        let _ = cache.get_or_compute(u32::MAX, &line, |l| {
            shape_line_with_default_family(&mut text, l)
        });
        let _ = cache.get_or_compute(u32::MAX, &line, |l| {
            shape_line_with_default_family(&mut text, l)
        });
        let stats = cache.take_stats();
        assert_eq!(stats.bypass, 2);
        assert_eq!(stats.hits, 0);
        assert_eq!(stats.misses, 0);
        assert!(cache.is_empty());
    }

    #[test]
    fn distinct_lines_share_cache() {
        let mut text = ParleyText::new(&FontConfig::default());
        let mut cache = LayoutCache::new();
        let _ = cache.get_or_compute(0, &default_line("first"), |l| {
            shape_line_with_default_family(&mut text, l)
        });
        let _ = cache.get_or_compute(1, &default_line("second"), |l| {
            shape_line_with_default_family(&mut text, l)
        });
        assert_eq!(cache.len(), 2);
        // Re-request both — both should hit.
        let _ = cache.get_or_compute(0, &default_line("first"), |l| {
            shape_line_with_default_family(&mut text, l)
        });
        let _ = cache.get_or_compute(1, &default_line("second"), |l| {
            shape_line_with_default_family(&mut text, l)
        });
        let stats = cache.take_stats();
        assert_eq!(stats.hits, 2);
        assert_eq!(stats.misses, 2); // from the initial population
    }
}
