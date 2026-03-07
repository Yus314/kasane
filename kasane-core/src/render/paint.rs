use unicode_width::UnicodeWidthStr;

use super::grid::CellGrid;
use super::theme::Theme;
use crate::element::{BorderConfig, BorderLineStyle, Element};
use crate::layout::Rect;
use crate::layout::flex::LayoutResult;
use crate::protocol::{Attributes, Color, Face};
use crate::state::AppState;

/// paint 再帰呼び出しで共有されるコンテキスト。
struct PaintContext<'a> {
    grid: &'a mut CellGrid,
    state: &'a AppState,
    theme: &'a Theme,
}

/// Paint an element tree into a CellGrid using pre-computed layout results.
pub fn paint(element: &Element, layout: &LayoutResult, grid: &mut CellGrid, state: &AppState) {
    crate::perf::perf_span!("paint");
    let theme = Theme::default_theme();
    let mut ctx = PaintContext {
        grid,
        state,
        theme: &theme,
    };
    paint_with_ctx(&mut ctx, element, layout);
}

/// Paint with an explicit theme for style resolution.
pub fn paint_themed(
    element: &Element,
    layout: &LayoutResult,
    grid: &mut CellGrid,
    state: &AppState,
    theme: &Theme,
) {
    let mut ctx = PaintContext { grid, state, theme };
    paint_with_ctx(&mut ctx, element, layout);
}

fn paint_with_ctx(ctx: &mut PaintContext, element: &Element, layout: &LayoutResult) {
    let area = layout.area;

    match element {
        Element::Text(text, style) => {
            let face = ctx.theme.resolve(style, &ctx.state.default_face);
            paint_text(ctx.grid, &area, text, &face);
        }
        Element::StyledLine(atoms) => {
            let line = atoms.to_vec();
            ctx.grid
                .put_line_with_base(area.y, area.x, &line, area.w, None);
        }
        Element::BufferRef { line_range } => {
            paint_buffer_ref(ctx.grid, &area, line_range.clone(), ctx.state);
        }
        Element::Empty => {}
        Element::Flex { children, .. } => {
            for (i, child) in children.iter().enumerate() {
                if let Some(child_layout) = layout.children.get(i) {
                    paint_with_ctx(ctx, &child.element, child_layout);
                }
            }
        }
        Element::Stack { base, overlays } => {
            if let Some(base_layout) = layout.children.first() {
                paint_with_ctx(ctx, base, base_layout);
            }
            for (i, overlay) in overlays.iter().enumerate() {
                if let Some(overlay_layout) = layout.children.get(i + 1) {
                    paint_with_ctx(ctx, &overlay.element, overlay_layout);
                }
            }
        }
        Element::Container {
            child,
            border,
            shadow,
            padding: _,
            style,
            title,
        } => {
            let face = ctx.theme.resolve(style, &ctx.state.default_face);
            paint_container(
                ctx,
                &area,
                child,
                border,
                *shadow,
                &face,
                title.as_deref(),
                layout,
            );
        }
        Element::Interactive { child, .. } => {
            if let Some(child_layout) = layout.children.first() {
                paint_with_ctx(ctx, child, child_layout);
            }
        }
        Element::Scrollable {
            child,
            offset: _,
            direction: _,
        } => {
            if let Some(child_layout) = layout.children.first() {
                paint_with_ctx(ctx, child, child_layout);
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
            grid.put_char(area.x, y, &state.padding_char, &state.padding_face);
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn paint_container(
    ctx: &mut PaintContext,
    area: &Rect,
    child: &Element,
    border: &Option<BorderConfig>,
    shadow: bool,
    face: &Face,
    title: Option<&[crate::protocol::Atom]>,
    layout: &LayoutResult,
) {
    // Shadow (drawn first, behind the container)
    if shadow {
        paint_shadow(ctx.grid, area);
    }

    // Fill entire container area with face
    for row in 0..area.h {
        let y = area.y + row;
        for x in area.x..(area.x + area.w).min(ctx.grid.width) {
            ctx.grid.put_char(x, y, " ", face);
        }
    }

    // Border
    if let Some(border_config) = border {
        let border_face = border_config
            .face
            .as_ref()
            .map(|s| ctx.theme.resolve(s, face))
            .unwrap_or(*face);
        paint_border(
            ctx.grid,
            area,
            &border_face,
            false,
            border_config.line_style,
        );
        // Title on top border
        if let Some(title_atoms) = title {
            paint_border_title(ctx.grid, area, &border_face, title_atoms);
        }
    }

    // Paint child
    if let Some(child_layout) = layout.children.first() {
        paint_with_ctx(ctx, child, child_layout);
    }
}

fn paint_border(
    grid: &mut CellGrid,
    area: &Rect,
    face: &Face,
    truncated: bool,
    border_style: BorderLineStyle,
) {
    if area.w < 2 || area.h < 2 {
        return;
    }

    // (top-left, top-right, bottom-left, bottom-right, horizontal, vertical)
    let (tl, tr, bl, br, horiz, vert) = match border_style {
        BorderLineStyle::Single => ("┌", "┐", "└", "┘", "─", "│"),
        BorderLineStyle::Rounded => ("╭", "╮", "╰", "╯", "─", "│"),
        BorderLineStyle::Double => ("╔", "╗", "╚", "╝", "═", "║"),
        BorderLineStyle::Heavy => ("┏", "┓", "┗", "┛", "━", "┃"),
        BorderLineStyle::Ascii => ("+", "+", "+", "+", "-", "|"),
    };

    let x1 = area.x;
    let y1 = area.y;
    let x2 = area.x + area.w - 1;
    let y2 = area.y + area.h - 1;
    let bottom_dash = if truncated {
        match border_style {
            BorderLineStyle::Double => "┄",
            BorderLineStyle::Heavy => "┅",
            BorderLineStyle::Ascii => ".",
            _ => "┄",
        }
    } else {
        horiz
    };

    // Corners
    grid.put_char(x1, y1, tl, face);
    grid.put_char(x2, y1, tr, face);
    grid.put_char(x1, y2, bl, face);
    grid.put_char(x2, y2, br, face);

    // Top and bottom edges
    for x in (x1 + 1)..x2 {
        grid.put_char(x, y1, horiz, face);
        grid.put_char(x, y2, bottom_dash, face);
    }

    // Left and right edges
    for y in (y1 + 1)..y2 {
        grid.put_char(x1, y, vert, face);
        grid.put_char(x2, y, vert, face);
    }
}

/// Paint title on the top border: ╭─┤title├─╮
fn paint_border_title(
    grid: &mut CellGrid,
    area: &Rect,
    face: &Face,
    title: &[crate::protocol::Atom],
) {
    use crate::layout::line_display_width;
    let title_vec = title.to_vec();
    let title_width = line_display_width(&title_vec);
    if title_width == 0 || area.w < 6 {
        return;
    }
    // Max title chars that fit: border_w - 2 corners - 2 min dashes - 2 delimiters (┤├)
    let max_title = ((area.w as usize).saturating_sub(6)).min(title_width) as u16;
    // Total dashes available on top border (excluding corners)
    let total_dashes = (area.w as usize).saturating_sub(2);
    // Dashes consumed by title + delimiters
    let title_slot = max_title as usize + 2; // ┤ + title + ├
    let dash_count = total_dashes.saturating_sub(title_slot);
    let left_dashes = dash_count / 2;
    // Position: corner(1) + left_dashes + ┤
    let tx = area.x + 1 + left_dashes as u16;
    grid.put_char(tx, area.y, "┤", face);
    grid.put_line_with_base(area.y, tx + 1, &title_vec, max_title, Some(face));
    let after = tx + 1 + max_title;
    if after < area.x + area.w - 1 {
        grid.put_char(after, area.y, "├", face);
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
    use crate::element::{
        BorderConfig, BorderLineStyle, Edges, Element, FlexChild, Overlay, OverlayAnchor, Style,
    };
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
            border: Some(BorderConfig::from(BorderLineStyle::Rounded)),
            shadow: false,
            padding: Edges::ZERO,
            style: Style::from(Face::default()),
            title: None,
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
    fn test_paint_buffer_ref_custom_padding_char() {
        let mut state = default_state();
        state.lines = vec![make_line("line1")];
        state.cols = 10;
        state.rows = 4;
        state.padding_char = "@".to_string();

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

        // Row 0 = line1, rows 1-2 = padding with "@"
        assert_eq!(grid.get(0, 0).unwrap().grapheme, "l");
        assert_eq!(grid.get(0, 1).unwrap().grapheme, "@");
        assert_eq!(grid.get(0, 2).unwrap().grapheme, "@");
    }

    #[test]
    fn test_paint_container_border_title() {
        let state = default_state();
        let mut grid = CellGrid::new(20, 10);
        let el = Element::Container {
            child: Box::new(Element::text("content", Face::default())),
            border: Some(BorderConfig::from(BorderLineStyle::Rounded)),
            shadow: false,
            padding: Edges::ZERO,
            style: Style::from(Face::default()),
            title: Some(make_line("Hi")),
        };
        let area = Rect {
            x: 0,
            y: 0,
            w: 12,
            h: 3,
        };
        let layout = place(&el, area, &state);
        paint(&el, &layout, &mut grid, &state);

        // Top border: ╭───┤Hi├───╮  (title centered)
        // w=12, total_dashes=10, title_slot=4(┤Hi├), dash_count=6, left=3, right=3
        assert_eq!(grid.get(0, 0).unwrap().grapheme, "╭");
        assert_eq!(grid.get(1, 0).unwrap().grapheme, "─");
        assert_eq!(grid.get(2, 0).unwrap().grapheme, "─");
        assert_eq!(grid.get(3, 0).unwrap().grapheme, "─");
        assert_eq!(grid.get(4, 0).unwrap().grapheme, "┤");
        assert_eq!(grid.get(5, 0).unwrap().grapheme, "H");
        assert_eq!(grid.get(6, 0).unwrap().grapheme, "i");
        assert_eq!(grid.get(7, 0).unwrap().grapheme, "├");
        assert_eq!(grid.get(8, 0).unwrap().grapheme, "─");
        assert_eq!(grid.get(9, 0).unwrap().grapheme, "─");
        assert_eq!(grid.get(10, 0).unwrap().grapheme, "─");
        assert_eq!(grid.get(11, 0).unwrap().grapheme, "╮");
    }

    #[test]
    fn test_paint_stack_overlays() {
        let state = default_state();
        let mut grid = CellGrid::new(20, 10);
        let el = Element::stack(
            Element::text("base_text", Face::default()),
            vec![Overlay {
                element: Element::text("pop", Face::default()),
                anchor: OverlayAnchor::Absolute {
                    x: 5,
                    y: 3,
                    w: 3,
                    h: 1,
                },
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
