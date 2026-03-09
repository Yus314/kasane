use super::test_helpers::{render_buffer, render_frame, render_status};
use super::*;
use crate::layout::Rect;
use crate::layout::flex;
use crate::plugin::PluginRegistry;
use crate::protocol::{Atom, Color, Coord, Face, MenuStyle, NamedColor};
use crate::state::{AppState, DirtyFlags};
use crate::test_utils::make_line;

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
            contents: "fn".into(),
        },
        Atom {
            face: Face::default(),
            contents: " ".into(),
        },
        Atom {
            face: string_face,
            contents: "main".into(),
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
        contents: "let".into(),
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
        contents: "let".into(),
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
    let buffer_rows = state.available_height();
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

// --- ViewCache tests ---

#[test]
fn test_view_cache_invalidate_buffer_clears_base() {
    let mut cache = ViewCache::new();
    cache.base = Some(crate::element::Element::Empty);
    cache.menu_overlay = Some(None);
    cache.info_overlays = Some(vec![]);

    cache.invalidate(DirtyFlags::BUFFER);
    assert!(cache.base.is_none(), "BUFFER should clear base");
    assert!(cache.menu_overlay.is_some(), "BUFFER should preserve menu");
    assert!(cache.info_overlays.is_some(), "BUFFER should preserve info");
}

#[test]
fn test_view_cache_invalidate_menu_selection_clears_menu() {
    let mut cache = ViewCache::new();
    cache.base = Some(crate::element::Element::Empty);
    cache.menu_overlay = Some(None);
    cache.info_overlays = Some(vec![]);

    cache.invalidate(DirtyFlags::MENU_SELECTION);
    assert!(cache.base.is_some(), "MENU_SELECTION should preserve base");
    assert!(
        cache.menu_overlay.is_none(),
        "MENU_SELECTION should clear menu"
    );
    assert!(
        cache.info_overlays.is_some(),
        "MENU_SELECTION should preserve info"
    );
}

#[test]
fn test_view_cache_invalidate_all_clears_everything() {
    let mut cache = ViewCache::new();
    cache.base = Some(crate::element::Element::Empty);
    cache.menu_overlay = Some(None);
    cache.info_overlays = Some(vec![]);

    cache.invalidate(DirtyFlags::ALL);
    assert!(cache.base.is_none());
    assert!(cache.menu_overlay.is_none());
    assert!(cache.info_overlays.is_none());
}

#[test]
fn test_view_cache_invalidate_info_clears_info() {
    let mut cache = ViewCache::new();
    cache.base = Some(crate::element::Element::Empty);
    cache.menu_overlay = Some(None);
    cache.info_overlays = Some(vec![]);

    cache.invalidate(DirtyFlags::INFO);
    assert!(cache.base.is_some(), "INFO should preserve base");
    assert!(cache.menu_overlay.is_some(), "INFO should preserve menu");
    assert!(cache.info_overlays.is_none(), "INFO should clear info");
}

#[test]
fn test_view_cache_invalidate_status_clears_base() {
    let mut cache = ViewCache::new();
    cache.base = Some(crate::element::Element::Empty);
    cache.menu_overlay = Some(None);

    cache.invalidate(DirtyFlags::STATUS);
    assert!(cache.base.is_none(), "STATUS should clear base");
    assert!(cache.menu_overlay.is_some(), "STATUS should preserve menu");
}

#[test]
fn test_view_cache_invalidate_options_clears_base() {
    let mut cache = ViewCache::new();
    cache.base = Some(crate::element::Element::Empty);

    cache.invalidate(DirtyFlags::OPTIONS);
    assert!(cache.base.is_none(), "OPTIONS should clear base");
}

/// Cached view output must match fresh construction.
#[test]
fn test_view_cached_matches_fresh() {
    let mut state = AppState::default();
    state.cols = 80;
    state.rows = 24;
    state.default_face = Face {
        fg: Color::Named(NamedColor::White),
        bg: Color::Named(NamedColor::Black),
        ..Face::default()
    };
    state.padding_face = state.default_face;
    state.status_default_face = state.default_face;
    state.lines = vec![make_line("hello"), make_line("world")];
    state.status_line = make_line("status");

    let registry = PluginRegistry::new();

    // Fresh render
    let mut grid_fresh = CellGrid::new(state.cols, state.rows);
    render_pipeline(&state, &registry, &mut grid_fresh);

    // Cached render (ALL dirty — cold cache)
    let mut grid_cached = CellGrid::new(state.cols, state.rows);
    let mut cache = ViewCache::new();
    render_pipeline_cached(
        &state,
        &registry,
        &mut grid_cached,
        DirtyFlags::ALL,
        &mut cache,
    );

    for y in 0..state.rows {
        for x in 0..state.cols {
            let fresh = grid_fresh.get(x, y).unwrap();
            let cached = grid_cached.get(x, y).unwrap();
            assert_eq!(
                fresh.grapheme, cached.grapheme,
                "grapheme mismatch at ({x}, {y})"
            );
            assert_eq!(fresh.face, cached.face, "face mismatch at ({x}, {y})");
        }
    }
}

// --- SceneCache tests ---

#[test]
fn test_scene_cache_invalidate_buffer_clears_base_only() {
    let cs = scene::CellSize {
        width: 10.0,
        height: 20.0,
    };
    let mut cache = SceneCache::new();
    cache.base_commands = Some(vec![]);
    cache.menu_commands = Some(vec![]);
    cache.info_commands = Some(vec![]);
    cache.cached_cell_size = Some((cs.width.to_bits(), cs.height.to_bits()));
    cache.cached_dims = Some((80, 24));

    cache.invalidate(DirtyFlags::BUFFER, cs, 80, 24);
    assert!(cache.base_commands.is_none(), "BUFFER should clear base");
    assert!(cache.menu_commands.is_some(), "BUFFER should preserve menu");
    assert!(cache.info_commands.is_some(), "BUFFER should preserve info");
}

#[test]
fn test_scene_cache_invalidate_menu_clears_menu_only() {
    let cs = scene::CellSize {
        width: 10.0,
        height: 20.0,
    };
    let mut cache = SceneCache::new();
    cache.base_commands = Some(vec![]);
    cache.menu_commands = Some(vec![]);
    cache.info_commands = Some(vec![]);
    cache.cached_cell_size = Some((cs.width.to_bits(), cs.height.to_bits()));
    cache.cached_dims = Some((80, 24));

    cache.invalidate(DirtyFlags::MENU_SELECTION, cs, 80, 24);
    assert!(
        cache.base_commands.is_some(),
        "MENU_SELECTION should preserve base"
    );
    assert!(
        cache.menu_commands.is_none(),
        "MENU_SELECTION should clear menu"
    );
    assert!(
        cache.info_commands.is_some(),
        "MENU_SELECTION should preserve info"
    );
}

#[test]
fn test_scene_cache_invalidate_info_clears_info_only() {
    let cs = scene::CellSize {
        width: 10.0,
        height: 20.0,
    };
    let mut cache = SceneCache::new();
    cache.base_commands = Some(vec![]);
    cache.menu_commands = Some(vec![]);
    cache.info_commands = Some(vec![]);
    cache.cached_cell_size = Some((cs.width.to_bits(), cs.height.to_bits()));
    cache.cached_dims = Some((80, 24));

    cache.invalidate(DirtyFlags::INFO, cs, 80, 24);
    assert!(cache.base_commands.is_some(), "INFO should preserve base");
    assert!(cache.menu_commands.is_some(), "INFO should preserve menu");
    assert!(cache.info_commands.is_none(), "INFO should clear info");
}

#[test]
fn test_scene_cache_cell_size_change_clears_all() {
    let cs1 = scene::CellSize {
        width: 10.0,
        height: 20.0,
    };
    let cs2 = scene::CellSize {
        width: 12.0,
        height: 24.0,
    };
    let mut cache = SceneCache::new();
    cache.base_commands = Some(vec![]);
    cache.menu_commands = Some(vec![]);
    cache.info_commands = Some(vec![]);
    cache.cached_cell_size = Some((cs1.width.to_bits(), cs1.height.to_bits()));
    cache.cached_dims = Some((80, 24));

    // Even with empty dirty flags, a cell size change should clear everything
    cache.invalidate(DirtyFlags::empty(), cs2, 80, 24);
    assert!(
        cache.base_commands.is_none(),
        "cell size change should clear base"
    );
    assert!(
        cache.menu_commands.is_none(),
        "cell size change should clear menu"
    );
    assert!(
        cache.info_commands.is_none(),
        "cell size change should clear info"
    );
}

#[test]
fn test_scene_cache_dims_change_clears_all() {
    let cs = scene::CellSize {
        width: 10.0,
        height: 20.0,
    };
    let mut cache = SceneCache::new();
    cache.base_commands = Some(vec![]);
    cache.menu_commands = Some(vec![]);
    cache.info_commands = Some(vec![]);
    cache.cached_cell_size = Some((cs.width.to_bits(), cs.height.to_bits()));
    cache.cached_dims = Some((80, 24));

    cache.invalidate(DirtyFlags::empty(), cs, 100, 30);
    assert!(
        cache.base_commands.is_none(),
        "dims change should clear base"
    );
    assert!(
        cache.menu_commands.is_none(),
        "dims change should clear menu"
    );
    assert!(
        cache.info_commands.is_none(),
        "dims change should clear info"
    );
}

#[test]
fn test_scene_cache_output_matches_uncached() {
    use super::scene_render_pipeline;
    use super::scene_render_pipeline_scene_cached;

    let mut state = AppState::default();
    state.cols = 80;
    state.rows = 24;
    state.default_face = Face {
        fg: Color::Named(NamedColor::White),
        bg: Color::Named(NamedColor::Black),
        ..Face::default()
    };
    state.padding_face = state.default_face;
    state.status_default_face = state.default_face;
    state.lines = vec![make_line("hello"), make_line("world")];
    state.status_line = make_line("status");

    let registry = PluginRegistry::new();
    let cs = scene::CellSize {
        width: 10.0,
        height: 20.0,
    };

    // Uncached (reference)
    let (expected, _) = scene_render_pipeline(&state, &registry, cs);

    // Cached (cold — DirtyFlags::ALL)
    let mut view_cache = ViewCache::new();
    let mut scene_cache = SceneCache::new();
    let (actual, _) = scene_render_pipeline_scene_cached(
        &state,
        &registry,
        cs,
        DirtyFlags::ALL,
        &mut view_cache,
        &mut scene_cache,
    );

    assert_eq!(
        expected,
        actual.to_vec(),
        "scene_cached output must match uncached for same state"
    );
}

#[test]
fn test_scene_cache_warm_matches_cold() {
    use super::scene_render_pipeline_scene_cached;

    let mut state = AppState::default();
    state.cols = 80;
    state.rows = 24;
    state.default_face = Face {
        fg: Color::Named(NamedColor::White),
        bg: Color::Named(NamedColor::Black),
        ..Face::default()
    };
    state.padding_face = state.default_face;
    state.status_default_face = state.default_face;
    state.lines = vec![make_line("hello"), make_line("world")];
    state.status_line = make_line("status");

    let registry = PluginRegistry::new();
    let cs = scene::CellSize {
        width: 10.0,
        height: 20.0,
    };

    // Cold render
    let mut view_cache = ViewCache::new();
    let mut scene_cache = SceneCache::new();
    let (cold, _) = scene_render_pipeline_scene_cached(
        &state,
        &registry,
        cs,
        DirtyFlags::ALL,
        &mut view_cache,
        &mut scene_cache,
    );
    let cold = cold.to_vec();

    // Warm render (empty dirty)
    let (warm, _) = scene_render_pipeline_scene_cached(
        &state,
        &registry,
        cs,
        DirtyFlags::empty(),
        &mut view_cache,
        &mut scene_cache,
    );

    assert_eq!(
        cold,
        warm.to_vec(),
        "warm cache must produce identical commands to cold cache"
    );
}

#[test]
fn test_scene_cache_menu_select_preserves_base() {
    use super::scene_render_pipeline_scene_cached;

    let mut state = AppState::default();
    state.cols = 80;
    state.rows = 24;
    state.default_face = Face {
        fg: Color::Named(NamedColor::White),
        bg: Color::Named(NamedColor::Black),
        ..Face::default()
    };
    state.padding_face = state.default_face;
    state.status_default_face = state.default_face;
    state.lines = vec![make_line("hello")];
    state.status_line = make_line("status");

    // Show menu
    state.apply(crate::protocol::KakouneRequest::MenuShow {
        items: vec![make_line("item1"), make_line("item2"), make_line("item3")],
        anchor: Coord { line: 1, column: 0 },
        selected_item_face: Face::default(),
        menu_face: Face::default(),
        style: MenuStyle::Inline,
    });

    let registry = PluginRegistry::new();
    let cs = scene::CellSize {
        width: 10.0,
        height: 20.0,
    };

    // Initial render
    let mut view_cache = ViewCache::new();
    let mut scene_cache = SceneCache::new();
    scene_render_pipeline_scene_cached(
        &state,
        &registry,
        cs,
        DirtyFlags::ALL,
        &mut view_cache,
        &mut scene_cache,
    );

    // Verify base is cached
    assert!(
        scene_cache.base_commands.is_some(),
        "base should be cached after initial render"
    );

    // Select item
    state.apply(crate::protocol::KakouneRequest::MenuSelect { selected: 1 });

    // Render with MENU_SELECTION only
    scene_render_pipeline_scene_cached(
        &state,
        &registry,
        cs,
        DirtyFlags::MENU_SELECTION,
        &mut view_cache,
        &mut scene_cache,
    );

    assert!(
        scene_cache.base_commands.is_some(),
        "base should remain cached on MENU_SELECTION"
    );
}

#[test]
fn test_scene_cache_overlay_ordering_with_menu_and_info() {
    use super::scene_render_pipeline;
    use super::scene_render_pipeline_scene_cached;

    let mut state = AppState::default();
    state.cols = 80;
    state.rows = 24;
    state.default_face = Face {
        fg: Color::Named(NamedColor::White),
        bg: Color::Named(NamedColor::Black),
        ..Face::default()
    };
    state.padding_face = state.default_face;
    state.status_default_face = state.default_face;
    state.lines = vec![make_line("hello"), make_line("world")];
    state.status_line = make_line("status");

    // Show menu
    state.apply(crate::protocol::KakouneRequest::MenuShow {
        items: vec![make_line("item1"), make_line("item2")],
        anchor: Coord { line: 1, column: 0 },
        selected_item_face: Face::default(),
        menu_face: Face::default(),
        style: MenuStyle::Inline,
    });

    // Show info
    state.apply(crate::protocol::KakouneRequest::InfoShow {
        title: make_line("Info Title"),
        content: vec![make_line("info content")],
        anchor: Coord { line: 0, column: 0 },
        face: Face::default(),
        style: crate::protocol::InfoStyle::Prompt,
    });

    let registry = PluginRegistry::new();
    let cs = scene::CellSize {
        width: 10.0,
        height: 20.0,
    };

    // Uncached
    let (uncached, _) = scene_render_pipeline(&state, &registry, cs);
    let uncached_overlay_count = uncached
        .iter()
        .filter(|c| matches!(c, DrawCommand::BeginOverlay))
        .count();

    // Cached
    let mut view_cache = ViewCache::new();
    let mut scene_cache = SceneCache::new();
    let (cached, _) = scene_render_pipeline_scene_cached(
        &state,
        &registry,
        cs,
        DirtyFlags::ALL,
        &mut view_cache,
        &mut scene_cache,
    );
    let cached_overlay_count = cached
        .iter()
        .filter(|c| matches!(c, DrawCommand::BeginOverlay))
        .count();

    // Both should have the same number of BeginOverlay markers
    assert_eq!(
        uncached_overlay_count, cached_overlay_count,
        "BeginOverlay count must match: uncached={uncached_overlay_count}, cached={cached_overlay_count}"
    );
    // Menu + info = at least 2 overlays
    assert!(
        cached_overlay_count >= 2,
        "expected at least 2 overlays (menu + info), got {cached_overlay_count}"
    );
}

/// MenuSelect-only dirty should keep base cached and rebuild menu.
#[test]
fn test_view_cache_menu_select_reuses_base() {
    let mut state = AppState::default();
    state.cols = 80;
    state.rows = 24;
    state.default_face = Face {
        fg: Color::Named(NamedColor::White),
        bg: Color::Named(NamedColor::Black),
        ..Face::default()
    };
    state.padding_face = state.default_face;
    state.status_default_face = state.default_face;
    state.lines = vec![make_line("hello")];
    state.status_line = make_line("status");

    // Show menu
    state.apply(crate::protocol::KakouneRequest::MenuShow {
        items: vec![make_line("item1"), make_line("item2"), make_line("item3")],
        anchor: Coord { line: 1, column: 0 },
        selected_item_face: Face::default(),
        menu_face: Face::default(),
        style: MenuStyle::Inline,
    });

    let registry = PluginRegistry::new();
    let mut cache = ViewCache::new();

    // Initial render (ALL dirty)
    let mut grid = CellGrid::new(state.cols, state.rows);
    render_pipeline_cached(&state, &registry, &mut grid, DirtyFlags::ALL, &mut cache);
    assert!(cache.base.is_some(), "base should be cached after render");

    // Select item
    state.apply(crate::protocol::KakouneRequest::MenuSelect { selected: 1 });

    // Render with MENU_SELECTION only — base should stay cached
    render_pipeline_cached(
        &state,
        &registry,
        &mut grid,
        DirtyFlags::MENU_SELECTION,
        &mut cache,
    );
    assert!(
        cache.base.is_some(),
        "base should remain cached on MENU_SELECTION"
    );
}
