//! Display Transformation Foundation — maps between buffer lines and display lines.
//!
//! When plugins fold, hide, or insert virtual text, the display line count
//! diverges from the buffer line count. `DisplayMap` provides O(1) bidirectional
//! mapping between the two coordinate systems.

pub mod resolve;
#[cfg(test)]
mod tests;

use std::ops::Range;
use std::sync::Arc;

use crate::protocol::Atom;

pub use resolve::{DirectiveSet, TaggedDirective, resolve};

/// Plugin-declared display transformation directive.
#[derive(Debug, Clone, PartialEq)]
pub enum DisplayDirective {
    /// Collapse a range of buffer lines into a single summary line.
    Fold {
        range: Range<usize>,
        summary: Vec<Atom>,
    },
    /// Insert a virtual text line after the given buffer line.
    InsertAfter { after: usize, content: Vec<Atom> },
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
    pub atoms: Vec<Atom>,
}

impl SyntheticContent {
    /// Concatenate all atom contents into a single string (useful for tests).
    pub fn text(&self) -> String {
        self.atoms.iter().map(|a| a.contents.as_str()).collect()
    }
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
            // Identity maps of same size are equal;
            // INV-7 guarantees entries.len() == buffer_to_display.len()
            return self.entries.len() == other.entries.len();
        }
        self.buffer_to_display.len() == other.buffer_to_display.len()
            && self.entries == other.entries
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
        let dm = DisplayMap {
            entries,
            buffer_to_display,
            is_identity: true,
        };
        dm.check_invariants();
        dm
    }

    /// Build a DisplayMap from a set of directives applied to a buffer with
    /// `line_count` lines.
    ///
    /// Directives are processed in order. In the initial implementation, only
    /// a single plugin may contribute directives (`debug_assert!` enforced
    /// at the collection site).
    ///
    /// # Preconditions (debug-asserted)
    ///
    /// - No fold range overlaps any hide range
    /// - No empty fold ranges (`range.start < range.end`)
    ///
    /// Use [`resolve::resolve()`] to produce valid directives from multi-plugin input.
    pub fn build(line_count: usize, directives: &[DisplayDirective]) -> Self {
        if directives.is_empty() {
            return Self::identity(line_count);
        }

        #[cfg(debug_assertions)]
        {
            for d in directives {
                if let DisplayDirective::Fold { range, .. } = d {
                    debug_assert!(
                        range.start < range.end,
                        "build() precondition: empty fold range {range:?}"
                    );
                }
            }
            for d1 in directives {
                if let DisplayDirective::Fold { range: fold_r, .. } = d1 {
                    for d2 in directives {
                        if let DisplayDirective::Hide { range: hide_r } = d2 {
                            debug_assert!(
                                !(fold_r.start < hide_r.end && hide_r.start < fold_r.end),
                                "build() precondition: fold {fold_r:?} overlaps hide {hide_r:?}. \
                                 Use resolve() to produce valid directives."
                            );
                        }
                    }
                }
            }
        }

        // Track which buffer lines are affected by directives
        let mut folded: Vec<Option<(Range<usize>, Vec<Atom>)>> = vec![None; line_count];
        let mut hidden: Vec<bool> = vec![false; line_count];
        let mut insert_after: Vec<Vec<Vec<Atom>>> = vec![vec![]; line_count];

        for directive in directives {
            match directive {
                DisplayDirective::Fold { range, summary } => {
                    if range.start < line_count
                        && range.end <= line_count
                        && range.start < range.end
                    {
                        for item in folded.iter_mut().take(range.end).skip(range.start) {
                            *item = Some((range.clone(), summary.clone()));
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
                DisplayDirective::InsertAfter { after, content } => {
                    if *after < line_count {
                        insert_after[*after].push(content.clone());
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

            if let Some((ref range, ref summary)) = folded[line] {
                if !fold_emitted[range.start] {
                    // Emit the fold summary line (once per fold range)
                    let display_idx = entries.len();
                    entries.push(DisplayEntry {
                        source: SourceMapping::LineRange(range.clone()),
                        interaction: InteractionPolicy::ReadOnly,
                        synthetic: Some(SyntheticContent {
                            atoms: summary.clone(),
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
            for atoms in &insert_after[line] {
                entries.push(DisplayEntry {
                    source: SourceMapping::None,
                    interaction: InteractionPolicy::ReadOnly,
                    synthetic: Some(SyntheticContent {
                        atoms: atoms.clone(),
                    }),
                });
            }
        }

        let dm = DisplayMap {
            entries,
            buffer_to_display,
            is_identity: false,
        };
        dm.check_invariants();
        dm
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

    /// Verify structural invariants (INV-1 through INV-7) in debug builds.
    ///
    /// Called automatically at the end of `identity()` and `build()`.
    /// No-op in release builds.
    fn check_invariants(&self) {
        #[cfg(debug_assertions)]
        {
            // INV-7: Identity flag correctness
            if self.is_identity {
                debug_assert_eq!(
                    self.entries.len(),
                    self.buffer_to_display.len(),
                    "INV-7: identity entries.len() != buffer_to_display.len()"
                );
                for i in 0..self.entries.len() {
                    debug_assert!(
                        matches!(&self.entries[i].source, SourceMapping::BufferLine(bl) if *bl == i),
                        "INV-7: identity entries[{i}] is not BufferLine({i})"
                    );
                    debug_assert_eq!(
                        self.entries[i].interaction,
                        InteractionPolicy::Normal,
                        "INV-7: identity entries[{i}] is not Normal"
                    );
                    debug_assert!(
                        self.entries[i].synthetic.is_none(),
                        "INV-7: identity entries[{i}] has synthetic content"
                    );
                    debug_assert_eq!(
                        self.buffer_to_display[i],
                        Some(i),
                        "INV-7: identity b2d[{i}] != Some({i})"
                    );
                }
                return; // identity trivially satisfies INV-1..6
            }

            let n_buf = self.buffer_to_display.len();

            // INV-1: Forward-Backward consistency
            for bl in 0..n_buf {
                if let Some(dl) = self.buffer_to_display[bl] {
                    debug_assert!(
                        dl < self.entries.len(),
                        "INV-1: b2d[{bl}] = {dl} out of entries range"
                    );
                    match &self.entries[dl].source {
                        SourceMapping::BufferLine(b) => debug_assert_eq!(
                            *b, bl,
                            "INV-1: b2d[{bl}] = {dl} but entries[{dl}] = BufferLine({b})"
                        ),
                        SourceMapping::LineRange(r) => debug_assert!(
                            r.contains(&bl),
                            "INV-1: b2d[{bl}] = {dl} but entries[{dl}] = LineRange({r:?})"
                        ),
                        SourceMapping::None => {
                            panic!("INV-3: b2d[{bl}] = {dl} but entries[{dl}] = None (virtual)")
                        }
                    }
                }
            }

            // INV-2: Backward-Forward + INV-5: Monotonicity
            let mut prev_buf: Option<usize> = None;
            for dl in 0..self.entries.len() {
                match &self.entries[dl].source {
                    SourceMapping::BufferLine(bl) => {
                        debug_assert!(
                            *bl < n_buf,
                            "INV-2: entries[{dl}] = BufferLine({bl}) but line_count = {n_buf}"
                        );
                        debug_assert_eq!(
                            self.buffer_to_display[*bl],
                            Some(dl),
                            "INV-2: entries[{dl}] = BufferLine({bl}) but b2d[{bl}] = {:?}",
                            self.buffer_to_display[*bl]
                        );
                        if let Some(p) = prev_buf {
                            debug_assert!(
                                *bl > p,
                                "INV-5: non-monotonic: entries[{dl}] = BufferLine({bl}) after {p}"
                            );
                        }
                        prev_buf = Some(*bl);
                    }
                    SourceMapping::LineRange(r) => {
                        let end = r.end.min(n_buf);
                        for bl in r.start..end {
                            debug_assert_eq!(
                                self.buffer_to_display[bl],
                                Some(dl),
                                "INV-2: entries[{dl}] = LineRange({r:?}) but b2d[{bl}] = {:?}",
                                self.buffer_to_display[bl]
                            );
                        }
                        if let Some(p) = prev_buf {
                            debug_assert!(
                                r.start > p,
                                "INV-5: non-monotonic: entries[{dl}] = LineRange({r:?}) after {p}"
                            );
                        }
                        prev_buf = r.end.checked_sub(1);
                    }
                    SourceMapping::None => {} // INV-3 checked in INV-1 loop
                }
            }

            // INV-4: Injectivity — no buffer line covered by multiple entries
            let mut covered = vec![false; n_buf];
            for dl in 0..self.entries.len() {
                let range = match &self.entries[dl].source {
                    SourceMapping::BufferLine(bl) => *bl..*bl + 1,
                    SourceMapping::LineRange(r) => r.start..r.end.min(n_buf),
                    SourceMapping::None => continue,
                };
                for bl in range {
                    debug_assert!(
                        !covered[bl],
                        "INV-4: buffer line {bl} covered by multiple entries"
                    );
                    covered[bl] = true;
                }
            }

            // INV-6: Synthetic consistency
            for (dl, entry) in self.entries.iter().enumerate() {
                match &entry.source {
                    SourceMapping::None => debug_assert!(
                        entry.synthetic.is_some(),
                        "INV-6: entries[{dl}] = None but synthetic is None"
                    ),
                    SourceMapping::BufferLine(_) => debug_assert!(
                        entry.synthetic.is_none(),
                        "INV-6: entries[{dl}] = BufferLine but has synthetic"
                    ),
                    SourceMapping::LineRange(_) => {
                        // Fold summary: LineRange + synthetic.is_some() is the expected case
                        debug_assert!(
                            entry.synthetic.is_some(),
                            "INV-6: entries[{dl}] = LineRange but synthetic is None"
                        );
                    }
                }
            }
        }
    }
}

/// Compute the display scroll offset so the cursor remains visible.
///
/// When plugins insert virtual lines (e.g. `InsertAfter`), the display line
/// count may exceed the viewport height.  This function returns the first
/// display line that should be rendered so the cursor stays on-screen.
///
/// Returns 0 for identity maps or when the content fits in the viewport.
pub fn compute_display_scroll_offset(
    display_map: &DisplayMap,
    cursor_buffer_line: usize,
    visible_height: usize,
) -> usize {
    if display_map.is_identity() {
        return 0;
    }
    let display_total = display_map.display_line_count();
    if display_total <= visible_height {
        return 0;
    }
    let cursor_display_y = display_map
        .buffer_to_display(cursor_buffer_line)
        .unwrap_or(cursor_buffer_line);
    if cursor_display_y < visible_height {
        return 0;
    }
    let offset = cursor_display_y - visible_height + 1;
    let max_offset = display_total.saturating_sub(visible_height);
    offset.min(max_offset)
}

#[cfg(test)]
pub(crate) fn assert_display_map_invariants(dm: &DisplayMap, line_count: usize) {
    assert_eq!(
        dm.buffer_to_display.len(),
        line_count,
        "line_count mismatch"
    );

    if dm.is_identity {
        assert_eq!(dm.entries.len(), dm.buffer_to_display.len(), "INV-7");
        for i in 0..dm.entries.len() {
            assert!(
                matches!(&dm.entries[i].source, SourceMapping::BufferLine(bl) if *bl == i),
                "INV-7: entries[{i}]"
            );
            assert_eq!(
                dm.entries[i].interaction,
                InteractionPolicy::Normal,
                "INV-7"
            );
            assert!(dm.entries[i].synthetic.is_none(), "INV-7");
            assert_eq!(dm.buffer_to_display[i], Some(i), "INV-7");
        }
        return;
    }

    let n_buf = dm.buffer_to_display.len();

    // INV-1
    for bl in 0..n_buf {
        if let Some(dl) = dm.buffer_to_display[bl] {
            assert!(dl < dm.entries.len(), "INV-1: b2d[{bl}] out of range");
            match &dm.entries[dl].source {
                SourceMapping::BufferLine(b) => {
                    assert_eq!(
                        *b, bl,
                        "INV-1: b2d[{bl}] = {dl} but source = BufferLine({b})"
                    );
                }
                SourceMapping::LineRange(r) => {
                    assert!(
                        r.contains(&bl),
                        "INV-1: b2d[{bl}] = {dl} but source = LineRange({r:?})"
                    );
                }
                SourceMapping::None => {
                    panic!("INV-3: b2d[{bl}] = {dl} but source = None");
                }
            }
        }
    }

    // INV-2 + INV-5
    let mut prev_buf: Option<usize> = None;
    for dl in 0..dm.entries.len() {
        match &dm.entries[dl].source {
            SourceMapping::BufferLine(bl) => {
                assert!(*bl < n_buf, "INV-2: BufferLine({bl}) >= line_count {n_buf}");
                assert_eq!(
                    dm.buffer_to_display[*bl],
                    Some(dl),
                    "INV-2: entries[{dl}] = BufferLine({bl})"
                );
                if let Some(p) = prev_buf {
                    assert!(*bl > p, "INV-5: non-monotonic at entries[{dl}]");
                }
                prev_buf = Some(*bl);
            }
            SourceMapping::LineRange(r) => {
                let end = r.end.min(n_buf);
                for bl in r.start..end {
                    assert_eq!(
                        dm.buffer_to_display[bl],
                        Some(dl),
                        "INV-2: entries[{dl}] = LineRange({r:?})"
                    );
                }
                if let Some(p) = prev_buf {
                    assert!(r.start > p, "INV-5: non-monotonic at entries[{dl}]");
                }
                prev_buf = r.end.checked_sub(1);
            }
            SourceMapping::None => {}
        }
    }

    // INV-4
    let mut covered = vec![false; n_buf];
    for dl in 0..dm.entries.len() {
        let range = match &dm.entries[dl].source {
            SourceMapping::BufferLine(bl) => *bl..*bl + 1,
            SourceMapping::LineRange(r) => r.start..r.end.min(n_buf),
            SourceMapping::None => continue,
        };
        for bl in range {
            assert!(!covered[bl], "INV-4: buffer line {bl} covered twice");
            covered[bl] = true;
        }
    }

    // INV-6
    for (dl, entry) in dm.entries.iter().enumerate() {
        match &entry.source {
            SourceMapping::None => {
                assert!(
                    entry.synthetic.is_some(),
                    "INV-6: entries[{dl}] None without synthetic"
                );
            }
            SourceMapping::BufferLine(_) => {
                assert!(
                    entry.synthetic.is_none(),
                    "INV-6: entries[{dl}] BufferLine with synthetic"
                );
            }
            SourceMapping::LineRange(_) => {
                assert!(
                    entry.synthetic.is_some(),
                    "INV-6: entries[{dl}] LineRange without synthetic"
                );
            }
        }
    }
}
