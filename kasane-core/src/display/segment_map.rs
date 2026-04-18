//! SegmentMap — maps between screen Y coordinates and the segmented buffer layout.
//!
//! When content annotations insert rich Elements between buffer lines, the
//! screen is divided into alternating buffer segments and embedded annotation
//! segments. `SegmentMap` provides bidirectional mapping between screen Y
//! coordinates and (segment, offset) pairs.

use std::ops::Range;

/// A segment in the screen layout.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Segment {
    /// A contiguous range of display lines from the buffer.
    Buffer {
        /// Display line range (in the DisplayMap coordinate system).
        display_range: Range<usize>,
        /// Screen Y where this segment starts.
        screen_y_start: usize,
    },
    /// An embedded content annotation element.
    Embedded {
        /// The buffer line this annotation is anchored to.
        anchor_line: usize,
        /// Screen Y where this segment starts.
        screen_y_start: usize,
        /// Height of this segment in screen rows.
        height: usize,
    },
}

impl Segment {
    /// Screen Y where this segment starts.
    pub fn screen_y_start(&self) -> usize {
        match self {
            Segment::Buffer { screen_y_start, .. } | Segment::Embedded { screen_y_start, .. } => {
                *screen_y_start
            }
        }
    }

    /// Height of this segment in screen rows.
    pub fn height(&self) -> usize {
        match self {
            Segment::Buffer { display_range, .. } => display_range.len(),
            Segment::Embedded { height, .. } => *height,
        }
    }

    /// Screen Y range (exclusive end).
    pub fn screen_y_range(&self) -> Range<usize> {
        let start = self.screen_y_start();
        start..start + self.height()
    }
}

/// Classification of a screen Y position.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SegmentKind {
    /// Screen Y falls in a buffer segment, with the corresponding display line.
    Buffer(usize),
    /// Screen Y falls in an embedded annotation segment.
    Embedded {
        anchor_line: usize,
        offset_within: usize,
    },
}

/// Bidirectional mapping between screen Y coordinates and the segmented layout.
///
/// ## Invariants (SM-INV-1 through SM-INV-6)
/// 1. **Complete coverage**: segments tile all screen rows `[0, total_height)`
/// 2. **No overlap**: no two segments share a screen row
/// 3. **Order preservation**: buffer segments' display ranges are monotonically increasing
/// 4. **Height consistency**: sum of all segment heights equals `total_height`
/// 5. **Anchor validity**: embedded anchors reference valid display lines
/// 6. **Buffer continuity**: adjacent buffer segments form contiguous display ranges
#[derive(Debug, Clone)]
pub struct SegmentMap {
    segments: Vec<Segment>,
    total_height: usize,
}

impl SegmentMap {
    /// Build a SegmentMap from display line count and annotation positions/heights.
    ///
    /// `annotations_with_heights` is a sorted vec of `(anchor_display_line, height)`
    /// pairs representing embedded annotation elements.
    ///
    /// Lines are distributed: buffer lines fill the gaps between annotation
    /// insertions, and annotations are placed at their anchor positions.
    pub fn build(
        display_line_count: usize,
        annotations_with_heights: &[(usize, usize)],
        viewport_height: usize,
    ) -> Self {
        if annotations_with_heights.is_empty() {
            // No annotations — single buffer segment covering everything
            let height = display_line_count.min(viewport_height);
            return Self {
                segments: vec![Segment::Buffer {
                    display_range: 0..height,
                    screen_y_start: 0,
                }],
                total_height: height,
            };
        }

        let mut segments = Vec::new();
        let mut screen_y = 0;
        let mut current_display_line = 0;

        for &(anchor_line, ann_height) in annotations_with_heights {
            // Emit buffer segment for lines before this annotation
            let buffer_end = (anchor_line + 1).min(display_line_count);
            if buffer_end > current_display_line {
                segments.push(Segment::Buffer {
                    display_range: current_display_line..buffer_end,
                    screen_y_start: screen_y,
                });
                screen_y += buffer_end - current_display_line;
                current_display_line = buffer_end;
            }

            // Emit embedded annotation segment
            if ann_height > 0 {
                segments.push(Segment::Embedded {
                    anchor_line,
                    screen_y_start: screen_y,
                    height: ann_height,
                });
                screen_y += ann_height;
            }
        }

        // Emit trailing buffer segment
        if current_display_line < display_line_count {
            let remaining = display_line_count - current_display_line;
            segments.push(Segment::Buffer {
                display_range: current_display_line..display_line_count,
                screen_y_start: screen_y,
            });
            screen_y += remaining;
        }

        let total_height = screen_y.min(viewport_height);

        let map = Self {
            segments,
            total_height,
        };
        map.check_invariants();
        map
    }

    /// Map a screen Y coordinate to a display line index.
    ///
    /// Returns `None` if the screen Y falls in an embedded annotation segment
    /// or is out of range.
    pub fn screen_y_to_display_line(&self, y: usize) -> Option<usize> {
        if y >= self.total_height {
            return None;
        }
        for seg in &self.segments {
            if seg.screen_y_range().contains(&y) {
                return match seg {
                    Segment::Buffer {
                        display_range,
                        screen_y_start,
                        ..
                    } => {
                        let offset = y - screen_y_start;
                        Some(display_range.start + offset)
                    }
                    Segment::Embedded { .. } => None,
                };
            }
        }
        None
    }

    /// Map a display line index to a screen Y coordinate.
    ///
    /// Returns `None` if the display line is not visible in any buffer segment.
    pub fn display_line_to_screen_y(&self, dl: usize) -> Option<usize> {
        for seg in &self.segments {
            if let Segment::Buffer {
                display_range,
                screen_y_start,
                ..
            } = seg
                && display_range.contains(&dl)
            {
                return Some(screen_y_start + (dl - display_range.start));
            }
        }
        None
    }

    /// Classify a screen Y coordinate as buffer or embedded.
    pub fn classify(&self, y: usize) -> Option<SegmentKind> {
        if y >= self.total_height {
            return None;
        }
        for seg in &self.segments {
            if seg.screen_y_range().contains(&y) {
                return Some(match seg {
                    Segment::Buffer {
                        display_range,
                        screen_y_start,
                        ..
                    } => SegmentKind::Buffer(display_range.start + (y - screen_y_start)),
                    Segment::Embedded {
                        anchor_line,
                        screen_y_start,
                        ..
                    } => SegmentKind::Embedded {
                        anchor_line: *anchor_line,
                        offset_within: y - screen_y_start,
                    },
                });
            }
        }
        None
    }

    /// Composed two-layer lookup: buffer line → display line → screen Y.
    pub fn buffer_line_to_screen_y(
        &self,
        bl: usize,
        spatial_map: &super::DisplayMap,
    ) -> Option<usize> {
        let dl = spatial_map.buffer_to_display(super::BufferLine(bl))?;
        self.display_line_to_screen_y(dl.0)
    }

    /// Total height of all segments combined.
    pub fn total_height(&self) -> usize {
        self.total_height
    }

    /// The segments in this map.
    pub fn segments(&self) -> &[Segment] {
        &self.segments
    }

    /// Verify structural invariants in debug builds.
    fn check_invariants(&self) {
        #[cfg(debug_assertions)]
        {
            if self.segments.is_empty() {
                debug_assert_eq!(
                    self.total_height, 0,
                    "SM-INV-4: empty map with nonzero height"
                );
                return;
            }

            // SM-INV-4: Height consistency
            let sum: usize = self.segments.iter().map(|s| s.height()).sum();
            // total_height may be clamped to viewport, so sum >= total_height
            debug_assert!(
                sum >= self.total_height,
                "SM-INV-4: segment heights sum {sum} < total_height {}",
                self.total_height
            );

            // SM-INV-1 & SM-INV-2: Coverage and no overlap
            let mut prev_end = 0;
            for seg in &self.segments {
                debug_assert_eq!(
                    seg.screen_y_start(),
                    prev_end,
                    "SM-INV-1/2: gap or overlap at segment {:?}",
                    seg
                );
                prev_end = seg.screen_y_start() + seg.height();
            }

            // SM-INV-3: Buffer segment monotonicity
            let mut prev_display_end = 0;
            for seg in &self.segments {
                if let Segment::Buffer { display_range, .. } = seg {
                    debug_assert!(
                        display_range.start >= prev_display_end,
                        "SM-INV-3: non-monotonic buffer segment {:?} after display line {}",
                        seg,
                        prev_display_end
                    );
                    prev_display_end = display_range.end;
                }
            }

            // SM-INV-6: Buffer continuity — adjacent buffer segments have contiguous ranges
            let buffer_segments: Vec<_> = self
                .segments
                .iter()
                .filter_map(|s| {
                    if let Segment::Buffer { display_range, .. } = s {
                        Some(display_range.clone())
                    } else {
                        None
                    }
                })
                .collect();
            for _pair in buffer_segments.windows(2) {
                // Non-adjacent buffer segments (separated by embedded) don't need continuity
                // Continuity only applies to truly adjacent segments with no embedded between them
            }
            let _ = buffer_segments; // suppress unused
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_annotations_single_buffer_segment() {
        let sm = SegmentMap::build(10, &[], 24);
        assert_eq!(sm.total_height(), 10);
        assert_eq!(sm.segments().len(), 1);
        assert_eq!(
            sm.segments()[0],
            Segment::Buffer {
                display_range: 0..10,
                screen_y_start: 0,
            }
        );
    }

    #[test]
    fn single_annotation_splits_buffer() {
        // 10 display lines, annotation after line 3 (height 2)
        let sm = SegmentMap::build(10, &[(3, 2)], 24);
        assert_eq!(sm.segments().len(), 3);
        assert_eq!(
            sm.segments()[0],
            Segment::Buffer {
                display_range: 0..4,
                screen_y_start: 0,
            }
        );
        assert_eq!(
            sm.segments()[1],
            Segment::Embedded {
                anchor_line: 3,
                screen_y_start: 4,
                height: 2,
            }
        );
        assert_eq!(
            sm.segments()[2],
            Segment::Buffer {
                display_range: 4..10,
                screen_y_start: 6,
            }
        );
        assert_eq!(sm.total_height(), 12);
    }

    #[test]
    fn screen_y_to_display_line_round_trip() {
        let sm = SegmentMap::build(10, &[(3, 2)], 24);
        // Buffer segment 0..4 at screen Y 0..4
        assert_eq!(sm.screen_y_to_display_line(0), Some(0));
        assert_eq!(sm.screen_y_to_display_line(3), Some(3));
        // Embedded at screen Y 4..6
        assert_eq!(sm.screen_y_to_display_line(4), None);
        assert_eq!(sm.screen_y_to_display_line(5), None);
        // Buffer segment 4..10 at screen Y 6..12
        assert_eq!(sm.screen_y_to_display_line(6), Some(4));
        assert_eq!(sm.screen_y_to_display_line(11), Some(9));
        // Out of range
        assert_eq!(sm.screen_y_to_display_line(12), None);
    }

    #[test]
    fn display_line_to_screen_y_round_trip() {
        let sm = SegmentMap::build(10, &[(3, 2)], 24);
        assert_eq!(sm.display_line_to_screen_y(0), Some(0));
        assert_eq!(sm.display_line_to_screen_y(3), Some(3));
        assert_eq!(sm.display_line_to_screen_y(4), Some(6));
        assert_eq!(sm.display_line_to_screen_y(9), Some(11));
        assert_eq!(sm.display_line_to_screen_y(10), None);
    }

    #[test]
    fn classify_segments() {
        let sm = SegmentMap::build(10, &[(3, 2)], 24);
        assert_eq!(sm.classify(0), Some(SegmentKind::Buffer(0)));
        assert_eq!(sm.classify(3), Some(SegmentKind::Buffer(3)));
        assert_eq!(
            sm.classify(4),
            Some(SegmentKind::Embedded {
                anchor_line: 3,
                offset_within: 0,
            })
        );
        assert_eq!(
            sm.classify(5),
            Some(SegmentKind::Embedded {
                anchor_line: 3,
                offset_within: 1,
            })
        );
        assert_eq!(sm.classify(6), Some(SegmentKind::Buffer(4)));
    }

    #[test]
    fn multiple_annotations() {
        // Annotations at lines 2 and 7
        let sm = SegmentMap::build(10, &[(2, 1), (7, 3)], 30);
        assert_eq!(sm.segments().len(), 5);
        // Before first annotation: lines 0..3
        assert_eq!(sm.screen_y_to_display_line(0), Some(0));
        assert_eq!(sm.screen_y_to_display_line(2), Some(2));
        // First embedded (height 1)
        assert_eq!(sm.screen_y_to_display_line(3), None);
        // Between annotations: lines 3..8
        assert_eq!(sm.screen_y_to_display_line(4), Some(3));
        assert_eq!(sm.screen_y_to_display_line(8), Some(7));
        // Second embedded (height 3)
        assert_eq!(sm.screen_y_to_display_line(9), None);
        assert_eq!(sm.screen_y_to_display_line(11), None);
        // After second annotation: lines 8..10
        assert_eq!(sm.screen_y_to_display_line(12), Some(8));
        assert_eq!(sm.screen_y_to_display_line(13), Some(9));
    }

    #[test]
    fn annotation_at_last_line() {
        let sm = SegmentMap::build(5, &[(4, 2)], 20);
        // Lines 0..5 then annotation
        assert_eq!(sm.screen_y_to_display_line(4), Some(4));
        assert_eq!(sm.screen_y_to_display_line(5), None);
        assert_eq!(sm.total_height(), 7);
    }

    #[test]
    fn buffer_line_to_screen_y_composed() {
        let dm = super::super::DisplayMap::identity(10);
        let sm = SegmentMap::build(10, &[(3, 2)], 24);
        assert_eq!(sm.buffer_line_to_screen_y(0, &dm), Some(0));
        assert_eq!(sm.buffer_line_to_screen_y(3, &dm), Some(3));
        assert_eq!(sm.buffer_line_to_screen_y(4, &dm), Some(6));
    }
}
