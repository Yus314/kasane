use super::Rect;
use super::flex::{Constraints, LayoutResult, Size, measure, place};
use crate::element::{Align, Element, GridColumn, GridWidth};
use crate::state::AppState;

/// Measure a Grid element: compute its intrinsic size.
pub fn measure_grid(
    columns: &[GridColumn],
    children: &[Element],
    col_gap: u16,
    row_gap: u16,
    constraints: Constraints,
    state: &AppState,
) -> Size {
    let num_cols = columns.len();
    if num_cols == 0 {
        return Size {
            width: constraints.min_width,
            height: constraints.min_height,
        };
    }
    let num_rows = children.len().div_ceil(num_cols);

    let total_col_gaps = if num_cols > 1 {
        col_gap * (num_cols as u16 - 1)
    } else {
        0
    };
    let total_row_gaps = if num_rows > 1 {
        row_gap * (num_rows as u16 - 1)
    } else {
        0
    };

    let col_widths = resolve_column_widths(
        columns,
        children,
        num_cols,
        num_rows,
        total_col_gaps,
        constraints,
        state,
    );

    // Compute row heights
    let row_heights = compute_row_heights(
        columns,
        children,
        &col_widths,
        num_cols,
        num_rows,
        constraints,
        state,
    );

    let total_w: u16 = col_widths.iter().sum::<u16>() + total_col_gaps;
    let total_h: u16 = row_heights.iter().sum::<u16>() + total_row_gaps;

    Size {
        width: total_w.clamp(constraints.min_width, constraints.max_width),
        height: total_h.clamp(constraints.min_height, constraints.max_height),
    }
}

/// Place a Grid element: assign concrete positions to all children.
#[allow(clippy::too_many_arguments)]
pub fn place_grid(
    columns: &[GridColumn],
    children: &[Element],
    col_gap: u16,
    row_gap: u16,
    align: Align,
    cross_align: Align,
    area: Rect,
    state: &AppState,
) -> LayoutResult {
    let num_cols = columns.len();
    if num_cols == 0 || children.is_empty() {
        return LayoutResult {
            area,
            children: vec![],
        };
    }
    let num_rows = children.len().div_ceil(num_cols);

    let total_col_gaps = if num_cols > 1 {
        col_gap * (num_cols as u16 - 1)
    } else {
        0
    };
    let total_row_gaps = if num_rows > 1 {
        row_gap * (num_rows as u16 - 1)
    } else {
        0
    };

    let constraints = Constraints::loose(area.w, area.h);
    let col_widths = resolve_column_widths(
        columns,
        children,
        num_cols,
        num_rows,
        total_col_gaps,
        constraints,
        state,
    );
    let row_heights = compute_row_heights(
        columns,
        children,
        &col_widths,
        num_cols,
        num_rows,
        constraints,
        state,
    );

    let used_w: u16 = col_widths.iter().sum::<u16>() + total_col_gaps;
    let used_h: u16 = row_heights.iter().sum::<u16>() + total_row_gaps;

    let align_offset_x = match align {
        Align::Start => 0u16,
        Align::Center => area.w.saturating_sub(used_w) / 2,
        Align::End => area.w.saturating_sub(used_w),
    };
    let align_offset_y = match cross_align {
        Align::Start => 0u16,
        Align::Center => area.h.saturating_sub(used_h) / 2,
        Align::End => area.h.saturating_sub(used_h),
    };

    // Warn in debug builds if children don't fill the last row
    #[cfg(debug_assertions)]
    if !children.is_empty() && !children.len().is_multiple_of(num_cols) {
        tracing::warn!(
            "Grid: children.len()={} is not a multiple of num_cols={}",
            children.len(),
            num_cols,
        );
    }

    let mut child_results = Vec::with_capacity(children.len());
    for (idx, child) in children.iter().enumerate() {
        let r = idx / num_cols;
        let c = idx % num_cols;

        let cell_x =
            area.x + align_offset_x + col_widths[..c].iter().sum::<u16>() + col_gap * c as u16;
        let cell_y =
            area.y + align_offset_y + row_heights[..r].iter().sum::<u16>() + row_gap * r as u16;
        let cell_w = col_widths[c];
        let cell_h = row_heights[r];

        let cell_rect = Rect {
            x: cell_x,
            y: cell_y,
            w: cell_w,
            h: cell_h,
        };
        child_results.push(place(child, cell_rect, state));
    }

    LayoutResult {
        area,
        children: child_results,
    }
}

/// Resolve column widths: Fixed → take value, Auto → max cell width, Flex → distribute remainder.
fn resolve_column_widths(
    columns: &[GridColumn],
    children: &[Element],
    num_cols: usize,
    num_rows: usize,
    total_col_gaps: u16,
    constraints: Constraints,
    state: &AppState,
) -> Vec<u16> {
    let available = constraints.max_width.saturating_sub(total_col_gaps);
    let mut col_widths = vec![0u16; num_cols];

    // Phase 1: Fixed and Auto
    let mut used = 0u16;
    let mut total_flex = 0.0f32;
    for (c, col) in columns.iter().enumerate() {
        match col.width {
            GridWidth::Fixed(w) => {
                col_widths[c] = w;
                used += w;
            }
            GridWidth::Auto => {
                let mut max_w = 0u16;
                for r in 0..num_rows {
                    let idx = r * num_cols + c;
                    if let Some(child) = children.get(idx) {
                        let size = measure(
                            child,
                            Constraints::loose(available, constraints.max_height),
                            state,
                        );
                        max_w = max_w.max(size.width);
                    }
                }
                col_widths[c] = max_w;
                used += max_w;
            }
            GridWidth::Flex(f) => {
                total_flex += f;
            }
        }
    }

    // Phase 2: Flex distribution
    if total_flex > 0.0 {
        let remaining = available.saturating_sub(used);
        let mut distributed = 0u16;
        let flex_cols: Vec<usize> = columns
            .iter()
            .enumerate()
            .filter(|(_, c)| matches!(c.width, GridWidth::Flex(_)))
            .map(|(i, _)| i)
            .collect();

        for (flex_idx, &c) in flex_cols.iter().enumerate() {
            let GridWidth::Flex(f) = columns[c].width else {
                unreachable!()
            };
            let share = if flex_idx + 1 == flex_cols.len() {
                remaining - distributed
            } else {
                (remaining as f32 * f / total_flex) as u16
            };
            col_widths[c] = share;
            distributed += share;
        }
    }

    col_widths
}

/// Compute row heights: max cell height per row.
fn compute_row_heights(
    _columns: &[GridColumn],
    children: &[Element],
    col_widths: &[u16],
    num_cols: usize,
    num_rows: usize,
    constraints: Constraints,
    state: &AppState,
) -> Vec<u16> {
    let mut row_heights = vec![0u16; num_rows];
    for (r, row_h) in row_heights.iter_mut().enumerate() {
        for (c, col_w) in col_widths.iter().enumerate() {
            let idx = r * num_cols + c;
            if let Some(child) = children.get(idx) {
                let cell_constraints = Constraints::loose(*col_w, constraints.max_height);
                let size = measure(child, cell_constraints, state);
                *row_h = (*row_h).max(size.height);
            }
        }
    }
    row_heights
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::element::{Align, Element, GridColumn};
    use crate::layout::flex::measure;
    use crate::protocol::Face;
    use crate::test_utils::*;

    #[test]
    fn test_measure_grid_fixed_columns() {
        let state = default_state();
        let el = Element::grid(
            vec![GridColumn::fixed(5), GridColumn::fixed(10)],
            vec![Element::plain_text("a"), Element::plain_text("b")],
        );
        let size = measure(&el, Constraints::loose(80, 24), &state);
        assert_eq!(size.width, 15); // 5 + 10
        assert_eq!(size.height, 1);
    }

    #[test]
    fn test_measure_grid_auto_columns() {
        let state = default_state();
        let el = Element::grid(
            vec![GridColumn::auto(), GridColumn::auto()],
            vec![
                Element::plain_text("hello"), // w=5
                Element::plain_text("ab"),    // w=2
                Element::plain_text("x"),     // w=1
                Element::plain_text("world"), // w=5
            ],
        );
        let size = measure(&el, Constraints::loose(80, 24), &state);
        // col0 = max(5, 1) = 5, col1 = max(2, 5) = 5
        assert_eq!(size.width, 10);
        assert_eq!(size.height, 2);
    }

    #[test]
    fn test_measure_grid_flex_columns() {
        let state = default_state();
        let el = Element::Grid {
            columns: vec![GridColumn::fixed(10), GridColumn::flex(1.0)],
            children: vec![Element::plain_text("a"), Element::plain_text("b")],
            col_gap: 0,
            row_gap: 0,
            align: Align::Start,
            cross_align: Align::Start,
        };
        let size = measure(&el, Constraints::loose(80, 24), &state);
        // Fixed=10, Flex gets remaining=70
        assert_eq!(size.width, 80);
        assert_eq!(size.height, 1);
    }

    #[test]
    fn test_measure_grid_mixed_columns() {
        let state = default_state();
        let el = Element::Grid {
            columns: vec![
                GridColumn::fixed(10),
                GridColumn::auto(),
                GridColumn::flex(1.0),
            ],
            children: vec![
                Element::plain_text("a"),
                Element::plain_text("hello"), // auto → 5
                Element::plain_text("c"),
            ],
            col_gap: 0,
            row_gap: 0,
            align: Align::Start,
            cross_align: Align::Start,
        };
        let size = measure(&el, Constraints::loose(80, 24), &state);
        // Fixed=10, Auto=5, Flex=65 → total=80
        assert_eq!(size.width, 80);
    }

    #[test]
    fn test_place_grid_cell_positions() {
        let state = default_state();
        let el = Element::grid(
            vec![GridColumn::fixed(5), GridColumn::fixed(10)],
            vec![
                Element::plain_text("a"),
                Element::plain_text("b"),
                Element::plain_text("c"),
                Element::plain_text("d"),
            ],
        );
        let result = crate::layout::flex::place(&el, root_area(80, 24), &state);
        assert_eq!(result.children.len(), 4);
        // Row 0: (0,0,5,1), (5,0,10,1)
        assert_eq!(
            result.children[0].area,
            Rect {
                x: 0,
                y: 0,
                w: 5,
                h: 1
            }
        );
        assert_eq!(
            result.children[1].area,
            Rect {
                x: 5,
                y: 0,
                w: 10,
                h: 1
            }
        );
        // Row 1: (0,1,5,1), (5,1,10,1)
        assert_eq!(
            result.children[2].area,
            Rect {
                x: 0,
                y: 1,
                w: 5,
                h: 1
            }
        );
        assert_eq!(
            result.children[3].area,
            Rect {
                x: 5,
                y: 1,
                w: 10,
                h: 1
            }
        );
    }

    #[test]
    fn test_place_grid_col_gap() {
        let state = default_state();
        let el = Element::Grid {
            columns: vec![GridColumn::fixed(5), GridColumn::fixed(5)],
            children: vec![Element::plain_text("a"), Element::plain_text("b")],
            col_gap: 2,
            row_gap: 0,
            align: Align::Start,
            cross_align: Align::Start,
        };
        let result = crate::layout::flex::place(&el, root_area(80, 24), &state);
        assert_eq!(result.children[0].area.x, 0);
        assert_eq!(result.children[1].area.x, 7); // 5 + 2 gap
    }

    #[test]
    fn test_place_grid_row_gap() {
        let state = default_state();
        let el = Element::Grid {
            columns: vec![GridColumn::fixed(5)],
            children: vec![Element::plain_text("a"), Element::plain_text("b")],
            col_gap: 0,
            row_gap: 3,
            align: Align::Start,
            cross_align: Align::Start,
        };
        let result = crate::layout::flex::place(&el, root_area(80, 24), &state);
        assert_eq!(result.children[0].area.y, 0);
        assert_eq!(result.children[1].area.y, 4); // 1 + 3 gap
    }

    #[test]
    fn test_place_grid_align_center() {
        let state = default_state();
        let el = Element::Grid {
            columns: vec![GridColumn::fixed(5), GridColumn::fixed(5)],
            children: vec![Element::plain_text("a"), Element::plain_text("b")],
            col_gap: 0,
            row_gap: 0,
            align: Align::Center,
            cross_align: Align::Start,
        };
        let result = crate::layout::flex::place(&el, root_area(20, 10), &state);
        // used_w=10, leftover=10, offset=5
        assert_eq!(result.children[0].area.x, 5);
        assert_eq!(result.children[1].area.x, 10);
    }

    #[test]
    fn test_place_grid_cross_align_end() {
        let state = default_state();
        let el = Element::Grid {
            columns: vec![GridColumn::fixed(5)],
            children: vec![Element::plain_text("a"), Element::plain_text("b")],
            col_gap: 0,
            row_gap: 0,
            align: Align::Start,
            cross_align: Align::End,
        };
        let result = crate::layout::flex::place(&el, root_area(20, 10), &state);
        // used_h=2, leftover=8, offset=8
        assert_eq!(result.children[0].area.y, 8);
        assert_eq!(result.children[1].area.y, 9);
    }

    #[test]
    fn test_place_grid_partial_last_row() {
        let state = default_state();
        // 7 children, 3 columns → 3 rows, last row has 1 child
        let el = Element::grid(
            vec![
                GridColumn::fixed(5),
                GridColumn::fixed(5),
                GridColumn::fixed(5),
            ],
            vec![
                Element::plain_text("1"),
                Element::plain_text("2"),
                Element::plain_text("3"),
                Element::plain_text("4"),
                Element::plain_text("5"),
                Element::plain_text("6"),
                Element::plain_text("7"),
            ],
        );
        let result = crate::layout::flex::place(&el, root_area(80, 24), &state);
        assert_eq!(result.children.len(), 7);
        // Last child at row 2, col 0
        assert_eq!(result.children[6].area.y, 2);
        assert_eq!(result.children[6].area.x, 0);
    }

    #[test]
    fn test_place_grid_empty_children() {
        let state = default_state();
        let el = Element::grid(vec![GridColumn::fixed(5), GridColumn::fixed(5)], vec![]);
        let result = crate::layout::flex::place(&el, root_area(80, 24), &state);
        assert_eq!(result.children.len(), 0);
    }

    #[test]
    fn test_place_grid_single_column() {
        let state = default_state();
        let el = Element::grid(
            vec![GridColumn::fixed(10)],
            vec![
                Element::plain_text("a"),
                Element::plain_text("b"),
                Element::plain_text("c"),
            ],
        );
        let result = crate::layout::flex::place(&el, root_area(80, 24), &state);
        assert_eq!(result.children.len(), 3);
        assert_eq!(result.children[0].area.y, 0);
        assert_eq!(result.children[1].area.y, 1);
        assert_eq!(result.children[2].area.y, 2);
        // All same column
        assert_eq!(result.children[0].area.x, 0);
        assert_eq!(result.children[1].area.x, 0);
        assert_eq!(result.children[2].area.x, 0);
        assert_eq!(result.children[0].area.w, 10);
    }
}
