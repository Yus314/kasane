//! Display Transformation Foundation — maps between buffer lines and display lines.
//!
//! When plugins fold, hide, or insert virtual text, the display line count
//! diverges from the buffer line count. `DisplayMap` provides O(1) bidirectional
//! mapping between the two coordinate systems.

#[cfg(test)]
mod tests;

use std::ops::Range;
use std::sync::Arc;

use crate::protocol::Face;

/// Plugin-declared display transformation directive.
#[derive(Debug, Clone, PartialEq)]
pub enum DisplayDirective {
    /// Collapse a range of buffer lines into a single summary line.
    Fold {
        range: Range<usize>,
        summary: String,
        face: Face,
    },
    /// Insert a virtual text line after the given buffer line.
    InsertAfter {
        after: usize,
        content: String,
        face: Face,
    },
    /// Hide a range of buffer lines entirely.
    Hide { range: Range<usize> },
}

/// Maps a display line back to its buffer origin.
#[derive(Debug, Clone, PartialEq)]
pub enum SourceMapping {
    /// This display line corresponds to exactly one buffer line.
    BufferLine(usize),
    /// This display line represents a folded range of buffer lines.
    LineRange(Range<usize>),
    /// This display line is virtual text (no buffer origin).
    None,
}

/// How interactions (clicks, cursor) behave on a display line.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InteractionPolicy {
    /// Normal editing behavior.
    Normal,
    /// Display-only, clicks are ignored.
    ReadOnly,
    /// Skip entirely during navigation.
    Skip,
}

/// Content for synthetic (non-buffer) display lines.
#[derive(Debug, Clone, PartialEq)]
pub struct SyntheticContent {
    pub text: String,
    pub face: Face,
}

/// A single entry in the DisplayMap, representing one display line.
#[derive(Debug, Clone, PartialEq)]
pub struct DisplayEntry {
    pub source: SourceMapping,
    pub interaction: InteractionPolicy,
    pub synthetic: Option<SyntheticContent>,
}

/// Bidirectional mapping between display lines and buffer lines.
///
/// Identity maps (no transformations) are marked with `is_identity` for
/// zero-cost fast paths throughout the rendering pipeline.
#[derive(Debug, Clone)]
pub struct DisplayMap {
    entries: Vec<DisplayEntry>,
    /// buffer_line → display_line (None if the line is hidden/folded)
    buffer_to_display: Vec<Option<usize>>,
    is_identity: bool,
}

/// Shared reference to a DisplayMap.
pub type DisplayMapRef = Arc<DisplayMap>;

impl PartialEq for DisplayMap {
    fn eq(&self, other: &Self) -> bool {
        if self.is_identity && other.is_identity {
            // Identity maps of same size are equal
            return self.entries.len() == other.entries.len();
        }
        self.entries == other.entries
    }
}

impl DisplayMap {
    /// Create an identity mapping for `n` buffer lines.
    ///
    /// Every display line maps 1:1 to the corresponding buffer line.
    pub fn identity(n: usize) -> Self {
        let entries: Vec<DisplayEntry> = (0..n)
            .map(|i| DisplayEntry {
                source: SourceMapping::BufferLine(i),
                interaction: InteractionPolicy::Normal,
                synthetic: None,
            })
            .collect();
        let buffer_to_display: Vec<Option<usize>> = (0..n).map(Some).collect();
        DisplayMap {
            entries,
            buffer_to_display,
            is_identity: true,
        }
    }

    /// Build a DisplayMap from a set of directives applied to a buffer with
    /// `line_count` lines.
    ///
    /// Directives are processed in order. In the initial implementation, only
    /// a single plugin may contribute directives (`debug_assert!` enforced
    /// at the collection site).
    pub fn build(line_count: usize, directives: &[DisplayDirective]) -> Self {
        if directives.is_empty() {
            return Self::identity(line_count);
        }

        // Track which buffer lines are affected by directives
        let mut folded: Vec<Option<(Range<usize>, String, Face)>> = vec![None; line_count];
        let mut hidden: Vec<bool> = vec![false; line_count];
        let mut insert_after: Vec<Vec<(String, Face)>> = vec![vec![]; line_count];

        for directive in directives {
            match directive {
                DisplayDirective::Fold {
                    range,
                    summary,
                    face,
                } => {
                    if range.start < line_count
                        && range.end <= line_count
                        && range.start < range.end
                    {
                        for item in folded.iter_mut().take(range.end).skip(range.start) {
                            *item = Some((range.clone(), summary.clone(), *face));
                        }
                    }
                }
                DisplayDirective::Hide { range } => {
                    if range.start < line_count && range.end <= line_count {
                        for item in hidden.iter_mut().take(range.end).skip(range.start) {
                            *item = true;
                        }
                    }
                }
                DisplayDirective::InsertAfter {
                    after,
                    content,
                    face,
                } => {
                    if *after < line_count {
                        insert_after[*after].push((content.clone(), *face));
                    }
                }
            }
        }

        let mut entries = Vec::new();
        let mut buffer_to_display = vec![None; line_count];
        let mut fold_emitted: Vec<bool> = vec![false; line_count];

        for line in 0..line_count {
            if hidden[line] {
                // Hidden lines produce no display entry
                continue;
            }

            if let Some((ref range, ref summary, face)) = folded[line] {
                if !fold_emitted[range.start] {
                    // Emit the fold summary line (once per fold range)
                    let display_idx = entries.len();
                    entries.push(DisplayEntry {
                        source: SourceMapping::LineRange(range.clone()),
                        interaction: InteractionPolicy::ReadOnly,
                        synthetic: Some(SyntheticContent {
                            text: summary.clone(),
                            face,
                        }),
                    });
                    // Map all lines in the fold range to the summary display line
                    let end = range.end.min(line_count);
                    for i in range.start..end {
                        buffer_to_display[i] = Some(display_idx);
                        fold_emitted[i] = true;
                    }
                }
                // Other lines in the fold range are consumed (no display entry)
                continue;
            }

            // Normal buffer line
            let display_idx = entries.len();
            entries.push(DisplayEntry {
                source: SourceMapping::BufferLine(line),
                interaction: InteractionPolicy::Normal,
                synthetic: None,
            });
            buffer_to_display[line] = Some(display_idx);

            // InsertAfter: add virtual lines after this buffer line
            for (content, face) in &insert_after[line] {
                entries.push(DisplayEntry {
                    source: SourceMapping::None,
                    interaction: InteractionPolicy::ReadOnly,
                    synthetic: Some(SyntheticContent {
                        text: content.clone(),
                        face: *face,
                    }),
                });
            }
        }

        DisplayMap {
            entries,
            buffer_to_display,
            is_identity: false,
        }
    }

    /// Returns true if this is an identity mapping (no transformations).
    pub fn is_identity(&self) -> bool {
        self.is_identity
    }

    /// Number of display lines.
    pub fn display_line_count(&self) -> usize {
        self.entries.len()
    }

    /// Map a display line index to the corresponding buffer line (O(1)).
    ///
    /// Returns `None` for virtual text lines.
    pub fn display_to_buffer(&self, display_y: usize) -> Option<usize> {
        self.entries
            .get(display_y)
            .and_then(|entry| match &entry.source {
                SourceMapping::BufferLine(line) => Some(*line),
                SourceMapping::LineRange(range) => Some(range.start),
                SourceMapping::None => None,
            })
    }

    /// Map a buffer line to its display line (O(1)).
    ///
    /// Returns `None` if the buffer line is hidden or folded away.
    pub fn buffer_to_display(&self, buffer_line: usize) -> Option<usize> {
        self.buffer_to_display.get(buffer_line).copied().flatten()
    }

    /// Get the display entry for a given display line (O(1)).
    pub fn entry(&self, display_y: usize) -> Option<&DisplayEntry> {
        self.entries.get(display_y)
    }

    /// Check if a display line is dirty based on the buffer's `lines_dirty` flags.
    ///
    /// For fold summary lines, returns true if any buffer line in the fold range is dirty.
    /// For virtual text lines, always returns false (they don't change from buffer edits).
    /// For identity maps, delegates directly to the `lines_dirty` array.
    pub fn is_display_line_dirty(&self, display_y: usize, lines_dirty: &[bool]) -> bool {
        let Some(entry) = self.entries.get(display_y) else {
            return true; // out of bounds → treat as dirty
        };
        match &entry.source {
            SourceMapping::BufferLine(line) => lines_dirty.get(*line).copied().unwrap_or(true),
            SourceMapping::LineRange(range) => range
                .clone()
                .any(|l| lines_dirty.get(l).copied().unwrap_or(true)),
            SourceMapping::None => false,
        }
    }
}
