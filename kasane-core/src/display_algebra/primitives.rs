//! The core algebra: `Display`, `Span`, `Content`, `AnchorPosition`.
//!
//! See `mod.rs` for the law statements (L1–L6); this file defines only
//! the data shape. Composition semantics live in `normalize.rs`.

use std::ops::Range;
use std::sync::Arc;

use crate::element::Element;
use crate::protocol::Atom;
use crate::state::shadow_cursor::EditableSpan;

/// A per-line, byte-addressed span. Multi-line spans are expressed as
/// `Then` chains over single-line `Span`s — this keeps the algebra flat
/// per line, which is the unit at which ADR-030 Level 4 already requires
/// directive locality.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Span {
    pub line: usize,
    pub byte_range: Range<usize>,
}

impl Span {
    /// Construct a span over `byte_range` on `line`.
    pub fn new(line: usize, byte_range: Range<usize>) -> Self {
        Self { line, byte_range }
    }

    /// Zero-length span anchored at the start of `line`.
    pub fn start_of_line(line: usize) -> Self {
        Self {
            line,
            byte_range: 0..0,
        }
    }

    /// Zero-length span anchored at the end of `line`. The byte offset
    /// is `usize::MAX` as a sentinel; resolution maps it to the actual
    /// line length when the buffer text is available.
    pub fn end_of_line(line: usize) -> Self {
        Self {
            line,
            byte_range: usize::MAX..usize::MAX,
        }
    }

    /// Zero-length span at a specific byte offset on a line.
    pub fn at(line: usize, byte_offset: usize) -> Self {
        Self {
            line,
            byte_range: byte_offset..byte_offset,
        }
    }

    /// Whether this span is degenerate (start == end), i.e. a pure
    /// insertion point rather than a content range.
    pub fn is_insertion_point(&self) -> bool {
        self.byte_range.start == self.byte_range.end
    }

    /// Whether two spans on the same line have any overlap. Insertion
    /// points (zero-length) overlap a non-degenerate range only when
    /// the point is strictly *inside* the range; touching the boundary
    /// does not overlap, so multiple plugins can each insert at the
    /// same end-of-line without conflict.
    pub fn overlaps(&self, other: &Span) -> bool {
        if self.line != other.line {
            return false;
        }
        let (a, b) = (&self.byte_range, &other.byte_range);
        // Degenerate-degenerate: only equal positions overlap.
        if a.start == a.end && b.start == b.end {
            return a.start == b.start;
        }
        // Degenerate-non-degenerate: point strictly inside the range.
        if a.start == a.end {
            return a.start > b.start && a.start < b.end;
        }
        if b.start == b.end {
            return b.start > a.start && b.start < a.end;
        }
        // Non-degenerate: standard half-open overlap.
        a.start < b.end && b.start < a.end
    }
}

/// Inline-content payload variants. `Empty` enables `Replace(range, Empty)`
/// to express hide; `Reference` is a slot reserved for ADR-036
/// (Cross-File Inlining) and is opaque to the resolver until that ADR
/// lands.
#[derive(Debug, Clone, PartialEq)]
pub enum Content {
    /// No content — hides whatever is being replaced.
    Empty,
    /// Styled inline atoms.
    Text(Vec<Atom>),
    /// Editable text bound to a shadow cursor. The `EditableSpan` list
    /// describes how local edits project back to the buffer; the
    /// `EditSpec` captures the chosen projection (Mirror / Computed /
    /// PluginDefined).
    Editable {
        atoms: Vec<Atom>,
        spans: Vec<EditableSpan>,
        spec: EditSpec,
    },
    /// A plugin-painted box. The `box_id` is plugin-supplied and stable
    /// across re-runs (ADR-031 Phase 10 wire shape).
    InlineBox {
        box_id: u64,
        width_cells: f32,
        height_lines: f32,
    },
    /// Pull content from another buffer. Reserved for ADR-036; the
    /// resolver currently treats this as opaque and forwards it.
    Reference(SegmentRef),
    /// Full Element (used by InsertBefore / InsertAfter / Gutter).
    Element(Arc<Element>),
    /// ADR-037 §1: a multi-line fold. The `Replace` carrying this
    /// content is anchored at `range.start` (Span.line == range.start
    /// — invariant enforced by the `derived::fold` smart constructor
    /// and validated by `debug_assert` in `normalize`). The fold
    /// visually consumes lines `range.start..range.end`, displaying
    /// `summary` at the anchor line.
    Fold {
        range: std::ops::Range<usize>,
        summary: Vec<Atom>,
    },
    /// ADR-037 §6 (post-Phase-3a): a multi-line hide. Mirrors the
    /// `Fold` shape — the carrying `Replace` is anchored at
    /// `range.start`, and the hide visually consumes
    /// `range.start..range.end` without rendering any summary content.
    /// Introduced to collapse `derived::hide_lines(start..end)` from
    /// N per-line `Replace(Empty)` leaves to a single leaf, restoring
    /// the bench performance criterion #6 lost when Phase 3a moved
    /// hides off the legacy fast path.
    ///
    /// `Hide`-`Hide` overlaps are *commutative* (set-union
    /// idempotent, like the legacy hidden_set semantics), so Pass B
    /// special-cases them as non-conflicting. `Hide` against any
    /// other `Replace` content (Fold, Text, Editable, …) still
    /// conflicts via the standard Pass B coverage rule.
    Hide { range: std::ops::Range<usize> },
}

/// How an editable inline content projects back to the canonical buffer.
/// `Mirror` and `PluginDefined` mirror the legacy `EditProjection`;
/// `Computed` is the new bidirectional pair from ADR-035 §EditProjection
/// reformulation.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum EditSpec {
    Mirror,
    PluginDefined,
    /// Bidirectional: the forward function describes how buffer text
    /// produces the displayed value; the inverse describes how an edit
    /// to the displayed value rewrites the source. Identifiers point
    /// into a registry maintained by the owning plugin.
    Computed {
        forward_id: u32,
        inverse_id: u32,
    },
}

/// Cross-buffer reference. Reserved slot (ADR-036).
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct SegmentRef {
    pub buffer: String,
    pub line_range: Range<usize>,
}

/// Side of a per-line ornament.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Side {
    Before,
    After,
    Left,
    Right,
}

/// Non-text anchor positions: gutters, ornaments, overlays. These
/// participate in display without consuming buffer-cell width.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum AnchorPosition {
    /// Numbered gutter lane on a specific line.
    Gutter { line: usize, lane: u8 },
    /// Pre- or post-line ornament.
    Ornament { line: usize, side: Side },
    /// Floating overlay over a cell rectangle.
    Overlay { rect: Rect },
}

/// Cell-coordinate rectangle for overlays.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Rect {
    pub line: usize,
    pub column: usize,
    pub width: usize,
    pub height: usize,
}

/// Style spec for `Decorate`. The `priority` resolves L5 stacking when
/// multiple decorates overlap. Not `Hash` because `WireFace` is not
/// `Hash`; conflict resolution uses `TaggedDisplay::cmp_key` over
/// `(priority, plugin_id, seq)` rather than the style payload.
#[derive(Debug, Clone, PartialEq)]
pub struct Style {
    pub face: crate::protocol::WireFace,
    pub priority: i16,
}

/// The Display algebra. Five primitives plus two composition operators,
/// unified as enum variants so the type is closed and serializable.
#[derive(Debug, Clone, PartialEq)]
pub enum Display {
    /// Identity — produces no change. Unit of `Then` and `Merge`.
    Identity,

    /// Substitute the content of `range` with `content`. Degenerate
    /// ranges express insertion; `Content::Empty` expresses hide.
    Replace { range: Span, content: Content },

    /// Apply `style` over `range`. No positional effect.
    Decorate { range: Span, style: Style },

    /// Attach `content` to a non-text anchor; no buffer-cell consumption.
    Anchor {
        position: AnchorPosition,
        content: Content,
    },

    /// Sequential composition. `b` evaluates against the post-`a` document.
    Then(Box<Display>, Box<Display>),

    /// Parallel composition. `a` and `b` evaluate against the same input;
    /// commutes when `support(a) ∩ support(b) = ∅`.
    Merge(Box<Display>, Box<Display>),
}

impl Display {
    /// Smart-constructed `Then` that absorbs identities (L1).
    pub fn then(a: Display, b: Display) -> Display {
        match (a, b) {
            (Display::Identity, x) | (x, Display::Identity) => x,
            (a, b) => Display::Then(Box::new(a), Box::new(b)),
        }
    }

    /// Smart-constructed `Merge` that absorbs identities (L1).
    pub fn merge(a: Display, b: Display) -> Display {
        match (a, b) {
            (Display::Identity, x) | (x, Display::Identity) => x,
            (a, b) => Display::Merge(Box::new(a), Box::new(b)),
        }
    }

    /// Compose a sequence with `Then`. Empty → `Identity`.
    pub fn then_all(items: impl IntoIterator<Item = Display>) -> Display {
        items.into_iter().fold(Display::Identity, Display::then)
    }

    /// Compose a sequence with `Merge`. Empty → `Identity`.
    pub fn merge_all(items: impl IntoIterator<Item = Display>) -> Display {
        items.into_iter().fold(Display::Identity, Display::merge)
    }

    /// Set of `Span`s touched by this display, used for L4 disjointness
    /// and L6 conflict detection. Anchors do not contribute (they are
    /// non-text positions and never conflict with text positions).
    pub fn support(&self) -> Vec<Span> {
        let mut acc = Vec::new();
        self.collect_support(&mut acc);
        acc
    }

    fn collect_support(&self, out: &mut Vec<Span>) {
        match self {
            Display::Identity => {}
            Display::Replace { range, .. } | Display::Decorate { range, .. } => {
                out.push(range.clone());
            }
            Display::Anchor { .. } => {}
            Display::Then(a, b) | Display::Merge(a, b) => {
                a.collect_support(out);
                b.collect_support(out);
            }
        }
    }

    /// Whether this is the algebraic identity. Useful for short-circuit
    /// checks in callers that build trees incrementally.
    pub fn is_identity(&self) -> bool {
        matches!(self, Display::Identity)
    }
}
