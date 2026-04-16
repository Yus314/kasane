//! Display Transformation Foundation — maps between buffer lines and display lines.
//!
//! When plugins fold, hide, or insert virtual text, the display line count
//! diverges from the buffer line count. `DisplayMap` provides O(1) bidirectional
//! mapping between the two coordinate systems.

pub mod fold_state;
pub mod navigation;
pub mod resolve;
pub mod stability;
#[cfg(test)]
mod tests;
pub mod unit;

use std::ops::Range;
use std::sync::Arc;

use crate::protocol::Atom;

/// A buffer line index (0-based).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(transparent)]
pub struct BufferLine(pub usize);

/// A display line index (0-based).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(transparent)]
pub struct DisplayLine(pub usize);

pub use fold_state::FoldToggleState;
pub use navigation::{ActionResult, NavigationAction, NavigationDirection, NavigationPolicy};
// InverseResult is defined in this module (not a submodule re-export).
pub use resolve::{
    DirectiveGroup, DirectiveSet, ResolveCache, TaggedDirective, partition_directives, resolve,
    resolve_incremental,
};
pub use stability::DirectiveStabilityMonitor;
pub use unit::{
    DisplayUnit, DisplayUnitId, DisplayUnitMap, SemanticRole, SourceStrength, UnitSource,
};

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
    /// Insert a virtual text line before the given buffer line.
    InsertBefore { before: usize, content: Vec<Atom> },
    /// Hide a range of buffer lines entirely.
    Hide { range: Range<usize> },
}

// =============================================================================
// DisplayDirective classification (ADR-030 Level 4)
// =============================================================================

/// All variant names of `DisplayDirective` (sorted).
pub const ALL_VARIANT_NAMES: &[&str] = &["Fold", "Hide", "InsertAfter", "InsertBefore"];

/// Variants classified as destructive (§10.2 Destructive).
pub const DESTRUCTIVE_VARIANTS: &[&str] = &["Hide"];

/// Variants classified as preserving (§10.2 Preserving).
pub const PRESERVING_VARIANTS: &[&str] = &["Fold"];

/// Variants classified as additive (§10.2 Additive).
pub const ADDITIVE_VARIANTS: &[&str] = &["InsertAfter", "InsertBefore"];

impl DisplayDirective {
    /// Variant name as a static string (exhaustive — no wildcard).
    pub fn variant_name(&self) -> &'static str {
        match self {
            DisplayDirective::Fold { .. } => "Fold",
            DisplayDirective::Hide { .. } => "Hide",
            DisplayDirective::InsertAfter { .. } => "InsertAfter",
            DisplayDirective::InsertBefore { .. } => "InsertBefore",
        }
    }

    /// Whether this directive is destructive (removes buffer content from display).
    pub const fn is_destructive(&self) -> bool {
        matches!(self, DisplayDirective::Hide { .. })
    }
}

/// Maps a display line back to its buffer origin.
#[derive(Debug, Clone, PartialEq)]
pub enum SourceMapping {
    /// This display line corresponds to exactly one buffer line.
    BufferLine(BufferLine),
    /// This display line represents a folded range of buffer lines.
    LineRange(Range<usize>),
    /// This display line is virtual text (no buffer origin).
    None,
}

/// Result of inverse projection: display line → buffer origin.
///
/// Encodes [`SourceStrength`] at the type level so that callers must handle
/// weak and absent mappings explicitly.  Only [`Actionable`](InverseResult::Actionable)
/// carries a buffer position safe for generating Kakoune actions.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum InverseResult {
    /// Strong source (1:1 `BufferLine`). Safe for action generation.
    Actionable(BufferLine),
    /// Weak source (fold summary). The representative line is the fold range
    /// start — informational only, not a precise action target.
    Informational {
        representative: BufferLine,
        range: Range<usize>,
    },
    /// No buffer origin (virtual text). Inverse projection is undefined.
    Virtual,
    /// Display line index out of range.
    OutOfRange,
}

impl InverseResult {
    /// Extract the buffer line if this is an actionable (strong) inverse.
    pub fn actionable(self) -> Option<BufferLine> {
        match self {
            Self::Actionable(bl) => Some(bl),
            _ => None,
        }
    }

    /// Extract any buffer representative (actionable or informational).
    pub fn any_representative(self) -> Option<BufferLine> {
        match self {
            Self::Actionable(bl)
            | Self::Informational {
                representative: bl, ..
            } => Some(bl),
            _ => None,
        }
    }
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
///
/// Constructed exclusively through smart constructors ([`buffer_line`],
/// [`fold_summary`], [`virtual_text`]) which enforce INV-6
/// (SourceMapping ↔ synthetic consistency) at construction time.
///
/// [`buffer_line`]: DisplayEntry::buffer_line
/// [`fold_summary`]: DisplayEntry::fold_summary
/// [`virtual_text`]: DisplayEntry::virtual_text
#[derive(Debug, Clone, PartialEq)]
pub struct DisplayEntry {
    source: SourceMapping,
    interaction: InteractionPolicy,
    synthetic: Option<SyntheticContent>,
}

impl DisplayEntry {
    /// Buffer line entry: Strong source, Normal interaction, no synthetic.
    ///
    /// INV-6 guaranteed by construction: `BufferLine` ↔ `synthetic: None`.
    pub fn buffer_line(line: BufferLine) -> Self {
        Self {
            source: SourceMapping::BufferLine(line),
            interaction: InteractionPolicy::Normal,
            synthetic: None,
        }
    }

    /// Fold summary entry: Weak source, ReadOnly interaction, synthetic summary.
    ///
    /// INV-6 guaranteed by construction: `LineRange` ↔ `synthetic: Some`.
    pub fn fold_summary(range: Range<usize>, atoms: Vec<Atom>) -> Self {
        Self {
            source: SourceMapping::LineRange(range),
            interaction: InteractionPolicy::ReadOnly,
            synthetic: Some(SyntheticContent { atoms }),
        }
    }

    /// Virtual text entry: Absent source, ReadOnly interaction, synthetic content.
    ///
    /// INV-6 guaranteed by construction: `None` ↔ `synthetic: Some`.
    pub fn virtual_text(atoms: Vec<Atom>) -> Self {
        Self {
            source: SourceMapping::None,
            interaction: InteractionPolicy::ReadOnly,
            synthetic: Some(SyntheticContent { atoms }),
        }
    }

    /// Source mapping for this display line.
    pub fn source(&self) -> &SourceMapping {
        &self.source
    }

    /// Interaction policy for this display line.
    pub fn interaction(&self) -> InteractionPolicy {
        self.interaction
    }

    /// Synthetic content (fold summary or virtual text), if any.
    pub fn synthetic(&self) -> Option<&SyntheticContent> {
        self.synthetic.as_ref()
    }
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
            .map(|i| DisplayEntry::buffer_line(BufferLine(i)))
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
        let mut insert_before: Vec<Vec<Vec<Atom>>> = vec![vec![]; line_count];

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
                DisplayDirective::InsertBefore { before, content } => {
                    if *before < line_count {
                        insert_before[*before].push(content.clone());
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
                    entries.push(DisplayEntry::fold_summary(range.clone(), summary.clone()));
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

            // InsertBefore: add virtual lines before this buffer line
            for atoms in &insert_before[line] {
                entries.push(DisplayEntry::virtual_text(atoms.clone()));
            }

            // Normal buffer line
            let display_idx = entries.len();
            entries.push(DisplayEntry::buffer_line(BufferLine(line)));
            buffer_to_display[line] = Some(display_idx);

            // InsertAfter: add virtual lines after this buffer line
            for atoms in &insert_after[line] {
                entries.push(DisplayEntry::virtual_text(atoms.clone()));
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

    /// Inverse projection: map a display line to its buffer origin (O(1)).
    ///
    /// Returns an [`InverseResult`] encoding the source strength:
    /// - `Actionable` for 1:1 buffer lines (safe for action generation)
    /// - `Informational` for fold summaries (range start, not a precise target)
    /// - `Virtual` for virtual text lines (no buffer origin)
    /// - `OutOfRange` if the display line index is beyond the map
    pub fn display_to_buffer(&self, display_y: DisplayLine) -> InverseResult {
        let Some(entry) = self.entries.get(display_y.0) else {
            return InverseResult::OutOfRange;
        };
        match &entry.source {
            SourceMapping::BufferLine(line) => InverseResult::Actionable(*line),
            SourceMapping::LineRange(range) => InverseResult::Informational {
                representative: BufferLine(range.start),
                range: range.clone(),
            },
            SourceMapping::None => InverseResult::Virtual,
        }
    }

    /// Map a buffer line to its display line (O(1)).
    ///
    /// Returns `None` if the buffer line is hidden or folded away.
    pub fn buffer_to_display(&self, buffer_line: BufferLine) -> Option<DisplayLine> {
        self.buffer_to_display
            .get(buffer_line.0)
            .copied()
            .flatten()
            .map(DisplayLine)
    }

    /// Get the display entry for a given display line (O(1)).
    pub fn entry(&self, display_y: DisplayLine) -> Option<&DisplayEntry> {
        self.entries.get(display_y.0)
    }

    /// Check if a display line is dirty based on the buffer's `lines_dirty` flags.
    ///
    /// For fold summary lines, returns true if any buffer line in the fold range is dirty.
    /// For virtual text lines, always returns false (they don't change from buffer edits).
    /// For identity maps, delegates directly to the `lines_dirty` array.
    pub fn is_display_line_dirty(&self, display_y: DisplayLine, lines_dirty: &[bool]) -> bool {
        let Some(entry) = self.entries.get(display_y.0) else {
            return true; // out of bounds → treat as dirty
        };
        match &entry.source {
            SourceMapping::BufferLine(line) => lines_dirty.get(line.0).copied().unwrap_or(true),
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
                        matches!(&self.entries[i].source, SourceMapping::BufferLine(bl) if bl.0 == i),
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
                            b.0, bl,
                            "INV-1: b2d[{bl}] = {dl} but entries[{dl}] = BufferLine({b:?})"
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
                            bl.0 < n_buf,
                            "INV-2: entries[{dl}] = BufferLine({:?}) but line_count = {n_buf}",
                            bl
                        );
                        debug_assert_eq!(
                            self.buffer_to_display[bl.0],
                            Some(dl),
                            "INV-2: entries[{dl}] = BufferLine({:?}) but b2d[{}] = {:?}",
                            bl,
                            bl.0,
                            self.buffer_to_display[bl.0]
                        );
                        if let Some(p) = prev_buf {
                            debug_assert!(
                                bl.0 > p,
                                "INV-5: non-monotonic: entries[{dl}] = BufferLine({:?}) after {p}",
                                bl
                            );
                        }
                        prev_buf = Some(bl.0);
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
                    SourceMapping::BufferLine(bl) => bl.0..bl.0 + 1,
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
    cursor_buffer_line: BufferLine,
    visible_height: usize,
) -> DisplayLine {
    if display_map.is_identity() {
        return DisplayLine(0);
    }
    let display_total = display_map.display_line_count();
    if display_total <= visible_height {
        return DisplayLine(0);
    }
    let cursor_display_y = display_map
        .buffer_to_display(cursor_buffer_line)
        .map(|dl| dl.0)
        .unwrap_or(cursor_buffer_line.0);
    if cursor_display_y < visible_height {
        return DisplayLine(0);
    }
    let offset = cursor_display_y - visible_height + 1;
    let max_offset = display_total.saturating_sub(visible_height);
    DisplayLine(offset.min(max_offset))
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
                matches!(&dm.entries[i].source, SourceMapping::BufferLine(bl) if bl.0 == i),
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
                        b.0, bl,
                        "INV-1: b2d[{bl}] = {dl} but source = BufferLine({b:?})"
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
                assert!(
                    bl.0 < n_buf,
                    "INV-2: BufferLine({bl:?}) >= line_count {n_buf}"
                );
                assert_eq!(
                    dm.buffer_to_display[bl.0],
                    Some(dl),
                    "INV-2: entries[{dl}] = BufferLine({bl:?})"
                );
                if let Some(p) = prev_buf {
                    assert!(bl.0 > p, "INV-5: non-monotonic at entries[{dl}]");
                }
                prev_buf = Some(bl.0);
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
            SourceMapping::BufferLine(bl) => bl.0..bl.0 + 1,
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
