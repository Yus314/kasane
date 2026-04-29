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

use std::sync::Arc;

use rustc_hash::FxHashMap;

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

        // Hashes are memoized on `StyledLine::from_atoms` (Phase 11 case A:
        // hot-path lookups previously paid hash_content + hash_style on
        // every hit, ~3-5 µs across 24 lines).
        let content_hash = line.content_hash;
        let style_hash = line.style_hash;
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

// Hash functions moved to `styled_line.rs` so they can be memoized at
// construction time. The `StyledLine::content_hash` / `style_hash` fields
// are populated by `from_atoms`; this cache reads them as plain `u64`.

#[cfg(test)]
mod tests {
    use super::*;
    use kasane_core::config::FontConfig;
    use kasane_core::protocol::{Atom, Color, Face, NamedColor, Style};

    use super::super::shaper::shape_line_with_default_family;
    use super::super::styled_line::StyledLine;
    use super::super::{Brush, ParleyText};

    fn ascii_atoms(s: &str) -> Vec<Atom> {
        vec![Atom::plain(s)]
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
        let red_atoms = vec![Atom::with_style("hello", Style::from_face(&red_face))];
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

    // === Negative tests pinning the cache key contract ===
    //
    // The L1 LayoutCache is keyed on shape-affecting inputs only. Paint-time
    // properties (background brush, reverse / dim / blink, decoration colour
    // / thickness — when decoration enablement is unchanged) are read from
    // the current `StyledLine.atom_styles` at draw time, so a cache hit is
    // correct even when those fields change. The shape-affecting properties
    // (foreground brush, font weight, slant, letter spacing, decoration
    // *enablement* because Parley emits underline/strikethrough run metadata)
    // must miss.
    //
    // These tests pin the contract by exercising both halves explicitly. If
    // a future change moves a paint-time property into shaping (e.g. variable
    // axes from `font_variations`) the corresponding test must move with it.

    fn line_with_style(content: &str, style: &Style) -> StyledLine {
        // Build via UnresolvedStyle so we exercise the same construction path
        // the protocol layer uses (Phase A.4 split). `final_*` flags stay
        // false; the LayoutCache key never reaches them.
        let unresolved = kasane_core::protocol::UnresolvedStyle {
            style: style.clone(),
            final_fg: false,
            final_bg: false,
            final_style: false,
        };
        let atoms = vec![Atom::from_style(content, std::sync::Arc::new(unresolved))];
        StyledLine::from_atoms(
            &atoms,
            &Style::default(),
            Brush::opaque(255, 255, 255),
            14.0,
            None,
        )
    }

    #[test]
    fn bg_change_does_not_miss() {
        let mut text = ParleyText::new(&FontConfig::default());
        let mut cache = LayoutCache::new();
        let plain = default_line("hello");
        let with_bg = line_with_style(
            "hello",
            &Style {
                bg: kasane_core::protocol::Brush::Named(NamedColor::Red),
                ..Style::default()
            },
        );

        let _ = cache.get_or_compute(0, &plain, |l| shape_line_with_default_family(&mut text, l));
        let _ = cache.get_or_compute(0, &with_bg, |l| {
            shape_line_with_default_family(&mut text, l)
        });
        let stats = cache.take_stats();
        assert_eq!(
            stats.misses, 1,
            "bg is paint-time only; second call must hit — mismatch indicates the cache key over-invalidates"
        );
        assert_eq!(stats.hits, 1);
    }

    #[test]
    fn reverse_dim_blink_changes_do_not_miss() {
        // SGR 7 / SGR 2 / SGR 5 are all post-shape effects.
        let mut text = ParleyText::new(&FontConfig::default());
        let mut cache = LayoutCache::new();
        let plain = default_line("hello");
        let toggled = line_with_style(
            "hello",
            &Style {
                reverse: true,
                dim: true,
                blink: true,
                ..Style::default()
            },
        );

        let _ = cache.get_or_compute(0, &plain, |l| shape_line_with_default_family(&mut text, l));
        let _ = cache.get_or_compute(0, &toggled, |l| {
            shape_line_with_default_family(&mut text, l)
        });
        let stats = cache.take_stats();
        assert_eq!(stats.misses, 1, "reverse/dim/blink are paint-time only");
        assert_eq!(stats.hits, 1);
    }

    #[test]
    fn font_weight_change_misses() {
        let mut text = ParleyText::new(&FontConfig::default());
        let mut cache = LayoutCache::new();
        let normal = default_line("hello");
        let bold = line_with_style(
            "hello",
            &Style {
                font_weight: kasane_core::protocol::FontWeight(700),
                ..Style::default()
            },
        );

        let _ = cache.get_or_compute(0, &normal, |l| shape_line_with_default_family(&mut text, l));
        let _ = cache.get_or_compute(0, &bold, |l| shape_line_with_default_family(&mut text, l));
        let stats = cache.take_stats();
        assert_eq!(
            stats.misses, 2,
            "font_weight changes glyph metrics; cache must miss"
        );
    }

    #[test]
    fn font_slant_change_misses() {
        let mut text = ParleyText::new(&FontConfig::default());
        let mut cache = LayoutCache::new();
        let upright = default_line("hello");
        let italic = line_with_style(
            "hello",
            &Style {
                font_slant: kasane_core::protocol::FontSlant::Italic,
                ..Style::default()
            },
        );

        let _ = cache.get_or_compute(0, &upright, |l| {
            shape_line_with_default_family(&mut text, l)
        });
        let _ = cache.get_or_compute(0, &italic, |l| shape_line_with_default_family(&mut text, l));
        let stats = cache.take_stats();
        assert_eq!(stats.misses, 2, "italic ≠ normal must miss");
    }

    #[test]
    fn letter_spacing_change_misses() {
        let mut text = ParleyText::new(&FontConfig::default());
        let mut cache = LayoutCache::new();
        let tight = default_line("hello");
        let loose = line_with_style(
            "hello",
            &Style {
                letter_spacing: 2.5,
                ..Style::default()
            },
        );

        let _ = cache.get_or_compute(0, &tight, |l| shape_line_with_default_family(&mut text, l));
        let _ = cache.get_or_compute(0, &loose, |l| shape_line_with_default_family(&mut text, l));
        let stats = cache.take_stats();
        assert_eq!(
            stats.misses, 2,
            "letter_spacing changes advance widths; cache must miss"
        );
    }

    #[test]
    fn underline_enablement_change_misses() {
        // Adding/removing the underline decoration toggles the
        // `StyleProperty::Underline(true)` push in
        // `kasane-gui/src/gpu/parley_text/shaper.rs:89`. Parley's run metrics
        // change when this property is set, so the cache key MUST observe it.
        let mut text = ParleyText::new(&FontConfig::default());
        let mut cache = LayoutCache::new();
        let plain = default_line("hello");
        let underlined = line_with_style(
            "hello",
            &Style {
                underline: Some(kasane_core::protocol::TextDecoration {
                    style: kasane_core::protocol::DecorationStyle::Solid,
                    color: kasane_core::protocol::Brush::Default,
                    thickness: None,
                }),
                ..Style::default()
            },
        );

        let _ = cache.get_or_compute(0, &plain, |l| shape_line_with_default_family(&mut text, l));
        let _ = cache.get_or_compute(0, &underlined, |l| {
            shape_line_with_default_family(&mut text, l)
        });
        let stats = cache.take_stats();
        assert_eq!(
            stats.misses, 2,
            "underline enablement toggles a Parley StyleProperty — cache must miss"
        );
    }

    #[test]
    fn decoration_color_change_does_not_miss() {
        // Underline colour is paint-time only — it influences the
        // `SetUnderlineColor` SGR / quad fill but not Parley shape.
        // `compute_style_hash` (styled_line.rs) hashes only
        // `decoration_enabled` (bool) for underline, not the full
        // TextDecoration — so two lines that share enablement but
        // differ in colour MUST share a layout cache slot.
        let mut text = ParleyText::new(&FontConfig::default());
        let mut cache = LayoutCache::new();
        let red_underline = line_with_style(
            "hello",
            &Style {
                underline: Some(kasane_core::protocol::TextDecoration {
                    style: kasane_core::protocol::DecorationStyle::Solid,
                    color: kasane_core::protocol::Brush::Named(NamedColor::Red),
                    thickness: None,
                }),
                ..Style::default()
            },
        );
        let blue_underline = line_with_style(
            "hello",
            &Style {
                underline: Some(kasane_core::protocol::TextDecoration {
                    style: kasane_core::protocol::DecorationStyle::Solid,
                    color: kasane_core::protocol::Brush::Named(NamedColor::Blue),
                    thickness: None,
                }),
                ..Style::default()
            },
        );
        let _ = cache.get_or_compute(0, &red_underline, |l| {
            shape_line_with_default_family(&mut text, l)
        });
        let _ = cache.get_or_compute(0, &blue_underline, |l| {
            shape_line_with_default_family(&mut text, l)
        });
        let stats = cache.take_stats();
        assert_eq!(
            stats.misses, 1,
            "underline colour is paint-time; second call must hit"
        );
        assert_eq!(stats.hits, 1);
    }

    #[test]
    fn decoration_thickness_change_does_not_miss() {
        // Underline thickness is paint-time — drives the quad geometry
        // amplitude in `quad_pipeline.rs`, not the shaped run metrics
        // Parley produces. The cache key must not pin thickness.
        let mut text = ParleyText::new(&FontConfig::default());
        let mut cache = LayoutCache::new();
        let thin = line_with_style(
            "hello",
            &Style {
                underline: Some(kasane_core::protocol::TextDecoration {
                    style: kasane_core::protocol::DecorationStyle::Solid,
                    color: kasane_core::protocol::Brush::Default,
                    thickness: Some(0.5),
                }),
                ..Style::default()
            },
        );
        let thick = line_with_style(
            "hello",
            &Style {
                underline: Some(kasane_core::protocol::TextDecoration {
                    style: kasane_core::protocol::DecorationStyle::Solid,
                    color: kasane_core::protocol::Brush::Default,
                    thickness: Some(2.0),
                }),
                ..Style::default()
            },
        );
        let _ = cache.get_or_compute(0, &thin, |l| shape_line_with_default_family(&mut text, l));
        let _ = cache.get_or_compute(0, &thick, |l| shape_line_with_default_family(&mut text, l));
        let stats = cache.take_stats();
        assert_eq!(
            stats.misses, 1,
            "underline thickness is paint-time; second call must hit"
        );
        assert_eq!(stats.hits, 1);
    }

    #[test]
    fn strikethrough_color_change_does_not_miss() {
        // Strikethrough mirror of `decoration_color_change_does_not_miss`.
        // Pin both decoration kinds so a future change to
        // `compute_style_hash` that accidentally pulls one but not the
        // other is caught uniformly.
        let mut text = ParleyText::new(&FontConfig::default());
        let mut cache = LayoutCache::new();
        let red_strike = line_with_style(
            "hello",
            &Style {
                strikethrough: Some(kasane_core::protocol::TextDecoration {
                    style: kasane_core::protocol::DecorationStyle::Solid,
                    color: kasane_core::protocol::Brush::Named(NamedColor::Red),
                    thickness: None,
                }),
                ..Style::default()
            },
        );
        let blue_strike = line_with_style(
            "hello",
            &Style {
                strikethrough: Some(kasane_core::protocol::TextDecoration {
                    style: kasane_core::protocol::DecorationStyle::Solid,
                    color: kasane_core::protocol::Brush::Named(NamedColor::Blue),
                    thickness: None,
                }),
                ..Style::default()
            },
        );
        let _ = cache.get_or_compute(0, &red_strike, |l| {
            shape_line_with_default_family(&mut text, l)
        });
        let _ = cache.get_or_compute(0, &blue_strike, |l| {
            shape_line_with_default_family(&mut text, l)
        });
        let stats = cache.take_stats();
        assert_eq!(
            stats.misses, 1,
            "strikethrough colour is paint-time; second call must hit"
        );
        assert_eq!(stats.hits, 1);
    }

    #[test]
    fn strikethrough_enablement_change_misses() {
        let mut text = ParleyText::new(&FontConfig::default());
        let mut cache = LayoutCache::new();
        let plain = default_line("hello");
        let struck = line_with_style(
            "hello",
            &Style {
                strikethrough: Some(kasane_core::protocol::TextDecoration {
                    style: kasane_core::protocol::DecorationStyle::Solid,
                    color: kasane_core::protocol::Brush::Default,
                    thickness: None,
                }),
                ..Style::default()
            },
        );

        let _ = cache.get_or_compute(0, &plain, |l| shape_line_with_default_family(&mut text, l));
        let _ = cache.get_or_compute(0, &struck, |l| shape_line_with_default_family(&mut text, l));
        let stats = cache.take_stats();
        assert_eq!(
            stats.misses, 2,
            "strikethrough enablement toggles a Parley StyleProperty — cache must miss"
        );
    }
}
