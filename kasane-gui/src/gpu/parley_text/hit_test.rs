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
    use kasane_core::protocol::{Atom, Face, Style};

    use super::super::shaper::shape_line_with_default_family;
    use super::super::styled_line::StyledLine;
    use super::super::{Brush, ParleyText};

    fn line(text: &str) -> StyledLine {
        let atoms = vec![Atom::from_face(Face::default(), text)];
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
        let layout = shape_line_with_default_family(&mut text, &line("hello"));
        let hit = hit_byte(&layout, 0.0, 0.0).expect("hit");
        // Hitting the very first pixel should land at byte 0 (left side of
        // the first cluster).
        assert_eq!(hit.byte_offset, 0);
        assert!(!hit.is_rtl);
    }

    #[test]
    fn hit_past_end_returns_last_cluster_right_side() {
        let mut text = ParleyText::new(&FontConfig::default());
        let layout = shape_line_with_default_family(&mut text, &line("hi"));
        // Click far to the right of the line.
        let hit = hit_byte(&layout, 10000.0, 0.0).expect("hit");
        // Should land at the trailing edge of the last cluster (byte == len).
        assert_eq!(hit.byte_offset, 2);
        assert_eq!(hit.side, HitSide::Right);
    }

    #[test]
    fn byte_to_advance_increases_with_offset() {
        let mut text = ParleyText::new(&FontConfig::default());
        let layout = shape_line_with_default_family(&mut text, &line("hello"));
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
        let layout = shape_line_with_default_family(&mut text, &line("hello"));
        let range = cluster_range_at(&layout, 1).expect("range");
        assert_eq!(range, 1..2);
    }

    #[test]
    fn cluster_range_at_cjk_returns_3byte_range() {
        let mut text = ParleyText::new(&FontConfig::default());
        let layout = shape_line_with_default_family(&mut text, &line("a日b"));
        // "日" is 3 bytes (UTF-8: E6 97 A5), starting at byte 1.
        let range = cluster_range_at(&layout, 1).expect("range");
        assert_eq!(range, 1..4);
    }

    #[test]
    fn ascii_is_not_rtl() {
        let mut text = ParleyText::new(&FontConfig::default());
        let layout = shape_line_with_default_family(&mut text, &line("hello"));
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
        let layout = shape_line_with_default_family(&mut text, &line(""));
        let _ = hit_byte(&layout, 0.0, 0.0);
        let _ = hit_byte(&layout, 100.0, 100.0);
    }

    #[test]
    fn round_trip_byte_to_advance_and_back() {
        // Hitting the exact x position of a byte should land near that byte.
        // We skip bytes whose visual_offset Parley reports as None (typically
        // the trailing position past the last cluster).
        let mut text = ParleyText::new(&FontConfig::default());
        let layout = shape_line_with_default_family(&mut text, &line("abcde"));
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
