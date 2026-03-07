use super::flex::LayoutResult;
use crate::element::{Element, InteractiveId};

/// Perform a hit test: find the topmost InteractiveId at the given screen coordinates.
///
/// Stack overlays are traversed in reverse order (front-to-back) so that the
/// visually topmost interactive region wins.
pub fn hit_test(
    element: &Element,
    layout: &LayoutResult,
    x: u16,
    y: u16,
) -> Option<InteractiveId> {
    let area = &layout.area;

    // Quick bounds check
    if x < area.x || x >= area.x + area.w || y < area.y || y >= area.y + area.h {
        return None;
    }

    match element {
        Element::Interactive { child, id } => {
            // If we're inside this Interactive's area, this is a hit.
            // But first check if a nested Interactive is more specific.
            if let Some(child_layout) = layout.children.first()
                && let Some(inner) = hit_test(child, child_layout, x, y)
            {
                return Some(inner);
            }
            Some(*id)
        }
        Element::Stack { base, overlays } => {
            // Reverse iterate overlays (front-to-back)
            for (i, overlay) in overlays.iter().enumerate().rev() {
                if let Some(overlay_layout) = layout.children.get(i + 1)
                    && let Some(id) = hit_test(&overlay.element, overlay_layout, x, y)
                {
                    return Some(id);
                }
            }
            // Fall back to base
            if let Some(base_layout) = layout.children.first() {
                return hit_test(base, base_layout, x, y);
            }
            None
        }
        Element::Flex { children, .. } => {
            for (i, child) in children.iter().enumerate() {
                if let Some(child_layout) = layout.children.get(i)
                    && let Some(id) = hit_test(&child.element, child_layout, x, y)
                {
                    return Some(id);
                }
            }
            None
        }
        Element::Container { child, .. } => {
            if let Some(child_layout) = layout.children.first() {
                return hit_test(child, child_layout, x, y);
            }
            None
        }
        Element::Scrollable { child, .. } => {
            if let Some(child_layout) = layout.children.first() {
                return hit_test(child, child_layout, x, y);
            }
            None
        }
        Element::Text(..) | Element::StyledLine(..) | Element::BufferRef { .. } | Element::Empty => {
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::element::{Element, InteractiveId, Overlay, OverlayAnchor};
    use crate::layout::Rect;
    use crate::layout::flex::place;
    use crate::protocol::Face;
    use crate::state::AppState;

    fn default_state() -> AppState {
        AppState::default()
    }

    #[test]
    fn test_hit_interactive_inside() {
        let state = default_state();
        let el = Element::Interactive {
            child: Box::new(Element::text("click me", Face::default())),
            id: InteractiveId(42),
        };
        let area = Rect {
            x: 5,
            y: 3,
            w: 8,
            h: 1,
        };
        let layout = place(&el, area, &state);
        assert_eq!(hit_test(&el, &layout, 5, 3), Some(InteractiveId(42)));
        assert_eq!(hit_test(&el, &layout, 12, 3), Some(InteractiveId(42)));
    }

    #[test]
    fn test_hit_interactive_outside() {
        let state = default_state();
        let el = Element::Interactive {
            child: Box::new(Element::text("click me", Face::default())),
            id: InteractiveId(42),
        };
        let area = Rect {
            x: 5,
            y: 3,
            w: 8,
            h: 1,
        };
        let layout = place(&el, area, &state);
        assert_eq!(hit_test(&el, &layout, 4, 3), None);
        assert_eq!(hit_test(&el, &layout, 13, 3), None);
        assert_eq!(hit_test(&el, &layout, 5, 4), None);
    }

    #[test]
    fn test_hit_stack_overlay_priority() {
        let state = default_state();
        let el = Element::stack(
            Element::Interactive {
                child: Box::new(Element::text("base", Face::default())),
                id: InteractiveId(1),
            },
            vec![Overlay {
                element: Element::Interactive {
                    child: Box::new(Element::text("pop", Face::default())),
                    id: InteractiveId(2),
                },
                anchor: OverlayAnchor::Absolute {
                    x: 0,
                    y: 0,
                    w: 3,
                    h: 1,
                },
            }],
        );
        let area = Rect {
            x: 0,
            y: 0,
            w: 20,
            h: 5,
        };
        let layout = place(&el, area, &state);
        // Overlay covers (0,0)-(2,0) → should return overlay's ID
        assert_eq!(hit_test(&el, &layout, 0, 0), Some(InteractiveId(2)));
        // Base covers the rest
        assert_eq!(hit_test(&el, &layout, 10, 0), Some(InteractiveId(1)));
    }

    #[test]
    fn test_hit_nested_interactive() {
        let state = default_state();
        let inner = Element::Interactive {
            child: Box::new(Element::text("inner", Face::default())),
            id: InteractiveId(10),
        };
        let outer = Element::Interactive {
            child: Box::new(inner),
            id: InteractiveId(20),
        };
        let area = Rect {
            x: 0,
            y: 0,
            w: 5,
            h: 1,
        };
        let layout = place(&outer, area, &state);
        // Inner ID wins (more specific)
        assert_eq!(hit_test(&outer, &layout, 0, 0), Some(InteractiveId(10)));
    }

    #[test]
    fn test_hit_no_interactive() {
        let state = default_state();
        let el = Element::text("plain text", Face::default());
        let area = Rect {
            x: 0,
            y: 0,
            w: 10,
            h: 1,
        };
        let layout = place(&el, area, &state);
        assert_eq!(hit_test(&el, &layout, 0, 0), None);
    }
}
