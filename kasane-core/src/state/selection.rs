//! ADR-035 §1 first-class selection types.
//!
//! `Selection` carries the canonical anchor / cursor / direction triple for
//! a single Kakoune-style selection. The plural form `SelectionSet` (in
//! `state::selection_set`) carries the algebraic operations.
//!
//! These types are introduced in parallel with the legacy heuristic
//! `state::derived::selection::Selection` (which infers selections from
//! styled atoms and is unrelated to the canonical multi-selection). The
//! migration that retires the heuristic version is tracked separately.

/// A buffer-relative position. Lines and columns are 0-indexed; the
/// translation to Kakoune's 1-indexed coordinate space happens at the
/// protocol boundary.
///
/// `column` is a byte offset within the line, not a display column —
/// this matches the semantics of selection ranges in the Kakoune
/// protocol (lines are addressed in bytes; the renderer translates to
/// display columns separately).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct BufferPos {
    pub line: u32,
    pub column: u32,
}

impl BufferPos {
    pub const fn new(line: u32, column: u32) -> Self {
        Self { line, column }
    }

    /// Origin position (line 0, column 0).
    pub const ORIGIN: Self = Self { line: 0, column: 0 };
}

/// Direction of a selection — which end the user is moving when keys
/// extend the range. Kakoune calls these "anchor" and "cursor"; the
/// `Direction` indicates which is the primary head.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Direction {
    /// Cursor is at or after anchor.
    Forward,
    /// Cursor is at or before anchor.
    Backward,
}

/// A single selection: a half-open range `[min, max)` where the cursor
/// is at one end and the anchor at the other; `direction` records
/// which.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Selection {
    pub anchor: BufferPos,
    pub cursor: BufferPos,
    pub direction: Direction,
}

impl Selection {
    /// Construct a selection from anchor / cursor positions; the
    /// direction is inferred from their relative order.
    pub fn new(anchor: BufferPos, cursor: BufferPos) -> Self {
        let direction = if cursor >= anchor {
            Direction::Forward
        } else {
            Direction::Backward
        };
        Self {
            anchor,
            cursor,
            direction,
        }
    }

    /// Construct a degenerate selection at a single position
    /// (anchor == cursor, Forward by convention).
    pub fn point(pos: BufferPos) -> Self {
        Self {
            anchor: pos,
            cursor: pos,
            direction: Direction::Forward,
        }
    }

    /// Lower-bounded position (start of the half-open range).
    pub fn min(&self) -> BufferPos {
        std::cmp::min(self.anchor, self.cursor)
    }

    /// Upper-bounded position (exclusive end of the half-open range).
    pub fn max(&self) -> BufferPos {
        std::cmp::max(self.anchor, self.cursor)
    }

    /// Whether this selection covers `pos` (half-open: `min ≤ pos < max`).
    /// A point selection covers exactly its position.
    pub fn covers(&self, pos: BufferPos) -> bool {
        if self.anchor == self.cursor {
            return pos == self.anchor;
        }
        pos >= self.min() && pos < self.max()
    }

    /// Whether two selections share at least one position (half-open).
    /// Two points overlap iff they coincide; otherwise standard
    /// half-open overlap.
    pub fn overlaps(&self, other: &Selection) -> bool {
        if self.anchor == self.cursor && other.anchor == other.cursor {
            return self.anchor == other.anchor;
        }
        if self.anchor == self.cursor {
            return other.covers(self.anchor);
        }
        if other.anchor == other.cursor {
            return self.covers(other.anchor);
        }
        self.min() < other.max() && other.min() < self.max()
    }

    /// Set-style union of two overlapping or adjacent selections. The
    /// caller is responsible for confirming overlap; if the selections
    /// are disjoint, the result is the bounding range and information
    /// about the gap is lost. Direction follows the *resulting cursor*
    /// position relative to the union anchor.
    pub fn merge_with(&self, other: &Selection) -> Selection {
        let new_min = std::cmp::min(self.min(), other.min());
        let new_max = std::cmp::max(self.max(), other.max());
        Selection {
            anchor: new_min,
            cursor: new_max,
            direction: Direction::Forward,
        }
    }
}

/// Identifier for a buffer. Kakoune's protocol identifies buffers by
/// name; Kasane mints stable `BufferId`s that survive renames within a
/// session.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct BufferId(pub String);

impl BufferId {
    pub fn new(s: impl Into<String>) -> Self {
        Self(s.into())
    }
}

/// A monotonic version counter for a buffer's content. Each observed
/// edit (Kakoune protocol echo) increments the version.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
#[repr(transparent)]
pub struct BufferVersion(pub u64);

impl BufferVersion {
    pub const INITIAL: Self = Self(0);

    pub fn next(self) -> Self {
        Self(self.0 + 1)
    }
}
