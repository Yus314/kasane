use unicode_width::UnicodeWidthStr;

use crate::protocol::{Attribute, Color, CursorMode, Face, InfoStyle, Line};

// ---------------------------------------------------------------------------
// Cell + CellGrid
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Cell {
    pub grapheme: String,
    pub face: Face,
    /// Display width: 1 for normal, 2 for wide chars, 0 for continuation cells.
    pub width: u8,
}

impl Default for Cell {
    fn default() -> Self {
        Cell {
            grapheme: " ".to_string(),
            face: Face::default(),
            width: 1,
        }
    }
}

pub struct CellGrid {
    pub width: u16,
    pub height: u16,
    current: Vec<Cell>,
    previous: Vec<Cell>,
}

impl CellGrid {
    pub fn new(width: u16, height: u16) -> Self {
        let size = width as usize * height as usize;
        CellGrid {
            width,
            height,
            current: vec![Cell::default(); size],
            previous: Vec::new(), // empty means "invalidated — full redraw needed"
        }
    }

    pub fn resize(&mut self, width: u16, height: u16) {
        self.width = width;
        self.height = height;
        let size = width as usize * height as usize;
        self.current = vec![Cell::default(); size];
        self.previous = Vec::new();
    }

    fn idx(&self, x: u16, y: u16) -> usize {
        y as usize * self.width as usize + x as usize
    }

    pub fn put_char(&mut self, x: u16, y: u16, grapheme: &str, face: &Face) {
        if x >= self.width || y >= self.height {
            return;
        }
        let w = UnicodeWidthStr::width(grapheme) as u8;
        let idx = self.idx(x, y);

        // --- Clean up orphaned wide-character halves before overwriting ---

        // If overwriting a continuation cell (width 0), the wide char at x-1 is orphaned.
        if self.current[idx].width == 0 && x > 0 {
            let prev_idx = self.idx(x - 1, y);
            self.current[prev_idx].grapheme = " ".to_string();
            self.current[prev_idx].width = 1;
        }

        // If overwriting a wide char (width 2), its continuation at x+1 is orphaned.
        if self.current[idx].width == 2 && x + 1 < self.width {
            let next_idx = self.idx(x + 1, y);
            self.current[next_idx].grapheme = " ".to_string();
            self.current[next_idx].width = 1;
        }

        // If placing a wide char, x+1 will become our continuation.
        // If x+1 is currently a wide char, its continuation at x+2 is orphaned.
        if w == 2 && x + 1 < self.width {
            let next_idx = self.idx(x + 1, y);
            if self.current[next_idx].width == 2 && x + 2 < self.width {
                let next2_idx = self.idx(x + 2, y);
                self.current[next2_idx].grapheme = " ".to_string();
                self.current[next2_idx].width = 1;
            }
        }

        // --- Write the new cell ---

        self.current[idx] = Cell {
            grapheme: grapheme.to_string(),
            face: face.clone(),
            width: w,
        };
        // If wide character, mark next cell as continuation
        if w == 2 && x + 1 < self.width {
            let next_idx = self.idx(x + 1, y);
            self.current[next_idx] = Cell {
                grapheme: String::new(),
                face: face.clone(),
                width: 0,
            };
        }
    }

    /// Write a protocol Line into the grid at row `y` starting at column `x_start`.
    /// Returns the number of columns consumed.
    pub fn put_line(&mut self, y: u16, x_start: u16, line: &Line, max_width: u16) -> u16 {
        self.put_line_with_base(y, x_start, line, max_width, None)
    }

    /// Write a protocol Line, resolving `Color::Default` against `base_face`.
    /// When base_face is Some, atom Default fg/bg inherit from it (Kakoune semantics).
    pub fn put_line_with_base(
        &mut self,
        y: u16,
        x_start: u16,
        line: &Line,
        max_width: u16,
        base_face: Option<&Face>,
    ) -> u16 {
        let mut x = x_start;
        let limit = x_start.saturating_add(max_width).min(self.width);

        for atom in line {
            let face = match base_face {
                Some(base) => resolve_face(&atom.face, base),
                None => atom.face.clone(),
            };
            for grapheme in atom.contents.split_inclusive(|_: char| true) {
                if grapheme.is_empty() {
                    continue;
                }
                let w = UnicodeWidthStr::width(grapheme) as u16;
                if w == 0 {
                    // Zero-width character — skip for now
                    continue;
                }
                if x + w > limit {
                    break;
                }
                self.put_char(x, y, grapheme, &face);
                x += w;
            }
        }

        x - x_start
    }

    pub fn clear(&mut self, face: &Face) {
        for cell in &mut self.current {
            cell.grapheme = " ".to_string();
            cell.face = face.clone();
            cell.width = 1;
        }
    }

    pub fn fill_row(&mut self, y: u16, face: &Face) {
        if y >= self.height {
            return;
        }
        for x in 0..self.width {
            let idx = self.idx(x, y);
            self.current[idx] = Cell {
                grapheme: " ".to_string(),
                face: face.clone(),
                width: 1,
            };
        }
    }

    pub fn diff(&self) -> Vec<CellDiff> {
        if self.previous.is_empty() {
            // Full redraw
            return self
                .current
                .iter()
                .enumerate()
                .filter(|(_, c)| c.width > 0) // skip continuation cells
                .map(|(i, cell)| {
                    let x = (i % self.width as usize) as u16;
                    let y = (i / self.width as usize) as u16;
                    CellDiff {
                        x,
                        y,
                        cell: cell.clone(),
                    }
                })
                .collect();
        }

        let mut diffs = Vec::new();
        for (i, (curr, prev)) in self.current.iter().zip(self.previous.iter()).enumerate() {
            if curr != prev && curr.width > 0 {
                let x = (i % self.width as usize) as u16;
                let y = (i / self.width as usize) as u16;
                diffs.push(CellDiff {
                    x,
                    y,
                    cell: curr.clone(),
                });
            }
        }
        diffs
    }

    pub fn swap(&mut self) {
        std::mem::swap(&mut self.previous, &mut self.current);
        // Reset current to blank
        let size = self.width as usize * self.height as usize;
        self.current.clear();
        self.current.resize(size, Cell::default());
    }

    pub fn invalidate_all(&mut self) {
        self.previous.clear();
    }

    /// Direct access to a cell in the current buffer.
    pub fn get(&self, x: u16, y: u16) -> Option<&Cell> {
        if x < self.width && y < self.height {
            Some(&self.current[self.idx(x, y)])
        } else {
            None
        }
    }
}

// ---------------------------------------------------------------------------
// CellDiff
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
pub struct CellDiff {
    pub x: u16,
    pub y: u16,
    pub cell: Cell,
}

// ---------------------------------------------------------------------------
// RenderBackend trait
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CursorStyle {
    Block,
    Bar,
    Underline,
}

pub trait RenderBackend {
    fn size(&self) -> (u16, u16);
    fn begin_frame(&mut self) -> anyhow::Result<()> { Ok(()) }
    fn end_frame(&mut self) -> anyhow::Result<()> { Ok(()) }
    fn draw(&mut self, diffs: &[CellDiff]) -> anyhow::Result<()>;
    fn flush(&mut self) -> anyhow::Result<()>;
    fn show_cursor(&mut self, x: u16, y: u16, style: CursorStyle) -> anyhow::Result<()>;
    fn hide_cursor(&mut self) -> anyhow::Result<()>;
}

// ---------------------------------------------------------------------------
// Buffer / Status / Cursor rendering
// ---------------------------------------------------------------------------

use crate::state::AppState;

/// Render the main buffer area (all lines except the last row which is status).
pub fn render_buffer(state: &AppState, grid: &mut CellGrid) {
    let buffer_rows = grid.height.saturating_sub(1);

    for y in 0..buffer_rows {
        if let Some(line) = state.lines.get(y as usize) {
            grid.fill_row(y, &state.default_face);
            grid.put_line_with_base(y, 0, line, grid.width, Some(&state.default_face));
        } else {
            // Padding row
            grid.fill_row(y, &state.padding_face);
            // Show tilde for padding like Kakoune
            grid.put_char(0, y, "~", &state.padding_face);
        }
    }
}

/// Render the status bar at the bottom row.
pub fn render_status(state: &AppState, grid: &mut CellGrid) {
    let y = grid.height.saturating_sub(1);
    grid.fill_row(y, &state.status_default_face);

    // Status line on the left
    grid.put_line_with_base(
        y,
        0,
        &state.status_line,
        grid.width,
        Some(&state.status_default_face),
    );

    // Mode line on the right
    let mode_width = line_display_width(&state.status_mode_line);
    if mode_width > 0 && grid.width as usize > mode_width {
        let mode_x = grid.width - mode_width as u16;
        grid.put_line_with_base(
            y,
            mode_x,
            &state.status_mode_line,
            mode_width as u16,
            Some(&state.status_default_face),
        );
    }
}

/// Compute the terminal cursor position from the application state.
/// Returns (x, y) coordinates for the terminal cursor.
pub fn cursor_position(state: &AppState, grid: &CellGrid) -> (u16, u16) {
    let cx = state.cursor_pos.column as u16;
    let cy = match state.cursor_mode {
        CursorMode::Prompt => grid.height.saturating_sub(1),
        CursorMode::Buffer => state.cursor_pos.line as u16,
    };
    (cx, cy)
}

/// Resolve Default colors in an atom face against a base face.
/// In Kakoune, `default` means "inherit from the containing context".
fn resolve_face(atom_face: &Face, base: &Face) -> Face {
    let has_final_attr = atom_face.attributes.contains(&Attribute::FinalAttr);
    Face {
        fg: if atom_face.fg == Color::Default {
            base.fg
        } else {
            atom_face.fg
        },
        bg: if atom_face.bg == Color::Default {
            base.bg
        } else {
            atom_face.bg
        },
        underline: if atom_face.underline == Color::Default {
            base.underline
        } else {
            atom_face.underline
        },
        attributes: if has_final_attr || base.attributes.is_empty() {
            atom_face.attributes.clone()
        } else {
            let mut attrs = base.attributes.clone();
            for attr in &atom_face.attributes {
                if !attrs.contains(attr) {
                    attrs.push(*attr);
                }
            }
            attrs
        },
    }
}

fn line_display_width(line: &Line) -> usize {
    line.iter()
        .map(|atom| UnicodeWidthStr::width(atom.contents.as_str()))
        .sum()
}

// ---------------------------------------------------------------------------
// Full frame rendering (Z-order)
// ---------------------------------------------------------------------------

pub fn render_frame(state: &AppState, grid: &mut CellGrid) {
    grid.clear(&state.default_face);
    render_buffer(state, grid); // Layer 0
    render_status(state, grid); // Layer 1
    render_menu(state, grid); // Layer 2 (+ shadow)
    render_info(state, grid); // Layer 3 (+ shadow)
    // Cursor face is already applied by Kakoune in draw data.
    // Terminal cursor positioning is handled separately via backend.show_cursor().
}

pub fn render_menu(state: &AppState, grid: &mut CellGrid) {
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

/// Maximum number of rows for prompt-style menus (matches Kakoune's ncurses UI).
const PROMPT_MENU_MAX_ROWS: usize = 10;

/// Compute the prompt menu layout parameters without rendering.
/// Returns (num_rows, num_cols, col_w) or None if menu is empty.
fn prompt_menu_geometry(
    menu: &crate::state::MenuState,
    screen_w: u16,
    screen_h: u16,
) -> Option<(usize, usize, usize)> {
    if menu.items.is_empty() {
        return None;
    }
    let max_item_w = menu
        .items
        .iter()
        .map(line_display_width)
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
fn get_menu_rect(state: &AppState) -> Option<crate::layout::Rect> {
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
fn render_menu_prompt(menu: &crate::state::MenuState, grid: &mut CellGrid) {
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
fn render_menu_inline(menu: &crate::state::MenuState, grid: &mut CellGrid) {
    use crate::layout::layout_menu;

    let win = layout_menu(
        &menu.anchor,
        &menu.items,
        menu.style,
        grid.width,
        grid.height.saturating_sub(1), // don't overlap status bar
    );

    draw_shadow(grid, &win);
    draw_border(grid, &win, &menu.menu_face, false, ("┌", "┐", "└", "┘"));

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
// Kakoune assistant (clippy)
// ---------------------------------------------------------------------------

/// The clippy assistant from Kakoune's terminal UI.
/// Each line is exactly 8 display columns wide.
const ASSISTANT_CLIPPY: &[&str] = &[
    " ╭──╮  ",
    " │  │  ",
    " @  @  ╭",
    " ││ ││ │",
    " ││ ││ ╯",
    " │╰─╯│ ",
    " ╰───╯ ",
    "        ",
];
const ASSISTANT_WIDTH: u16 = 8;

pub fn render_info(state: &AppState, grid: &mut CellGrid) {
    let info = match &state.info {
        Some(i) => i,
        None => return,
    };

    let menu_rect = get_menu_rect(state);
    let win = crate::layout::layout_info(
        &info.title,
        &info.content,
        &info.anchor,
        info.style,
        grid.width,
        grid.height.saturating_sub(1),
        menu_rect,
    );

    match info.style {
        InfoStyle::Prompt => render_info_prompt(info, grid, &win),
        InfoStyle::Modal => render_info_framed(info, grid, &win),
        InfoStyle::Inline | InfoStyle::InlineAbove | InfoStyle::MenuDoc => {
            render_info_nonframed(info, grid, &win)
        }
    }
}

/// Render prompt-style info with the clippy assistant (matches Kakoune's terminal UI).
fn render_info_prompt(
    info: &crate::state::InfoState,
    grid: &mut CellGrid,
    win: &crate::layout::FloatingWindow,
) {
    use crate::layout::word_wrap_line_height;

    if win.width < ASSISTANT_WIDTH + 5 || win.height < 3 {
        return;
    }

    let y_start = win.y;
    let total_h = win.height as usize;
    let frame_x = win.x + ASSISTANT_WIDTH;
    let cw = win.width.saturating_sub(ASSISTANT_WIDTH + 4);
    if cw == 0 {
        return;
    }

    // Trim trailing empty content lines (Kakoune doesn't render them)
    let content_end = info
        .content
        .iter()
        .rposition(|line| line_display_width(line) > 0)
        .map(|i| i + 1)
        .unwrap_or(0);
    let trimmed = &info.content[..content_end];

    // Wrapped height for truncation detection
    let wrapped_h: usize = trimmed
        .iter()
        .map(|line| word_wrap_line_height(line, cw) as usize)
        .sum::<usize>()
        .max(1);

    // Fill entire popup area with info face
    for row in 0..total_h as u16 {
        let y = y_start + row;
        for x in win.x..win.x + win.width {
            grid.put_char(x, y, " ", &info.face);
        }
    }

    // Draw assistant (vertically centered)
    let asst_top = ((total_h as i32 - ASSISTANT_CLIPPY.len() as i32 + 1) / 2).max(0) as usize;
    for row in 0..total_h {
        let y = y_start + row as u16;
        let idx = if row >= asst_top {
            (row - asst_top).min(ASSISTANT_CLIPPY.len() - 1)
        } else {
            ASSISTANT_CLIPPY.len() - 1 // padding (empty line)
        };
        for (i, ch) in (0u16..).zip(ASSISTANT_CLIPPY[idx].chars()) {
            let x = win.x + i;
            if x < grid.width {
                let s: String = ch.into();
                grid.put_char(x, y, &s, &info.face);
            }
        }
    }

    // --- Top border: ╭─left─┤title├─right─╮ ---
    {
        let title_w = line_display_width(&info.title);
        let y = y_start;
        let mut x = frame_x;
        grid.put_char(x, y, "╭", &info.face);
        x += 1;
        grid.put_char(x, y, "─", &info.face);
        x += 1;

        if info.title.is_empty() || cw < 4 {
            for _ in 0..cw {
                grid.put_char(x, y, "─", &info.face);
                x += 1;
            }
        } else {
            let max_title_w = (cw as usize).saturating_sub(2);
            let title_display_w = title_w.min(max_title_w);
            let dash_count = cw as usize - title_display_w - 2;
            let left_dashes = dash_count / 2;
            let right_dashes = dash_count - left_dashes;

            for _ in 0..left_dashes {
                grid.put_char(x, y, "─", &info.face);
                x += 1;
            }
            grid.put_char(x, y, "┤", &info.face);
            x += 1;
            grid.put_line_with_base(y, x, &info.title, title_display_w as u16, Some(&info.face));
            x += title_display_w as u16;
            grid.put_char(x, y, "├", &info.face);
            x += 1;
            for _ in 0..right_dashes {
                grid.put_char(x, y, "─", &info.face);
                x += 1;
            }
        }

        grid.put_char(x, y, "─", &info.face);
        x += 1;
        if x < grid.width {
            grid.put_char(x, y, "╮", &info.face);
        }
    }

    // --- Side borders + content ---
    let max_visible_rows = (total_h - 2) as u16;
    let content_rows = (wrapped_h as u16).min(max_visible_rows);
    let truncated = wrapped_h as u16 > max_visible_rows;
    let content_x = frame_x + 2; // after "│ "
    let content_y = y_start + 1;
    let content_end_y = content_y + content_rows;

    // Draw left/right borders for all content rows
    for row in 0..content_rows {
        let y = content_y + row;
        grid.put_char(frame_x, y, "│", &info.face);
        grid.put_char(frame_x + 1, y, " ", &info.face);
        let right_space = frame_x + 2 + cw;
        let right_border = frame_x + 3 + cw;
        if right_space < grid.width {
            grid.put_char(right_space, y, " ", &info.face);
        }
        if right_border < grid.width {
            grid.put_char(right_border, y, "│", &info.face);
        }
    }

    // Render content with wrapping
    let mut y = content_y;
    for line in trimmed {
        if y >= content_end_y {
            break;
        }
        let rows = render_wrapped_line(
            grid,
            y,
            content_x,
            line,
            cw,
            Some(&info.face),
            content_end_y,
        );
        y += rows;
    }

    // --- Bottom border: ╰─...─╯ (uses ┄ when content is truncated) ---
    let bottom_y = y_start + (content_rows as usize + 1).min(total_h - 1) as u16;
    let dash = if truncated { "┄" } else { "─" };
    {
        let mut x = frame_x;
        grid.put_char(x, bottom_y, "╰", &info.face);
        x += 1;
        grid.put_char(x, bottom_y, dash, &info.face);
        x += 1;
        for _ in 0..cw {
            grid.put_char(x, bottom_y, dash, &info.face);
            x += 1;
        }
        grid.put_char(x, bottom_y, dash, &info.face);
        x += 1;
        if x < grid.width {
            grid.put_char(x, bottom_y, "╯", &info.face);
        }
    }

    // Ellipsis truncation post-pass
    let padding_x = frame_x + 2 + cw;
    let border_x = frame_x + 3 + cw;
    for row in 0..content_rows {
        let y = content_y + row;
        if let Some(cell) = grid.get(padding_x, y)
            && cell.grapheme != " "
        {
            grid.put_char(padding_x, y, "…", &info.face);
            grid.put_char(border_x, y, "│", &info.face);
        }
    }
}

/// Render framed info popup (modal style): border + shadow + title + content.
fn render_info_framed(
    info: &crate::state::InfoState,
    grid: &mut CellGrid,
    win: &crate::layout::FloatingWindow,
) {
    draw_shadow(grid, win);

    // Framed: │ + space padding on each side → inner offset 2, inner width -4
    let inner_x = win.x + 2;
    let inner_y = win.y + 1;
    let inner_w = win.width.saturating_sub(4).max(1);
    let inner_h = win.height.saturating_sub(2);
    let y_limit = inner_y + inner_h;

    // Fill entire inner area (including space padding columns) with info face
    for row in 0..inner_h {
        let y = inner_y + row;
        for x in (win.x + 1)..(win.x + win.width).saturating_sub(1) {
            grid.put_char(x, y, " ", &info.face);
        }
    }

    // Render content lines with wrapping
    let mut y = inner_y;
    let mut all_rendered = true;
    for (i, line) in info.content.iter().enumerate() {
        if y >= y_limit {
            if i < info.content.len() {
                all_rendered = false;
            }
            break;
        }
        let rows = render_wrapped_line(grid, y, inner_x, line, inner_w, Some(&info.face), y_limit);
        y += rows;
    }
    let truncated = !all_rendered;

    draw_border(grid, win, &info.face, truncated, ("╭", "╮", "╰", "╯"));

    // Draw title on top border: ╭─┤title├─╮
    if !info.title.is_empty() {
        let title_width = line_display_width(&info.title);
        if title_width > 0 && win.width > 6 {
            let tx = win.x + 2; // after ╭─
            grid.put_char(tx, win.y, "┤", &info.face);
            let max_title = (win.width as usize - 6).min(title_width) as u16;
            grid.put_line_with_base(win.y, tx + 1, &info.title, max_title, Some(&info.face));
            let after = tx + 1 + max_title;
            if after < win.x + win.width - 2 {
                grid.put_char(after, win.y, "├", &info.face);
            }
        }
    }

    // Ellipsis truncation post-pass: if content overflowed into padding column
    let padding_x = win.x + win.width - 2; // space before right │
    let border_x = win.x + win.width - 1; // right │
    for row in 0..inner_h {
        let y = inner_y + row;
        if let Some(cell) = grid.get(padding_x, y)
            && cell.grapheme != " "
        {
            grid.put_char(padding_x, y, "…", &info.face);
            grid.put_char(border_x, y, "│", &info.face);
        }
    }
}

/// Render non-framed info popup (inline, inlineAbove, menuDoc): no border, no shadow.
fn render_info_nonframed(
    info: &crate::state::InfoState,
    grid: &mut CellGrid,
    win: &crate::layout::FloatingWindow,
) {
    let y_limit = win.y + win.height;

    // Fill background with info face
    for row in 0..win.height {
        let y = win.y + row;
        for x in win.x..(win.x + win.width) {
            grid.put_char(x, y, " ", &info.face);
        }
    }

    // Render content directly (no border, no title)
    let mut y = win.y;
    for line in &info.content {
        if y >= y_limit {
            break;
        }
        let rows = render_wrapped_line(grid, y, win.x, line, win.width, Some(&info.face), y_limit);
        y += rows;
    }
}

/// Render a protocol Line with word-boundary wrapping at `max_width` columns
/// (matching Kakoune's `wrap_lines`).
/// Returns the number of visual rows consumed.
/// `y_limit` is the exclusive upper bound for y coordinates (content must not exceed this).
fn render_wrapped_line(
    grid: &mut CellGrid,
    y_start: u16,
    x_start: u16,
    line: &Line,
    max_width: u16,
    base_face: Option<&Face>,
    y_limit: u16,
) -> u16 {
    if max_width == 0 {
        return 1;
    }

    // Phase 1: collect graphemes with resolved faces and widths
    let mut graphemes: Vec<(&str, Face, u16)> = Vec::new();
    for atom in line {
        let face = match base_face {
            Some(base) => resolve_face(&atom.face, base),
            None => atom.face.clone(),
        };
        for grapheme in atom.contents.split_inclusive(|_: char| true) {
            if grapheme.is_empty() {
                continue;
            }
            let w = UnicodeWidthStr::width(grapheme) as u16;
            if w == 0 {
                continue;
            }
            graphemes.push((grapheme, face.clone(), w));
        }
    }

    if graphemes.is_empty() {
        return 1;
    }

    // Phase 2: compute word-wrap layout — (row_offset, column) per grapheme
    let layout = word_wrap_layout(&graphemes, max_width);

    // Phase 3: render
    let mut max_row = 0u16;
    for (idx, &(row, col)) in layout.iter().enumerate() {
        let y = y_start + row;
        if y >= y_limit {
            break;
        }
        let x = x_start + col;
        let (grapheme, ref face, _) = graphemes[idx];
        grid.put_char(x, y, grapheme, face);
        max_row = row;
    }

    max_row + 1
}

/// Compute word-boundary-aware layout: returns `(row_offset, column)` for each grapheme.
fn word_wrap_layout(graphemes: &[(&str, Face, u16)], max_width: u16) -> Vec<(u16, u16)> {
    let mut result: Vec<(u16, u16)> = Vec::with_capacity(graphemes.len());
    let mut row = 0u16;
    let mut col = 0u16;
    let mut last_break_result_len: Option<usize> = None;
    let mut last_break_grapheme_idx: Option<usize> = None;
    let mut i = 0;

    while i < graphemes.len() {
        let (text, _, w) = graphemes[i];

        if col + w > max_width {
            if col == 0 {
                // Grapheme wider than max_width: force-place it
                result.push((row, 0));
                row += 1;
                col = 0;
                last_break_result_len = None;
                last_break_grapheme_idx = None;
                i += 1;
                continue;
            }
            // Wrap to next row
            row += 1;
            col = 0;
            if let Some(brk_len) = last_break_result_len {
                let brk_idx = last_break_grapheme_idx.unwrap();
                result.truncate(brk_len);
                i = brk_idx;
                last_break_result_len = None;
                last_break_grapheme_idx = None;
            }
            // Don't increment i; re-process current grapheme on new row
            continue;
        }

        result.push((row, col));
        col += w;

        if !crate::layout::is_word_char(text) {
            last_break_result_len = Some(result.len());
            last_break_grapheme_idx = Some(i + 1);
        }

        i += 1;
    }

    result
}

fn draw_border(
    grid: &mut CellGrid,
    win: &crate::layout::FloatingWindow,
    face: &Face,
    truncated: bool,
    corners: (&str, &str, &str, &str), // (top-left, top-right, bottom-left, bottom-right)
) {
    let x1 = win.x;
    let y1 = win.y;
    let x2 = win.x + win.width - 1;
    let y2 = win.y + win.height - 1;
    let bottom_dash = if truncated { "┄" } else { "─" };

    // Corners
    grid.put_char(x1, y1, corners.0, face);
    grid.put_char(x2, y1, corners.1, face);
    grid.put_char(x1, y2, corners.2, face);
    grid.put_char(x2, y2, corners.3, face);

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

fn draw_shadow(grid: &mut CellGrid, win: &crate::layout::FloatingWindow) {
    let dim_face = Face {
        fg: Color::Default,
        bg: Color::Default,
        underline: Color::Default,
        attributes: vec![Attribute::Dim],
    };

    // Right shadow (1 cell wide)
    let sx = win.x + win.width;
    if sx < grid.width {
        for y in (win.y + 1)..=(win.y + win.height) {
            if y < grid.height {
                grid.put_char(sx, y, " ", &dim_face);
            }
        }
    }

    // Bottom shadow (1 cell tall)
    let sy = win.y + win.height;
    if sy < grid.height {
        for x in (win.x + 1)..=(win.x + win.width) {
            if x < grid.width {
                grid.put_char(x, sy, " ", &dim_face);
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
    use crate::protocol::{Atom, Face};

    fn default_face() -> Face {
        Face::default()
    }

    fn make_line(text: &str) -> Line {
        vec![Atom {
            face: default_face(),
            contents: text.to_string(),
        }]
    }

    #[test]
    fn test_grid_new() {
        let grid = CellGrid::new(10, 5);
        assert_eq!(grid.width, 10);
        assert_eq!(grid.height, 5);
        assert_eq!(grid.current.len(), 50);
    }

    #[test]
    fn test_put_char() {
        let mut grid = CellGrid::new(10, 5);
        let face = default_face();
        grid.put_char(0, 0, "A", &face);
        assert_eq!(grid.get(0, 0).unwrap().grapheme, "A");
        assert_eq!(grid.get(0, 0).unwrap().width, 1);
    }

    #[test]
    fn test_put_wide_char() {
        let mut grid = CellGrid::new(10, 5);
        let face = default_face();
        grid.put_char(0, 0, "漢", &face);
        assert_eq!(grid.get(0, 0).unwrap().grapheme, "漢");
        assert_eq!(grid.get(0, 0).unwrap().width, 2);
        // Continuation cell
        assert_eq!(grid.get(1, 0).unwrap().width, 0);
    }

    #[test]
    fn test_put_line() {
        let mut grid = CellGrid::new(20, 5);
        let line = make_line("hello");
        let cols = grid.put_line(0, 0, &line, 20);
        assert_eq!(cols, 5);
        assert_eq!(grid.get(0, 0).unwrap().grapheme, "h");
        assert_eq!(grid.get(4, 0).unwrap().grapheme, "o");
    }

    #[test]
    fn test_put_line_cjk() {
        let mut grid = CellGrid::new(20, 5);
        let line = make_line("漢字");
        let cols = grid.put_line(0, 0, &line, 20);
        assert_eq!(cols, 4); // 2 wide chars × 2
    }

    #[test]
    fn test_put_line_truncation() {
        let mut grid = CellGrid::new(5, 1);
        let line = make_line("hello world");
        let cols = grid.put_line(0, 0, &line, 5);
        assert_eq!(cols, 5);
    }

    #[test]
    fn test_diff_full_redraw() {
        let grid = CellGrid::new(3, 2);
        let diffs = grid.diff();
        // All non-continuation cells should be in the diff
        assert_eq!(diffs.len(), 6);
    }

    #[test]
    fn test_diff_after_swap() {
        let mut grid = CellGrid::new(3, 1);
        grid.swap(); // previous = current, current = blank
        // Now current and previous are the same (both blank)
        let diffs = grid.diff();
        assert_eq!(diffs.len(), 0);
    }

    #[test]
    fn test_diff_detects_change() {
        let mut grid = CellGrid::new(3, 1);
        grid.swap(); // previous = blank
        let face = default_face();
        grid.put_char(1, 0, "X", &face);
        let diffs = grid.diff();
        assert_eq!(diffs.len(), 1);
        assert_eq!(diffs[0].x, 1);
        assert_eq!(diffs[0].cell.grapheme, "X");
    }

    #[test]
    fn test_clear() {
        let mut grid = CellGrid::new(3, 1);
        let face = Face {
            fg: Color::Named(crate::protocol::NamedColor::Red),
            ..Face::default()
        };
        grid.put_char(0, 0, "A", &face);
        grid.clear(&Face::default());
        assert_eq!(grid.get(0, 0).unwrap().grapheme, " ");
    }

    #[test]
    fn test_resize() {
        let mut grid = CellGrid::new(10, 5);
        grid.resize(20, 10);
        assert_eq!(grid.width, 20);
        assert_eq!(grid.height, 10);
        assert_eq!(grid.current.len(), 200);
    }

    #[test]
    fn test_invalidate_all() {
        let mut grid = CellGrid::new(3, 1);
        grid.swap();
        assert!(!grid.previous.is_empty());
        grid.invalidate_all();
        assert!(grid.previous.is_empty());
        // After invalidation, diff should return all cells
        assert_eq!(grid.diff().len(), 3);
    }

    #[test]
    fn test_resolve_face_fg_bg() {
        let base = Face {
            fg: Color::Named(crate::protocol::NamedColor::Red),
            bg: Color::Named(crate::protocol::NamedColor::Blue),
            ..Face::default()
        };
        let atom = Face {
            fg: Color::Default,
            bg: Color::Named(crate::protocol::NamedColor::Green),
            ..Face::default()
        };
        let resolved = resolve_face(&atom, &base);
        assert_eq!(resolved.fg, base.fg); // inherited
        assert_eq!(resolved.bg, atom.bg); // kept
    }

    #[test]
    fn test_resolve_face_underline() {
        let base = Face {
            underline: Color::Named(crate::protocol::NamedColor::Red),
            ..Face::default()
        };
        // Default underline inherits from base
        let atom_default = Face::default();
        let resolved = resolve_face(&atom_default, &base);
        assert_eq!(resolved.underline, base.underline);

        // Explicit underline is kept
        let atom_explicit = Face {
            underline: Color::Named(crate::protocol::NamedColor::Green),
            ..Face::default()
        };
        let resolved2 = resolve_face(&atom_explicit, &base);
        assert_eq!(resolved2.underline, atom_explicit.underline);
    }

    #[test]
    fn test_resolve_face_attributes_merge() {
        let base = Face {
            attributes: vec![Attribute::Bold],
            ..Face::default()
        };
        let atom = Face {
            attributes: vec![Attribute::Italic],
            ..Face::default()
        };
        let resolved = resolve_face(&atom, &base);
        assert!(resolved.attributes.contains(&Attribute::Bold));
        assert!(resolved.attributes.contains(&Attribute::Italic));
        assert_eq!(resolved.attributes.len(), 2);
    }

    #[test]
    fn test_resolve_face_attributes_final() {
        let base = Face {
            attributes: vec![Attribute::Bold],
            ..Face::default()
        };
        let atom = Face {
            attributes: vec![Attribute::Italic, Attribute::FinalAttr],
            ..Face::default()
        };
        let resolved = resolve_face(&atom, &base);
        // FinalAttr means atom attributes replace base entirely
        assert!(!resolved.attributes.contains(&Attribute::Bold));
        assert!(resolved.attributes.contains(&Attribute::Italic));
        assert!(resolved.attributes.contains(&Attribute::FinalAttr));
    }

    #[test]
    fn test_render_buffer_resolves_default_face() {
        use crate::protocol::NamedColor;
        use crate::state::AppState;

        let default_face = Face {
            fg: Color::Named(NamedColor::Yellow),
            bg: Color::Named(NamedColor::Blue),
            ..Face::default()
        };
        // Atom has Color::Default fg/bg — should inherit from default_face
        let line = vec![Atom {
            face: Face::default(),
            contents: "x".to_string(),
        }];

        let mut state = AppState::default();
        state.lines = vec![line];
        state.default_face = default_face.clone();

        let mut grid = CellGrid::new(10, 2);
        render_buffer(&state, &mut grid);

        let cell = grid.get(0, 0).unwrap();
        assert_eq!(cell.grapheme, "x");
        assert_eq!(cell.face.fg, Color::Named(NamedColor::Yellow));
        assert_eq!(cell.face.bg, Color::Named(NamedColor::Blue));
    }

    #[test]
    fn test_render_status_resolves_default_face() {
        use crate::protocol::NamedColor;
        use crate::state::AppState;

        let status_face = Face {
            fg: Color::Named(NamedColor::Cyan),
            bg: Color::Named(NamedColor::Magenta),
            ..Face::default()
        };
        let status_line = vec![Atom {
            face: Face::default(),
            contents: "s".to_string(),
        }];
        let mode_line = vec![Atom {
            face: Face::default(),
            contents: "m".to_string(),
        }];

        let mut state = AppState::default();
        state.status_line = status_line;
        state.status_mode_line = mode_line;
        state.status_default_face = status_face.clone();

        let mut grid = CellGrid::new(10, 2);
        render_status(&state, &mut grid);

        // Status line at row 1 (last row of 2-row grid)
        let cell = grid.get(0, 1).unwrap();
        assert_eq!(cell.grapheme, "s");
        assert_eq!(cell.face.fg, Color::Named(NamedColor::Cyan));
        assert_eq!(cell.face.bg, Color::Named(NamedColor::Magenta));

        // Mode line at rightmost position
        let cell_mode = grid.get(9, 1).unwrap();
        assert_eq!(cell_mode.grapheme, "m");
        assert_eq!(cell_mode.face.fg, Color::Named(NamedColor::Cyan));
        assert_eq!(cell_mode.face.bg, Color::Named(NamedColor::Magenta));
    }

    #[test]
    fn test_render_menu_prompt_horizontal() {
        use crate::protocol::NamedColor;
        use crate::protocol::MenuStyle;
        use crate::state::{AppState, MenuState};

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
        use crate::protocol::MenuStyle;
        use crate::state::{AppState, MenuState};

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
        use crate::protocol::MenuStyle;
        use crate::state::{AppState, MenuState};

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
