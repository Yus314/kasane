use crate::state::{AppState, MenuState};
use super::grid::CellGrid;

/// Maximum number of rows for prompt-style menus (matches Kakoune's ncurses UI).
const PROMPT_MENU_MAX_ROWS: usize = 10;

pub(super) fn render_menu(state: &AppState, grid: &mut CellGrid) {
    let menu = match &state.menu {
        Some(m) => m,
        None => return,
    };

    if menu.style.is_prompt_like() {
        render_menu_prompt(menu, grid);
    } else {
        render_menu_inline(menu, grid);
    }
}

/// Compute the prompt menu layout parameters without rendering.
/// Returns (num_rows, num_cols, col_w) or None if menu is empty.
fn prompt_menu_geometry(
    menu: &MenuState,
    screen_w: u16,
    screen_h: u16,
) -> Option<(usize, usize, usize)> {
    if menu.items.is_empty() {
        return None;
    }
    let max_item_w = menu
        .items
        .iter()
        .map(super::line_display_width)
        .max()
        .unwrap_or(1)
        .max(1);
    let col_w = max_item_w + 1;
    let num_cols = (screen_w as usize / col_w).max(1);
    let total_rows = menu.items.len().div_ceil(num_cols);
    let num_rows = total_rows.min(PROMPT_MENU_MAX_ROWS).min(screen_h as usize);
    if num_rows == 0 {
        return None;
    }
    Some((num_rows, num_cols, col_w))
}

/// Compute the screen rectangle of the active menu, for use in info popup placement.
/// Returns `None` when there is no active menu (or it has zero size).
pub(super) fn get_menu_rect(state: &AppState) -> Option<crate::layout::Rect> {
    use crate::layout::{Rect, layout_menu};

    let menu = state.menu.as_ref()?;
    if menu.items.is_empty() {
        return None;
    }

    if menu.style.is_prompt_like() {
        let status_row = state.rows.saturating_sub(1);
        let (num_rows, _num_cols, _col_w) = prompt_menu_geometry(menu, state.cols, status_row)?;
        let start_y = status_row.saturating_sub(num_rows as u16);
        Some(Rect {
            x: 0,
            y: start_y,
            w: state.cols,
            h: num_rows as u16,
        })
    } else {
        let screen_h = state.rows.saturating_sub(1);
        let win = layout_menu(&menu.anchor, &menu.items, menu.style, state.cols, screen_h);
        if win.width == 0 || win.height == 0 {
            return None;
        }
        Some(Rect {
            x: win.x,
            y: win.y,
            w: win.width,
            h: win.height,
        })
    }
}

/// Render a prompt-style menu: horizontal multi-column layout above the status bar.
/// Items are arranged in column-major order with paging (like Kakoune's ncurses UI).
fn render_menu_prompt(menu: &MenuState, grid: &mut CellGrid) {
    let status_row = grid.height.saturating_sub(1);
    let (num_rows, num_cols, col_w) = match prompt_menu_geometry(menu, grid.width, status_row) {
        Some(g) => g,
        None => return,
    };

    // Page-based scrolling: each page holds num_rows * num_cols items
    let page_size = num_rows * num_cols;
    let selected = menu.selected.max(0) as usize;
    let page = selected / page_size;
    let item_offset = page * page_size;

    // Menu rows go from (status_row - num_rows) to (status_row - 1)
    let start_y = status_row.saturating_sub(num_rows as u16);

    // Fill menu area with menu_face
    for y in start_y..status_row {
        grid.fill_row(y, &menu.menu_face);
    }

    // Draw items in column-major order within the current page
    let page_items = menu.items.len().saturating_sub(item_offset).min(page_size);
    for i in 0..page_items {
        let item_idx = item_offset + i;
        let col = i / num_rows;
        let row = i % num_rows;

        let x = (col * col_w) as u16;
        let y = start_y + row as u16;

        if x >= grid.width || y >= status_row {
            break;
        }

        let is_selected = item_idx as i32 == menu.selected;
        let face = if is_selected {
            &menu.selected_item_face
        } else {
            &menu.menu_face
        };

        // Fill column width with face
        let fill_end = (x + col_w as u16).min(grid.width);
        for fx in x..fill_end {
            grid.put_char(fx, y, " ", face);
        }

        // Draw item text with face resolution
        let avail = (grid.width - x).min(col_w as u16);
        grid.put_line_with_base(y, x, &menu.items[item_idx], avail, Some(face));
    }
}

/// Render an inline-style menu: vertical floating window with borders.
fn render_menu_inline(menu: &MenuState, grid: &mut CellGrid) {
    use crate::layout::layout_menu;

    let win = layout_menu(
        &menu.anchor,
        &menu.items,
        menu.style,
        grid.width,
        grid.height.saturating_sub(1), // don't overlap status bar
    );

    super::draw_shadow(grid, &win);
    super::draw_border(grid, &win, &menu.menu_face, false, ("┌", "┐", "└", "┘"));

    // Draw menu items inside the border
    let inner_x = win.x + 1;
    let inner_y = win.y + 1;
    let inner_w = win.width.saturating_sub(2);
    let inner_h = win.height.saturating_sub(2);

    for i in 0..inner_h {
        let item_idx = i as usize;
        let y = inner_y + i;

        if let Some(item) = menu.items.get(item_idx) {
            let face = if item_idx as i32 == menu.selected {
                &menu.selected_item_face
            } else {
                &menu.menu_face
            };
            // Fill the line with menu face first
            for x in inner_x..inner_x + inner_w {
                grid.put_char(x, y, " ", face);
            }
            grid.put_line_with_base(y, inner_x, item, inner_w, Some(face));
        } else {
            for x in inner_x..inner_x + inner_w {
                grid.put_char(x, y, " ", &menu.menu_face);
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::protocol::{Atom, Color, Face, Line, MenuStyle, NamedColor};

    fn make_line(text: &str) -> Line {
        vec![Atom {
            face: Face::default(),
            contents: text.to_string(),
        }]
    }

    #[test]
    fn test_render_menu_prompt_horizontal() {
        // 40 cols wide, 10 rows. Items: "abc", "defgh", "ij" (max width = 5)
        let mut grid = CellGrid::new(40, 10);
        let menu_face = Face {
            fg: Color::Named(NamedColor::Blue),
            bg: Color::Named(NamedColor::White),
            ..Face::default()
        };
        let selected_face = Face {
            fg: Color::Named(NamedColor::White),
            bg: Color::Named(NamedColor::Blue),
            ..Face::default()
        };
        let state = AppState {
            menu: Some(MenuState {
                items: vec![make_line("abc"), make_line("defgh"), make_line("ij")],
                anchor: crate::protocol::Coord { line: 0, column: 0 },
                selected_item_face: selected_face.clone(),
                menu_face: menu_face.clone(),
                style: MenuStyle::Prompt,
                selected: 1,
            }),
            ..AppState::default()
        };

        render_menu(&state, &mut grid);

        // col_w = 5 + 1 = 6, num_cols = 40/6 = 6, num_rows = min(ceil(3/6), 10) = 1
        // All 3 items on 1 row, at y = 10 - 1 (status) - 1 (menu row) = 8
        let status_row = 9u16; // last row
        let menu_row = status_row - 1; // row 8

        // First item "abc" at x=0
        assert_eq!(grid.get(0, menu_row).unwrap().grapheme, "a");

        // Second item "defgh" at x=6, should be selected
        assert_eq!(grid.get(6, menu_row).unwrap().grapheme, "d");
        assert_eq!(grid.get(6, menu_row).unwrap().face, selected_face);

        // Third item "ij" at x=12
        assert_eq!(grid.get(12, menu_row).unwrap().grapheme, "i");
    }

    #[test]
    fn test_render_menu_prompt_paging() {
        // 20 cols wide, 15 rows tall.
        // Items: "aa".."zz" (26 items), max width = 2, col_w = 3
        // num_cols = 20/3 = 6, max_rows = 10, page_size = 60
        // All 26 items fit in one page (ceil(26/6)=5 rows <= 10)
        let mut grid = CellGrid::new(20, 15);
        let items: Vec<Line> = (b'a'..=b'z')
            .map(|c| make_line(&format!("{}{}", c as char, c as char)))
            .collect();
        assert_eq!(items.len(), 26);

        let state = AppState {
            menu: Some(MenuState {
                items: items.clone(),
                anchor: crate::protocol::Coord { line: 0, column: 0 },
                selected_item_face: Face::default(),
                menu_face: Face::default(),
                style: MenuStyle::Prompt,
                selected: 0,
            }),
            ..AppState::default()
        };

        render_menu(&state, &mut grid);

        // num_rows = min(ceil(26/6), 10) = min(5, 10) = 5
        // Menu occupies rows 9..14 (status_row=14, start_y=14-5=9)
        let status_row = 14u16;
        let start_y = status_row - 5;

        // Column-major: item 0="aa" at (col=0,row=0), item 1="bb" at (col=0,row=1), ...
        // item 5="ff" at (col=1,row=0), etc.
        assert_eq!(grid.get(0, start_y).unwrap().grapheme, "a"); // "aa"
        assert_eq!(grid.get(0, start_y + 1).unwrap().grapheme, "b"); // "bb"
        assert_eq!(grid.get(3, start_y).unwrap().grapheme, "f"); // "ff" at col=1
    }

    #[test]
    fn test_render_menu_prompt_max_rows_capped() {
        // 10 cols wide, 20 rows tall
        // 200 items of width 3, col_w = 4
        // num_cols = 10/4 = 2, total_rows = ceil(200/2) = 100
        // But capped to PROMPT_MENU_MAX_ROWS = 10
        // page_size = 10 * 2 = 20
        let mut grid = CellGrid::new(10, 20);
        let items: Vec<Line> = (0..200).map(|i| make_line(&format!("{i:>3}"))).collect();

        // Select item 25 → page = 25/20 = 1, offset = 20
        let state = AppState {
            menu: Some(MenuState {
                items,
                anchor: crate::protocol::Coord { line: 0, column: 0 },
                selected_item_face: Face::default(),
                menu_face: Face::default(),
                style: MenuStyle::Prompt,
                selected: 25,
            }),
            ..AppState::default()
        };

        render_menu(&state, &mut grid);

        // status_row = 19, start_y = 19 - 10 = 9
        let start_y = 9u16;
        // Page 1: items 20..39
        // Item 20 at (col=0, row=0) = grid(0, 9)
        // " 20" → first char is space
        assert_eq!(grid.get(0, start_y).unwrap().grapheme, " ");
        assert_eq!(grid.get(1, start_y).unwrap().grapheme, "2");
        assert_eq!(grid.get(2, start_y).unwrap().grapheme, "0");
    }
}
