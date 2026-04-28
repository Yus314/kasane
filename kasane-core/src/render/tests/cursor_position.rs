//! End-to-end tests for cursor position accuracy.
//!
//! Verifies that `RenderResult.cursor_x/cursor_y` from the full rendering
//! pipeline points to the correct cell in the CellGrid.
//!
//! Context: <https://github.com/Yus314/kasane/issues/58>

use super::super::*;
use crate::display::DisplayMapRef;
use crate::layout::line_display_width;
use crate::plugin::PluginRuntime;
use crate::protocol::{Atom, Attributes, Color, Coord, CursorMode, Face, NamedColor};
use crate::render::cursor;
use crate::state::AppState;
use crate::test_support::test_state_80x24;
use crate::test_utils::make_line;

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Build a cursor face matching Kakoune's typical REVERSE + FINAL_FG + FINAL_BG.
fn cursor_face() -> Face {
    Face {
        fg: Color::Named(NamedColor::Black),
        bg: Color::Named(NamedColor::White),
        underline: Color::Default,
        attributes: Attributes::REVERSE | Attributes::FINAL_FG | Attributes::FINAL_BG,
    }
}

/// Build a line with a cursor on a specific character, mimicking Kakoune's atom
/// structure: `[pre_text] [cursor_char (with REVERSE face)] [post_text]`.
fn make_cursor_line(pre: &str, cursor_char: &str, post: &str) -> Vec<Atom> {
    let normal = Face::default();
    let mut atoms = Vec::new();
    if !pre.is_empty() {
        atoms.push(Atom::from_face(normal, pre));
    }
    atoms.push(Atom::from_face(cursor_face(), cursor_char));
    if !post.is_empty() {
        atoms.push(Atom::from_face(normal, post));
    }
    atoms
}

/// Set up a minimal state for buffer-mode cursor testing.
fn buffer_state(cols: u16, rows: u16) -> AppState {
    let mut state = test_state_80x24();
    state.runtime.cols = cols;
    state.runtime.rows = rows;
    state.inference.cursor_mode = CursorMode::Buffer;
    state.inference.status_line = make_line("");
    state
}

/// Run the full pipeline and return (grid, result, display_map).
fn render_full(state: &AppState) -> (CellGrid, RenderResult, DisplayMapRef) {
    let registry = PluginRuntime::new();
    let mut grid = CellGrid::new(state.runtime.cols, state.runtime.rows);
    grid.clear(&state.observed.default_style.to_face());
    let (result, dm) = pipeline::render_pipeline(state, &registry.view(), &mut grid);
    (grid, result, dm)
}

// ---------------------------------------------------------------------------
// Buffer mode — basic ASCII
// ---------------------------------------------------------------------------

/// Cursor on 'w' of "hello world" at column 6.
#[test]
fn buffer_cursor_ascii_mid_line() {
    let mut state = buffer_state(40, 5);
    state.observed.cursor_pos = Coord { line: 0, column: 6 };
    state.observed.lines = vec![make_cursor_line("hello ", "w", "orld\n")];

    let (grid, result, _) = render_full(&state);

    assert_eq!(result.cursor_x, 6, "cursor_x should be 6");
    assert_eq!(result.cursor_y, 0, "cursor_y should be 0");
    let cell = grid.get(result.cursor_x, result.cursor_y).unwrap();
    assert_eq!(cell.grapheme, "w", "cursor should point to 'w'");
}

/// Cursor on 'h' at column 0 (beginning of line).
#[test]
fn buffer_cursor_ascii_start_of_line() {
    let mut state = buffer_state(40, 5);
    state.observed.cursor_pos = Coord { line: 0, column: 0 };
    state.observed.lines = vec![make_cursor_line("", "h", "ello world\n")];

    let (grid, result, _) = render_full(&state);

    assert_eq!(result.cursor_x, 0);
    assert_eq!(result.cursor_y, 0);
    let cell = grid.get(0, 0).unwrap();
    assert_eq!(cell.grapheme, "h");
}

/// Cursor on second line.
#[test]
fn buffer_cursor_multiline() {
    let mut state = buffer_state(40, 5);
    state.observed.cursor_pos = Coord { line: 1, column: 3 };
    state.observed.lines = vec![
        make_line("first line\n"),
        make_cursor_line("sec", "o", "nd line\n"),
    ];

    let (grid, result, _) = render_full(&state);

    assert_eq!(result.cursor_x, 3);
    assert_eq!(result.cursor_y, 1);
    let cell = grid.get(3, 1).unwrap();
    assert_eq!(cell.grapheme, "o");
}

/// Cursor at end of short line.
#[test]
fn buffer_cursor_end_of_line() {
    let mut state = buffer_state(40, 5);
    state.observed.cursor_pos = Coord { line: 0, column: 2 };
    state.observed.lines = vec![make_cursor_line("ab", "c", "\n")];

    let (grid, result, _) = render_full(&state);

    assert_eq!(result.cursor_x, 2);
    let cell = grid.get(2, 0).unwrap();
    assert_eq!(cell.grapheme, "c");
}

// ---------------------------------------------------------------------------
// Buffer mode — CJK / wide characters
// ---------------------------------------------------------------------------

/// Cursor on CJK character '界' after "hi世".
/// Display columns: "hi"=2, "世"=2, "界" starts at column 4.
#[test]
fn buffer_cursor_cjk_on_wide_char() {
    let mut state = buffer_state(40, 5);
    // "hi世" = 4 display columns, cursor on "界" at column 4
    state.observed.cursor_pos = Coord { line: 0, column: 4 };
    state.observed.lines = vec![make_cursor_line("hi世", "界", "\n")];

    let (grid, result, _) = render_full(&state);

    assert_eq!(result.cursor_x, 4);
    let cell = grid.get(4, 0).unwrap();
    assert_eq!(cell.grapheme, "界");
}

/// Cursor on ASCII char after CJK text.
/// "hi世界" = 6 display columns, "o" at column 6.
#[test]
fn buffer_cursor_ascii_after_cjk() {
    let mut state = buffer_state(40, 5);
    state.observed.cursor_pos = Coord { line: 0, column: 6 };
    state.observed.lines = vec![make_cursor_line("hi世界", "o", "k\n")];

    let (grid, result, _) = render_full(&state);

    assert_eq!(result.cursor_x, 6);
    let cell = grid.get(6, 0).unwrap();
    assert_eq!(cell.grapheme, "o");
}

// ---------------------------------------------------------------------------
// Buffer mode — widget columns (line numbers in atoms)
// ---------------------------------------------------------------------------

/// Kakoune with `number-lines`: atoms include " 1│" prefix, cursor_pos
/// is absolute (includes widget columns).
#[test]
fn buffer_cursor_with_widget_columns() {
    let mut state = buffer_state(40, 5);
    // " 1│" = 3 display columns, then "hello " = 6, cursor on "w" at column 9
    state.observed.cursor_pos = Coord { line: 0, column: 9 };
    state.observed.widget_columns = 3;

    let gutter_face = Face {
        fg: Color::Named(NamedColor::Yellow),
        bg: Color::Default,
        ..Face::default()
    };
    let normal = Face::default();
    state.observed.lines = vec![vec![
        Atom::from_face(gutter_face, " 1│"),
        Atom::from_face(normal, "hello "),
        Atom::from_face(cursor_face(), "w"),
        Atom::from_face(normal, "orld\n"),
    ]];

    let (grid, result, _) = render_full(&state);

    assert_eq!(
        result.cursor_x, 9,
        "cursor_x should account for widget columns"
    );
    let cell = grid.get(9, 0).unwrap();
    assert_eq!(cell.grapheme, "w");
}

// ---------------------------------------------------------------------------
// Prompt mode
// ---------------------------------------------------------------------------

/// Basic prompt ":cmd" with cursor at position 3.
#[test]
fn prompt_cursor_basic() {
    let mut state = buffer_state(40, 5);
    state.inference.cursor_mode = CursorMode::Prompt;
    state.observed.status_prompt = vec![Atom::from_face(Face::default(), ":")];
    state.observed.status_content_cursor_pos = 3;
    // Need a status line for rendering
    state.inference.status_line = vec![Atom::from_face(Face::default(), ":cmd")];

    let (_grid, result, _) = render_full(&state);

    // prompt width = 1 (":"), cursor offset = 3, total = 4
    let expected_x = line_display_width(&state.observed.status_prompt) as u16
        + state.observed.status_content_cursor_pos.max(0) as u16;
    assert_eq!(result.cursor_x, expected_x);
    // status bar is at the bottom (status_at_top = false by default)
    assert_eq!(result.cursor_y, state.runtime.rows - 1);
}

/// Prompt with CJK prefix "検索:".
#[test]
fn prompt_cursor_cjk_prefix() {
    let mut state = buffer_state(40, 5);
    state.inference.cursor_mode = CursorMode::Prompt;
    state.observed.status_prompt = vec![Atom::from_face(Face::default(), "検索:")];
    state.observed.status_content_cursor_pos = 0;
    state.inference.status_line = vec![Atom::from_face(Face::default(), "検索:")];

    let (_, result, _) = render_full(&state);

    // "検"=2 + "索"=2 + ":"=1 = 5 display columns
    assert_eq!(result.cursor_x, 5);
}

// ---------------------------------------------------------------------------
// Prompt mode — status_content_x_offset (unit test)
// ---------------------------------------------------------------------------

/// When status-left slot has width W, the prompt cursor must shift by W.
/// This tests cursor_position() directly with a non-zero status_content_x_offset.
#[test]
fn prompt_cursor_with_status_left_offset() {
    let mut state = buffer_state(40, 5);
    state.inference.cursor_mode = CursorMode::Prompt;
    state.observed.status_prompt = vec![Atom::from_face(Face::default(), ":")];
    state.observed.status_content_cursor_pos = 3;

    let grid = CellGrid::new(40, 5);

    // Without offset: cursor_x = prompt_width(1) + cursor_pos(3) = 4
    let (cx_no_offset, _) = cursor::cursor_position(&state, &grid, 0, None, 0, 0, None, None, 0);
    assert_eq!(cx_no_offset, 4);

    // With offset 8 (simulating " prompt " widget in status-left):
    // cursor_x = 8 + 1 + 3 = 12
    let (cx_with_offset, _) = cursor::cursor_position(&state, &grid, 0, None, 0, 0, None, None, 8);
    assert_eq!(cx_with_offset, 12);
}

// ---------------------------------------------------------------------------
// extract_cursor_color
// ---------------------------------------------------------------------------

/// Verify cursor color extraction for ASCII content under REVERSE face.
#[test]
fn extract_cursor_color_ascii() {
    let mut state = buffer_state(40, 5);
    state.observed.cursor_pos = Coord { line: 0, column: 5 };
    let cf = Face {
        fg: Color::Rgb { r: 255, g: 0, b: 0 },
        bg: Color::Named(NamedColor::Black),
        underline: Color::Default,
        attributes: Attributes::REVERSE | Attributes::FINAL_FG | Attributes::FINAL_BG,
    };
    state.observed.lines = vec![vec![
        Atom::from_face(Face::default(), "hello"),
        Atom::from_face(cf, "w"),
        Atom::from_face(Face::default(), "orld\n"),
    ]];

    let (_, result, _) = render_full(&state);

    // Under REVERSE, cursor visual color is face.fg
    assert_eq!(
        result.cursor_color,
        Color::Rgb { r: 255, g: 0, b: 0 },
        "cursor color should be the REVERSE face's fg (Red)"
    );
}

/// Verify cursor color extraction with CJK characters before cursor.
///
/// This tests the known bug: `extract_cursor_color` uses `chars().count()`
/// instead of display width, causing incorrect color for text with wide chars.
#[test]
fn extract_cursor_color_after_cjk() {
    let mut state = buffer_state(40, 5);
    // "hi世" = 4 chars but 4 display columns (h=1,i=1,世=2)
    // Cursor at display column 4 (the "w")
    state.observed.cursor_pos = Coord { line: 0, column: 4 };
    let cf = Face {
        fg: Color::Rgb { r: 0, g: 255, b: 0 },
        bg: Color::Named(NamedColor::Black),
        underline: Color::Default,
        attributes: Attributes::REVERSE | Attributes::FINAL_FG | Attributes::FINAL_BG,
    };
    state.observed.lines = vec![vec![
        Atom::from_face(Face::default(), "hi世"),
        Atom::from_face(cf, "w"),
        Atom::from_face(Face::default(), "orld\n"),
    ]];

    let (_, result, _) = render_full(&state);

    // "hi世" has chars().count()=3 but display width=4.
    // cursor_pos.column=4 should land in the "w" atom.
    // BUG: chars().count() walks 3, so pos=3 after first atom;
    //      column 4 >= 3, enters second atom → happens to work here because
    //      second atom starts at char pos 3 and column 4 >= 3 && < 3+1=4...
    //      but 4 < 4 is false, so it falls through!
    // With display width: pos=4 after first atom, column 4 >= 4 && < 4+1=5 → correct.
    //
    // The expected behavior after fix:
    assert_eq!(
        result.cursor_color,
        Color::Rgb { r: 0, g: 255, b: 0 },
        "cursor color should be Green (REVERSE face's fg) — \
         fails if extract_cursor_color uses chars().count() instead of display width"
    );
}

// ---------------------------------------------------------------------------
// TUI / GPU cursor position consistency
// ---------------------------------------------------------------------------

/// Verify that the TUI path (render_cached_core via render_pipeline) and the
/// GPU path (scene_render_pipeline) produce the same cursor coordinates.
#[test]
fn tui_gpu_cursor_position_consistent() {
    let mut state = buffer_state(40, 5);
    state.observed.cursor_pos = Coord { line: 1, column: 7 };
    state.observed.lines = vec![
        make_line("first line\n"),
        make_cursor_line("second ", "l", "ine\n"),
    ];

    // TUI path
    let (_, tui_result, _) = render_full(&state);

    // GPU path
    let registry = PluginRuntime::new();
    let cell_size = scene::CellSize {
        width: 10.0,
        height: 20.0,
    };
    let (_, gpu_result, _) = pipeline::scene_render_pipeline(&state, &registry.view(), cell_size);

    assert_eq!(
        tui_result.cursor_x, gpu_result.cursor_x,
        "TUI and GPU cursor_x must match"
    );
    assert_eq!(
        tui_result.cursor_y, gpu_result.cursor_y,
        "TUI and GPU cursor_y must match"
    );
}
