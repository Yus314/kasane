use super::super::test_helpers::{render_buffer, render_status};
use super::super::*;
use crate::protocol::{Atom, Attributes, Color, Coord, CursorMode, Face, NamedColor};
use crate::state::AppState;

#[test]
fn test_render_buffer_resolves_default_face() {
    let default_face = Face {
        fg: Color::Named(NamedColor::Yellow),
        bg: Color::Named(NamedColor::Blue),
        ..Face::default()
    };
    // Atom has Color::Default fg/bg — should inherit from default_face
    let line = vec![Atom::plain("x")];

    let mut state = AppState::default();
    state.observed.lines = vec![line];
    state.observed.default_style = default_face.into();

    let mut grid = CellGrid::new(10, 2);
    render_buffer(&state, &mut grid);

    let cell = grid.get(0, 0).unwrap();
    assert_eq!(cell.grapheme, "x");
    assert_eq!(cell.face().fg, Color::Named(NamedColor::Yellow));
    assert_eq!(cell.face().bg, Color::Named(NamedColor::Blue));
}

#[test]
fn test_render_status_resolves_default_face() {
    let status_face = Face {
        fg: Color::Named(NamedColor::Cyan),
        bg: Color::Named(NamedColor::Magenta),
        ..Face::default()
    };
    let status_line = vec![Atom::plain("s")];
    let mode_line = vec![Atom::plain("m")];

    let mut state = AppState::default();
    state.inference.status_line = status_line;
    state.observed.status_mode_line = mode_line;
    state.observed.status_default_style = status_face.into();

    let mut grid = CellGrid::new(10, 2);
    render_status(&state, &mut grid);

    // Status line at row 1 (last row of 2-row grid)
    let cell = grid.get(0, 1).unwrap();
    assert_eq!(cell.grapheme, "s");
    assert_eq!(cell.face().fg, Color::Named(NamedColor::Cyan));
    assert_eq!(cell.face().bg, Color::Named(NamedColor::Magenta));

    // Mode line at rightmost position
    let cell_mode = grid.get(9, 1).unwrap();
    assert_eq!(cell_mode.grapheme, "m");
    assert_eq!(cell_mode.face().fg, Color::Named(NamedColor::Cyan));
    assert_eq!(cell_mode.face().bg, Color::Named(NamedColor::Magenta));
}

#[test]
fn test_cursor_style_ui_option_bar() {
    let mut state = AppState::default();
    state
        .observed
        .ui_options
        .insert("kasane_cursor_style".into(), "bar".into());
    assert_eq!(cursor_style_default(&state), CursorStyle::Bar);
}

#[test]
fn test_cursor_style_ui_option_underline() {
    let mut state = AppState::default();
    state
        .observed
        .ui_options
        .insert("kasane_cursor_style".into(), "underline".into());
    assert_eq!(cursor_style_default(&state), CursorStyle::Underline);
}

#[test]
fn test_cursor_style_prompt_mode() {
    let mut state = AppState::default();
    state.inference.cursor_mode = CursorMode::Prompt;
    assert_eq!(cursor_style_default(&state), CursorStyle::Bar);
}

#[test]
fn test_cursor_style_insert_mode_line() {
    let mut state = AppState::default();
    state.observed.status_mode_line = vec![Atom::plain("insert")];
    assert_eq!(cursor_style_default(&state), CursorStyle::Bar);
}

#[test]
fn test_cursor_style_replace_mode_line() {
    let mut state = AppState::default();
    state.observed.status_mode_line = vec![Atom::plain("replace")];
    assert_eq!(cursor_style_default(&state), CursorStyle::Underline);
}

#[test]
fn test_cursor_style_default_block() {
    let state = AppState::default();
    assert_eq!(cursor_style_default(&state), CursorStyle::Block);
}

#[test]
fn test_cursor_style_unfocused_outline() {
    let mut state = AppState::default();
    state.runtime.focused = false;
    assert_eq!(cursor_style_default(&state), CursorStyle::Outline);
}

#[test]
fn test_cursor_style_ui_option_overrides_unfocused() {
    let mut state = AppState::default();
    state.runtime.focused = false;
    state
        .observed
        .ui_options
        .insert("kasane_cursor_style".into(), "bar".into());
    assert_eq!(cursor_style_default(&state), CursorStyle::Bar);
}

#[test]
fn test_cursor_style_ui_option_overrides_mode_line() {
    let mut state = AppState::default();
    state
        .observed
        .ui_options
        .insert("kasane_cursor_style".into(), "block".into());
    state.observed.status_mode_line = vec![Atom::plain("insert")];
    assert_eq!(cursor_style_default(&state), CursorStyle::Block);
}

// --- clear_cursor_face_at tests ---

#[test]
fn test_clear_cursor_face_at_bar() {
    let mut state = AppState::default();
    state.observed.default_style = Face {
        fg: Color::Named(NamedColor::White),
        bg: Color::Named(NamedColor::Black),
        ..Face::default()
    }
    .into();

    let mut grid = CellGrid::new(10, 5);
    let cursor_face = Face {
        fg: Color::Named(NamedColor::Black),
        bg: Color::Named(NamedColor::White),
        ..Face::default()
    };
    grid.put_char(
        2,
        0,
        "x",
        &crate::render::TerminalStyle::from_face(&cursor_face),
    );

    clear_cursor_face_at(&state, &mut grid, CursorStyle::Bar, 2, 0);

    let cell = grid.get(2, 0).unwrap();
    assert_eq!(cell.face(), state.observed.default_style.to_face());
}

#[test]
fn test_clear_cursor_face_at_underline() {
    let mut state = AppState::default();
    state.observed.default_style = Face {
        fg: Color::Named(NamedColor::Yellow),
        bg: Color::Named(NamedColor::Blue),
        ..Face::default()
    }
    .into();

    let mut grid = CellGrid::new(10, 5);
    let cursor_face = Face {
        fg: Color::Named(NamedColor::Black),
        bg: Color::Named(NamedColor::White),
        ..Face::default()
    };
    grid.put_char(
        3,
        1,
        "y",
        &crate::render::TerminalStyle::from_face(&cursor_face),
    );

    clear_cursor_face_at(&state, &mut grid, CursorStyle::Underline, 3, 1);

    let cell = grid.get(3, 1).unwrap();
    assert_eq!(cell.face(), state.observed.default_style.to_face());
}

#[test]
fn test_clear_cursor_face_at_block_noop() {
    let mut grid = CellGrid::new(10, 5);
    let cursor_face = Face {
        fg: Color::Named(NamedColor::Black),
        bg: Color::Named(NamedColor::White),
        ..Face::default()
    };
    grid.put_char(
        0,
        0,
        "z",
        &crate::render::TerminalStyle::from_face(&cursor_face),
    );

    let state = AppState::default();
    clear_cursor_face_at(&state, &mut grid, CursorStyle::Block, 0, 0);

    let cell = grid.get(0, 0).unwrap();
    assert_eq!(cell.face(), cursor_face);
}

#[test]
fn test_clear_cursor_face_at_prompt() {
    let mut state = AppState::default();
    state.inference.cursor_mode = crate::protocol::CursorMode::Prompt;
    state.observed.status_default_style = Face {
        fg: Color::Named(NamedColor::Cyan),
        bg: Color::Named(NamedColor::Magenta),
        ..Face::default()
    }
    .into();

    let mut grid = CellGrid::new(10, 5);
    let cursor_face = Face {
        fg: Color::Named(NamedColor::Black),
        bg: Color::Named(NamedColor::White),
        ..Face::default()
    };
    grid.put_char(
        4,
        4,
        "p",
        &crate::render::TerminalStyle::from_face(&cursor_face),
    );

    clear_cursor_face_at(&state, &mut grid, CursorStyle::Bar, 4, 4);

    let cell = grid.get(4, 4).unwrap();
    assert_eq!(cell.face(), state.observed.status_default_style.to_face());
}

#[test]
fn test_clear_cursor_face_at_out_of_bounds() {
    let state = AppState::default();
    let mut grid = CellGrid::new(10, 5);
    // Should not panic
    clear_cursor_face_at(&state, &mut grid, CursorStyle::Bar, 100, 100);
}

// --- make_secondary_cursor_style tests ---

use crate::protocol::{Brush, Style};

fn cursor_style_white_on_black() -> Style {
    Style {
        fg: Brush::rgb(255, 255, 255),
        bg: Brush::rgb(0, 0, 0),
        reverse: true,
        ..Style::default()
    }
}

#[test]
fn test_secondary_style_removes_reverse() {
    let cursor = cursor_style_white_on_black();
    let default = Style {
        fg: Brush::rgb(255, 255, 255),
        bg: Brush::rgb(0, 0, 0),
        ..Style::default()
    };
    let secondary = make_secondary_cursor_style(&cursor, &default, 0.4);
    assert!(
        !secondary.reverse,
        "reverse should be removed from secondary cursor style"
    );
}

#[test]
fn test_secondary_style_has_blended_bg() {
    let cursor = Style {
        fg: Brush::rgb(255, 255, 255),
        bg: Brush::rgb(0, 0, 0),
        reverse: true,
        ..Style::default()
    };
    let default = Style {
        fg: Brush::rgb(255, 255, 255),
        bg: Brush::rgb(0, 0, 0),
        ..Style::default()
    };
    let secondary = make_secondary_cursor_style(&cursor, &default, 0.4);

    // Cursor highlight (fg) = white (255), bg = black (0)
    // linear_blend white→black with ratio 0.4 = round(255*0.4 + 0*0.6) = 102
    match secondary.bg {
        Brush::Solid([r, g, b, a]) => {
            assert_eq!((r, g, b, a), (102, 102, 102, 0xff));
        }
        other => panic!("expected Solid bg, got {other:?}"),
    }
}

#[test]
fn test_secondary_style_preserves_text_color() {
    let cursor = Style {
        fg: Brush::rgb(200, 200, 200),
        bg: Brush::rgb(50, 50, 50),
        reverse: true,
        ..Style::default()
    };
    let default = Style::default();
    let secondary = make_secondary_cursor_style(&cursor, &default, 0.4);

    // fg should be the original bg (the text shown under REVERSE)
    assert_eq!(secondary.fg, Brush::rgb(50, 50, 50));
}

#[test]
fn test_apply_secondary_cursor_faces_on_grid() {
    let mut state = AppState::default();
    state.observed.default_style = Face {
        fg: Color::Named(NamedColor::White),
        bg: Color::Named(NamedColor::Black),
        ..Face::default()
    }
    .into();
    state.inference.secondary_cursors = vec![Coord { line: 0, column: 3 }];

    let cursor_face = Face {
        fg: Color::Named(NamedColor::White),
        bg: Color::Named(NamedColor::Black),
        underline: Color::Default,
        attributes: Attributes::FINAL_FG | Attributes::REVERSE,
    };

    let mut grid = CellGrid::new(10, 5);
    grid.put_char(
        3,
        0,
        "x",
        &crate::render::TerminalStyle::from_face(&cursor_face),
    );

    apply_secondary_cursor_faces(&state, &mut grid, 0, None, 0, 0, None);

    let cell = grid.get(3, 0).unwrap();
    // REVERSE should be gone
    assert!(!cell.face().attributes.contains(Attributes::REVERSE));
    // bg should be a blended RGB
    assert!(matches!(cell.face().bg, Color::Rgb { .. }));
}

#[test]
fn test_apply_secondary_cursor_faces_with_offset() {
    let mut state = AppState::default();
    state.observed.default_style = Face {
        fg: Color::Named(NamedColor::White),
        bg: Color::Named(NamedColor::Black),
        ..Face::default()
    }
    .into();
    // column=2 in Kakoune coordinates, buffer starts at grid x=3
    state.inference.secondary_cursors = vec![Coord { line: 1, column: 2 }];

    let cursor_face = Face {
        fg: Color::Named(NamedColor::White),
        bg: Color::Named(NamedColor::Black),
        underline: Color::Default,
        attributes: Attributes::FINAL_FG | Attributes::REVERSE,
    };

    let mut grid = CellGrid::new(10, 5);
    // buffer_x_offset=3, so the cell is at grid x=5
    grid.put_char(
        5,
        1,
        "y",
        &crate::render::TerminalStyle::from_face(&cursor_face),
    );

    apply_secondary_cursor_faces(&state, &mut grid, 3, None, 0, 0, None);

    let cell = grid.get(5, 1).unwrap();
    assert!(!cell.face().attributes.contains(Attributes::REVERSE));
}
