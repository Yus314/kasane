use unicode_segmentation::UnicodeSegmentation;
use unicode_width::UnicodeWidthStr;

use super::grid::CellGrid;
use super::theme::Theme;
use crate::element::{BorderLineStyle, Element};
use crate::layout::Rect;
use crate::layout::flex::LayoutResult;
use crate::protocol::{Attributes, Color, Face};
use crate::state::AppState;

/// Paint an element tree into a CellGrid using pre-computed layout results.
pub fn paint(element: &Element, layout: &LayoutResult, grid: &mut CellGrid, state: &AppState) {
    crate::perf::perf_span!("paint");
    let theme = Theme::default_theme();
    super::walk::walk_paint_grid(element, layout, grid, state, &theme);
}

/// Paint with an explicit theme for style resolution.
pub fn paint_themed(
    element: &Element,
    layout: &LayoutResult,
    grid: &mut CellGrid,
    state: &AppState,
    theme: &Theme,
) {
    super::walk::walk_paint_grid(element, layout, grid, state, theme);
}

pub(crate) fn paint_text(grid: &mut CellGrid, area: &Rect, text: &str, face: &Face) {
    let mut x = area.x;
    let limit = area.x + area.w;
    for grapheme in text.graphemes(true) {
        if x >= limit {
            break;
        }
        if grapheme.starts_with(|c: char| c.is_control()) {
            continue;
        }
        let w = UnicodeWidthStr::width(grapheme) as u16;
        if w == 0 {
            continue;
        }
        if x + w > limit {
            break;
        }
        grid.put_char(x, area.y, grapheme, face);
        x += w;
    }
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn paint_buffer_ref(
    grid: &mut CellGrid,
    area: &Rect,
    line_range: std::ops::Range<usize>,
    state: &AppState,
    buffer_state: Option<&crate::element::BufferRefState>,
    line_backgrounds: Option<&[Option<Face>]>,
    display_map: Option<&crate::display::DisplayMap>,
    inline_decorations: Option<&[Option<crate::render::InlineDecoration>]>,
) {
    let lines = buffer_state.map(|s| &s.lines).unwrap_or(&state.lines);
    let lines_dirty = buffer_state
        .map(|s| &s.lines_dirty)
        .unwrap_or(&state.lines_dirty);
    let default_face = buffer_state
        .map(|s| s.default_face)
        .unwrap_or(state.default_face);
    let padding_face = buffer_state
        .map(|s| s.padding_face)
        .unwrap_or(state.padding_face);
    let padding_char = buffer_state
        .map(|s| s.padding_char.as_str())
        .unwrap_or(&state.padding_char);

    let has_line_dirty = !lines_dirty.is_empty();
    for y_offset in 0..area.h {
        let display_line = line_range.start + y_offset as usize;
        let y = area.y + y_offset;

        // Resolve display line → buffer line via DisplayMap
        let (buffer_line_idx, synthetic) = if let Some(dm) = display_map {
            if let Some(entry) = dm.entry(display_line) {
                let buf_line = match &entry.source {
                    crate::display::SourceMapping::BufferLine(l) => Some(*l),
                    crate::display::SourceMapping::LineRange(r) => Some(r.start),
                    crate::display::SourceMapping::None => None,
                };
                (buf_line, entry.synthetic.as_ref())
            } else {
                // Beyond display map range — render padding, not a buffer line
                (None, None)
            }
        } else {
            (Some(display_line), None)
        };

        // Skip clean lines — grid retains valid content from previous frame.
        // Synthetic lines (virtual text, fold summaries) are always repainted:
        // lines_dirty tracks buffer lines only, so SourceMapping::None lines
        // would be incorrectly skipped after a grid.clear().
        if has_line_dirty && synthetic.is_none() {
            let is_dirty = if let Some(dm) = display_map {
                dm.is_display_line_dirty(display_line, lines_dirty)
            } else {
                lines_dirty.get(display_line).copied().unwrap_or(true)
            };
            if !is_dirty {
                continue;
            }
        }

        // Render synthetic content (fold summary, virtual text)
        if let Some(syn) = synthetic {
            grid.fill_region(y, area.x, area.w, &syn.face);
            let atom = crate::protocol::Atom {
                face: syn.face,
                contents: syn.text.clone().into(),
            };
            grid.put_line_with_base(y, area.x, &[atom], area.w, Some(&syn.face));
            continue;
        }

        let line_idx = match buffer_line_idx {
            Some(idx) => idx,
            None => continue,
        };

        if let Some(line) = lines.get(line_idx) {
            // Use plugin background override if available, otherwise default_face
            let base_face = line_backgrounds
                .and_then(|bgs| bgs.get(line_idx).copied().flatten())
                .unwrap_or(default_face);
            grid.fill_region(y, area.x, area.w, &base_face);
            // Apply inline decorations if present
            let decorated;
            let line_to_render: &[crate::protocol::Atom] = match inline_decorations
                .and_then(|ds| ds.get(line_idx))
                .and_then(|d| d.as_ref())
            {
                Some(deco) if !deco.is_empty() => {
                    decorated = crate::render::inline_decoration::apply_inline_ops(line, deco);
                    decorated.as_slice()
                }
                _ => line,
            };
            grid.put_line_with_base(y, area.x, line_to_render, area.w, Some(&base_face));
        } else {
            // Padding row
            grid.fill_region(y, area.x, area.w, &padding_face);
            let mut pad_face = padding_face;
            if pad_face.fg == pad_face.bg {
                pad_face.fg = default_face.fg;
            }
            grid.put_char(area.x, y, padding_char, &pad_face);
        }
    }
}

pub(crate) fn paint_border(
    grid: &mut CellGrid,
    area: &Rect,
    face: &Face,
    truncated: bool,
    border_style: BorderLineStyle,
) {
    if area.w < 2 || area.h < 2 {
        return;
    }

    // Custom border chars storage (must outlive the match)
    let custom_strs: [String; 6];
    // (top-left, top-right, bottom-left, bottom-right, horizontal, vertical)
    let (tl, tr, bl, br, horiz, vert): (&str, &str, &str, &str, &str, &str) = match border_style {
        BorderLineStyle::Single => ("┌", "┐", "└", "┘", "─", "│"),
        BorderLineStyle::Rounded => ("╭", "╮", "╰", "╯", "─", "│"),
        BorderLineStyle::Double => ("╔", "╗", "╚", "╝", "═", "║"),
        BorderLineStyle::Heavy => ("┏", "┓", "┗", "┛", "━", "┃"),
        BorderLineStyle::Ascii => ("+", "+", "+", "+", "-", "|"),
        BorderLineStyle::Custom(ref chars) => {
            // chars: [TL, T, TR, R, BR, B, BL, L, title-left, title-right, shadow]
            custom_strs = [
                chars[0].to_string(), // TL
                chars[2].to_string(), // TR
                chars[6].to_string(), // BL
                chars[4].to_string(), // BR
                chars[1].to_string(), // T (horizontal)
                chars[7].to_string(), // L (vertical)
            ];
            (
                custom_strs[0].as_str(),
                custom_strs[1].as_str(),
                custom_strs[2].as_str(),
                custom_strs[3].as_str(),
                custom_strs[4].as_str(),
                custom_strs[5].as_str(),
            )
        }
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
            BorderLineStyle::Custom(_) => horiz,
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
pub(crate) fn paint_border_title(
    grid: &mut CellGrid,
    area: &Rect,
    face: &Face,
    title: &[crate::protocol::Atom],
) {
    use crate::layout::line_display_width;
    let title_width = line_display_width(title);
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
    grid.put_line_with_base(area.y, tx + 1, title, max_title, Some(face));
    let after = tx + 1 + max_title;
    if after < area.x + area.w - 1 {
        grid.put_char(after, area.y, "├", face);
    }
}

pub(crate) fn paint_shadow(grid: &mut CellGrid, area: &Rect) {
    let dim_face = Face {
        fg: Color::Default,
        bg: Color::Default,
        underline: Color::Default,
        attributes: Attributes::DIM,
    };

    // Right shadow (1 cell wide)
    let sx = area.x + area.w;
    if sx < grid.width() {
        for y in (area.y + 1)..=(area.y + area.h) {
            if y < grid.height() {
                grid.put_char(sx, y, " ", &dim_face);
            }
        }
    }

    // Bottom shadow (1 cell tall)
    let sy = area.y + area.h;
    if sy < grid.height() {
        for x in (area.x + 1)..=(area.x + area.w) {
            if x < grid.width() {
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
    use crate::protocol::Face;
    use crate::test_utils::*;

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
    fn test_paint_grid() {
        let state = default_state();
        let mut grid = CellGrid::new(20, 5);
        let el = Element::Grid {
            columns: vec![
                crate::element::GridColumn::fixed(5),
                crate::element::GridColumn::fixed(5),
            ],
            children: vec![
                Element::text("hello", Face::default()),
                Element::text("world", Face::default()),
            ],
            col_gap: 0,
            row_gap: 0,
            align: crate::element::Align::Start,
            cross_align: crate::element::Align::Start,
        };
        let area = root_area(20, 5);
        let layout = place(&el, area, &state);
        paint(&el, &layout, &mut grid, &state);

        // "hello" at (0,0)
        assert_eq!(grid.get(0, 0).unwrap().grapheme, "h");
        assert_eq!(grid.get(4, 0).unwrap().grapheme, "o");
        // "world" at (5,0)
        assert_eq!(grid.get(5, 0).unwrap().grapheme, "w");
        assert_eq!(grid.get(9, 0).unwrap().grapheme, "d");
    }

    #[test]
    fn test_paint_container_over_wide_chars() {
        let state = default_state();
        let mut grid = CellGrid::new(20, 10);
        // Place wide chars in the grid before painting container
        let wide_face = Face::default();
        grid.put_char(1, 0, "漢", &wide_face);
        grid.put_char(3, 0, "字", &wide_face);
        grid.put_char(1, 1, "あ", &wide_face);

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

        // Container fill should have replaced wide chars with spaces
        assert_eq!(grid.get(0, 0).unwrap().grapheme, "╭");
        assert_eq!(grid.get(1, 0).unwrap().grapheme, "─");
        assert_eq!(grid.get(0, 1).unwrap().grapheme, "│");
        // Interior: child "hi" is painted, no leftover wide chars
        assert_eq!(grid.get(1, 1).unwrap().grapheme, "h");
        assert_eq!(grid.get(1, 1).unwrap().width, 1);
        assert_eq!(grid.get(2, 1).unwrap().grapheme, "i");
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
