//! Smart constructors that recover the legacy `DisplayDirective`
//! variants over the algebra (ADR-034 §4 Derived Constructors).
//!
//! Plugin authors keep ergonomic factories like `hide(range)` and
//! `gutter(line, lane, content)`; the compiler sees a single `Display`
//! type, and the resolver no longer has to special-case 12 variants.

use std::ops::Range;
use std::sync::Arc;

use crate::element::Element;
use crate::protocol::{Atom, WireFace};
use crate::state::shadow_cursor::EditableSpan;

use super::primitives::{AnchorPosition, Content, Display, EditSpec, Rect, Side, Span, Style};

/// Hide a range of buffer lines entirely (legacy `Hide`).
///
/// ADR-037 §6: emits a *single* leaf carrying `Content::Hide` with the
/// multi-line range as a payload. Mirrors the `derived::fold` Phase 1
/// transition. Pass B in `normalize` treats every line in the hide's
/// range as claimed by this leaf, with one special case: two
/// overlapping `Content::Hide` leaves are *commutative* and do not
/// conflict — they compose as a set union, matching the legacy
/// `hidden_set` semantics.
pub fn hide_lines(line_range: Range<usize>) -> Display {
    if line_range.start >= line_range.end {
        return Display::Identity;
    }
    Display::Replace {
        range: Span::new(line_range.start, 0..usize::MAX),
        content: Content::Hide { range: line_range },
    }
}

/// Hide a byte range within a single line (legacy `HideInline`).
pub fn hide_inline(line: usize, byte_range: Range<usize>) -> Display {
    Display::Replace {
        range: Span::new(line, byte_range),
        content: Content::Empty,
    }
}

/// Collapse a range of buffer lines into a summary (legacy `Fold`).
///
/// ADR-037 §2: emits a *single* leaf carrying `Content::Fold`, not a
/// multi-line decomposition. The carrying `Replace`'s `Span` is
/// anchored at `line_range.start` (full line); the multi-line range
/// lives inside the `Content::Fold` payload. Conflict detection in
/// `normalize` Pass B treats every line in the fold's range as
/// claimed by this leaf, so a non-fold `Replace` on any line in the
/// range conflicts with this fold.
pub fn fold(line_range: Range<usize>, summary: Vec<Atom>) -> Display {
    if line_range.start >= line_range.end {
        return Display::Identity;
    }
    Display::Replace {
        range: Span::new(line_range.start, 0..usize::MAX),
        content: Content::Fold {
            range: line_range,
            summary,
        },
    }
}

/// Insert a full Element before a buffer line (legacy `InsertBefore`).
pub fn insert_before(line: usize, element: Element) -> Display {
    Display::Anchor {
        position: AnchorPosition::Ornament {
            line,
            side: Side::Before,
        },
        content: Content::Element(Arc::new(element)),
    }
}

/// Insert a full Element after a buffer line (legacy `InsertAfter`).
pub fn insert_after(line: usize, element: Element) -> Display {
    Display::Anchor {
        position: AnchorPosition::Ornament {
            line,
            side: Side::After,
        },
        content: Content::Element(Arc::new(element)),
    }
}

/// Insert inline atoms at a byte offset (legacy `InsertInline`).
pub fn insert_inline(line: usize, byte_offset: usize, content: Vec<Atom>) -> Display {
    Display::Replace {
        range: Span::at(line, byte_offset),
        content: Content::Text(content),
    }
}

/// Reserve an inline box slot (legacy `InlineBox`).
pub fn inline_box(
    line: usize,
    byte_offset: usize,
    box_id: u64,
    width_cells: f32,
    height_lines: f32,
) -> Display {
    Display::Replace {
        range: Span::at(line, byte_offset),
        content: Content::InlineBox {
            box_id,
            width_cells,
            height_lines,
        },
    }
}

/// Apply face styling to a byte range (legacy `StyleInline`).
pub fn style_inline(
    line: usize,
    byte_range: Range<usize>,
    face: WireFace,
    priority: i16,
) -> Display {
    Display::Decorate {
        range: Span::new(line, byte_range),
        style: Style { face, priority },
    }
}

/// Apply a background face to an entire buffer line (legacy `StyleLine`).
pub fn style_line(line: usize, face: WireFace, z_order: i16) -> Display {
    Display::Decorate {
        range: Span::new(line, 0..usize::MAX),
        style: Style {
            face,
            priority: z_order,
        },
    }
}

/// Add content to the gutter of a buffer line (legacy `Gutter`).
pub fn gutter(line: usize, lane: u8, element: Element) -> Display {
    Display::Anchor {
        position: AnchorPosition::Gutter { line, lane },
        content: Content::Element(Arc::new(element)),
    }
}

/// Add virtual text at the end of a line (legacy `VirtualText` with
/// `EndOfLine` position).
pub fn virtual_text_eol(line: usize, content: Vec<Atom>) -> Display {
    Display::Replace {
        range: Span::end_of_line(line),
        content: Content::Text(content),
    }
}

/// Insert an editable virtual text line after the given buffer line
/// (legacy `EditableVirtualText`).
pub fn editable_virtual_text(
    after: usize,
    atoms: Vec<Atom>,
    spans: Vec<EditableSpan>,
    spec: EditSpec,
) -> Display {
    Display::Anchor {
        position: AnchorPosition::Ornament {
            line: after,
            side: Side::After,
        },
        content: Content::Editable { atoms, spans, spec },
    }
}

/// Floating overlay anchor (no legacy parallel; ADR-034 §1 generalisation).
pub fn overlay(rect: Rect, element: Element) -> Display {
    Display::Anchor {
        position: AnchorPosition::Overlay { rect },
        content: Content::Element(Arc::new(element)),
    }
}
