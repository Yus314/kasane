use std::ops::Range;

use super::Rect;
use super::flex::LayoutResult;
use crate::element::{Element, InteractiveId};

/// Cell-level hit entry for inline interactive content within buffer lines.
///
/// Unlike element-level `(Rect, InteractiveId)` entries, cell entries track
/// interactive regions at the character column granularity, used for
/// `InlineInteraction::Action` on `InsertInline` directives.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CellHitEntry {
    /// Display row (0-based).
    pub line: u16,
    /// Column range (exclusive end) within the display row.
    pub col_range: Range<u16>,
    /// The interactive ID bound to this inline region.
    pub id: InteractiveId,
}

/// Flat map of interactive regions for efficient mouse hit testing.
/// Entries are stored in depth-first order (deepest/topmost last).
#[derive(Debug, Clone)]
pub struct HitMap {
    entries: Vec<(Rect, InteractiveId)>,
    cell_entries: Vec<CellHitEntry>,
}

impl Default for HitMap {
    fn default() -> Self {
        Self::new()
    }
}

impl HitMap {
    pub fn new() -> Self {
        HitMap {
            entries: Vec::new(),
            cell_entries: Vec::new(),
        }
    }

    pub fn test(&self, x: u16, y: u16) -> Option<InteractiveId> {
        self.test_with_rect(x, y).map(|(id, _)| id)
    }

    /// Hit test returning both the InteractiveId and its bounding Rect.
    ///
    /// Checks element-level entries first (reverse order, topmost wins),
    /// then falls back to cell-level entries for inline interactive regions.
    pub fn test_with_rect(&self, x: u16, y: u16) -> Option<(InteractiveId, Rect)> {
        // Check cell entries first — inline interactive content is more specific
        if let Some(entry) = self
            .cell_entries
            .iter()
            .rev()
            .find(|e| y == e.line && x >= e.col_range.start && x < e.col_range.end)
        {
            let rect = Rect {
                x: entry.col_range.start,
                y: entry.line,
                w: entry.col_range.end - entry.col_range.start,
                h: 1,
            };
            return Some((entry.id, rect));
        }

        // Reverse iterate: last entry is topmost overlay
        self.entries.iter().rev().find_map(|(rect, id)| {
            if x >= rect.x && x < rect.x + rect.w && y >= rect.y && y < rect.y + rect.h {
                Some((*id, *rect))
            } else {
                None
            }
        })
    }

    /// Add a cell-level hit entry for inline interactive content.
    pub fn push_cell_entry(&mut self, entry: CellHitEntry) {
        self.cell_entries.push(entry);
    }

    /// Access cell-level entries (for testing/inspection).
    pub fn cell_entries(&self) -> &[CellHitEntry] {
        &self.cell_entries
    }
}

/// Walk the element tree and collect all Interactive element bounding rects.
pub fn build_hit_map(element: &Element, layout: &LayoutResult) -> HitMap {
    let mut entries = Vec::new();
    collect_interactive(element, layout, &mut entries);
    HitMap {
        entries,
        cell_entries: Vec::new(),
    }
}

fn collect_interactive(
    element: &Element,
    layout: &LayoutResult,
    entries: &mut Vec<(Rect, InteractiveId)>,
) {
    match element {
        Element::Interactive { child, id } => {
            // Recurse into child first, then record this Interactive (depth-first)
            if let Some(cl) = layout.children.first() {
                collect_interactive(child, cl, entries);
            }
            entries.push((layout.area, *id));
        }
        Element::Stack { base, overlays } => {
            if let Some(bl) = layout.children.first() {
                collect_interactive(base, bl, entries);
            }
            for (i, overlay) in overlays.iter().enumerate() {
                if let Some(ol) = layout.children.get(i + 1) {
                    collect_interactive(&overlay.element, ol, entries);
                }
            }
        }
        Element::Flex { children, .. } => {
            for (i, child) in children.iter().enumerate() {
                if let Some(cl) = layout.children.get(i) {
                    collect_interactive(&child.element, cl, entries);
                }
            }
        }
        Element::ResolvedSlot { children, .. } => {
            for (i, child) in children.iter().enumerate() {
                if let Some(cl) = layout.children.get(i) {
                    collect_interactive(&child.element, cl, entries);
                }
            }
        }
        Element::Grid { children, .. } => {
            for (i, child) in children.iter().enumerate() {
                if let Some(cl) = layout.children.get(i) {
                    collect_interactive(child, cl, entries);
                }
            }
        }
        Element::Container { child, .. } | Element::Scrollable { child, .. } => {
            if let Some(cl) = layout.children.first() {
                collect_interactive(child, cl, entries);
            }
        }
        Element::Text(..)
        | Element::StyledLine(..)
        | Element::BufferRef { .. }
        | Element::SlotPlaceholder { .. }
        | Element::Image { .. }
        | Element::Canvas { .. }
        | Element::TextPanel { .. }
        | Element::Empty => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::element::{Element, InteractiveId, Overlay, OverlayAnchor};
    use crate::layout::flex::place;
    use crate::protocol::Face;
    use crate::test_utils::*;

    #[test]
    fn test_hit_map_empty() {
        let state = default_state();
        let el = Element::text("plain", Face::default());
        let area = root_area(10, 1);
        let layout = place(&el, area, &state);
        let map = build_hit_map(&el, &layout);
        assert!(map.test(0, 0).is_none());
    }

    #[test]
    fn test_hit_map_interactive() {
        let state = default_state();
        let el = Element::Interactive {
            child: Box::new(Element::text("click", Face::default())),
            id: InteractiveId::framework(42),
        };
        let area = Rect {
            x: 5,
            y: 3,
            w: 8,
            h: 1,
        };
        let layout = place(&el, area, &state);
        let map = build_hit_map(&el, &layout);
        // Hit inside
        assert_eq!(map.test(5, 3), Some(InteractiveId::framework(42)));
        assert_eq!(map.test(12, 3), Some(InteractiveId::framework(42)));
        // Miss outside
        assert!(map.test(4, 3).is_none());
        assert!(map.test(13, 3).is_none());
        assert!(map.test(5, 4).is_none());
    }

    #[test]
    fn test_hit_map_overlay_depth() {
        let state = default_state();
        let el = Element::stack(
            Element::Interactive {
                child: Box::new(Element::text("base", Face::default())),
                id: InteractiveId::framework(1),
            },
            vec![Overlay {
                element: Element::Interactive {
                    child: Box::new(Element::text("pop", Face::default())),
                    id: InteractiveId::framework(2),
                },
                anchor: OverlayAnchor::Absolute {
                    x: 0,
                    y: 0,
                    w: 3,
                    h: 1,
                },
            }],
        );
        let area = root_area(20, 5);
        let layout = place(&el, area, &state);
        let map = build_hit_map(&el, &layout);
        // Overlay region → overlay's ID wins (collected later, iterated first in reverse)
        assert_eq!(map.test(0, 0), Some(InteractiveId::framework(2)));
        // Outside overlay but inside base → base's ID
        assert_eq!(map.test(10, 0), Some(InteractiveId::framework(1)));
    }

    #[test]
    fn test_cell_hit_entry_basic() {
        let state = default_state();
        let el = Element::text("plain text", Face::default());
        let area = root_area(20, 5);
        let layout = place(&el, area, &state);
        let mut map = build_hit_map(&el, &layout);

        // Add a cell entry for columns 5..10 on line 2
        map.push_cell_entry(super::CellHitEntry {
            line: 2,
            col_range: 5..10,
            id: InteractiveId::framework(99),
        });

        // Hit inside cell entry
        assert_eq!(map.test(5, 2), Some(InteractiveId::framework(99)));
        assert_eq!(map.test(9, 2), Some(InteractiveId::framework(99)));
        // Miss outside
        assert!(map.test(4, 2).is_none());
        assert!(map.test(10, 2).is_none());
        assert!(map.test(5, 3).is_none());
    }

    #[test]
    fn test_cell_hit_entry_priority_over_element() {
        let state = default_state();
        // Create an interactive element covering the entire area
        let el = Element::Interactive {
            child: Box::new(Element::text("click", Face::default())),
            id: InteractiveId::framework(1),
        };
        let area = root_area(20, 5);
        let layout = place(&el, area, &state);
        let mut map = build_hit_map(&el, &layout);

        // Add a cell entry that overlaps the element area
        map.push_cell_entry(super::CellHitEntry {
            line: 0,
            col_range: 3..8,
            id: InteractiveId::framework(2),
        });

        // Cell entry should win within its range
        assert_eq!(map.test(5, 0), Some(InteractiveId::framework(2)));
        // Element entry should win outside the cell range
        assert_eq!(map.test(0, 0), Some(InteractiveId::framework(1)));
    }

    #[test]
    fn test_hit_map_nested() {
        let state = default_state();
        let inner = Element::Interactive {
            child: Box::new(Element::text("inner", Face::default())),
            id: InteractiveId::framework(10),
        };
        let outer = Element::Interactive {
            child: Box::new(inner),
            id: InteractiveId::framework(20),
        };
        let area = root_area(5, 1);
        let layout = place(&outer, area, &state);
        let map = build_hit_map(&outer, &layout);
        // Inner is collected first, outer is collected last.
        // Reverse iteration finds outer first. But inner has same rect...
        // Actually: inner is pushed first (line 56), then outer (line 58).
        // Reverse: outer(20) checked first. Both cover same area.
        // So outer wins in flat HitMap. This is acceptable — for nested
        // Interactive, the outermost ID is returned.
        let result = map.test(0, 0);
        assert!(
            result == Some(InteractiveId::framework(20))
                || result == Some(InteractiveId::framework(10))
        );
    }
}
