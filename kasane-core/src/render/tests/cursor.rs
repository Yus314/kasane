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
    let line = vec![Atom {
        face: Face::default(),
        contents: "x".into(),
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
        contents: "s".into(),
    }];
    let mode_line = vec![Atom {
        face: Face::default(),
        contents: "m".into(),
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

    clear_block_cursor_face(&state, &mut grid, CursorStyle::Bar, 0);

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

    clear_block_cursor_face(&state, &mut grid, CursorStyle::Underline, 0);

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

    clear_block_cursor_face(&state, &mut grid, CursorStyle::Block, 0);

    let cell = grid.get(0, 0).unwrap();
    assert_eq!(cell.face, cursor_face);
}

#[test]
fn test_clear_block_cursor_face_prompt() {
    let mut state = AppState::default();
    state.cursor_mode = crate::protocol::CursorMode::Prompt;
    state.status_prompt = vec![Atom {
        face: Face::default(),
        contents: ":".into(),
    }];
    state.status_content_cursor_pos = 3;
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
    // Prompt cursor at col = prompt_width(1) + cursor_pos(3) = 4, last row = 4
    grid.put_char(4, 4, "p", &cursor_face);

    clear_block_cursor_face(&state, &mut grid, CursorStyle::Bar, 0);

    let cell = grid.get(4, 4).unwrap();
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
    clear_block_cursor_face(&state, &mut grid, CursorStyle::Bar, 0);
}

// --- make_secondary_cursor_face tests ---

fn cursor_face_white_on_black() -> Face {
    Face {
        fg: Color::Named(NamedColor::White),
        bg: Color::Named(NamedColor::Black),
        underline: Color::Default,
        attributes: Attributes::FINAL_FG | Attributes::REVERSE,
    }
}

#[test]
fn test_secondary_face_removes_reverse() {
    let cursor = cursor_face_white_on_black();
    let default = Face {
        fg: Color::Named(NamedColor::White),
        bg: Color::Named(NamedColor::Black),
        ..Face::default()
    };
    let secondary = make_secondary_cursor_face(&cursor, &default);
    assert!(
        !secondary.attributes.contains(Attributes::REVERSE),
        "REVERSE should be removed from secondary cursor face"
    );
    assert!(
        secondary.attributes.contains(Attributes::FINAL_FG),
        "FINAL_FG should be preserved"
    );
}

#[test]
fn test_secondary_face_has_blended_bg() {
    let cursor = Face {
        fg: Color::Rgb {
            r: 255,
            g: 255,
            b: 255,
        },
        bg: Color::Rgb { r: 0, g: 0, b: 0 },
        underline: Color::Default,
        attributes: Attributes::FINAL_FG | Attributes::REVERSE,
    };
    let default = Face {
        fg: Color::Rgb {
            r: 255,
            g: 255,
            b: 255,
        },
        bg: Color::Rgb { r: 0, g: 0, b: 0 },
        ..Face::default()
    };
    let secondary = make_secondary_cursor_face(&cursor, &default);

    // Cursor color (fg) = white (255,255,255), bg = black (0,0,0)
    // Blend: 0.4 * 255 + 0.6 * 0 = 102
    match secondary.bg {
        Color::Rgb { r, g, b } => {
            assert_eq!(r, 102);
            assert_eq!(g, 102);
            assert_eq!(b, 102);
        }
        _ => panic!("expected RGB bg, got {:?}", secondary.bg),
    }
}

#[test]
fn test_secondary_face_preserves_text_color() {
    let cursor = Face {
        fg: Color::Rgb {
            r: 200,
            g: 200,
            b: 200,
        },
        bg: Color::Rgb {
            r: 50,
            g: 50,
            b: 50,
        },
        underline: Color::Default,
        attributes: Attributes::FINAL_FG | Attributes::REVERSE,
    };
    let default = Face::default();
    let secondary = make_secondary_cursor_face(&cursor, &default);

    // fg should be the original bg (the text shown under REVERSE)
    assert_eq!(
        secondary.fg,
        Color::Rgb {
            r: 50,
            g: 50,
            b: 50
        }
    );
}

#[test]
fn test_apply_secondary_cursor_faces_on_grid() {
    let mut state = AppState::default();
    state.default_face = Face {
        fg: Color::Named(NamedColor::White),
        bg: Color::Named(NamedColor::Black),
        ..Face::default()
    };
    state.secondary_cursors = vec![Coord { line: 0, column: 3 }];

    let cursor_face = Face {
        fg: Color::Named(NamedColor::White),
        bg: Color::Named(NamedColor::Black),
        underline: Color::Default,
        attributes: Attributes::FINAL_FG | Attributes::REVERSE,
    };

    let mut grid = CellGrid::new(10, 5);
    grid.put_char(3, 0, "x", &cursor_face);

    apply_secondary_cursor_faces(&state, &mut grid, 0);

    let cell = grid.get(3, 0).unwrap();
    // REVERSE should be gone
    assert!(!cell.face.attributes.contains(Attributes::REVERSE));
    // bg should be a blended RGB
    assert!(matches!(cell.face.bg, Color::Rgb { .. }));
}

#[test]
fn test_apply_secondary_cursor_faces_with_offset() {
    let mut state = AppState::default();
    state.default_face = Face {
        fg: Color::Named(NamedColor::White),
        bg: Color::Named(NamedColor::Black),
        ..Face::default()
    };
    // column=2 in Kakoune coordinates, buffer starts at grid x=3
    state.secondary_cursors = vec![Coord { line: 1, column: 2 }];

    let cursor_face = Face {
        fg: Color::Named(NamedColor::White),
        bg: Color::Named(NamedColor::Black),
        underline: Color::Default,
        attributes: Attributes::FINAL_FG | Attributes::REVERSE,
    };

    let mut grid = CellGrid::new(10, 5);
    // buffer_x_offset=3, so the cell is at grid x=5
    grid.put_char(5, 1, "y", &cursor_face);

    apply_secondary_cursor_faces(&state, &mut grid, 3);

    let cell = grid.get(5, 1).unwrap();
    assert!(!cell.face.attributes.contains(Attributes::REVERSE));
}
