//! Non-destructive display directive projection (ADR-030 Level 4).
//!
//! `SafeDisplayDirective` is to `DisplayDirective` what `TransparentCommand`
//! is to `Command`: a newtype restricting construction to the non-destructive
//! subset. There is no constructor for `Hide`, making non-destructiveness a
//! compile-time property.

use std::ops::Range;

use crate::display::{
    DisplayDirective, GutterSide, InlineBoxAlignment, InlineInteraction, VirtualTextPosition,
};
use crate::element::Element;
use crate::protocol::{Atom, Face};
use crate::state::shadow_cursor::EditableSpan;

/// A display directive guaranteed not to be destructive.
///
/// Construction is restricted to non-destructive `DisplayDirective` variants.
/// `Hide` and `HideInline` have no constructor on this type, making
/// non-destructiveness a compile-time property.
pub struct SafeDisplayDirective(DisplayDirective);

impl std::fmt::Debug for SafeDisplayDirective {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "SafeDisplayDirective({})", self.0.variant_name())
    }
}

impl SafeDisplayDirective {
    /// All variant names covered by this projection (sorted).
    pub const VARIANT_NAMES: &'static [&'static str] = &[
        "EditableVirtualText",
        "Fold",
        "Gutter",
        "InlineBox",
        "InsertAfter",
        "InsertBefore",
        "InsertInline",
        "StyleInline",
        "StyleLine",
        "VirtualText",
    ];

    /// Collapse a range of buffer lines into a single summary line.
    pub fn fold(range: Range<usize>, summary: Vec<Atom>) -> Self {
        Self(DisplayDirective::Fold { range, summary })
    }

    /// Insert a full Element before a buffer line.
    pub fn insert_before(line: usize, content: Element, priority: i16) -> Self {
        Self(DisplayDirective::InsertBefore {
            line,
            content,
            priority,
        })
    }

    /// Insert a full Element after a buffer line.
    pub fn insert_after(line: usize, content: Element, priority: i16) -> Self {
        Self(DisplayDirective::InsertAfter {
            line,
            content,
            priority,
        })
    }

    /// Insert inline content at a byte offset within a buffer line.
    pub fn insert_inline(
        line: usize,
        byte_offset: usize,
        content: Vec<Atom>,
        interaction: InlineInteraction,
    ) -> Self {
        Self(DisplayDirective::InsertInline {
            line,
            byte_offset,
            content,
            interaction,
        })
    }

    /// Apply face styling to a byte range within a buffer line.
    pub fn style_inline(line: usize, byte_range: Range<usize>, face: Face) -> Self {
        Self(DisplayDirective::StyleInline {
            line,
            byte_range,
            face,
        })
    }

    /// Apply a background face to an entire buffer line.
    pub fn style_line(line: usize, face: Face, z_order: i16) -> Self {
        Self(DisplayDirective::StyleLine {
            line,
            face,
            z_order,
        })
    }

    /// Add content to the gutter of a buffer line.
    pub fn gutter(line: usize, side: GutterSide, content: Element, priority: i16) -> Self {
        Self(DisplayDirective::Gutter {
            line,
            side,
            content,
            priority,
        })
    }

    /// Add virtual text at the end of a line.
    pub fn virtual_text(
        line: usize,
        position: VirtualTextPosition,
        content: Vec<Atom>,
        priority: i16,
    ) -> Self {
        Self(DisplayDirective::VirtualText {
            line,
            position,
            content,
            priority,
        })
    }

    /// Insert an editable virtual text line after the given buffer line.
    pub fn editable_virtual_text(
        after: usize,
        content: Vec<Atom>,
        editable_spans: Vec<EditableSpan>,
    ) -> Self {
        Self(DisplayDirective::EditableVirtualText {
            after,
            content,
            editable_spans,
        })
    }

    /// Reserve a non-text inline slot at a byte offset within a buffer line.
    ///
    /// Mirrors [`DisplayDirective::InlineBox`]; the actual paint content is
    /// queried via the `paint-inline-box(box-id)` extension point (Phase 10
    /// Step 2 — pending). See ADR-031 §Phase 10 Wire Shape #2.
    pub fn inline_box(
        line: usize,
        byte_offset: usize,
        width_cells: f32,
        height_lines: f32,
        box_id: u64,
        alignment: InlineBoxAlignment,
    ) -> Self {
        Self(DisplayDirective::InlineBox {
            line,
            byte_offset,
            width_cells,
            height_lines,
            box_id,
            alignment,
        })
    }
}

impl From<SafeDisplayDirective> for DisplayDirective {
    fn from(safe: SafeDisplayDirective) -> Self {
        safe.0
    }
}
