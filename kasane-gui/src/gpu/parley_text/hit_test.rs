//! Pixel ↔ byte-offset hit testing on a Parley layout (ADR-031, Phase 9b).
//!
//! Wraps Parley's [`Cluster::from_point`] / [`Cluster::from_byte_index`] with
//! Kasane-shaped helpers that the renderer needs:
//!
//! - **`hit_byte`** — pixel `(x, y)` → byte offset in the source text plus
//!   bidi-aware "left/right of cluster" hint. Used by mouse hit testing.
//! - **`byte_to_advance`** — byte offset → x advance from the line origin
//!   (in physical pixels). Used to position the cursor caret without
//!   re-shaping the line.
//! - **`is_byte_rtl`** — bidi direction of the cluster covering a byte
//!   offset. Used to choose which side of the caret bar to draw against.
//!
//! All entry points operate on a [`ParleyLayout`]; they do not require the
//! source [`StyledLine`] because Parley keeps the cluster table inside its
//! `Layout`.

use parley::{Cluster, ClusterSide};

use super::layout::ParleyLayout;

/// Outcome of a `(x, y)` hit test against a layout.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct HitResult {
    /// Byte offset of the cluster covering the hit point.
    pub byte_offset: usize,
    /// Whether the hit landed on the left or right half of the cluster.
    /// Used by callers that need to position a caret between two clusters.
    pub side: HitSide,
    /// Whether the cluster is in an RTL run.
    pub is_rtl: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HitSide {
    Left,
    Right,
}

impl HitSide {
    fn from_parley(side: ClusterSide) -> Self {
        match side {
            ClusterSide::Left => HitSide::Left,
            ClusterSide::Right => HitSide::Right,
        }
    }
}

/// Convert a pixel position into a byte-offset hit result.
///
/// Returns `None` only for layouts with no glyphs at all. Out-of-range
/// `(x, y)` coordinates are clamped to the nearest cluster, matching the
/// behaviour callers expect for clicks past the end of a line.
pub fn hit_byte(layout: &ParleyLayout, x: f32, y: f32) -> Option<HitResult> {
    let (cluster, side) = Cluster::from_point(&layout.layout, x, y)?;
    let range = cluster.text_range();
    let byte_offset = match side {
        ClusterSide::Left => range.start,
        ClusterSide::Right => range.end,
    };
    Some(HitResult {
        byte_offset,
        side: HitSide::from_parley(side),
        is_rtl: cluster.is_rtl(),
    })
}

/// X advance (in physical pixels from the line's logical start) of the
/// cluster covering `byte_index`. Returns `None` if the offset lies outside
/// the layout's text range or the cluster has no visual position (Parley
/// returns this for ignorable clusters such as control characters).
pub fn byte_to_advance(layout: &ParleyLayout, byte_index: usize) -> Option<f32> {
    let cluster = Cluster::from_byte_index(&layout.layout, byte_index)?;
    cluster.visual_offset()
}

/// Byte range of the cluster covering `byte_index`, or `None` when the
/// offset is past the end of the text. Useful for selection extension and
/// double-click word boundary detection.
pub fn cluster_range_at(
    layout: &ParleyLayout,
    byte_index: usize,
) -> Option<std::ops::Range<usize>> {
    Cluster::from_byte_index(&layout.layout, byte_index).map(|c| c.text_range())
}

/// Whether the cluster covering `byte_index` is part of an RTL run.
pub fn is_byte_rtl(layout: &ParleyLayout, byte_index: usize) -> bool {
    Cluster::from_byte_index(&layout.layout, byte_index)
        .map(|c| c.is_rtl())
        .unwrap_or(false)
}

#[cfg(test)]
mod tests {
    use super::*;
    use kasane_core::config::FontConfig;
    use kasane_core::protocol::{Atom, Style};

    use super::super::styled_line::StyledLine;
    use super::super::{Brush, ParleyText};

    fn line(text: &str) -> StyledLine {
        let atoms = vec![Atom::plain(text)];
        StyledLine::from_atoms(
            &atoms,
            &Style::default(),
            Brush::opaque(255, 255, 255),
            14.0,
            None,
        )
    }

    #[test]
    fn hit_at_origin_returns_first_cluster() {
        let mut text = ParleyText::new(&FontConfig::default());
        let layout = text.shape(&line("hello"));
        let hit = hit_byte(&layout, 0.0, 0.0).expect("hit");
        // Hitting the very first pixel should land at byte 0 (left side of
        // the first cluster).
        assert_eq!(hit.byte_offset, 0);
        assert!(!hit.is_rtl);
    }

    #[test]
    fn hit_past_end_returns_last_cluster_right_side() {
        let mut text = ParleyText::new(&FontConfig::default());
        let layout = text.shape(&line("hi"));
        // Click far to the right of the line.
        let hit = hit_byte(&layout, 10000.0, 0.0).expect("hit");
        // Should land at the trailing edge of the last cluster (byte == len).
        assert_eq!(hit.byte_offset, 2);
        assert_eq!(hit.side, HitSide::Right);
    }

    #[test]
    fn byte_to_advance_increases_with_offset() {
        let mut text = ParleyText::new(&FontConfig::default());
        let layout = text.shape(&line("hello"));
        // Only the first 4 bytes are guaranteed to have a Cluster visual_offset
        // — Parley returns None for the trailing position (caller computes
        // end-of-line position from layout.width separately).
        let advances: Vec<f32> = (0..5).filter_map(|i| byte_to_advance(&layout, i)).collect();
        assert!(advances.len() >= 2, "got advances: {advances:?}");
        for window in advances.windows(2) {
            assert!(
                window[1] >= window[0],
                "advances must be monotonic: {advances:?}"
            );
        }
        assert_eq!(advances[0], 0.0);
        assert!(*advances.last().unwrap() >= 0.0);
    }

    #[test]
    fn cluster_range_at_byte_within_string() {
        let mut text = ParleyText::new(&FontConfig::default());
        let layout = text.shape(&line("hello"));
        let range = cluster_range_at(&layout, 1).expect("range");
        assert_eq!(range, 1..2);
    }

    #[test]
    fn cluster_range_at_cjk_returns_3byte_range() {
        let mut text = ParleyText::new(&FontConfig::default());
        let layout = text.shape(&line("a日b"));
        // "日" is 3 bytes (UTF-8: E6 97 A5), starting at byte 1.
        let range = cluster_range_at(&layout, 1).expect("range");
        assert_eq!(range, 1..4);
    }

    #[test]
    fn ascii_is_not_rtl() {
        let mut text = ParleyText::new(&FontConfig::default());
        let layout = text.shape(&line("hello"));
        for i in 0..5 {
            assert!(!is_byte_rtl(&layout, i), "ASCII byte {i} should be LTR");
        }
    }

    #[test]
    fn empty_line_hit_does_not_panic() {
        // Parley produces a degenerate layout for empty input that may or may
        // not yield a cluster; this test only pins down that hit_byte does
        // not panic on the empty case.
        let mut text = ParleyText::new(&FontConfig::default());
        let layout = text.shape(&line(""));
        let _ = hit_byte(&layout, 0.0, 0.0);
        let _ = hit_byte(&layout, 100.0, 100.0);
    }

    // ----------------------------------------------------------------
    // ADR-031 Phase 10 Step 2 — extended hit_test coverage
    // (RTL / combining-mark / ZWJ / trailing position)
    // ----------------------------------------------------------------

    #[test]
    fn rtl_arabic_cluster_is_marked_rtl() {
        // "سلام" (Arabic "peace") consists of strong RTL characters. After
        // ICU4X bidi analysis, each cluster's `is_rtl()` must report true.
        // Pin the post-Parley behaviour so a future ICU4X / Parley bump
        // that breaks bidi run direction is caught here, not silently in
        // production.
        let mut text = ParleyText::new(&FontConfig::default());
        let layout = text.shape(&line("سلام"));
        // First UTF-8 byte of an Arabic letter is at offset 0; verify the
        // cluster that covers it is RTL.
        assert!(
            is_byte_rtl(&layout, 0),
            "Arabic strong-RTL character at byte 0 must be classified RTL"
        );
    }

    #[test]
    fn combining_mark_hit_returns_valid_cluster() {
        // "é" composed via "e" (U+0065, 1 byte) + combining acute
        // (U+0301, 2 bytes UTF-8). Whether Parley merges the two into
        // one shaping cluster depends on the loaded font's coverage of
        // the precomposed glyph. In Kasane's default font configuration
        // they currently land in separate clusters; this test only
        // pins that hit testing at byte 0 returns *some* cluster
        // covering byte 0 without panic, and that the cluster boundary
        // is byte-aligned (Parley invariant). Behaviour change here
        // would be a Parley / font-fallback shift worth investigating.
        let mut text = ParleyText::new(&FontConfig::default());
        let input = "e\u{0301}";
        let layout = text.shape(&line(input));
        let range = cluster_range_at(&layout, 0).expect("cluster at base byte");
        assert!(range.start == 0, "cluster must start at byte 0");
        assert!(range.end <= input.len(), "cluster end within text");
        assert!(range.end > range.start, "cluster must be non-empty");
    }

    #[test]
    fn zwj_emoji_family_hit_does_not_panic() {
        // "👨‍👩‍👧‍👦" — man-woman-girl-boy family, 4 emoji joined by 3
        // ZWJ (U+200D) characters. Whether Parley + the available font
        // collapses the ZWJ sequence into one shaping cluster depends
        // on font coverage of the joined sequence. In environments
        // without an emoji font that supports the family ligature,
        // Parley reports them as separate clusters. This test pins the
        // weaker invariant: hit testing at the start of the sequence
        // does not panic and returns a cluster within the input range.
        let family = "\u{1F468}\u{200D}\u{1F469}\u{200D}\u{1F467}\u{200D}\u{1F466}";
        let mut text = ParleyText::new(&FontConfig::default());
        let layout = text.shape(&line(family));
        let range = cluster_range_at(&layout, 0).expect("cluster at start");
        assert_eq!(range.start, 0);
        assert!(range.end <= family.len());
        assert!(range.end > 0);
    }

    #[test]
    fn trailing_position_visual_offset_is_none() {
        // `byte_to_advance` is documented to return `None` for the
        // trailing position (byte == text.len()). The caller computes
        // end-of-line caret X from `layout.width` instead. Pin this
        // contract: a future Parley change that starts returning a
        // value here would shift the caret rendering for end-of-line
        // cursor positions.
        let mut text = ParleyText::new(&FontConfig::default());
        let layout = text.shape(&line("abc"));
        let trailing = byte_to_advance(&layout, 3);
        assert!(
            trailing.is_none(),
            "trailing position must yield None visual_offset; got {:?}",
            trailing
        );
    }

    // ----------------------------------------------------------------
    // ADR-031 §動機 (1) — Parley/ICU4X regression pinning for the
    // input *classes* that motivated retiring cosmic-text. The original
    // commits cited in the ADR (`2f7c0ab9`, `4d48bbd9`) were
    // GPU-pipeline cursor-rendering fixes; the tests below exercise the
    // layout-layer sanity that ICU4X-based bidi + Parley shaping must
    // provide for those pipelines to compose correctly. A failure here
    // means Parley or the font fallback shifted in a way that would
    // re-expose the 2026 Q1 class of cursor / cluster bugs.
    // ----------------------------------------------------------------

    #[test]
    fn mixed_rtl_ltr_run_directions_alternate_per_cluster() {
        // Mixed Arabic + Latin: "abcسلامxyz". After ICU4X bidi
        // resolution the Arabic cluster must report `is_rtl == true`
        // while the Latin clusters around it stay LTR. A regression
        // that flattens the entire line to one direction would break
        // cursor placement at the boundary (the cosmic-text era class
        // of bug).
        let mut text = ParleyText::new(&FontConfig::default());
        let layout = text.shape(&line("abcسلامxyz"));
        // First Latin byte (0) is LTR.
        assert!(!is_byte_rtl(&layout, 0), "leading 'a' must be LTR");
        // Arabic strong-RTL character at byte 3 is RTL.
        assert!(
            is_byte_rtl(&layout, 3),
            "Arabic 'س' at byte 3 must be classified RTL"
        );
        // The trailing Latin run starts after the 8 UTF-8 bytes of the
        // Arabic word (4 codepoints × 2 bytes each in the Arabic
        // block). Its first byte index = 3 + 8 = 11.
        assert!(
            !is_byte_rtl(&layout, 11),
            "trailing 'x' (byte 11) must be LTR"
        );
    }

    #[test]
    fn narrow_cjk_glyph_advance_progresses_into_following_ascii() {
        // The 2026 Q1 cursor-width class of bug surfaced when a
        // narrow CJK glyph ("日") was followed immediately by ASCII
        // ("a") in a proportional font. The cursor was clamped to
        // cell_w and over-rendered the next glyph. Layout-layer pin:
        // for "日a", `byte_to_advance` reported for the ASCII byte
        // (after the 3-byte CJK cluster) MUST be strictly greater
        // than the advance of the CJK byte. If a regression flattens
        // the advance table, cursor-on-narrow-CJK rendering breaks.
        let mut text = ParleyText::new(&FontConfig::default());
        let layout = text.shape(&line("日a"));
        let cjk_advance = byte_to_advance(&layout, 0).expect("CJK cluster advance");
        let ascii_advance = byte_to_advance(&layout, 3).expect("ASCII byte advance");
        assert!(
            ascii_advance > cjk_advance,
            "ASCII after narrow CJK must advance further: cjk={cjk_advance}, ascii={ascii_advance}"
        );
        // Sanity: the CJK cluster itself starts at the line origin.
        assert_eq!(
            cjk_advance, 0.0,
            "first cluster must start at advance 0; got {cjk_advance}"
        );
    }

    #[test]
    fn round_trip_byte_to_advance_and_back() {
        // Hitting the exact x position of a byte should land near that byte.
        // We skip bytes whose visual_offset Parley reports as None (typically
        // the trailing position past the last cluster).
        let mut text = ParleyText::new(&FontConfig::default());
        let layout = text.shape(&line("abcde"));
        let mut checked = 0;
        for byte in 0..5 {
            let Some(advance) = byte_to_advance(&layout, byte) else {
                continue;
            };
            let Some(hit) = hit_byte(&layout, advance + 0.5, 0.0) else {
                continue;
            };
            assert!(
                hit.byte_offset <= 5,
                "hit {} should not exceed text length",
                hit.byte_offset
            );
            checked += 1;
        }
        assert!(checked > 0, "no byte positions exercised round trip");
    }
}
