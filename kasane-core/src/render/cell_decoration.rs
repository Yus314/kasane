//! Cell-level decoration application.
//!
//! Maps `CellDecoration` items (buffer coordinates) to grid coordinates and
//! merges decoration faces onto the `CellGrid` according to the specified
//! `FaceMerge` mode.

use crate::display::DisplayMap;
use crate::plugin::{CellDecoration, DecorationTarget};
use crate::render::grid::CellGrid;

/// Apply a sorted (by priority) list of cell decorations to the grid.
///
/// Coordinate mapping follows the same pattern as `apply_secondary_cursor_faces`:
///   grid_x = coord.column + buffer_x_offset
///   grid_y = display_map(coord.line).saturating_sub(display_scroll_offset) + buffer_y_offset
pub fn apply_cell_decorations(
    decorations: &[CellDecoration],
    grid: &mut CellGrid,
    buffer_x_offset: u16,
    display_map: Option<&DisplayMap>,
    buffer_y_offset: u16,
    display_scroll_offset: u16,
) {
    for dec in decorations {
        match &dec.target {
            DecorationTarget::Cell(coord) => {
                if let Some((gx, gy)) = buffer_to_grid(
                    coord.column as u16,
                    coord.line as u16,
                    buffer_x_offset,
                    display_map,
                    buffer_y_offset,
                    display_scroll_offset,
                ) && let Some(cell) = grid.get_mut(gx, gy)
                {
                    dec.merge.apply(&mut cell.face, &dec.face);
                }
            }
            DecorationTarget::Range { start, end } => {
                // Iterate all lines in the range
                let start_line = start.line.max(0) as usize;
                let end_line = end.line.max(0) as usize;
                for buf_line in start_line..=end_line {
                    let col_start = if buf_line == start_line {
                        start.column.max(0) as u16
                    } else {
                        0
                    };
                    let col_end = if buf_line == end_line {
                        end.column.max(0) as u16
                    } else {
                        // Extend to grid width as a practical upper bound
                        grid.width()
                            .saturating_sub(buffer_x_offset)
                            .saturating_sub(1)
                    };
                    if let Some((_, gy)) = buffer_to_grid(
                        0,
                        buf_line as u16,
                        buffer_x_offset,
                        display_map,
                        buffer_y_offset,
                        display_scroll_offset,
                    ) {
                        for col in col_start..=col_end {
                            let gx = col + buffer_x_offset;
                            if let Some(cell) = grid.get_mut(gx, gy) {
                                dec.merge.apply(&mut cell.face, &dec.face);
                            }
                        }
                    }
                }
            }
            DecorationTarget::Column { column } => {
                // Apply to all visible rows at this column
                let gx = *column + buffer_x_offset;
                for gy in 0..grid.height() {
                    if let Some(cell) = grid.get_mut(gx, gy) {
                        dec.merge.apply(&mut cell.face, &dec.face);
                    }
                }
            }
        }
    }
}

/// Convert buffer coordinates to grid coordinates.
///
/// Returns `None` if the buffer line is folded away by the display map.
fn buffer_to_grid(
    buf_col: u16,
    buf_line: u16,
    buffer_x_offset: u16,
    display_map: Option<&DisplayMap>,
    buffer_y_offset: u16,
    display_scroll_offset: u16,
) -> Option<(u16, u16)> {
    let gx = buf_col + buffer_x_offset;
    let gy = display_map
        .and_then(|dm| {
            if dm.is_identity() {
                None
            } else {
                dm.buffer_to_display(buf_line as usize).map(|y| y as u16)
            }
        })
        .unwrap_or(buf_line)
        .saturating_sub(display_scroll_offset)
        + buffer_y_offset;
    Some((gx, gy))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::plugin::FaceMerge;
    use crate::protocol::{Color, Coord, Face};

    fn make_decoration(target: DecorationTarget, merge: FaceMerge) -> CellDecoration {
        CellDecoration {
            target,
            face: Face {
                fg: Color::Default,
                bg: Color::Rgb { r: 255, g: 0, b: 0 },
                underline: Color::Default,
                attributes: Default::default(),
            },
            merge,
            priority: 0,
        }
    }

    #[test]
    fn cell_decoration_single_cell() {
        let mut grid = CellGrid::new(10, 5);
        let dec = make_decoration(
            DecorationTarget::Cell(Coord { line: 1, column: 3 }),
            FaceMerge::Background,
        );
        apply_cell_decorations(&[dec], &mut grid, 0, None, 0, 0);
        let cell = grid.get(3, 1).unwrap();
        assert_eq!(cell.face.bg, Color::Rgb { r: 255, g: 0, b: 0 });
        // Other cells unchanged
        let other = grid.get(0, 0).unwrap();
        assert_eq!(other.face.bg, Color::Default);
    }

    #[test]
    fn cell_decoration_column() {
        let mut grid = CellGrid::new(10, 5);
        let dec = make_decoration(
            DecorationTarget::Column { column: 2 },
            FaceMerge::Background,
        );
        apply_cell_decorations(&[dec], &mut grid, 0, None, 0, 0);
        for y in 0..5 {
            let cell = grid.get(2, y).unwrap();
            assert_eq!(cell.face.bg, Color::Rgb { r: 255, g: 0, b: 0 });
        }
        // Adjacent column unchanged
        let cell = grid.get(3, 0).unwrap();
        assert_eq!(cell.face.bg, Color::Default);
    }

    #[test]
    fn cell_decoration_range_single_line() {
        let mut grid = CellGrid::new(10, 5);
        let dec = make_decoration(
            DecorationTarget::Range {
                start: Coord { line: 2, column: 1 },
                end: Coord { line: 2, column: 4 },
            },
            FaceMerge::Background,
        );
        apply_cell_decorations(&[dec], &mut grid, 0, None, 0, 0);
        for col in 1..=4 {
            let cell = grid.get(col, 2).unwrap();
            assert_eq!(cell.face.bg, Color::Rgb { r: 255, g: 0, b: 0 });
        }
        // Outside range unchanged
        let cell = grid.get(0, 2).unwrap();
        assert_eq!(cell.face.bg, Color::Default);
    }

    #[test]
    fn cell_decoration_with_buffer_offset() {
        let mut grid = CellGrid::new(15, 5);
        let dec = make_decoration(
            DecorationTarget::Cell(Coord { line: 0, column: 0 }),
            FaceMerge::Background,
        );
        // buffer_x_offset=3, buffer_y_offset=1
        apply_cell_decorations(&[dec], &mut grid, 3, None, 1, 0);
        let cell = grid.get(3, 1).unwrap();
        assert_eq!(cell.face.bg, Color::Rgb { r: 255, g: 0, b: 0 });
    }

    #[test]
    fn replace_merge_mode() {
        let mut grid = CellGrid::new(10, 5);
        // Set initial face
        if let Some(cell) = grid.get_mut(0, 0) {
            cell.face.fg = Color::Rgb { r: 0, g: 255, b: 0 };
        }
        let dec = make_decoration(
            DecorationTarget::Cell(Coord { line: 0, column: 0 }),
            FaceMerge::Replace,
        );
        apply_cell_decorations(&[dec], &mut grid, 0, None, 0, 0);
        let cell = grid.get(0, 0).unwrap();
        // Replace should have overwritten fg to Default
        assert_eq!(cell.face.fg, Color::Default);
        assert_eq!(cell.face.bg, Color::Rgb { r: 255, g: 0, b: 0 });
    }
}
