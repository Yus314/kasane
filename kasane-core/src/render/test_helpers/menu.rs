use super::super::grid::CellGrid;
use crate::layout::line_display_width;
use crate::protocol::MenuStyle;
use crate::state::{AppState, MenuState};

pub(in crate::render) fn render_menu(state: &AppState, grid: &mut CellGrid) {
    let menu = match &state.observed.menu {
        Some(m) => m,
        None => return,
    };

    match menu.style {
        MenuStyle::Prompt => render_menu_prompt(menu, grid),
        MenuStyle::Search => render_menu_search(menu, grid),
        MenuStyle::Inline => render_menu_inline(menu, grid),
    }
}

// ---------------------------------------------------------------------------
// Scrollbar helper
// ---------------------------------------------------------------------------

/// Draw a vertical scrollbar in the rightmost column of a region.
/// Uses Kakoune's scrollbar calculation from terminal_ui.cc draw_menu.
fn draw_scrollbar(
    grid: &mut CellGrid,
    x: u16,
    y_start: u16,
    win_height: u16,
    menu: &MenuState,
    face: &crate::protocol::Face,
) {
    let wh = win_height as usize;
    let item_count = menu.items.len();
    let columns = menu.columns as usize;
    if wh == 0 || item_count == 0 {
        return;
    }

    // Kakoune terminal_ui.cc draw_menu scrollbar
    let menu_lines = item_count.div_ceil(columns);
    let mark_h = (wh * wh).div_ceil(menu_lines).min(wh);
    let menu_cols = item_count.div_ceil(wh);
    let first_col = menu.first_item / wh;
    let denom = menu_cols.saturating_sub(columns).max(1);
    let mark_y = ((wh - mark_h) * first_col / denom).min(wh - mark_h);

    for row in 0..wh {
        let ch = if row >= mark_y && row < mark_y + mark_h {
            "█"
        } else {
            "░"
        };
        grid.put_char(x, y_start + row as u16, ch, face);
    }
}

// ---------------------------------------------------------------------------
// Inline menu
// ---------------------------------------------------------------------------

/// Render an inline-style menu: vertical floating window without borders,
/// with a scrollbar on the right edge.
fn render_menu_inline(menu: &MenuState, grid: &mut CellGrid) {
    use crate::layout::{MenuPlacement, layout_menu_inline};

    if menu.items.is_empty() || menu.win_height == 0 {
        return;
    }

    // content width + 1 for scrollbar
    let win_w = (menu.max_item_width + 1).min(grid.width());
    let content_w = win_w.saturating_sub(1);
    let screen_h = grid.height().saturating_sub(1);

    let win = layout_menu_inline(
        &menu.anchor,
        win_w,
        menu.win_height,
        grid.width(),
        screen_h,
        MenuPlacement::Auto,
    );
    if win.width == 0 || win.height == 0 {
        return;
    }

    // Fill and draw items (row-based vertical scroll for inline)
    for line in 0..win.height {
        let item_idx = menu.first_item + line as usize;
        let y = win.y + line;

        let face = if item_idx < menu.items.len() && Some(item_idx) == menu.selected {
            &menu.selected_item_face.to_face()
        } else {
            &menu.menu_face.to_face()
        };

        // Fill row with face
        for x in win.x..win.x + content_w {
            grid.put_char(x, y, " ", face);
        }

        // Draw item text
        if item_idx < menu.items.len() {
            grid.put_line_with_base(y, win.x, &menu.items[item_idx], content_w, Some(face));
        }
    }

    // Scrollbar
    draw_scrollbar(
        grid,
        win.x + content_w,
        win.y,
        win.height,
        menu,
        &menu.menu_face.to_face(),
    );
}

// ---------------------------------------------------------------------------
// Prompt menu
// ---------------------------------------------------------------------------

/// Render a prompt-style menu: horizontal multi-column layout above the status bar.
/// Items are arranged in column-major order with column-based scrolling.
fn render_menu_prompt(menu: &MenuState, grid: &mut CellGrid) {
    if menu.items.is_empty() || menu.win_height == 0 || menu.columns == 0 {
        return;
    }

    let status_row = grid.height().saturating_sub(1);
    let wh = menu.win_height;
    let columns = menu.columns as usize;
    let stride = wh as usize;

    // -1 for scrollbar column
    let col_w = (grid.width().saturating_sub(1) as usize / columns).max(1);
    let first_col = menu.first_item / stride;

    // Menu rows go from (status_row - wh) to (status_row - 1)
    let start_y = status_row.saturating_sub(wh);

    // Fill menu area with menu_face
    for y in start_y..status_row {
        grid.fill_row(y, &menu.menu_face.to_face());
    }

    // Draw items in column-major order
    for col in 0..columns {
        for line in 0..wh as usize {
            let item_idx = (first_col + col) * stride + line;
            if item_idx >= menu.items.len() {
                continue;
            }

            let x = (col * col_w) as u16;
            let y = start_y + line as u16;

            if x >= grid.width().saturating_sub(1) || y >= status_row {
                break;
            }

            let is_selected = Some(item_idx) == menu.selected;
            let face = if is_selected {
                &menu.selected_item_face.to_face()
            } else {
                &menu.menu_face.to_face()
            };

            // Fill column width with face
            let fill_end = (x + col_w as u16).min(grid.width().saturating_sub(1));
            for fx in x..fill_end {
                grid.put_char(fx, y, " ", face);
            }

            // Draw item text
            let avail = fill_end.saturating_sub(x);
            grid.put_line_with_base(y, x, &menu.items[item_idx], avail, Some(face));
        }
    }

    // Scrollbar on rightmost column
    let scrollbar_x = grid.width().saturating_sub(1);
    draw_scrollbar(
        grid,
        scrollbar_x,
        start_y,
        wh,
        menu,
        &menu.menu_face.to_face(),
    );
}

// ---------------------------------------------------------------------------
// Search menu
// ---------------------------------------------------------------------------

/// Render a search-style menu: single line of horizontally laid out items
/// above the status bar, with `< ` / ` >` scroll indicators.
fn render_menu_search(menu: &MenuState, grid: &mut CellGrid) {
    if menu.items.is_empty() {
        return;
    }

    let status_row = grid.height().saturating_sub(1);
    let y = status_row.saturating_sub(1);
    let screen_w = grid.width() as usize;

    // Fill the row with menu_face
    grid.fill_row(y, &menu.menu_face.to_face());

    let first = menu.first_item;
    let has_prefix = first > 0;
    let mut x = 0usize;

    // Draw "< " prefix if scrolled
    if has_prefix {
        grid.put_char(x as u16, y, "<", &menu.menu_face.to_face());
        x += 1;
        grid.put_char(x as u16, y, " ", &menu.menu_face.to_face());
        x += 1;
    }

    // Draw items horizontally
    for idx in first..menu.items.len() {
        let item_w = line_display_width(&menu.items[idx]);

        // Check if we need space for " >" suffix
        let has_more = idx + 1 < menu.items.len();
        let suffix_reserve = if has_more { 2 } else { 0 };

        if x + item_w + suffix_reserve > screen_w && x > 0 {
            // Can't fit this item; draw ">" indicator
            if has_more && x < screen_w {
                // Pad remaining space, then draw ">"
                while x + 1 < screen_w {
                    grid.put_char(x as u16, y, " ", &menu.menu_face.to_face());
                    x += 1;
                }
                grid.put_char(x as u16, y, ">", &menu.menu_face.to_face());
            }
            break;
        }

        let is_selected = Some(idx) == menu.selected;
        let face = if is_selected {
            &menu.selected_item_face.to_face()
        } else {
            &menu.menu_face.to_face()
        };

        // Draw item
        let avail = (screen_w - x).min(item_w) as u16;
        grid.put_line_with_base(y, x as u16, &menu.items[idx], avail, Some(face));
        x += item_w;

        // Gap between items
        if x < screen_w {
            grid.put_char(x as u16, y, " ", &menu.menu_face.to_face());
            x += 1;
        }
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::layout::get_menu_rect;
    use crate::protocol::{Color, Coord, Face, Line, MenuStyle, NamedColor};
    use crate::test_utils::make_line;

    fn make_menu_state(
        items: Vec<Line>,
        style: MenuStyle,
        selected: Option<usize>,
        screen_w: u16,
        screen_h: u16,
    ) -> MenuState {
        make_menu_state_at(
            items,
            style,
            selected,
            Coord { line: 0, column: 0 },
            screen_w,
            screen_h,
        )
    }

    fn make_menu_state_at(
        items: Vec<Line>,
        style: MenuStyle,
        selected: Option<usize>,
        anchor: Coord,
        screen_w: u16,
        screen_h: u16,
    ) -> MenuState {
        use crate::state::MenuParams;
        let mut ms = MenuState::new(
            items,
            MenuParams {
                anchor,
                selected_item_face: crate::protocol::Style::default(),
                menu_face: crate::protocol::Style::default(),
                style,
                screen_w,
                screen_h,
                max_height: 10,
            },
        );
        ms.selected = selected;
        ms
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
        let items = vec![make_line("abc"), make_line("defgh"), make_line("ij")];
        // screen_h = 10 - 1 = 9 (excl status), longest = 5, col_w = (40-1)/(6) = 6
        // columns = (40-1)/6 = 6, win_height = min(ceil(3/6), 10) = 1
        let mut ms = make_menu_state(items, MenuStyle::Prompt, Some(1), 40, 9);
        ms.menu_face = menu_face.into();
        ms.selected_item_face = selected_face.into();
        let mut state = AppState::default();
        state.observed.menu = Some(ms);
        state.runtime.cols = 40;
        state.runtime.rows = 10;

        render_menu(&state, &mut grid);

        // All 3 items on 1 row, at y = 10 - 1 (status) - 1 (menu row) = 8
        let menu_row = 8u16;

        // First item "abc" at x=0
        assert_eq!(grid.get(0, menu_row).unwrap().grapheme, "a");

        // Second item "defgh" at x=6, should be selected
        assert_eq!(grid.get(6, menu_row).unwrap().grapheme, "d");
        assert_eq!(grid.get(6, menu_row).unwrap().face(), selected_face);

        // Third item "ij" at x=12
        assert_eq!(grid.get(12, menu_row).unwrap().grapheme, "i");
    }

    #[test]
    fn test_render_menu_prompt_column_scrolling() {
        // 20 cols wide, 15 rows tall.
        // Items: "aa".."zz" (26 items), max width = 2, col_w = 3
        // columns = (20-1)/3 = 6, win_height = min(ceil(26/6), 10) = 5
        let mut grid = CellGrid::new(20, 15);
        let items: Vec<Line> = (b'a'..=b'z')
            .map(|c| make_line(&format!("{}{}", c as char, c as char)))
            .collect();
        assert_eq!(items.len(), 26);

        let ms = make_menu_state(items, MenuStyle::Prompt, Some(0), 20, 14);
        let mut state = AppState::default();
        state.observed.menu = Some(ms);
        state.runtime.cols = 20;
        state.runtime.rows = 15;

        render_menu(&state, &mut grid);

        // status_row = 14, win_height = 5, start_y = 14 - 5 = 9
        let start_y = 9u16;

        // Column-major: item 0="aa" at (col=0,row=0), item 1="bb" at (col=0,row=1), ...
        // item 5="ff" at (col=1,row=0), etc.
        assert_eq!(grid.get(0, start_y).unwrap().grapheme, "a"); // "aa"
        assert_eq!(grid.get(0, start_y + 1).unwrap().grapheme, "b"); // "bb"
        // col_w = (20-1)/6 = 3
        assert_eq!(grid.get(3, start_y).unwrap().grapheme, "f"); // "ff" at col=1
    }

    #[test]
    fn test_render_menu_prompt_scrollbar() {
        // Verify scrollbar column is drawn on the rightmost column
        let mut grid = CellGrid::new(20, 15);
        let items: Vec<Line> = (0..50).map(|i| make_line(&format!("{i:>3}"))).collect();

        let ms = make_menu_state(items, MenuStyle::Prompt, Some(0), 20, 14);
        let mut state = AppState::default();
        state.observed.menu = Some(ms);
        state.runtime.cols = 20;
        state.runtime.rows = 15;

        render_menu(&state, &mut grid);

        // Rightmost column (19) should have scrollbar characters
        let status_row = 14u16;
        let start_y = status_row.saturating_sub(state.observed.menu.as_ref().unwrap().win_height);
        let scrollbar_cell = grid.get(19, start_y).unwrap();
        assert!(
            scrollbar_cell.grapheme == "█" || scrollbar_cell.grapheme == "░",
            "expected scrollbar char, got: {}",
            scrollbar_cell.grapheme
        );
    }

    #[test]
    fn test_render_menu_search_basic() {
        // 40 cols wide, 10 rows. 3 items displayed on one line.
        let mut grid = CellGrid::new(40, 10);
        let items = vec![make_line("abc"), make_line("def"), make_line("ghi")];
        let ms = make_menu_state(items, MenuStyle::Search, Some(1), 40, 9);
        let mut state = AppState::default();
        state.observed.menu = Some(ms);
        state.runtime.cols = 40;
        state.runtime.rows = 10;

        render_menu(&state, &mut grid);

        // Search draws on row = status_row - 1 = 8
        let y = 8u16;
        // first_item=0, no prefix
        assert_eq!(grid.get(0, y).unwrap().grapheme, "a"); // "abc"
        assert_eq!(grid.get(4, y).unwrap().grapheme, "d"); // "def" after "abc" + gap
    }

    #[test]
    fn test_render_menu_search_with_scroll() {
        // 20 cols wide, 5 rows. Items too wide to all fit.
        let mut grid = CellGrid::new(20, 5);
        let items = vec![
            make_line("alpha"),
            make_line("bravo"),
            make_line("charlie"),
            make_line("delta"),
        ];
        let mut ms = make_menu_state(items, MenuStyle::Search, Some(2), 20, 4);
        ms.first_item = 1; // scrolled past first item
        let mut state = AppState::default();
        state.observed.menu = Some(ms);
        state.runtime.cols = 20;
        state.runtime.rows = 5;

        render_menu(&state, &mut grid);

        // Should show "< " prefix since first_item > 0
        let y = 3u16; // status_row(4) - 1
        assert_eq!(grid.get(0, y).unwrap().grapheme, "<");
        assert_eq!(grid.get(1, y).unwrap().grapheme, " ");
        // "bravo" starts at x=2
        assert_eq!(grid.get(2, y).unwrap().grapheme, "b");
    }

    #[test]
    fn test_render_menu_inline_no_border() {
        // Verify inline menu has no border/shadow
        let mut grid = CellGrid::new(40, 20);
        let items = vec![make_line("item1"), make_line("item2")];
        let ms = make_menu_state_at(
            items,
            MenuStyle::Inline,
            Some(0),
            Coord { line: 5, column: 5 },
            40,
            19,
        );
        let mut state = AppState::default();
        state.observed.menu = Some(ms);
        state.runtime.cols = 40;
        state.runtime.rows = 20;

        render_menu(&state, &mut grid);

        // Items should start directly at the win position, no border offset
        // anchor.line=5, so menu at y=6
        assert_eq!(grid.get(5, 6).unwrap().grapheme, "i"); // "item1"
        assert_eq!(grid.get(5, 7).unwrap().grapheme, "i"); // "item2"
    }

    #[test]
    fn test_render_menu_inline_scrollbar() {
        // Inline menu with enough items to need scrollbar
        let mut grid = CellGrid::new(40, 20);
        let items: Vec<Line> = (0..20).map(|i| make_line(&format!("item{i:>2}"))).collect();
        let ms = make_menu_state_at(
            items,
            MenuStyle::Inline,
            Some(0),
            Coord { line: 2, column: 0 },
            40,
            19,
        );
        let mut state = AppState::default();
        state.observed.menu = Some(ms);
        state.runtime.cols = 40;
        state.runtime.rows = 20;

        render_menu(&state, &mut grid);

        // Scrollbar should be at x = content_width (longest=6) → scrollbar at x=6
        // anchor at line 2, menu starts at y=3
        let scrollbar_cell = grid.get(6, 3).unwrap();
        assert!(
            scrollbar_cell.grapheme == "█" || scrollbar_cell.grapheme == "░",
            "expected scrollbar char at scrollbar column, got: {}",
            scrollbar_cell.grapheme
        );
    }

    #[test]
    fn test_get_menu_rect_prompt() {
        let items = vec![make_line("a"), make_line("b"), make_line("c")];
        let ms = make_menu_state(items, MenuStyle::Prompt, Some(0), 80, 23);
        let mut state = AppState::default();
        state.observed.menu = Some(ms);
        state.runtime.cols = 80;
        state.runtime.rows = 24;
        let rect = get_menu_rect(&state).unwrap();
        assert_eq!(rect.w, 80);
        assert!(rect.h > 0);
        assert!(rect.y + rect.h <= 23);
    }

    #[test]
    fn test_get_menu_rect_search() {
        let items = vec![make_line("a"), make_line("b")];
        let ms = make_menu_state(items, MenuStyle::Search, Some(0), 80, 23);
        let mut state = AppState::default();
        state.observed.menu = Some(ms);
        state.runtime.cols = 80;
        state.runtime.rows = 24;
        let rect = get_menu_rect(&state).unwrap();
        assert_eq!(rect.h, 1);
        assert_eq!(rect.w, 80);
    }

    #[test]
    fn test_render_menu_prompt_stride_uses_win_height() {
        // 50 items of width 3, screen_w=14 → columns = (14-1)/(3+1) = 3
        // menu_lines = ceil(50/3) = 17, win_height = min(17, 10) = 10
        // Stride = win_height = 10 (matching Kakoune terminal_ui.cc)
        // Column 0 holds items 0..9, column 1 holds items 10..19, etc.
        let mut grid = CellGrid::new(14, 15);
        let items: Vec<Line> = (0..50).map(|i| make_line(&format!("{i:>3}"))).collect();

        let ms = make_menu_state(items, MenuStyle::Prompt, None, 14, 14);
        // Verify the computed values
        assert_eq!(ms.columns, 3);
        assert_eq!(ms.menu_lines, 17);
        assert_eq!(ms.win_height, 10);

        let mut state = AppState::default();
        state.observed.menu = Some(ms);
        state.runtime.cols = 14;
        state.runtime.rows = 15;

        render_menu(&state, &mut grid);

        // status_row = 14, win_height = 10, start_y = 4
        let start_y = 4u16;
        // col_w = (14-1)/3 = 4

        // Column 0, row 0 → item 0 = "  0"
        assert_eq!(grid.get(2, start_y).unwrap().grapheme, "0");
        // Column 1, row 0 → item 10 = " 10" (stride = win_height = 10)
        assert_eq!(grid.get(5, start_y).unwrap().grapheme, "1");
        assert_eq!(grid.get(6, start_y).unwrap().grapheme, "0");
        // Column 2, row 0 → item 20 = " 20"
        assert_eq!(grid.get(9, start_y).unwrap().grapheme, "2");
        assert_eq!(grid.get(10, start_y).unwrap().grapheme, "0");
    }
}
