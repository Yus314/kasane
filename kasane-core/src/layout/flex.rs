use unicode_width::UnicodeWidthStr;

use super::Rect;
use crate::element::{Align, Direction, Element, FlexChild};
use crate::layout::line_display_width;
use crate::state::AppState;

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
            let w = line_display_width(atoms) as u16;
            Size {
                width: w.clamp(constraints.min_width, constraints.max_width),
                height: 1u16.clamp(constraints.min_height, constraints.max_height),
            }
        }
        Element::BufferRef { line_range, .. } => {
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
        Element::Image { size, .. } | Element::Canvas { size, .. } => Size {
            width: size.0.clamp(constraints.min_width, constraints.max_width),
            height: size.1.clamp(constraints.min_height, constraints.max_height),
        },
        Element::TextPanel {
            lines,
            line_numbers,
            ..
        } => {
            let gutter_w = if *line_numbers {
                // digits for max line number + 1 separator
                let digits = (lines.len().max(1) as f64).log10().floor() as u16 + 1;
                digits + 1
            } else {
                0
            };
            let h = (lines.len() as u16).clamp(constraints.min_height, constraints.max_height);
            Size {
                width: constraints.max_width.max(gutter_w),
                height: h,
            }
        }
        Element::SlotPlaceholder { .. } => {
            debug_assert!(false, "unresolved SlotPlaceholder reached layout::measure");
            Size {
                width: constraints.min_width,
                height: constraints.min_height,
            }
        }
        Element::Flex {
            direction,
            children,
            gap,
            ..
        } => measure_flex(*direction, children, *gap, constraints, state),
        Element::ResolvedSlot {
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
                width: (child_size.width + extra_w)
                    .clamp(constraints.min_width, constraints.max_width),
                height: (child_size.height + extra_h)
                    .clamp(constraints.min_height, constraints.max_height),
            }
        }
        Element::Grid {
            columns,
            children,
            col_gap,
            row_gap,
            ..
        } => super::grid::measure_grid(columns, children, *col_gap, *row_gap, constraints, state),
        Element::Interactive { child, .. } => measure(child, constraints, state),
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
                width: child_size
                    .width
                    .clamp(constraints.min_width, constraints.max_width),
                height: child_size
                    .height
                    .clamp(constraints.min_height, constraints.max_height),
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

    let max_main = direction.main(Size {
        width: constraints.max_width,
        height: constraints.max_height,
    });

    let mut main_fixed = 0u16;
    let mut cross_max = 0u16;
    let mut total_flex = 0.0f32;

    for child in children {
        let child_constraints = Constraints::loose(constraints.max_width, constraints.max_height);
        let size = measure(&child.element, child_constraints, state);
        let (main, cross) = direction.decompose(size);
        cross_max = cross_max.max(cross);

        if child.flex > 0.0 {
            total_flex += child.flex;
        } else {
            main_fixed += main;
        }
    }

    let main_total = if total_flex > 0.0 {
        max_main
    } else {
        main_fixed + total_gap
    };

    let result = direction.compose(main_total, cross_max);
    Size {
        width: result
            .width
            .clamp(constraints.min_width, constraints.max_width),
        height: result
            .height
            .clamp(constraints.min_height, constraints.max_height),
    }
}

/// Place an element top-down: assign concrete positions to all children.
pub fn place(element: &Element, area: Rect, state: &AppState) -> LayoutResult {
    crate::perf::perf_span!("layout_place");
    match element {
        Element::Text(..)
        | Element::StyledLine(..)
        | Element::BufferRef { .. }
        | Element::SlotPlaceholder { .. }
        | Element::Image { .. }
        | Element::Canvas { .. }
        | Element::TextPanel { .. }
        | Element::Empty => LayoutResult {
            area,
            children: vec![],
        },
        Element::Grid {
            columns,
            children,
            col_gap,
            row_gap,
            align,
            cross_align,
        } => super::grid::place_grid(
            columns,
            children,
            *col_gap,
            *row_gap,
            *align,
            *cross_align,
            area,
            state,
        ),
        Element::Interactive { child, .. } => {
            let child_result = place(child, area, state);
            LayoutResult {
                area,
                children: vec![child_result],
            }
        }
        Element::Flex {
            direction,
            children,
            gap,
            align,
            cross_align,
            ..
        } => place_flex(
            *direction,
            children,
            *gap,
            *align,
            *cross_align,
            area,
            state,
        ),
        Element::ResolvedSlot {
            direction,
            children,
            gap,
            ..
        } => place_flex(
            *direction,
            children,
            *gap,
            Align::Start,
            Align::Start,
            area,
            state,
        ),
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
                h: area.h.saturating_sub(padding.vertical() + border_size * 2),
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
    align: Align,
    cross_align: Align,
    area: Rect,
    state: &AppState,
) -> LayoutResult {
    if children.is_empty() {
        return LayoutResult {
            area,
            children: vec![],
        };
    }

    let area_size = Size {
        width: area.w,
        height: area.h,
    };
    let main_total = direction.main(area_size);
    let cross_total = direction.cross(area_size);

    let total_gaps = if children.len() > 1 {
        gap * (children.len() as u16 - 1)
    } else {
        0
    };

    let mut child_main_sizes: Vec<u16> = vec![0; children.len()];
    let mut child_cross_sizes: Vec<u16> = vec![cross_total; children.len()];

    // Phase 1: measure fixed children, collect flex totals
    let (total_fixed, total_flex) = measure_fixed_children(
        direction,
        children,
        main_total,
        cross_total,
        &mut child_main_sizes,
        &mut child_cross_sizes,
        state,
    );

    // Phase 2: distribute remaining space to flex children
    let flex_measure = FlexMeasure {
        main_total,
        cross_total,
        total_gaps,
        total_fixed,
        total_flex,
    };
    distribute_flex_space(
        direction,
        children,
        &flex_measure,
        &mut child_main_sizes,
        &mut child_cross_sizes,
        state,
    );

    // Main-axis align: compute start offset (only effective when no flex children)
    let used_main: u16 = child_main_sizes.iter().sum::<u16>() + total_gaps;
    let main_offset = if total_flex > 0.0 {
        0u16
    } else {
        let leftover = main_total.saturating_sub(used_main);
        match align {
            Align::Start => 0,
            Align::Center => leftover / 2,
            Align::End => leftover,
        }
    };

    // Phase 3: place children sequentially
    let mut offset = main_offset;
    let mut child_results = Vec::with_capacity(children.len());
    for (i, child) in children.iter().enumerate() {
        let main_size = child_main_sizes[i];
        let child_cross = child_cross_sizes[i];

        // Cross-axis offset
        let cross_offset = match cross_align {
            Align::Start => 0u16,
            Align::Center => cross_total.saturating_sub(child_cross) / 2,
            Align::End => cross_total.saturating_sub(child_cross),
        };

        // Cross-axis size: for Start, use full cross_total (current behavior);
        // for Center/End, use child's measured cross size
        let child_cross_size = match cross_align {
            Align::Start => cross_total,
            Align::Center | Align::End => child_cross,
        };

        let child_area = direction.rect(
            (area.x, area.y),
            offset,
            cross_offset,
            main_size,
            child_cross_size,
        );
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

/// Phase 1: measure fixed children and collect flex totals.
/// Returns `(total_fixed, total_flex)`.
fn measure_fixed_children(
    direction: Direction,
    children: &[FlexChild],
    main_total: u16,
    cross_total: u16,
    child_main_sizes: &mut [u16],
    child_cross_sizes: &mut [u16],
    state: &AppState,
) -> (u16, f32) {
    let mut total_fixed = 0u16;
    let mut total_flex = 0.0f32;

    for (i, child) in children.iter().enumerate() {
        if child.flex > 0.0 {
            total_flex += child.flex;
        } else {
            let loose = direction.compose(main_total, cross_total);
            let child_constraints = Constraints::loose(loose.width, loose.height);
            let size = measure(&child.element, child_constraints, state);
            let (main, cross) = direction.decompose(size);
            let main = apply_min_max(main, child.min_size, child.max_size);
            child_main_sizes[i] = main;
            child_cross_sizes[i] = cross;
            total_fixed += main;
        }
    }

    (total_fixed, total_flex)
}

/// Measurement results from Phase 1 (fixed children), consumed by Phase 2 (flex distribution).
struct FlexMeasure {
    main_total: u16,
    cross_total: u16,
    total_gaps: u16,
    total_fixed: u16,
    total_flex: f32,
}

/// Phase 2: distribute remaining main-axis space to flex children.
fn distribute_flex_space(
    direction: Direction,
    children: &[FlexChild],
    measure: &FlexMeasure,
    child_main_sizes: &mut [u16],
    child_cross_sizes: &mut [u16],
    state: &AppState,
) {
    if measure.total_flex <= 0.0 {
        return;
    }

    let remaining = measure
        .main_total
        .saturating_sub(measure.total_fixed + measure.total_gaps);
    let mut distributed = 0u16;
    let flex_count = children.iter().filter(|c| c.flex > 0.0).count();
    let mut flex_idx = 0;
    for (i, child) in children.iter().enumerate() {
        if child.flex > 0.0 {
            flex_idx += 1;
            let share = if flex_idx == flex_count {
                remaining - distributed
            } else {
                (remaining as f32 * child.flex / measure.total_flex).round() as u16
            };
            let share = apply_min_max(share, child.min_size, child.max_size);
            child_main_sizes[i] = share;
            distributed += share;
            // Measure cross size for flex children too
            let loose = direction.compose(share, measure.cross_total);
            let child_constraints = Constraints::loose(loose.width, loose.height);
            let size = self::measure(&child.element, child_constraints, state);
            child_cross_sizes[i] = direction.cross(size);
        }
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
        let overlay_result = crate::layout::layout_single_overlay(overlay, area, state);
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
    use crate::test_utils::*;

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
            border: Some(crate::element::BorderConfig::from(
                crate::element::BorderLineStyle::Rounded,
            )),
            shadow: false,
            padding: Edges::ZERO,
            style: Style::from(Face::default()),
            title: None,
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
                anchor: crate::element::OverlayAnchor::Absolute {
                    x: 10,
                    y: 5,
                    w: 5,
                    h: 1,
                },
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
                anchor: crate::element::OverlayAnchor::Absolute {
                    x: 5,
                    y: 3,
                    w: 10,
                    h: 1,
                },
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
                FlexChild::fixed(Element::row(vec![FlexChild::flexible(
                    Element::text("x", Face::default()),
                    1.0,
                )]))
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

    #[test]
    fn test_align_center_column() {
        let state = default_state();
        // Column with 2 fixed children (h=1 each) in h=10 area → 8 leftover
        let el = Element::Flex {
            direction: Direction::Column,
            children: vec![
                FlexChild::fixed(Element::text("a", Face::default())),
                FlexChild::fixed(Element::text("b", Face::default())),
            ],
            gap: 0,
            align: Align::Center,
            cross_align: Align::Start,
        };
        let result = place(&el, root_area(80, 10), &state);
        // Center offset = 8 / 2 = 4
        assert_eq!(result.children[0].area.y, 4);
        assert_eq!(result.children[1].area.y, 5);
    }

    #[test]
    fn test_align_end_row() {
        let state = default_state();
        // Row with 1 fixed child (w=3) in w=20 area → 17 leftover
        let el = Element::Flex {
            direction: Direction::Row,
            children: vec![FlexChild::fixed(Element::text("abc", Face::default()))],
            gap: 0,
            align: Align::End,
            cross_align: Align::Start,
        };
        let result = place(&el, root_area(20, 10), &state);
        assert_eq!(result.children[0].area.x, 17);
    }

    #[test]
    fn test_align_start_unchanged() {
        let state = default_state();
        let el = Element::Flex {
            direction: Direction::Row,
            children: vec![FlexChild::fixed(Element::text("abc", Face::default()))],
            gap: 0,
            align: Align::Start,
            cross_align: Align::Start,
        };
        let result = place(&el, root_area(20, 10), &state);
        assert_eq!(result.children[0].area.x, 0);
    }

    #[test]
    fn test_cross_align_center_row() {
        let state = default_state();
        // Row with a text child (h=1) in h=10 area → cross center offset = (10-1)/2 = 4
        let el = Element::Flex {
            direction: Direction::Row,
            children: vec![FlexChild::fixed(Element::text("abc", Face::default()))],
            gap: 0,
            align: Align::Start,
            cross_align: Align::Center,
        };
        let result = place(&el, root_area(20, 10), &state);
        assert_eq!(result.children[0].area.y, 4);
        assert_eq!(result.children[0].area.h, 1);
    }

    #[test]
    fn test_cross_align_end_column() {
        let state = default_state();
        // Column with a text child (w=3) in w=20 area → cross end offset = 20-3 = 17
        let el = Element::Flex {
            direction: Direction::Column,
            children: vec![FlexChild::fixed(Element::text("abc", Face::default()))],
            gap: 0,
            align: Align::Start,
            cross_align: Align::End,
        };
        let result = place(&el, root_area(20, 10), &state);
        assert_eq!(result.children[0].area.x, 17);
        assert_eq!(result.children[0].area.w, 3);
    }

    #[test]
    fn test_align_ignored_with_flex_children() {
        let state = default_state();
        // align should be ignored when flex children consume all space
        let el = Element::Flex {
            direction: Direction::Row,
            children: vec![
                FlexChild::fixed(Element::text("ab", Face::default())),
                FlexChild::flexible(Element::Empty, 1.0),
            ],
            gap: 0,
            align: Align::End,
            cross_align: Align::Start,
        };
        let result = place(&el, root_area(20, 10), &state);
        assert_eq!(result.children[0].area.x, 0); // still starts at 0
    }

    #[test]
    fn test_measure_image() {
        let state = default_state();
        let el = Element::image(
            crate::element::ImageSource::FilePath("test.png".into()),
            10,
            5,
        );
        let size = measure(&el, Constraints::loose(80, 24), &state);
        assert_eq!(size.width, 10);
        assert_eq!(size.height, 5);
    }

    #[test]
    fn test_measure_image_clamped() {
        let state = default_state();
        let el = Element::image(
            crate::element::ImageSource::FilePath("test.png".into()),
            100,
            50,
        );
        let size = measure(&el, Constraints::loose(80, 24), &state);
        assert_eq!(size.width, 80);
        assert_eq!(size.height, 24);
    }

    #[test]
    fn test_place_image_leaf() {
        let state = default_state();
        let el = Element::image(
            crate::element::ImageSource::FilePath("test.png".into()),
            10,
            5,
        );
        let area = root_area(80, 24);
        let result = place(&el, area, &state);
        assert_eq!(result.area, area);
        assert!(result.children.is_empty());
    }

    #[test]
    fn test_measure_text_panel() {
        let state = default_state();
        let lines: Vec<Vec<crate::protocol::Atom>> = (0..10)
            .map(|i| {
                vec![crate::protocol::Atom {
                    face: Face::default(),
                    contents: format!("line {i}").into(),
                }]
            })
            .collect();
        let el = Element::text_panel(lines);
        let size = measure(&el, Constraints::loose(80, 24), &state);
        assert_eq!(size.width, 80);
        assert_eq!(size.height, 10);
    }

    #[test]
    fn test_measure_text_panel_clamped_height() {
        let state = default_state();
        let lines: Vec<Vec<crate::protocol::Atom>> = (0..50)
            .map(|i| {
                vec![crate::protocol::Atom {
                    face: Face::default(),
                    contents: format!("line {i}").into(),
                }]
            })
            .collect();
        let el = Element::text_panel(lines);
        let size = measure(&el, Constraints::loose(80, 24), &state);
        assert_eq!(size.height, 24); // clamped to max
    }

    #[test]
    fn test_measure_text_panel_with_line_numbers() {
        let state = default_state();
        let lines: Vec<Vec<crate::protocol::Atom>> = (0..100)
            .map(|i| {
                vec![crate::protocol::Atom {
                    face: Face::default(),
                    contents: format!("line {i}").into(),
                }]
            })
            .collect();
        let el = Element::TextPanel {
            lines,
            scroll_offset: 0,
            cursor: None,
            line_numbers: true,
            wrap: false,
        };
        let size = measure(&el, Constraints::loose(80, 24), &state);
        assert_eq!(size.width, 80); // still fills width
        assert_eq!(size.height, 24); // clamped
    }

    #[test]
    fn test_place_text_panel_leaf() {
        let state = default_state();
        let el = Element::text_panel(vec![]);
        let area = root_area(40, 10);
        let result = place(&el, area, &state);
        assert_eq!(result.area, area);
        assert!(result.children.is_empty());
    }
}
