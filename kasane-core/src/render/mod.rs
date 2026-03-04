mod grid;
mod menu;
mod info;

pub use grid::{Cell, CellGrid, CellDiff};

use unicode_width::UnicodeWidthStr;

use crate::protocol::{Attribute, Color, CursorMode, Face, Line};

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

/// Determine the cursor style from the application state.
///
/// Priority: ui_option `kasane_cursor_style` > prompt mode > mode_line heuristic > Block.
pub fn cursor_style(state: &AppState) -> CursorStyle {
    if let Some(style) = state.ui_options.get("kasane_cursor_style") {
        return match style.as_str() {
            "bar" => CursorStyle::Bar,
            "underline" => CursorStyle::Underline,
            _ => CursorStyle::Block,
        };
    }
    if state.cursor_mode == CursorMode::Prompt {
        return CursorStyle::Bar;
    }
    if state
        .status_mode_line
        .iter()
        .any(|atom| atom.contents == "[insert]")
    {
        return CursorStyle::Bar;
    }
    CursorStyle::Block
}

// ---------------------------------------------------------------------------
// Full frame rendering (Z-order)
// ---------------------------------------------------------------------------

pub fn render_frame(state: &AppState, grid: &mut CellGrid) {
    grid.clear(&state.default_face);
    render_buffer(state, grid);       // Layer 0
    render_status(state, grid);       // Layer 1
    menu::render_menu(state, grid);   // Layer 2 (+ shadow)
    info::render_info(state, grid);   // Layer 3 (+ shadow)
    // Cursor face is already applied by Kakoune in draw data.
    // Terminal cursor positioning is handled separately via backend.show_cursor().
}

// ---------------------------------------------------------------------------
// Shared helpers (used by menu.rs and info.rs)
// ---------------------------------------------------------------------------

fn line_display_width(line: &Line) -> usize {
    line.iter()
        .map(|atom| UnicodeWidthStr::width(atom.contents.as_str()))
        .sum()
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
            Some(base) => grid::resolve_face(&atom.face, base),
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
    use crate::protocol::{Atom, Color, Face, NamedColor};
    use crate::state::AppState;

    #[test]
    fn test_render_buffer_resolves_default_face() {
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
    fn test_cursor_style_ui_option_bar() {
        let mut state = AppState::default();
        state
            .ui_options
            .insert("kasane_cursor_style".into(), "bar".into());
        assert_eq!(cursor_style(&state), CursorStyle::Bar);
    }

    #[test]
    fn test_cursor_style_ui_option_underline() {
        let mut state = AppState::default();
        state
            .ui_options
            .insert("kasane_cursor_style".into(), "underline".into());
        assert_eq!(cursor_style(&state), CursorStyle::Underline);
    }

    #[test]
    fn test_cursor_style_prompt_mode() {
        let mut state = AppState::default();
        state.cursor_mode = CursorMode::Prompt;
        assert_eq!(cursor_style(&state), CursorStyle::Bar);
    }

    #[test]
    fn test_cursor_style_insert_mode_line() {
        let mut state = AppState::default();
        state.status_mode_line = vec![Atom {
            face: Face::default(),
            contents: "[insert]".into(),
        }];
        assert_eq!(cursor_style(&state), CursorStyle::Bar);
    }

    #[test]
    fn test_cursor_style_default_block() {
        let state = AppState::default();
        assert_eq!(cursor_style(&state), CursorStyle::Block);
    }

    #[test]
    fn test_cursor_style_ui_option_overrides_mode_line() {
        let mut state = AppState::default();
        state
            .ui_options
            .insert("kasane_cursor_style".into(), "block".into());
        state.status_mode_line = vec![Atom {
            face: Face::default(),
            contents: "[insert]".into(),
        }];
        assert_eq!(cursor_style(&state), CursorStyle::Block);
    }
}
