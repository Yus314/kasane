use unicode_segmentation::UnicodeSegmentation;
use unicode_width::UnicodeWidthStr;

use super::text::is_word_char;
use crate::protocol::Line;

/// A segment of graphemes that fits on one visual row after word wrapping.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WrapSegment {
    /// Grapheme index (inclusive).
    pub start: usize,
    /// Grapheme index (exclusive).
    pub end: usize,
}

/// Collect per-grapheme metrics: `(display_width, is_word_boundary)`.
///
/// Shared by [`word_wrap_line_height`] and [`word_wrap_max_row_width`] to
/// avoid duplicating the grapheme-iteration + width-measurement loop.
pub(super) fn collect_metrics(line: &Line) -> Vec<(u16, bool)> {
    let mut metrics: Vec<(u16, bool)> = Vec::new();
    for atom in line {
        for grapheme in atom.contents.graphemes(true) {
            if grapheme.is_empty() {
                continue;
            }
            if grapheme.starts_with(|c: char| c.is_control()) {
                continue;
            }
            let w = UnicodeWidthStr::width(grapheme) as u16;
            if w == 0 {
                continue;
            }
            metrics.push((w, !is_word_char(grapheme)));
        }
    }
    metrics
}

/// Compute word-boundary-aware wrap segments from per-grapheme metrics.
///
/// Each segment represents one visual row: the half-open range `[start, end)`
/// of grapheme indices that fit on that row. The wrap logic matches Kakoune's
/// `wrap_lines` (overflow check → word-boundary backtrack → forced placement).
pub fn word_wrap_segments(metrics: &[(u16, bool)], max_width: u16) -> Vec<WrapSegment> {
    // Caller must ensure max_width > 0.
    if metrics.is_empty() {
        return vec![WrapSegment { start: 0, end: 0 }];
    }

    let mut segments = Vec::new();
    let mut row_start = 0usize;
    let mut col = 0u16;
    let mut last_break_idx: Option<usize> = None;
    let mut i = 0;

    while i < metrics.len() {
        let (w, is_boundary) = metrics[i];

        if col + w > max_width {
            if col == 0 {
                // Single grapheme wider than max_width: force-place it.
                segments.push(WrapSegment {
                    start: row_start,
                    end: i + 1,
                });
                row_start = i + 1;
                i += 1;
                last_break_idx = None;
                continue;
            }
            if let Some(brk) = last_break_idx {
                segments.push(WrapSegment {
                    start: row_start,
                    end: brk,
                });
                row_start = brk;
                i = brk;
                last_break_idx = None;
            } else {
                segments.push(WrapSegment {
                    start: row_start,
                    end: i,
                });
                row_start = i;
            }
            col = 0;
            continue;
        }

        col += w;
        if is_boundary {
            last_break_idx = Some(i + 1);
        }
        i += 1;
    }

    // Final segment (skip if the last grapheme was force-placed and nothing remains).
    if row_start < metrics.len() {
        segments.push(WrapSegment {
            start: row_start,
            end: metrics.len(),
        });
    }
    segments
}

/// Compute the number of visual rows a line occupies when wrapped at word boundaries
/// (matching Kakoune's `wrap_lines`). Returns at least 1 for non-empty lines.
pub fn word_wrap_line_height(line: &Line, max_width: u16) -> u16 {
    if max_width == 0 {
        return 1;
    }
    let metrics = collect_metrics(line);
    word_wrap_segments(&metrics, max_width).len() as u16
}

/// Return the maximum display width of any row after word-wrapping a line
/// at `max_width` (matching Kakoune's `compute_size` after `wrap_lines`).
///
/// This is the width counterpart of [`word_wrap_line_height`]: same wrapping
/// logic, but tracks the widest row instead of counting rows.
pub fn word_wrap_max_row_width(line: &Line, max_width: u16) -> u16 {
    if max_width == 0 {
        return 0;
    }
    let metrics = collect_metrics(line);
    let segments = word_wrap_segments(&metrics, max_width);
    segments
        .iter()
        .map(|seg| {
            metrics[seg.start..seg.end]
                .iter()
                .map(|(w, _)| w)
                .sum::<u16>()
        })
        .max()
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_utils::make_line;

    // ----- word_wrap_line_height tests -----

    #[test]
    fn test_word_wrap_no_wrap() {
        let line = make_line("hello world");
        assert_eq!(word_wrap_line_height(&line, 20), 1);
    }

    #[test]
    fn test_word_wrap_at_word_boundary() {
        // "hello world" (11 chars) in 8-col width
        // Should break at the space: "hello " (6) + "world" (5)
        let line = make_line("hello world");
        assert_eq!(word_wrap_line_height(&line, 8), 2);
    }

    #[test]
    fn test_word_wrap_no_boundary_forces_char_break() {
        // "abcdefghij" (10 chars) in 5-col width — no spaces, forced break
        let line = make_line("abcdefghij");
        assert_eq!(word_wrap_line_height(&line, 5), 2);
    }

    #[test]
    fn test_word_wrap_multiple_rows() {
        // "aa bb cc dd ee" in 6-col width
        // Row 1: "aa bb " (6), Row 2: "cc dd " (6), Row 3: "ee" (2)
        let line = make_line("aa bb cc dd ee");
        assert_eq!(word_wrap_line_height(&line, 6), 3);
    }

    #[test]
    fn test_word_wrap_empty_line() {
        let line = make_line("");
        assert_eq!(word_wrap_line_height(&line, 10), 1);
    }

    #[test]
    fn test_word_wrap_exact_fit() {
        // "hello" in 5-col width — exactly fits, no wrap
        let line = make_line("hello");
        assert_eq!(word_wrap_line_height(&line, 5), 1);
    }

    // ----- word_wrap_max_row_width tests -----

    #[test]
    fn test_max_row_width_no_wrap() {
        // Short line that fits — returns raw width
        let line = make_line("hello");
        assert_eq!(word_wrap_max_row_width(&line, 20), 5);
    }

    #[test]
    fn test_max_row_width_word_boundary() {
        // "hello world" (11 chars) wraps at 8: "hello " (6) + "world" (5) → max = 6
        let line = make_line("hello world");
        assert_eq!(word_wrap_max_row_width(&line, 8), 6);
    }

    #[test]
    fn test_max_row_width_forced_break() {
        // "abcdefghij" (10 chars, no spaces) in 5-col: forced break at 5 → max = 5
        let line = make_line("abcdefghij");
        assert_eq!(word_wrap_max_row_width(&line, 5), 5);
    }

    #[test]
    fn test_max_row_width_empty_line() {
        let line = make_line("");
        assert_eq!(word_wrap_max_row_width(&line, 10), 0);
    }

    #[test]
    fn test_max_row_width_multiple_rows() {
        // "aa bb cc dd ee" in 6-col: "aa bb " (6) + "cc dd " (6) + "ee" (2) → max = 6
        let line = make_line("aa bb cc dd ee");
        assert_eq!(word_wrap_max_row_width(&line, 6), 6);
    }

    #[test]
    fn test_max_row_width_less_than_budget() {
        // Verify that wrapped width can be strictly less than max_width.
        // "aaa bbbb" in 10-col: fits entirely (8 chars) → 8 < 10 ✓
        let line = make_line("aaa bbbb");
        assert_eq!(word_wrap_max_row_width(&line, 10), 8);
    }

    // ----- word_wrap_segments tests -----

    #[test]
    fn test_segments_empty_input() {
        let segments = word_wrap_segments(&[], 10);
        assert_eq!(segments, vec![WrapSegment { start: 0, end: 0 }]);
    }

    #[test]
    fn test_segments_no_wrap() {
        // "hello" — 5 graphemes, all word chars, fits in 10 cols
        let metrics: Vec<(u16, bool)> = vec![(1, false); 5];
        let segments = word_wrap_segments(&metrics, 10);
        assert_eq!(segments, vec![WrapSegment { start: 0, end: 5 }]);
    }

    #[test]
    fn test_segments_word_boundary_wrap() {
        // "hello world" — break at space: "hello " (6) + "world" (5)
        // h(1,f) e(1,f) l(1,f) l(1,f) o(1,f) ' '(1,t) w(1,f) o(1,f) r(1,f) l(1,f) d(1,f)
        let metrics: Vec<(u16, bool)> = vec![
            (1, false),
            (1, false),
            (1, false),
            (1, false),
            (1, false),
            (1, true), // space
            (1, false),
            (1, false),
            (1, false),
            (1, false),
            (1, false),
        ];
        let segments = word_wrap_segments(&metrics, 8);
        assert_eq!(
            segments,
            vec![
                WrapSegment { start: 0, end: 6 },
                WrapSegment { start: 6, end: 11 },
            ]
        );
    }

    #[test]
    fn test_segments_forced_break_no_boundary() {
        // "abcdefghij" — 10 chars, no boundaries, max_width=5
        let metrics: Vec<(u16, bool)> = vec![(1, false); 10];
        let segments = word_wrap_segments(&metrics, 5);
        assert_eq!(
            segments,
            vec![
                WrapSegment { start: 0, end: 5 },
                WrapSegment { start: 5, end: 10 },
            ]
        );
    }

    #[test]
    fn test_segments_multiple_rows() {
        // "aa bb cc dd ee" in 6-col → 3 segments
        // a(1,f) a(1,f) ' '(1,t) b(1,f) b(1,f) ' '(1,t) c(1,f) c(1,f) ' '(1,t)
        // d(1,f) d(1,f) ' '(1,t) e(1,f) e(1,f)
        let metrics: Vec<(u16, bool)> = vec![
            (1, false),
            (1, false),
            (1, true), // "aa "
            (1, false),
            (1, false),
            (1, true), // "bb "
            (1, false),
            (1, false),
            (1, true), // "cc "
            (1, false),
            (1, false),
            (1, true), // "dd "
            (1, false),
            (1, false), // "ee"
        ];
        let segments = word_wrap_segments(&metrics, 6);
        assert_eq!(
            segments,
            vec![
                WrapSegment { start: 0, end: 6 },
                WrapSegment { start: 6, end: 12 },
                WrapSegment { start: 12, end: 14 },
            ]
        );
    }

    #[test]
    fn test_segments_wide_grapheme_force_place() {
        // Single wide grapheme (width=3) in max_width=2 → forced placement
        let metrics: Vec<(u16, bool)> = vec![(3, false)];
        let segments = word_wrap_segments(&metrics, 2);
        assert_eq!(segments, vec![WrapSegment { start: 0, end: 1 }]);
    }

    #[test]
    fn test_segments_wide_grapheme_after_content() {
        // "a" (1) then wide (3) in max_width=2
        // 'a' fits (col=1), wide doesn't (1+3>2), no boundary → forced break
        // row1: [0,1) = "a", row2: wide forced → [1,2)
        let metrics: Vec<(u16, bool)> = vec![(1, false), (3, false)];
        let segments = word_wrap_segments(&metrics, 2);
        assert_eq!(
            segments,
            vec![
                WrapSegment { start: 0, end: 1 },
                WrapSegment { start: 1, end: 2 },
            ]
        );
    }

    // ----- grapheme cluster tests -----

    #[test]
    fn test_collect_metrics_combining_character() {
        // "e\u{0301}x" → 2 graphemes ("é" and "x"), not 3
        let line = make_line("e\u{0301}x");
        let metrics = collect_metrics(&line);
        assert_eq!(metrics.len(), 2);
    }

    #[test]
    fn test_word_wrap_combining_character() {
        // "é" is a single grapheme of width 1, total "éx" fits in any reasonable width
        let line = make_line("e\u{0301}x");
        assert_eq!(word_wrap_line_height(&line, 10), 1);
    }

    #[test]
    fn test_word_wrap_combining_character_width() {
        // "éx" → 2 display columns
        let line = make_line("e\u{0301}x");
        assert_eq!(word_wrap_max_row_width(&line, 10), 2);
    }
}
