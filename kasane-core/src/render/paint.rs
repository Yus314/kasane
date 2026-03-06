use unicode_width::UnicodeWidthStr;

use crate::element::{BorderStyle, Element};
use crate::layout::Rect;
use crate::layout::flex::LayoutResult;
use crate::protocol::{Attributes, Color, Face};
use crate::state::AppState;
use super::grid::CellGrid;

/// Paint an element tree into a CellGrid using pre-computed layout results.
pub fn paint(element: &Element, layout: &LayoutResult, grid: &mut CellGrid, state: &AppState) {
    let area = layout.area;

    match element {
        Element::Text(text, style) => {
            paint_text(grid, &area, text, &style.face);
        }
        Element::StyledLine(atoms) => {
            let line = atoms.to_vec();
            grid.put_line_with_base(area.y, area.x, &line, area.w, None);
        }
        Element::BufferRef { line_range } => {
            paint_buffer_ref(grid, &area, line_range.clone(), state);
        }
        Element::Empty => {}
        Element::Flex { children, .. } => {
            for (i, child) in children.iter().enumerate() {
                if let Some(child_layout) = layout.children.get(i) {
                    paint(&child.element, child_layout, grid, state);
                }
            }
        }
        Element::Stack { base, overlays } => {
            // Paint base
            if let Some(base_layout) = layout.children.first() {
                paint(base, base_layout, grid, state);
            }
            // Paint overlays in Z-order
            for (i, overlay) in overlays.iter().enumerate() {
                if let Some(overlay_layout) = layout.children.get(i + 1) {
                    paint(&overlay.element, overlay_layout, grid, state);
                }
            }
        }
        Element::Container {
            child,
            border,
            shadow,
            padding: _,
            style,
        } => {
            paint_container(grid, &area, child, border, *shadow, &style.face, layout, state);
        }
        Element::Scrollable {
            child,
            offset: _,
            direction: _,
        } => {
            // Paint child using the child's layout (which has virtual coordinates).
            // CellGrid::put_char already clips to grid bounds.
            if let Some(child_layout) = layout.children.first() {
                paint(child, child_layout, grid, state);
            }
        }
    }
}

fn paint_text(grid: &mut CellGrid, area: &Rect, text: &str, face: &Face) {
    let mut x = area.x;
    let limit = area.x + area.w;
    for ch in text.chars() {
        if x >= limit {
            break;
        }
        if ch.is_control() {
            continue;
        }
        let s = ch.to_string();
        let w = UnicodeWidthStr::width(s.as_str()) as u16;
        if w == 0 {
            continue;
        }
        if x + w > limit {
            break;
        }
        grid.put_char(x, area.y, &s, face);
        x += w;
    }
}

fn paint_buffer_ref(
    grid: &mut CellGrid,
    area: &Rect,
    line_range: std::ops::Range<usize>,
    state: &AppState,
) {
    for y_offset in 0..area.h {
        let line_idx = line_range.start + y_offset as usize;
        let y = area.y + y_offset;

        if let Some(line) = state.lines.get(line_idx) {
            grid.fill_row(y, &state.default_face);
            grid.put_line_with_base(y, area.x, line, area.w, Some(&state.default_face));
        } else {
            // Padding row
            grid.fill_row(y, &state.padding_face);
            grid.put_char(area.x, y, "~", &state.padding_face);
        }
    }
}

fn paint_container(
    grid: &mut CellGrid,
    area: &Rect,
    child: &Element,
    border: &Option<BorderStyle>,
    shadow: bool,
    face: &Face,
    layout: &LayoutResult,
    state: &AppState,
) {
    // Shadow (drawn first, behind the container)
    if shadow {
        paint_shadow(grid, area);
    }

    // Fill entire container area with face
    for row in 0..area.h {
        let y = area.y + row;
        for x in area.x..(area.x + area.w).min(grid.width) {
            grid.put_char(x, y, " ", face);
        }
    }

    // Border
    if let Some(border_style) = border {
        paint_border(grid, area, face, false, *border_style);
    }

    // Paint child
    if let Some(child_layout) = layout.children.first() {
        paint(child, child_layout, grid, state);
    }
}

fn paint_border(
    grid: &mut CellGrid,
    area: &Rect,
    face: &Face,
    truncated: bool,
    border_style: BorderStyle,
) {
    if area.w < 2 || area.h < 2 {
        return;
    }

    let (tl, tr, bl, br) = match border_style {
        BorderStyle::Single => ("┌", "┐", "└", "┘"),
        BorderStyle::Rounded => ("╭", "╮", "╰", "╯"),
    };

    let x1 = area.x;
    let y1 = area.y;
    let x2 = area.x + area.w - 1;
    let y2 = area.y + area.h - 1;
    let bottom_dash = if truncated { "┄" } else { "─" };

    // Corners
    grid.put_char(x1, y1, tl, face);
    grid.put_char(x2, y1, tr, face);
    grid.put_char(x1, y2, bl, face);
    grid.put_char(x2, y2, br, face);

    // Top and bottom edges
    for x in (x1 + 1)..x2 {
        grid.put_char(x, y1, "─", face);
        grid.put_char(x, y2, bottom_dash, face);
    }

    // Left and right edges
    for y in (y1 + 1)..y2 {
        grid.put_char(x1, y, "│", face);
        grid.put_char(x2, y, "│", face);
    }
}

fn paint_shadow(grid: &mut CellGrid, area: &Rect) {
    let dim_face = Face {
        fg: Color::Default,
        bg: Color::Default,
        underline: Color::Default,
        attributes: Attributes::DIM,
    };

    // Right shadow (1 cell wide)
    let sx = area.x + area.w;
    if sx < grid.width {
        for y in (area.y + 1)..=(area.y + area.h) {
            if y < grid.height {
                grid.put_char(sx, y, " ", &dim_face);
            }
        }
    }

    // Bottom shadow (1 cell tall)
    let sy = area.y + area.h;
    if sy < grid.height {
        for x in (area.x + 1)..=(area.x + area.w) {
            if x < grid.width {
                grid.put_char(x, sy, " ", &dim_face);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::element::{Edges, Element, FlexChild, Overlay, OverlayAnchor, Style};
    use crate::layout::flex::place;
    use crate::protocol::{Atom, Face};

    fn default_state() -> AppState {
        AppState::default()
    }

    fn root_area(w: u16, h: u16) -> Rect {
        Rect { x: 0, y: 0, w, h }
    }

    fn make_line(s: &str) -> Vec<Atom> {
        vec![Atom {
            face: Face::default(),
            contents: s.to_string(),
        }]
    }

    #[test]
    fn test_paint_text() {
        let state = default_state();
        let mut grid = CellGrid::new(20, 5);
        let el = Element::text("hello", Face::default());
        let area = root_area(20, 5);
        let layout = place(&el, area, &state);
        paint(&el, &layout, &mut grid, &state);
        assert_eq!(grid.get(0, 0).unwrap().grapheme, "h");
        assert_eq!(grid.get(4, 0).unwrap().grapheme, "o");
    }

    #[test]
    fn test_paint_buffer_ref() {
        let mut state = default_state();
        state.lines = vec![make_line("line1"), make_line("line2")];
        state.cols = 10;
        state.rows = 4;

        let mut grid = CellGrid::new(10, 4);
        let el = Element::buffer_ref(0..3);
        let area = Rect {
            x: 0,
            y: 0,
            w: 10,
            h: 3,
        };
        let layout = place(&el, area, &state);
        paint(&el, &layout, &mut grid, &state);

        assert_eq!(grid.get(0, 0).unwrap().grapheme, "l"); // "line1"
        assert_eq!(grid.get(0, 1).unwrap().grapheme, "l"); // "line2"
        assert_eq!(grid.get(0, 2).unwrap().grapheme, "~"); // padding
    }

    #[test]
    fn test_paint_flex_column() {
        let state = default_state();
        let mut grid = CellGrid::new(20, 5);
        let el = Element::column(vec![
            FlexChild::fixed(Element::text("aaa", Face::default())),
            FlexChild::fixed(Element::text("bbb", Face::default())),
        ]);
        let area = root_area(20, 5);
        let layout = place(&el, area, &state);
        paint(&el, &layout, &mut grid, &state);

        assert_eq!(grid.get(0, 0).unwrap().grapheme, "a");
        assert_eq!(grid.get(0, 1).unwrap().grapheme, "b");
    }

    #[test]
    fn test_paint_container_border() {
        let state = default_state();
        let mut grid = CellGrid::new(20, 10);
        let el = Element::Container {
            child: Box::new(Element::text("hi", Face::default())),
            border: Some(BorderStyle::Rounded),
            shadow: false,
            padding: Edges::ZERO,
            style: Style::from(Face::default()),
        };
        let area = Rect {
            x: 0,
            y: 0,
            w: 6,
            h: 3,
        };
        let layout = place(&el, area, &state);
        paint(&el, &layout, &mut grid, &state);

        assert_eq!(grid.get(0, 0).unwrap().grapheme, "╭");
        assert_eq!(grid.get(5, 0).unwrap().grapheme, "╮");
        assert_eq!(grid.get(0, 2).unwrap().grapheme, "╰");
        assert_eq!(grid.get(5, 2).unwrap().grapheme, "╯");
        assert_eq!(grid.get(1, 0).unwrap().grapheme, "─");
        assert_eq!(grid.get(0, 1).unwrap().grapheme, "│");
    }

    #[test]
    fn test_paint_stack_overlays() {
        let state = default_state();
        let mut grid = CellGrid::new(20, 10);
        let el = Element::stack(
            Element::text("base_text", Face::default()),
            vec![Overlay {
                element: Element::text("pop", Face::default()),
                anchor: OverlayAnchor::Absolute { x: 5, y: 3, w: 3, h: 1 },
            }],
        );
        let area = root_area(20, 10);
        let layout = place(&el, area, &state);
        paint(&el, &layout, &mut grid, &state);

        // Base text at (0,0)
        assert_eq!(grid.get(0, 0).unwrap().grapheme, "b");
        // Overlay at (5,3)
        assert_eq!(grid.get(5, 3).unwrap().grapheme, "p");
        assert_eq!(grid.get(6, 3).unwrap().grapheme, "o");
        assert_eq!(grid.get(7, 3).unwrap().grapheme, "p");
    }
}
