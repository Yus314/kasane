use unicode_width::UnicodeWidthStr;

use crate::element::{Align, Direction, Element, FlexChild};
use crate::layout::line_display_width;
use crate::state::AppState;
use super::Rect;

/// Layout constraints passed top-down.
#[derive(Debug, Clone, Copy)]
pub struct Constraints {
    pub min_width: u16,
    pub max_width: u16,
    pub min_height: u16,
    pub max_height: u16,
}

impl Constraints {
    pub fn tight(width: u16, height: u16) -> Self {
        Constraints {
            min_width: width,
            max_width: width,
            min_height: height,
            max_height: height,
        }
    }

    pub fn loose(max_width: u16, max_height: u16) -> Self {
        Constraints {
            min_width: 0,
            max_width,
            min_height: 0,
            max_height,
        }
    }
}

/// Measured size of an element.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Size {
    pub width: u16,
    pub height: u16,
}

/// Layout result: the placed area plus children layout results.
#[derive(Debug, Clone)]
pub struct LayoutResult {
    pub area: Rect,
    pub children: Vec<LayoutResult>,
}

/// Measure an element bottom-up: compute its intrinsic size.
pub fn measure(element: &Element, constraints: Constraints, state: &AppState) -> Size {
    match element {
        Element::Text(text, _) => {
            let w = UnicodeWidthStr::width(text.as_str()) as u16;
            Size {
                width: w.clamp(constraints.min_width, constraints.max_width),
                height: 1u16.clamp(constraints.min_height, constraints.max_height),
            }
        }
        Element::StyledLine(atoms) => {
            let line = atoms.to_vec();
            let w = line_display_width(&line) as u16;
            Size {
                width: w.clamp(constraints.min_width, constraints.max_width),
                height: 1u16.clamp(constraints.min_height, constraints.max_height),
            }
        }
        Element::BufferRef { line_range } => {
            let h = line_range.len() as u16;
            Size {
                width: constraints.max_width,
                height: h.clamp(constraints.min_height, constraints.max_height),
            }
        }
        Element::Empty => Size {
            width: constraints.min_width,
            height: constraints.min_height,
        },
        Element::Flex {
            direction,
            children,
            gap,
            ..
        } => measure_flex(*direction, children, *gap, constraints, state),
        Element::Container {
            child,
            border,
            padding,
            ..
        } => {
            let border_size = if border.is_some() { 2 } else { 0 };
            let extra_w = padding.horizontal() + border_size;
            let extra_h = padding.vertical() + border_size;
            let child_constraints = Constraints {
                min_width: constraints.min_width.saturating_sub(extra_w),
                max_width: constraints.max_width.saturating_sub(extra_w),
                min_height: constraints.min_height.saturating_sub(extra_h),
                max_height: constraints.max_height.saturating_sub(extra_h),
            };
            let child_size = measure(child, child_constraints, state);
            Size {
                width: (child_size.width + extra_w).clamp(constraints.min_width, constraints.max_width),
                height: (child_size.height + extra_h).clamp(constraints.min_height, constraints.max_height),
            }
        }
        Element::Stack { base, .. } => measure(base, constraints, state),
        Element::Scrollable {
            child, direction, ..
        } => {
            // Scrollable measures its child unconstrained in the scroll direction
            let child_constraints = match direction {
                Direction::Column => Constraints {
                    min_width: constraints.min_width,
                    max_width: constraints.max_width,
                    min_height: 0,
                    max_height: u16::MAX,
                },
                Direction::Row => Constraints {
                    min_width: 0,
                    max_width: u16::MAX,
                    min_height: constraints.min_height,
                    max_height: constraints.max_height,
                },
            };
            let child_size = measure(child, child_constraints, state);
            // But reports the constrained size
            Size {
                width: child_size.width.clamp(constraints.min_width, constraints.max_width),
                height: child_size.height.clamp(constraints.min_height, constraints.max_height),
            }
        }
    }
}

fn measure_flex(
    direction: Direction,
    children: &[FlexChild],
    gap: u16,
    constraints: Constraints,
    state: &AppState,
) -> Size {
    let total_gap = if children.len() > 1 {
        gap * (children.len() as u16 - 1)
    } else {
        0
    };

    let mut main_fixed = 0u16;
    let mut cross_max = 0u16;
    let mut total_flex = 0.0f32;

    for child in children {
        let child_constraints = match direction {
            Direction::Column => Constraints::loose(constraints.max_width, constraints.max_height),
            Direction::Row => Constraints::loose(constraints.max_width, constraints.max_height),
        };
        let size = measure(&child.element, child_constraints, state);
        let (main, cross) = match direction {
            Direction::Column => (size.height, size.width),
            Direction::Row => (size.width, size.height),
        };
        cross_max = cross_max.max(cross);

        if child.flex > 0.0 {
            total_flex += child.flex;
        } else {
            main_fixed += main;
        }
    }

    let main_total = if total_flex > 0.0 {
        match direction {
            Direction::Column => constraints.max_height,
            Direction::Row => constraints.max_width,
        }
    } else {
        main_fixed + total_gap
    };

    match direction {
        Direction::Column => Size {
            width: cross_max.clamp(constraints.min_width, constraints.max_width),
            height: main_total.clamp(constraints.min_height, constraints.max_height),
        },
        Direction::Row => Size {
            width: main_total.clamp(constraints.min_width, constraints.max_width),
            height: cross_max.clamp(constraints.min_height, constraints.max_height),
        },
    }
}

/// Place an element top-down: assign concrete positions to all children.
pub fn place(element: &Element, area: Rect, state: &AppState) -> LayoutResult {
    match element {
        Element::Text(..) | Element::StyledLine(..) | Element::BufferRef { .. } | Element::Empty => {
            LayoutResult {
                area,
                children: vec![],
            }
        }
        Element::Flex {
            direction,
            children,
            gap,
            align,
            cross_align,
            ..
        } => place_flex(*direction, children, *gap, *align, *cross_align, area, state),
        Element::Container {
            child,
            border,
            padding,
            ..
        } => {
            let border_size = if border.is_some() { 1 } else { 0 };
            let inner = Rect {
                x: area.x + padding.left + border_size,
                y: area.y + padding.top + border_size,
                w: area
                    .w
                    .saturating_sub(padding.horizontal() + border_size * 2),
                h: area
                    .h
                    .saturating_sub(padding.vertical() + border_size * 2),
            };
            let child_result = place(child, inner, state);
            LayoutResult {
                area,
                children: vec![child_result],
            }
        }
        Element::Stack { base, overlays } => place_stack(base, overlays, area, state),
        Element::Scrollable {
            child,
            offset,
            direction,
            ..
        } => {
            // Place child in a virtual area, shifted by offset
            let virtual_area = match direction {
                Direction::Column => Rect {
                    x: area.x,
                    y: area.y.wrapping_sub(*offset),
                    w: area.w,
                    h: area.h + *offset,
                },
                Direction::Row => Rect {
                    x: area.x.wrapping_sub(*offset),
                    y: area.y,
                    w: area.w + *offset,
                    h: area.h,
                },
            };
            let child_result = place(child, virtual_area, state);
            LayoutResult {
                area,
                children: vec![child_result],
            }
        }
    }
}

fn place_flex(
    direction: Direction,
    children: &[FlexChild],
    gap: u16,
    _align: Align,
    _cross_align: Align,
    area: Rect,
    state: &AppState,
) -> LayoutResult {
    if children.is_empty() {
        return LayoutResult {
            area,
            children: vec![],
        };
    }

    let main_total = match direction {
        Direction::Column => area.h,
        Direction::Row => area.w,
    };
    let cross_total = match direction {
        Direction::Column => area.w,
        Direction::Row => area.h,
    };

    let total_gaps = if children.len() > 1 {
        gap * (children.len() as u16 - 1)
    } else {
        0
    };

    // Phase 1: measure fixed children, collect flex totals
    let mut child_main_sizes: Vec<u16> = vec![0; children.len()];
    let mut total_fixed = 0u16;
    let mut total_flex = 0.0f32;

    for (i, child) in children.iter().enumerate() {
        if child.flex > 0.0 {
            total_flex += child.flex;
        } else {
            let child_constraints = match direction {
                Direction::Column => Constraints::loose(cross_total, main_total),
                Direction::Row => Constraints::loose(main_total, cross_total),
            };
            let size = measure(&child.element, child_constraints, state);
            let main = match direction {
                Direction::Column => size.height,
                Direction::Row => size.width,
            };
            let main = apply_min_max(main, child.min_size, child.max_size);
            child_main_sizes[i] = main;
            total_fixed += main;
        }
    }

    // Phase 2: distribute remaining space to flex children
    let remaining = main_total.saturating_sub(total_fixed + total_gaps);
    if total_flex > 0.0 {
        let mut distributed = 0u16;
        let flex_count = children.iter().filter(|c| c.flex > 0.0).count();
        let mut flex_idx = 0;
        for (i, child) in children.iter().enumerate() {
            if child.flex > 0.0 {
                flex_idx += 1;
                let share = if flex_idx == flex_count {
                    // Last flex child gets remaining to avoid rounding errors
                    remaining - distributed
                } else {
                    (remaining as f32 * child.flex / total_flex) as u16
                };
                let share = apply_min_max(share, child.min_size, child.max_size);
                child_main_sizes[i] = share;
                distributed += share;
            }
        }
    }

    // Phase 3: place children sequentially
    let mut offset = 0u16;
    let mut child_results = Vec::with_capacity(children.len());
    for (i, child) in children.iter().enumerate() {
        let main_size = child_main_sizes[i];
        let child_area = match direction {
            Direction::Column => Rect {
                x: area.x,
                y: area.y + offset,
                w: cross_total,
                h: main_size,
            },
            Direction::Row => Rect {
                x: area.x + offset,
                y: area.y,
                w: main_size,
                h: cross_total,
            },
        };
        let result = place(&child.element, child_area, state);
        child_results.push(result);
        offset += main_size;
        if i + 1 < children.len() {
            offset += gap;
        }
    }

    LayoutResult {
        area,
        children: child_results,
    }
}

fn place_stack(
    base: &Element,
    overlays: &[crate::element::Overlay],
    area: Rect,
    state: &AppState,
) -> LayoutResult {
    let base_result = place(base, area, state);

    let mut children = vec![base_result];

    for overlay in overlays {
        let (ox, oy, ow, oh) = match &overlay.anchor {
            crate::element::OverlayAnchor::Absolute { x, y, w, h } => (*x, *y, *w, *h),
            crate::element::OverlayAnchor::AnchorPoint {
                coord,
                prefer_above,
                avoid,
            } => {
                let overlay_size = measure(
                    &overlay.element,
                    Constraints::loose(area.w, area.h),
                    state,
                );
                let to_avoid = avoid.first().copied();
                let (y, x) = crate::layout::compute_pos(
                    (coord.line, coord.column),
                    (overlay_size.height, overlay_size.width),
                    area,
                    to_avoid,
                    *prefer_above,
                );
                (x, y, overlay_size.width, overlay_size.height)
            }
        };

        let overlay_area = Rect {
            x: ox,
            y: oy,
            w: ow,
            h: oh,
        };

        let overlay_result = place(&overlay.element, overlay_area, state);
        children.push(overlay_result);
    }

    LayoutResult { area, children }
}

fn apply_min_max(size: u16, min: Option<u16>, max: Option<u16>) -> u16 {
    let mut s = size;
    if let Some(min) = min {
        s = s.max(min);
    }
    if let Some(max) = max {
        s = s.min(max);
    }
    s
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::element::{Edges, Element, FlexChild, Style};
    use crate::protocol::Face;

    fn default_state() -> AppState {
        AppState::default()
    }

    fn root_area(w: u16, h: u16) -> Rect {
        Rect { x: 0, y: 0, w, h }
    }

    #[test]
    fn test_measure_text() {
        let state = default_state();
        let el = Element::text("hello", Face::default());
        let size = measure(&el, Constraints::loose(80, 24), &state);
        assert_eq!(size.width, 5);
        assert_eq!(size.height, 1);
    }

    #[test]
    fn test_measure_flex_column() {
        let state = default_state();
        let el = Element::column(vec![
            FlexChild::fixed(Element::text("aaa", Face::default())),
            FlexChild::fixed(Element::text("bb", Face::default())),
            FlexChild::fixed(Element::text("c", Face::default())),
        ]);
        let size = measure(&el, Constraints::loose(80, 24), &state);
        assert_eq!(size.height, 3); // 1 + 1 + 1
        assert_eq!(size.width, 3); // max cross
    }

    #[test]
    fn test_measure_flex_row_with_flex() {
        let state = default_state();
        let el = Element::row(vec![
            FlexChild::fixed(Element::text("abc", Face::default())),
            FlexChild::flexible(Element::Empty, 1.0),
        ]);
        let size = measure(&el, Constraints::loose(80, 24), &state);
        // With flex children, takes max width
        assert_eq!(size.width, 80);
    }

    #[test]
    fn test_place_flex_column() {
        let state = default_state();
        let el = Element::column(vec![
            FlexChild::fixed(Element::text("a", Face::default())),
            FlexChild::fixed(Element::text("b", Face::default())),
            FlexChild::fixed(Element::text("c", Face::default())),
        ]);
        let result = place(&el, root_area(80, 24), &state);
        assert_eq!(result.children.len(), 3);
        assert_eq!(result.children[0].area.y, 0);
        assert_eq!(result.children[1].area.y, 1);
        assert_eq!(result.children[2].area.y, 2);
    }

    #[test]
    fn test_place_flex_row_gap() {
        let state = default_state();
        let el = Element::Flex {
            direction: Direction::Row,
            children: vec![
                FlexChild::fixed(Element::text("aa", Face::default())),
                FlexChild::fixed(Element::text("bb", Face::default())),
            ],
            gap: 1,
            align: Align::Start,
            cross_align: Align::Start,
        };
        let result = place(&el, root_area(80, 24), &state);
        assert_eq!(result.children.len(), 2);
        assert_eq!(result.children[0].area.x, 0);
        assert_eq!(result.children[0].area.w, 2);
        // gap of 1 between children
        assert_eq!(result.children[1].area.x, 3); // 2 + 1 gap
        assert_eq!(result.children[1].area.w, 2);
    }

    #[test]
    fn test_measure_container_with_border() {
        let state = default_state();
        let el = Element::Container {
            child: Box::new(Element::text("hi", Face::default())),
            border: Some(crate::element::BorderStyle::Rounded),
            shadow: false,
            padding: Edges::ZERO,
            style: Style::from(Face::default()),
        };
        let size = measure(&el, Constraints::loose(80, 24), &state);
        assert_eq!(size.width, 4); // 2 + border(2)
        assert_eq!(size.height, 3); // 1 + border(2)
    }

    #[test]
    fn test_stack_base_fills_area() {
        let state = default_state();
        let el = Element::stack(Element::Empty, vec![]);
        let result = place(&el, root_area(80, 24), &state);
        assert_eq!(result.area, root_area(80, 24));
        // base child fills area
        assert_eq!(result.children.len(), 1);
        assert_eq!(result.children[0].area, root_area(80, 24));
    }

    #[test]
    fn test_stack_overlay_absolute() {
        let state = default_state();
        let el = Element::stack(
            Element::Empty,
            vec![crate::element::Overlay {
                element: Element::text("popup", Face::default()),
                anchor: crate::element::OverlayAnchor::Absolute { x: 10, y: 5, w: 5, h: 1 },
            }],
        );
        let result = place(&el, root_area(80, 24), &state);
        assert_eq!(result.children.len(), 2); // base + 1 overlay
        let overlay = &result.children[1];
        assert_eq!(overlay.area.x, 10);
        assert_eq!(overlay.area.y, 5);
    }

    #[test]
    fn test_place_flex_column_with_flex_children() {
        let state = default_state();
        let el = Element::column(vec![
            FlexChild::fixed(Element::text("top", Face::default())),
            FlexChild::flexible(Element::buffer_ref(0..20), 1.0),
            FlexChild::fixed(Element::text("bottom", Face::default())),
        ]);
        let result = place(&el, root_area(80, 24), &state);
        assert_eq!(result.children.len(), 3);
        assert_eq!(result.children[0].area.y, 0);
        assert_eq!(result.children[0].area.h, 1); // fixed "top"
        assert_eq!(result.children[1].area.y, 1);
        assert_eq!(result.children[1].area.h, 22); // 24 - 1 - 1
        assert_eq!(result.children[2].area.y, 23);
        assert_eq!(result.children[2].area.h, 1); // fixed "bottom"
    }

    #[test]
    fn test_overlay_uses_anchor_size_not_measure() {
        let state = default_state();
        let overlay_el = Element::row(vec![
            FlexChild::flexible(Element::text("ab", Face::default()), 1.0),
            FlexChild::fixed(Element::text("x", Face::default())),
        ]);
        let el = Element::stack(
            Element::Empty,
            vec![crate::element::Overlay {
                element: overlay_el,
                anchor: crate::element::OverlayAnchor::Absolute { x: 5, y: 3, w: 10, h: 1 },
            }],
        );
        let result = place(&el, root_area(80, 24), &state);
        let overlay = &result.children[1];
        assert_eq!(overlay.area.w, 10); // not 80
        assert_eq!(overlay.area.x, 5);
    }

    #[test]
    fn test_measure_text_unicode_width() {
        let state = default_state();
        let el = Element::text("█", Face::default());
        let size = measure(&el, Constraints::loose(80, 24), &state);
        assert_eq!(size.width, 1); // display width 1, not byte length 3
    }

    #[test]
    fn test_measure_row_all_flexible_has_height() {
        // A Row where ALL children are flexible must still report cross-axis
        // height based on children's intrinsic size (e.g. 1 for text).
        let state = default_state();
        let el = Element::row(vec![
            FlexChild::flexible(Element::text("a", Face::default()), 1.0),
            FlexChild::flexible(Element::text("b", Face::default()), 1.0),
        ]);
        let size = measure(&el, Constraints::loose(80, 24), &state);
        assert_eq!(size.height, 1);
    }

    #[test]
    fn test_place_column_of_all_flexible_rows() {
        // Regression: Column of Rows with all-flexible children must place
        // each row at a distinct y, not collapse them to the same position.
        let state = default_state();
        let rows: Vec<FlexChild> = (0..3)
            .map(|_| {
                FlexChild::fixed(Element::row(vec![
                    FlexChild::flexible(Element::text("x", Face::default()), 1.0),
                ]))
            })
            .collect();
        let el = Element::column(rows);
        let result = place(&el, root_area(80, 10), &state);
        assert_eq!(result.children.len(), 3);
        assert_eq!(result.children[0].area.y, 0);
        assert_eq!(result.children[1].area.y, 1);
        assert_eq!(result.children[2].area.y, 2);
        assert_eq!(result.children[0].area.h, 1);
    }
}
