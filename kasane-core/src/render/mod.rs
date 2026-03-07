mod grid;
mod info;
pub mod markup;
pub(crate) mod menu;
pub mod paint;
pub(crate) mod theme;
pub mod view;

pub use grid::{Cell, CellDiff, CellGrid};
pub use theme::Theme;

use crate::protocol::CursorMode;
use crate::state::AppState;

// ---------------------------------------------------------------------------
// RenderBackend trait
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CursorStyle {
    Block,
    Bar,
    Underline,
    Outline,
}

pub trait RenderBackend {
    fn size(&self) -> (u16, u16);
    fn begin_frame(&mut self) -> anyhow::Result<()> {
        Ok(())
    }
    fn end_frame(&mut self) -> anyhow::Result<()> {
        Ok(())
    }
    fn draw(&mut self, diffs: &[CellDiff]) -> anyhow::Result<()>;
    fn flush(&mut self) -> anyhow::Result<()>;
    fn show_cursor(&mut self, x: u16, y: u16, style: CursorStyle) -> anyhow::Result<()>;
    fn hide_cursor(&mut self) -> anyhow::Result<()>;
}

// ---------------------------------------------------------------------------
// Buffer / Status / Cursor rendering
// ---------------------------------------------------------------------------

/// Render the main buffer area (all lines except the last row which is status).
/// Retained for regression testing against the new declarative pipeline.
#[cfg(test)]
fn render_buffer(state: &AppState, grid: &mut CellGrid) {
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
/// Retained for regression testing against the new declarative pipeline.
#[cfg(test)]
fn render_status(state: &AppState, grid: &mut CellGrid) {
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
    let mode_width = crate::layout::line_display_width(&state.status_mode_line);
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
    if !state.focused {
        return CursorStyle::Outline;
    }
    if state.cursor_mode == CursorMode::Prompt {
        return CursorStyle::Bar;
    }
    let mode = state
        .status_mode_line
        .iter()
        .find_map(|atom| match atom.contents.as_str() {
            "insert" => Some(CursorStyle::Bar),
            "replace" => Some(CursorStyle::Underline),
            _ => None,
        });
    mode.unwrap_or(CursorStyle::Block)
}

/// In non-block cursor modes (insert/replace), clear the PrimaryCursor face
/// highlight from the cursor cell so the terminal cursor shape is visible.
pub fn clear_block_cursor_face(state: &AppState, grid: &mut CellGrid, style: CursorStyle) {
    if style == CursorStyle::Block || style == CursorStyle::Outline {
        return;
    }
    let cx = state.cursor_pos.column as u16;
    let cy = match state.cursor_mode {
        CursorMode::Prompt => grid.height.saturating_sub(1),
        CursorMode::Buffer => state.cursor_pos.line as u16,
    };
    let base_face = match state.cursor_mode {
        CursorMode::Buffer => &state.default_face,
        CursorMode::Prompt => &state.status_default_face,
    };
    if let Some(cell) = grid.get_mut(cx, cy) {
        cell.face = *base_face;
    }
}

// ---------------------------------------------------------------------------
// Full frame rendering (Z-order)
// ---------------------------------------------------------------------------

/// Retained for regression testing against the new declarative pipeline.
#[cfg(test)]
fn render_frame(state: &AppState, grid: &mut CellGrid) {
    grid.clear(&state.default_face);
    render_buffer(state, grid); // Layer 0
    render_status(state, grid); // Layer 1
    menu::render_menu(state, grid); // Layer 2 (+ shadow)
    info::render_info(state, grid); // Layer 3 (+ shadow)
    // Cursor face is already applied by Kakoune in draw data.
    // Terminal cursor positioning is handled separately via backend.show_cursor().
}

// ---------------------------------------------------------------------------
// Shared helpers (used by menu.rs and info.rs)
// ---------------------------------------------------------------------------

/// Render a protocol Line with word-boundary wrapping at `max_width` columns
/// (matching Kakoune's `wrap_lines`).
/// Returns the number of visual rows consumed.
/// `y_limit` is the exclusive upper bound for y coordinates (content must not exceed this).
#[cfg(test)]
fn render_wrapped_line(
    grid: &mut CellGrid,
    y_start: u16,
    x_start: u16,
    line: &crate::protocol::Line,
    max_width: u16,
    base_face: Option<&crate::protocol::Face>,
    y_limit: u16,
) -> u16 {
    use unicode_width::UnicodeWidthStr;
    if max_width == 0 {
        return 1;
    }

    // Phase 1: collect graphemes with resolved faces and widths
    let mut graphemes: Vec<(&str, crate::protocol::Face, u16)> = Vec::new();
    for atom in line {
        let face = match base_face {
            Some(base) => grid::resolve_face(&atom.face, base),
            None => atom.face,
        };
        for grapheme in atom.contents.split_inclusive(|_: char| true) {
            if grapheme.is_empty() {
                continue;
            }
            // Skip control characters (see grid.rs for rationale).
            if grapheme.starts_with(|c: char| c.is_control()) {
                continue;
            }
            let w = UnicodeWidthStr::width(grapheme) as u16;
            if w == 0 {
                continue;
            }
            graphemes.push((grapheme, face, w));
        }
    }

    if graphemes.is_empty() {
        return 1;
    }

    // Phase 2: build metrics and compute segments
    let metrics: Vec<(u16, bool)> = graphemes
        .iter()
        .map(|(text, _, w)| (*w, !crate::layout::is_word_char(text)))
        .collect();
    let segments = crate::layout::word_wrap_segments(&metrics, max_width);

    // Phase 3: render from segments
    let mut max_row = 0u16;
    for (row_idx, seg) in segments.iter().enumerate() {
        let y = y_start + row_idx as u16;
        if y >= y_limit {
            break;
        }
        let mut col = 0u16;
        for i in seg.start..seg.end {
            let (grapheme, ref face, w) = graphemes[i];
            grid.put_char(x_start + col, y, grapheme, face);
            col += w;
        }
        max_row = row_idx as u16;
    }

    max_row + 1
}

#[cfg(test)]
fn draw_border(
    grid: &mut CellGrid,
    win: &crate::layout::FloatingWindow,
    face: &crate::protocol::Face,
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

#[cfg(test)]
fn draw_shadow(grid: &mut CellGrid, win: &crate::layout::FloatingWindow) {
    use crate::protocol::{Attributes, Color, Face};
    let dim_face = Face {
        fg: Color::Default,
        bg: Color::Default,
        underline: Color::Default,
        attributes: Attributes::DIM,
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
    use crate::layout::Rect;
    use crate::layout::flex;
    use crate::plugin::PluginRegistry;
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
        state.default_face = default_face;

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
        state.status_default_face = status_face;

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
            contents: "insert".into(),
        }];
        assert_eq!(cursor_style(&state), CursorStyle::Bar);
    }

    #[test]
    fn test_cursor_style_replace_mode_line() {
        let mut state = AppState::default();
        state.status_mode_line = vec![Atom {
            face: Face::default(),
            contents: "replace".into(),
        }];
        assert_eq!(cursor_style(&state), CursorStyle::Underline);
    }

    #[test]
    fn test_cursor_style_default_block() {
        let state = AppState::default();
        assert_eq!(cursor_style(&state), CursorStyle::Block);
    }

    #[test]
    fn test_cursor_style_unfocused_outline() {
        let mut state = AppState::default();
        state.focused = false;
        assert_eq!(cursor_style(&state), CursorStyle::Outline);
    }

    #[test]
    fn test_cursor_style_ui_option_overrides_unfocused() {
        let mut state = AppState::default();
        state.focused = false;
        state
            .ui_options
            .insert("kasane_cursor_style".into(), "bar".into());
        assert_eq!(cursor_style(&state), CursorStyle::Bar);
    }

    #[test]
    fn test_cursor_style_ui_option_overrides_mode_line() {
        let mut state = AppState::default();
        state
            .ui_options
            .insert("kasane_cursor_style".into(), "block".into());
        state.status_mode_line = vec![Atom {
            face: Face::default(),
            contents: "insert".into(),
        }];
        assert_eq!(cursor_style(&state), CursorStyle::Block);
    }

    // --- clear_block_cursor_face tests ---

    #[test]
    fn test_clear_block_cursor_face_bar() {
        let mut state = AppState::default();
        state.cursor_pos = crate::protocol::Coord { line: 0, column: 2 };
        state.default_face = Face {
            fg: Color::Named(NamedColor::White),
            bg: Color::Named(NamedColor::Black),
            ..Face::default()
        };

        let mut grid = CellGrid::new(10, 5);
        let cursor_face = Face {
            fg: Color::Named(NamedColor::Black),
            bg: Color::Named(NamedColor::White),
            ..Face::default()
        };
        grid.put_char(2, 0, "x", &cursor_face);

        clear_block_cursor_face(&state, &mut grid, CursorStyle::Bar);

        let cell = grid.get(2, 0).unwrap();
        assert_eq!(cell.face, state.default_face);
    }

    #[test]
    fn test_clear_block_cursor_face_underline() {
        let mut state = AppState::default();
        state.cursor_pos = crate::protocol::Coord { line: 1, column: 3 };
        state.default_face = Face {
            fg: Color::Named(NamedColor::Yellow),
            bg: Color::Named(NamedColor::Blue),
            ..Face::default()
        };

        let mut grid = CellGrid::new(10, 5);
        let cursor_face = Face {
            fg: Color::Named(NamedColor::Black),
            bg: Color::Named(NamedColor::White),
            ..Face::default()
        };
        grid.put_char(3, 1, "y", &cursor_face);

        clear_block_cursor_face(&state, &mut grid, CursorStyle::Underline);

        let cell = grid.get(3, 1).unwrap();
        assert_eq!(cell.face, state.default_face);
    }

    #[test]
    fn test_clear_block_cursor_face_block_noop() {
        let mut state = AppState::default();
        state.cursor_pos = crate::protocol::Coord { line: 0, column: 0 };

        let mut grid = CellGrid::new(10, 5);
        let cursor_face = Face {
            fg: Color::Named(NamedColor::Black),
            bg: Color::Named(NamedColor::White),
            ..Face::default()
        };
        grid.put_char(0, 0, "z", &cursor_face);

        clear_block_cursor_face(&state, &mut grid, CursorStyle::Block);

        let cell = grid.get(0, 0).unwrap();
        assert_eq!(cell.face, cursor_face);
    }

    #[test]
    fn test_clear_block_cursor_face_prompt() {
        let mut state = AppState::default();
        state.cursor_mode = crate::protocol::CursorMode::Prompt;
        state.cursor_pos = crate::protocol::Coord { line: 0, column: 1 };
        state.status_default_face = Face {
            fg: Color::Named(NamedColor::Cyan),
            bg: Color::Named(NamedColor::Magenta),
            ..Face::default()
        };

        let mut grid = CellGrid::new(10, 5);
        let cursor_face = Face {
            fg: Color::Named(NamedColor::Black),
            bg: Color::Named(NamedColor::White),
            ..Face::default()
        };
        // Prompt cursor is at the last row
        grid.put_char(1, 4, "p", &cursor_face);

        clear_block_cursor_face(&state, &mut grid, CursorStyle::Bar);

        let cell = grid.get(1, 4).unwrap();
        assert_eq!(cell.face, state.status_default_face);
    }

    #[test]
    fn test_clear_block_cursor_face_out_of_bounds() {
        let mut state = AppState::default();
        state.cursor_pos = crate::protocol::Coord {
            line: 100,
            column: 100,
        };

        let mut grid = CellGrid::new(10, 5);
        // Should not panic
        clear_block_cursor_face(&state, &mut grid, CursorStyle::Bar);
    }

    // --- Regression test: declarative pipeline vs imperative ---

    fn make_line(s: &str) -> Vec<Atom> {
        vec![Atom {
            face: Face::default(),
            contents: s.to_string(),
        }]
    }

    /// Tree-sitter colors: verify that explicit RGB colors in atoms survive the
    /// full declarative pipeline (view → layout → paint) and match the old pipeline.
    #[test]
    fn test_treesitter_rgb_colors_preserved() {
        let mut state = AppState::default();
        state.cols = 40;
        state.rows = 5;
        state.default_face = Face {
            fg: Color::Named(NamedColor::White),
            bg: Color::Named(NamedColor::Black),
            ..Face::default()
        };
        state.padding_face = state.default_face;
        state.status_default_face = state.default_face;

        // Simulate tree-sitter: atoms with explicit RGB fg, Default bg
        let keyword_face = Face {
            fg: Color::Rgb {
                r: 255,
                g: 100,
                b: 0,
            },
            bg: Color::Default,
            ..Face::default()
        };
        let string_face = Face {
            fg: Color::Rgb {
                r: 0,
                g: 200,
                b: 100,
            },
            bg: Color::Default,
            ..Face::default()
        };
        let ts_line = vec![
            Atom {
                face: keyword_face,
                contents: "fn".to_string(),
            },
            Atom {
                face: Face::default(),
                contents: " ".to_string(),
            },
            Atom {
                face: string_face,
                contents: "main".to_string(),
            },
        ];
        state.lines = vec![ts_line];
        state.status_line = make_line("status");

        // Old pipeline
        let mut grid_old = CellGrid::new(state.cols, state.rows);
        render_frame(&state, &mut grid_old);

        // New pipeline
        let mut grid_new = CellGrid::new(state.cols, state.rows);
        grid_new.clear(&state.default_face);
        let registry = PluginRegistry::new();
        let element = view::view(&state, &registry);
        let root_area = Rect {
            x: 0,
            y: 0,
            w: state.cols,
            h: state.rows,
        };
        let layout_result = flex::place(&element, root_area, &state);
        paint::paint(&element, &layout_result, &mut grid_new, &state);

        // Check tree-sitter colors in buffer row 0
        // "fn" at columns 0-1 should have keyword_face fg
        let cell_f = grid_new.get(0, 0).unwrap();
        assert_eq!(cell_f.grapheme, "f");
        assert_eq!(
            cell_f.face.fg,
            Color::Rgb {
                r: 255,
                g: 100,
                b: 0
            },
            "tree-sitter keyword fg lost in declarative pipeline"
        );
        assert_eq!(
            cell_f.face.bg,
            Color::Named(NamedColor::Black),
            "tree-sitter keyword bg not resolved against default_face"
        );

        let cell_n = grid_new.get(1, 0).unwrap();
        assert_eq!(cell_n.grapheme, "n");
        assert_eq!(
            cell_n.face.fg,
            Color::Rgb {
                r: 255,
                g: 100,
                b: 0
            }
        );

        // " " at column 2 should have default_face (resolved from Default)
        let cell_sp = grid_new.get(2, 0).unwrap();
        assert_eq!(
            cell_sp.face.fg,
            Color::Named(NamedColor::White),
            "default space fg should resolve to default_face.fg"
        );

        // "main" at columns 3-6 should have string_face fg
        let cell_m = grid_new.get(3, 0).unwrap();
        assert_eq!(cell_m.grapheme, "m");
        assert_eq!(
            cell_m.face.fg,
            Color::Rgb {
                r: 0,
                g: 200,
                b: 100
            },
            "tree-sitter string fg lost in declarative pipeline"
        );

        // Cross-check: old and new pipelines produce identical results
        for y in 0..state.rows {
            for x in 0..state.cols {
                let old = grid_old.get(x, y).unwrap();
                let new = grid_new.get(x, y).unwrap();
                assert_eq!(
                    old.grapheme, new.grapheme,
                    "grapheme mismatch at ({x}, {y})"
                );
                assert_eq!(
                    old.face.fg, new.face.fg,
                    "fg mismatch at ({x}, {y}): old={:?} new={:?}",
                    old.face.fg, new.face.fg
                );
                assert_eq!(
                    old.face.bg, new.face.bg,
                    "bg mismatch at ({x}, {y}): old={:?} new={:?}",
                    old.face.bg, new.face.bg
                );
            }
        }
    }

    /// Multi-frame test: verify tree-sitter colors survive across swap/diff cycles.
    #[test]
    fn test_treesitter_colors_persist_across_frames() {
        let mut state = AppState::default();
        state.cols = 20;
        state.rows = 3;
        state.default_face = Face {
            fg: Color::Named(NamedColor::White),
            bg: Color::Named(NamedColor::Black),
            ..Face::default()
        };
        state.padding_face = state.default_face;
        state.status_default_face = state.default_face;

        let keyword_face = Face {
            fg: Color::Rgb { r: 255, g: 0, b: 0 },
            bg: Color::Default,
            ..Face::default()
        };
        state.lines = vec![vec![Atom {
            face: keyword_face,
            contents: "let".to_string(),
        }]];
        state.status_line = make_line("st");

        let registry = PluginRegistry::new();
        let mut grid = CellGrid::new(state.cols, state.rows);

        // Frame 1
        grid.clear(&state.default_face);
        let el = view::view(&state, &registry);
        let area = Rect {
            x: 0,
            y: 0,
            w: state.cols,
            h: state.rows,
        };
        let layout = flex::place(&el, area, &state);
        paint::paint(&el, &layout, &mut grid, &state);

        let diffs1 = grid.diff();
        // First frame: full redraw (previous is empty)
        assert!(!diffs1.is_empty(), "frame 1 should produce diffs");

        // Check "l" has RGB red fg
        let l_cell = diffs1.iter().find(|d| d.x == 0 && d.y == 0).unwrap();
        assert_eq!(
            l_cell.cell.face.fg,
            Color::Rgb { r: 255, g: 0, b: 0 },
            "frame 1: tree-sitter fg lost"
        );

        grid.swap();

        // Frame 2: same content → diff should be empty (colors retained)
        grid.clear(&state.default_face);
        let el = view::view(&state, &registry);
        let layout = flex::place(&el, area, &state);
        paint::paint(&el, &layout, &mut grid, &state);

        let diffs2 = grid.diff();
        assert!(
            diffs2.is_empty(),
            "frame 2 with same content should have empty diff, got {} diffs",
            diffs2.len()
        );

        grid.swap();

        // Frame 3: new content with different colors
        let new_face = Face {
            fg: Color::Rgb { r: 0, g: 0, b: 255 },
            bg: Color::Default,
            ..Face::default()
        };
        state.lines = vec![vec![Atom {
            face: new_face,
            contents: "let".to_string(),
        }]];

        grid.clear(&state.default_face);
        let el = view::view(&state, &registry);
        let layout = flex::place(&el, area, &state);
        paint::paint(&el, &layout, &mut grid, &state);

        let diffs3 = grid.diff();
        // Color changed → should detect diff
        let l_cell3 = diffs3.iter().find(|d| d.x == 0 && d.y == 0);
        assert!(
            l_cell3.is_some(),
            "frame 3: color change should be detected"
        );
        assert_eq!(
            l_cell3.unwrap().cell.face.fg,
            Color::Rgb { r: 0, g: 0, b: 255 },
            "frame 3: new tree-sitter color not reflected"
        );
    }

    /// Compare the buffer and status bar regions between old and new pipelines.
    /// The old pipeline uses render_frame() which includes render_buffer + render_status.
    /// The new pipeline uses view() → layout() → paint().
    #[test]
    fn test_declarative_matches_imperative_buffer_status() {
        let mut state = AppState::default();
        state.cols = 40;
        state.rows = 10;
        state.default_face = Face {
            fg: Color::Named(NamedColor::White),
            bg: Color::Named(NamedColor::Black),
            ..Face::default()
        };
        state.padding_face = Face {
            fg: Color::Named(NamedColor::Blue),
            bg: Color::Named(NamedColor::Black),
            ..Face::default()
        };
        state.lines = vec![
            make_line("first line"),
            make_line("second line"),
            make_line("third line with more text"),
        ];
        state.status_line = make_line("status text");
        state.status_mode_line = make_line("normal");
        state.status_default_face = Face {
            fg: Color::Named(NamedColor::Cyan),
            bg: Color::Default,
            ..Face::default()
        };

        // Old pipeline
        let mut grid_old = CellGrid::new(state.cols, state.rows);
        render_frame(&state, &mut grid_old);

        // New pipeline
        let mut grid_new = CellGrid::new(state.cols, state.rows);
        grid_new.clear(&state.default_face);
        let registry = PluginRegistry::new();
        let element = view::view(&state, &registry);
        let root_area = Rect {
            x: 0,
            y: 0,
            w: state.cols,
            h: state.rows,
        };
        let layout_result = flex::place(&element, root_area, &state);
        paint::paint(&element, &layout_result, &mut grid_new, &state);

        // Compare buffer rows (0..rows-1)
        let buffer_rows = state.rows.saturating_sub(1);
        for y in 0..buffer_rows {
            for x in 0..state.cols {
                let old = grid_old.get(x, y).unwrap();
                let new = grid_new.get(x, y).unwrap();
                assert_eq!(
                    old.grapheme, new.grapheme,
                    "grapheme mismatch at ({x}, {y}): old={:?} new={:?}",
                    old.grapheme, new.grapheme
                );
                assert_eq!(old.face.fg, new.face.fg, "fg mismatch at ({x}, {y})");
                assert_eq!(old.face.bg, new.face.bg, "bg mismatch at ({x}, {y})");
            }
        }

        // Compare status bar row (grapheme + fg + bg)
        let status_y = state.rows - 1;
        for x in 0..state.cols {
            let old = grid_old.get(x, status_y).unwrap();
            let new = grid_new.get(x, status_y).unwrap();
            assert_eq!(
                old.grapheme, new.grapheme,
                "status grapheme mismatch at ({x}, {status_y}): old={:?} new={:?}",
                old.grapheme, new.grapheme
            );
            assert_eq!(
                old.face.fg, new.face.fg,
                "status fg mismatch at ({x}, {status_y})"
            );
            assert_eq!(
                old.face.bg, new.face.bg,
                "status bg mismatch at ({x}, {status_y})"
            );
        }
    }
}
