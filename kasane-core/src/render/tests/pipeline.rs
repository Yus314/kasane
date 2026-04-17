use super::super::test_helpers::render_frame;
use super::super::*;
use crate::layout::Rect;
use crate::layout::flex;
use crate::plugin::PluginRuntime;
use crate::protocol::{Atom, Color, Face, NamedColor};
use crate::state::AppState;
use crate::test_utils::make_line;

/// Tree-sitter colors: verify that explicit RGB colors in atoms survive the
/// full declarative pipeline (view → layout → paint) and match the old pipeline.
#[test]
fn test_treesitter_rgb_colors_preserved() {
    let mut state = AppState::default();
    state.runtime.cols = 40;
    state.runtime.rows = 5;
    state.observed.default_face = Face {
        fg: Color::Named(NamedColor::White),
        bg: Color::Named(NamedColor::Black),
        ..Face::default()
    };
    state.observed.padding_face = state.observed.default_face;
    state.observed.status_default_face = state.observed.default_face;

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
    state.observed.lines = vec![ts_line];
    state.inference.status_line = make_line("status");

    // Old pipeline
    let mut grid_old = CellGrid::new(state.runtime.cols, state.runtime.rows);
    render_frame(&state, &mut grid_old);

    // New pipeline
    let mut grid_new = CellGrid::new(state.runtime.cols, state.runtime.rows);
    grid_new.clear(&state.observed.default_face);
    let registry = PluginRuntime::new();
    let element = view::view(&state, &registry.view());
    let root_area = Rect {
        x: 0,
        y: 0,
        w: state.runtime.cols,
        h: state.runtime.rows,
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
    for y in 0..state.runtime.rows {
        for x in 0..state.runtime.cols {
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
    state.runtime.cols = 20;
    state.runtime.rows = 3;
    state.observed.default_face = Face {
        fg: Color::Named(NamedColor::White),
        bg: Color::Named(NamedColor::Black),
        ..Face::default()
    };
    state.observed.padding_face = state.observed.default_face;
    state.observed.status_default_face = state.observed.default_face;

    let keyword_face = Face {
        fg: Color::Rgb { r: 255, g: 0, b: 0 },
        bg: Color::Default,
        ..Face::default()
    };
    state.observed.lines = vec![vec![Atom {
        face: keyword_face,
        contents: "let".into(),
    }]];
    state.inference.status_line = make_line("st");

    let registry = PluginRuntime::new();
    let mut grid = CellGrid::new(state.runtime.cols, state.runtime.rows);

    // Frame 1
    grid.clear(&state.observed.default_face);
    let el = view::view(&state, &registry.view());
    let area = Rect {
        x: 0,
        y: 0,
        w: state.runtime.cols,
        h: state.runtime.rows,
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
    grid.clear(&state.observed.default_face);
    let el = view::view(&state, &registry.view());
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
    state.observed.lines = vec![vec![Atom {
        face: new_face,
        contents: "let".into(),
    }]];

    grid.clear(&state.observed.default_face);
    let el = view::view(&state, &registry.view());
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

/// Stage P3: Verify line_dirty optimization works with BUFFER|STATUS.
#[test]
fn test_line_dirty_buffer_and_status() {
    use crate::state::DirtyFlags;

    let mut state = AppState::default();
    state.runtime.cols = 20;
    state.runtime.rows = 5;
    state.observed.default_face = Face {
        fg: Color::Named(NamedColor::White),
        bg: Color::Named(NamedColor::Black),
        ..Face::default()
    };
    state.observed.padding_face = state.observed.default_face;
    state.observed.status_default_face = state.observed.default_face;
    state.observed.lines = vec![
        make_line("line0"),
        make_line("line1"),
        make_line("line2"),
        make_line("line3"),
    ];
    state.inference.status_line = make_line("status");

    let registry = PluginRuntime::new();
    let mut grid = CellGrid::new(state.runtime.cols, state.runtime.rows);

    // Frame 1: full render (first frame — swap_with_dirty falls back to swap)
    render_pipeline(&state, &registry.view(), &mut grid);
    grid.swap_with_dirty();

    // Frame 2: identical content — populates both current and previous properly
    render_pipeline(&state, &registry.view(), &mut grid);
    grid.swap_with_dirty();
    // Now swap_with_dirty preserved current (it has content from frame 2)

    // Frame 3: change only line 1 and status, with BUFFER|STATUS dirty
    state.observed.lines[1] = make_line("CHANGED");
    state.inference.status_line = make_line("new_st");
    state.inference.lines_dirty = vec![false, true, false, false];

    render_pipeline_direct(
        &state,
        &registry.view(),
        &mut grid,
        DirtyFlags::BUFFER | DirtyFlags::STATUS,
    );

    // Verify: line 0 should still have old content (clean row preserved)
    assert_eq!(grid.get(0, 0).unwrap().grapheme, "l");
    assert_eq!(grid.get(4, 0).unwrap().grapheme, "0");

    // Changed line should have new content
    assert_eq!(grid.get(0, 1).unwrap().grapheme, "C");

    // Status bar should be repainted
    let status_y = state.runtime.rows - 1;
    assert_eq!(grid.get(0, status_y).unwrap().grapheme, "n");
}

/// Stage P3: Verify BUFFER-only still works identically (regression test).
#[test]
fn test_line_dirty_buffer_only_regression() {
    use crate::state::DirtyFlags;

    let mut state = AppState::default();
    state.runtime.cols = 20;
    state.runtime.rows = 5;
    state.observed.default_face = Face {
        fg: Color::Named(NamedColor::White),
        bg: Color::Named(NamedColor::Black),
        ..Face::default()
    };
    state.observed.padding_face = state.observed.default_face;
    state.observed.status_default_face = state.observed.default_face;
    state.observed.lines = vec![
        make_line("line0"),
        make_line("line1"),
        make_line("line2"),
        make_line("line3"),
    ];
    state.inference.status_line = make_line("status");

    let registry = PluginRuntime::new();
    let mut grid = CellGrid::new(state.runtime.cols, state.runtime.rows);

    // Frame 1: full render (first frame)
    render_pipeline(&state, &registry.view(), &mut grid);
    grid.swap_with_dirty();

    // Frame 2: identical — establishes current with valid content
    render_pipeline(&state, &registry.view(), &mut grid);
    grid.swap_with_dirty();

    // Frame 3: change only line 2, BUFFER dirty only
    state.observed.lines[2] = make_line("EDIT2");
    state.inference.lines_dirty = vec![false, false, true, false];

    render_pipeline_direct(&state, &registry.view(), &mut grid, DirtyFlags::BUFFER);

    // Clean lines preserved
    assert_eq!(grid.get(0, 0).unwrap().grapheme, "l");
    assert_eq!(grid.get(0, 1).unwrap().grapheme, "l");

    // Changed line updated
    assert_eq!(grid.get(0, 2).unwrap().grapheme, "E");
}

/// Compare the buffer and status bar regions between old and new pipelines.
/// The old pipeline uses render_frame() which includes render_buffer + render_status.
/// The new pipeline uses view() → layout() → paint().
#[test]
fn test_declarative_matches_imperative_buffer_status() {
    let mut state = AppState::default();
    state.runtime.cols = 40;
    state.runtime.rows = 10;
    state.observed.default_face = Face {
        fg: Color::Named(NamedColor::White),
        bg: Color::Named(NamedColor::Black),
        ..Face::default()
    };
    state.observed.padding_face = Face {
        fg: Color::Named(NamedColor::Blue),
        bg: Color::Named(NamedColor::Black),
        ..Face::default()
    };
    state.observed.lines = vec![
        make_line("first line"),
        make_line("second line"),
        make_line("third line with more text"),
    ];
    state.inference.status_line = make_line("status text");
    state.observed.status_mode_line = make_line("normal");
    state.observed.status_default_face = Face {
        fg: Color::Named(NamedColor::Cyan),
        bg: Color::Default,
        ..Face::default()
    };

    // Old pipeline
    let mut grid_old = CellGrid::new(state.runtime.cols, state.runtime.rows);
    render_frame(&state, &mut grid_old);

    // New pipeline
    let mut grid_new = CellGrid::new(state.runtime.cols, state.runtime.rows);
    grid_new.clear(&state.observed.default_face);
    let registry = PluginRuntime::new();
    let element = view::view(&state, &registry.view());
    let root_area = Rect {
        x: 0,
        y: 0,
        w: state.runtime.cols,
        h: state.runtime.rows,
    };
    let layout_result = flex::place(&element, root_area, &state);
    paint::paint(&element, &layout_result, &mut grid_new, &state);

    // Compare buffer rows (0..rows-1)
    let buffer_rows = state.available_height();
    for y in 0..buffer_rows {
        for x in 0..state.runtime.cols {
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
    let status_y = state.runtime.rows - 1;
    for x in 0..state.runtime.cols {
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
