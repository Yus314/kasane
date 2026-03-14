//! Integration tests for the full rendering pipeline:
//!   JSON-RPC parse → State update → view() → layout → paint() → CellGrid
//!
//! These tests exercise the end-to-end flow to catch regressions in the
//! interaction between subsystems, complementing unit tests within each module.

use kasane_core::layout::Rect;
use kasane_core::layout::flex::place;
use kasane_core::plugin::PluginRegistry;
use kasane_core::protocol::{
    Atom, Color, Coord, Face, InfoStyle, KakouneRequest, Line, MenuStyle, NamedColor,
};
use kasane_core::render::CellGrid;
use kasane_core::render::paint;
use kasane_core::render::view;
use kasane_core::state::{AppState, DirtyFlags, Msg, update};
use kasane_core::test_support::make_line;

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Set up a standard 80x24 state with given buffer lines.
fn setup_state(lines: Vec<Line>) -> AppState {
    let mut state = kasane_core::test_support::test_state_80x24();
    state.lines = lines;
    state.status_default_face = state.default_face;
    state.status_line = make_line(" main.rs ");
    state.status_mode_line = make_line("normal");
    state
}

/// Run the full pipeline: view → place → paint, returning the grid.
fn render(state: &AppState) -> CellGrid {
    let registry = PluginRegistry::new();
    let element = view::view(state, &registry);
    let root = Rect {
        x: 0,
        y: 0,
        w: state.cols,
        h: state.rows,
    };
    let layout = place(&element, root, state);
    let mut grid = CellGrid::new(state.cols, state.rows);
    grid.clear(&state.default_face);
    paint::paint(&element, &layout, &mut grid, state);
    grid
}

/// Extract a row from the grid as a string (trimming trailing spaces).
fn row_text(grid: &CellGrid, y: u16) -> String {
    let mut s = String::new();
    for x in 0..grid.width() {
        if let Some(cell) = grid.get(x, y)
            && cell.width > 0
        {
            s.push_str(&cell.grapheme);
        }
    }
    s.trim_end().to_string()
}

// ===========================================================================
// Basic buffer rendering
// ===========================================================================

#[test]
fn basic_buffer_draw() {
    let state = setup_state(vec![make_line("hello world"), make_line("second line")]);
    let grid = render(&state);

    assert_eq!(row_text(&grid, 0), "hello world");
    assert_eq!(row_text(&grid, 1), "second line");
    // Padding rows show tilde
    assert!(row_text(&grid, 2).starts_with('~'));
}

#[test]
fn empty_buffer_shows_padding() {
    let state = setup_state(vec![]);
    let grid = render(&state);

    // All buffer rows (0..23) should be padding
    for y in 0..23 {
        assert!(
            row_text(&grid, y).starts_with('~'),
            "row {y} should be padding, got: {:?}",
            row_text(&grid, y),
        );
    }
}

#[test]
fn buffer_with_colored_atoms() {
    let red = Color::Rgb { r: 255, g: 0, b: 0 };
    let line = vec![
        Atom {
            face: Face {
                fg: red,
                ..Face::default()
            },
            contents: "red".into(),
        },
        Atom {
            face: Face::default(),
            contents: " plain".into(),
        },
    ];
    let state = setup_state(vec![line]);
    let grid = render(&state);

    // Text content correct
    assert_eq!(row_text(&grid, 0), "red plain");
    // First cell inherits red foreground
    let cell = grid.get(0, 0).unwrap();
    assert_eq!(cell.face.fg, red);
}

// ===========================================================================
// Status bar
// ===========================================================================

#[test]
fn status_bar_rendered_at_bottom() {
    let state = setup_state(vec![make_line("buffer")]);
    let grid = render(&state);

    // Status bar is the last row (row 23 for 24-row terminal)
    let status = row_text(&grid, 23);
    assert!(
        status.contains("main.rs"),
        "status bar should contain filename, got: {status:?}"
    );
    assert!(
        status.contains("normal"),
        "status bar should contain mode, got: {status:?}"
    );
}

#[test]
fn status_bar_at_top() {
    let mut state = setup_state(vec![make_line("buffer")]);
    state.status_at_top = true;
    let grid = render(&state);

    // When status_at_top, row 0 is status bar
    let status = row_text(&grid, 0);
    assert!(
        status.contains("main.rs"),
        "top status bar should contain filename"
    );
    // Buffer starts at row 1
    assert_eq!(row_text(&grid, 1), "buffer");
}

// ===========================================================================
// Menu (completion) lifecycle
// ===========================================================================

#[test]
fn menu_show_and_select() {
    let mut state = setup_state(vec![make_line("fn main() {}")]);
    state.cursor_pos = Coord { line: 0, column: 3 };

    // Show inline menu
    let items = vec![make_line("foo"), make_line("bar"), make_line("baz")];
    state.apply(KakouneRequest::MenuShow {
        items,
        anchor: Coord { line: 0, column: 3 },
        selected_item_face: Face {
            fg: Color::Named(NamedColor::Black),
            bg: Color::Named(NamedColor::Cyan),
            ..Face::default()
        },
        menu_face: Face {
            fg: Color::Named(NamedColor::White),
            bg: Color::Named(NamedColor::Blue),
            ..Face::default()
        },
        style: MenuStyle::Inline,
    });
    assert!(state.menu.is_some());

    let grid = render(&state);
    // Menu items should appear somewhere in the grid
    let mut found_foo = false;
    let mut found_bar = false;
    for y in 0..state.rows {
        let text = row_text(&grid, y);
        if text.contains("foo") {
            found_foo = true;
        }
        if text.contains("bar") {
            found_bar = true;
        }
    }
    assert!(found_foo, "menu should show 'foo'");
    assert!(found_bar, "menu should show 'bar'");

    // Select item
    state.apply(KakouneRequest::MenuSelect { selected: 1 });
    assert_eq!(state.menu.as_ref().unwrap().selected, Some(1));
}

#[test]
fn menu_hide() {
    let mut state = setup_state(vec![make_line("text")]);
    state.apply(KakouneRequest::MenuShow {
        items: vec![make_line("item1"), make_line("item2")],
        anchor: Coord { line: 0, column: 0 },
        selected_item_face: Face::default(),
        menu_face: Face::default(),
        style: MenuStyle::Inline,
    });
    assert!(state.menu.is_some());

    state.apply(KakouneRequest::MenuHide);
    assert!(state.menu.is_none());
}

#[test]
fn prompt_menu_multi_column() {
    let mut state = setup_state(vec![make_line("text")]);
    let items: Vec<Line> = (0..20).map(|i| make_line(&format!("cmd_{i}"))).collect();
    state.apply(KakouneRequest::MenuShow {
        items,
        anchor: Coord { line: 0, column: 0 },
        selected_item_face: Face::default(),
        menu_face: Face::default(),
        style: MenuStyle::Prompt,
    });
    assert!(state.menu.is_some());
    let menu = state.menu.as_ref().unwrap();
    // Prompt style should have multiple columns on an 80-col screen
    assert!(menu.columns >= 1);

    // Should render without panic
    let _grid = render(&state);
}

// ===========================================================================
// Info popup
// ===========================================================================

#[test]
fn info_show_and_hide() {
    let mut state = setup_state(vec![make_line("code")]);

    let flags = state.apply(KakouneRequest::InfoShow {
        title: make_line("Help"),
        content: vec![make_line("This is help text")],
        anchor: Coord { line: 0, column: 0 },
        face: Face::default(),
        style: InfoStyle::Inline,
    });
    assert!(flags.contains(DirtyFlags::INFO));
    assert_eq!(state.infos.len(), 1);

    let grid = render(&state);
    // Info content should appear in the grid
    let mut found = false;
    for y in 0..state.rows {
        if row_text(&grid, y).contains("help text") {
            found = true;
            break;
        }
    }
    assert!(found, "info popup content should be visible");

    // Hide
    state.apply(KakouneRequest::InfoHide);
    assert!(state.infos.is_empty());
}

#[test]
fn multiple_infos_coexist() {
    let mut state = setup_state(vec![make_line("code line here")]);

    // Two infos at different anchors/styles should coexist
    state.apply(KakouneRequest::InfoShow {
        title: make_line("Lint"),
        content: vec![make_line("error: unused var")],
        anchor: Coord { line: 0, column: 0 },
        face: Face::default(),
        style: InfoStyle::Inline,
    });
    state.apply(KakouneRequest::InfoShow {
        title: make_line("Doc"),
        content: vec![make_line("fn doc text")],
        anchor: Coord {
            line: 0,
            column: 10,
        },
        face: Face::default(),
        style: InfoStyle::Modal,
    });
    assert_eq!(state.infos.len(), 2);

    // Should render without panic
    let _grid = render(&state);
}

// ===========================================================================
// Resize
// ===========================================================================

#[test]
fn resize_updates_grid() {
    let mut state = setup_state(vec![make_line("hello")]);
    let mut grid = CellGrid::new(state.cols, state.rows);
    let mut registry = PluginRegistry::new();

    let (flags, _cmds) = update(
        &mut state,
        Msg::Resize {
            cols: 120,
            rows: 40,
        },
        &mut registry,
        3,
    );
    // Caller must resize grid after update() returns ALL
    grid.resize(state.cols, state.rows);
    grid.invalidate_all();
    assert!(flags.contains(DirtyFlags::ALL));
    assert_eq!(state.cols, 120);
    assert_eq!(state.rows, 40);
    assert_eq!(grid.width(), 120);
    assert_eq!(grid.height(), 40);

    // Re-render at new size
    let grid = render(&state);
    assert_eq!(row_text(&grid, 0), "hello");
    // Padding at new height
    assert!(row_text(&grid, 2).starts_with('~'));
}

// ===========================================================================
// Protocol parse → state apply round-trip
// ===========================================================================

#[test]
fn parse_draw_and_render() {
    let json = r#"{"jsonrpc":"2.0","method":"draw","params":[[[{"face":{"fg":"default","bg":"default","underline":"default","attributes":[]},"contents":"fn main()"}]],{"line":0,"column":0},{"fg":"white","bg":"black","underline":"default","attributes":[]},{"fg":"white","bg":"black","underline":"default","attributes":[]},0]}"#;
    let mut buf = json.as_bytes().to_vec();
    let req = kasane_core::protocol::parse_request(&mut buf).unwrap();

    let mut state = setup_state(vec![]);
    let flags = state.apply(req);
    assert!(flags.contains(DirtyFlags::BUFFER));
    assert_eq!(state.lines.len(), 1);

    let grid = render(&state);
    assert_eq!(row_text(&grid, 0), "fn main()");
}

#[test]
fn parse_draw_status_and_render() {
    let json = r#"{"jsonrpc":"2.0","method":"draw_status","params":[[{"face":{"fg":"default","bg":"default","underline":"default","attributes":[]},"contents":"[scratch]"}],[],-1,[{"face":{"fg":"default","bg":"default","underline":"default","attributes":[]},"contents":"insert"}],{"fg":"cyan","bg":"default","underline":"default","attributes":[]}]}"#;
    let mut buf = json.as_bytes().to_vec();
    let req = kasane_core::protocol::parse_request(&mut buf).unwrap();

    let mut state = setup_state(vec![make_line("buf")]);
    state.apply(req);

    let grid = render(&state);
    let status = row_text(&grid, 23);
    assert!(
        status.contains("[scratch]"),
        "status should contain '[scratch]', got: {status:?}"
    );
    assert!(
        status.contains("insert"),
        "status should contain 'insert', got: {status:?}"
    );
}

// ===========================================================================
// CellGrid diff
// ===========================================================================

#[test]
fn diff_detects_changes() {
    let state = setup_state(vec![make_line("hello")]);
    let mut grid = CellGrid::new(80, 24);
    grid.clear(&state.default_face);

    let registry = PluginRegistry::new();
    let element = view::view(&state, &registry);
    let root = Rect {
        x: 0,
        y: 0,
        w: 80,
        h: 24,
    };
    let layout = place(&element, root, &state);
    paint::paint(&element, &layout, &mut grid, &state);

    // First diff: everything changed (no previous frame)
    let diffs = grid.diff();
    assert!(!diffs.is_empty(), "first frame should have diffs");
    grid.swap();

    // Second identical render: no changes
    grid.clear(&state.default_face);
    paint::paint(&element, &layout, &mut grid, &state);
    let diffs = grid.diff();
    assert!(
        diffs.is_empty(),
        "identical frame should have no diffs, got {}",
        diffs.len()
    );
}

// ===========================================================================
// Unicode / wide characters
// ===========================================================================

#[test]
fn cjk_wide_chars() {
    let state = setup_state(vec![make_line("Hello\u{4e16}\u{754c}")]); // 世界
    let grid = render(&state);

    assert_eq!(row_text(&grid, 0), "Hello\u{4e16}\u{754c}");
    // '世' is at x=5, width 2
    let cell = grid.get(5, 0).unwrap();
    assert_eq!(cell.grapheme.as_str(), "\u{4e16}");
    assert_eq!(cell.width, 2);
    // x=6 is continuation (width 0)
    let cont = grid.get(6, 0).unwrap();
    assert_eq!(cont.width, 0);
}

#[test]
fn emoji_rendering() {
    let state = setup_state(vec![make_line("a\u{1f600}b")]); // a😀b
    let grid = render(&state);

    let text = row_text(&grid, 0);
    assert!(
        text.contains("\u{1f600}"),
        "should contain emoji, got: {text:?}"
    );
}

// ===========================================================================
// TEA update() integration
// ===========================================================================

#[test]
fn update_kakoune_draw_message() {
    let mut state = setup_state(vec![]);
    let mut grid = CellGrid::new(80, 24);
    let mut registry = PluginRegistry::new();

    let req = KakouneRequest::Draw {
        lines: vec![make_line("updated content")],
        cursor_pos: Coord::default(),
        default_face: Face::default(),
        padding_face: Face::default(),
        widget_columns: 0,
    };
    let (flags, cmds) = update(&mut state, Msg::Kakoune(req), &mut registry, 3);
    assert!(flags.contains(DirtyFlags::BUFFER));
    assert!(cmds.is_empty(), "draw should not produce commands");

    let grid = render(&state);
    assert_eq!(row_text(&grid, 0), "updated content");
}

#[test]
fn update_focus_changes() {
    let mut state = setup_state(vec![make_line("text")]);
    let mut grid = CellGrid::new(80, 24);
    let mut registry = PluginRegistry::new();

    let (flags, _) = update(&mut state, Msg::FocusLost, &mut registry, 3);
    assert!(!state.focused);
    assert!(flags.contains(DirtyFlags::ALL));

    let (flags, _) = update(&mut state, Msg::FocusGained, &mut registry, 3);
    assert!(state.focused);
    assert!(flags.contains(DirtyFlags::ALL));
}

// ===========================================================================
// Edge cases
// ===========================================================================

#[test]
fn small_terminal_1x1() {
    let state = AppState {
        cols: 1,
        rows: 1,
        lines: vec![make_line("x")],
        default_face: Face::default(),
        padding_face: Face::default(),
        status_default_face: Face::default(),
        status_line: make_line(""),
        status_mode_line: make_line(""),
        ..Default::default()
    };

    // Should not panic
    let _grid = render(&state);
}

#[test]
fn many_buffer_lines_overflow() {
    // More lines than screen height
    let lines: Vec<Line> = (0..100).map(|i| make_line(&format!("line {i}"))).collect();
    let state = setup_state(lines);
    let grid = render(&state);

    // Only first 23 lines visible (row 23 = status bar)
    assert_eq!(row_text(&grid, 0), "line 0");
    assert_eq!(row_text(&grid, 22), "line 22");
    // Row 23 is status bar, not line 23
    let status = row_text(&grid, 23);
    assert!(status.contains("main.rs"));
}

#[test]
fn long_line_truncated_at_screen_width() {
    let long = "x".repeat(200);
    let state = setup_state(vec![make_line(&long)]);
    let grid = render(&state);

    // Only 80 chars should be in the grid
    let text = row_text(&grid, 0);
    assert_eq!(text.len(), 80);
}

// ===========================================================================
// Line-level dirty tracking
// ===========================================================================

/// Helper: render with line-dirty optimization using render_pipeline_cached.
fn render_with_dirty(state: &AppState, dirty: DirtyFlags, grid: &mut CellGrid) {
    use kasane_core::render::{ViewCache, render_pipeline_cached};

    let registry = PluginRegistry::new();
    let mut cache = ViewCache::new();
    render_pipeline_cached(state, &registry, grid, dirty, &mut cache);
}

#[test]
fn test_line_dirty_single_edit_diff() {
    // Frame 1: full render
    let mut state = setup_state(vec![
        make_line("line 0"),
        make_line("line 1"),
        make_line("line 2"),
    ]);
    let mut grid = CellGrid::new(state.cols, state.rows);
    render_with_dirty(&state, DirtyFlags::ALL, &mut grid);
    grid.swap();

    // Frame 2: edit only line 1
    state.apply(KakouneRequest::Draw {
        lines: vec![
            make_line("line 0"),
            make_line("EDITED"),
            make_line("line 2"),
        ],
        cursor_pos: Coord::default(),
        default_face: state.default_face,
        padding_face: state.padding_face,
        widget_columns: 0,
    });
    assert_eq!(state.lines_dirty, vec![false, true, false]);

    render_with_dirty(&state, DirtyFlags::BUFFER, &mut grid);
    let diffs = grid.diff();

    // Only the changed line's cells should appear in diffs
    let dirty_rows: std::collections::HashSet<u16> = diffs.iter().map(|d| d.y).collect();
    assert!(dirty_rows.contains(&1), "changed line 1 should be in diffs");
    assert!(
        !dirty_rows.contains(&0),
        "unchanged line 0 should NOT be in diffs"
    );
    assert!(
        !dirty_rows.contains(&2),
        "unchanged line 2 should NOT be in diffs"
    );
}

#[test]
fn test_line_dirty_consecutive_edits() {
    let mut state = setup_state((0..23).map(|i| make_line(&format!("line {i}"))).collect());
    let mut grid = CellGrid::new(state.cols, state.rows);
    render_with_dirty(&state, DirtyFlags::ALL, &mut grid);
    grid.swap();

    // Frame 2: edit line 5
    let mut lines: Vec<Line> = (0..23).map(|i| make_line(&format!("line {i}"))).collect();
    lines[5] = make_line("EDITED_5");
    state.apply(KakouneRequest::Draw {
        lines,
        cursor_pos: Coord::default(),
        default_face: state.default_face,
        padding_face: state.padding_face,
        widget_columns: 0,
    });
    render_with_dirty(&state, DirtyFlags::BUFFER, &mut grid);
    let diffs = grid.diff();
    let dirty_rows: std::collections::HashSet<u16> = diffs.iter().map(|d| d.y).collect();
    assert!(dirty_rows.contains(&5));
    assert!(!dirty_rows.contains(&0));
    grid.swap_with_dirty();
    state.lines_dirty.clear();

    // Frame 3: edit line 10
    let mut lines: Vec<Line> = state.lines.clone();
    lines[10] = make_line("EDITED_10");
    state.apply(KakouneRequest::Draw {
        lines,
        cursor_pos: Coord::default(),
        default_face: state.default_face,
        padding_face: state.padding_face,
        widget_columns: 0,
    });
    render_with_dirty(&state, DirtyFlags::BUFFER, &mut grid);
    let diffs = grid.diff();
    let dirty_rows: std::collections::HashSet<u16> = diffs.iter().map(|d| d.y).collect();
    assert!(dirty_rows.contains(&10));
    assert!(
        !dirty_rows.contains(&5),
        "line 5 should not be in diffs (already synced)"
    );
}

#[test]
fn test_line_dirty_full_repaint_on_overlay() {
    let mut state = setup_state(vec![make_line("line 0"), make_line("line 1")]);
    let mut grid = CellGrid::new(state.cols, state.rows);

    // Show then hide a menu to get MENU|BUFFER dirty flags
    state.apply(KakouneRequest::MenuShow {
        items: vec![make_line("item")],
        anchor: Coord { line: 0, column: 0 },
        selected_item_face: Face::default(),
        menu_face: Face::default(),
        style: MenuStyle::Inline,
    });
    render_with_dirty(&state, DirtyFlags::ALL, &mut grid);
    grid.swap();

    let flags = state.apply(KakouneRequest::MenuHide);
    // MenuHide returns MENU|BUFFER — not just BUFFER
    assert!(flags.contains(DirtyFlags::MENU));
    assert!(flags.contains(DirtyFlags::BUFFER));

    // Full repaint should happen (dirty != BUFFER alone)
    render_with_dirty(&state, flags, &mut grid);
    // Should not crash; verifies the pipeline handles overlay dismissal correctly
    let diffs = grid.diff();
    assert!(
        !diffs.is_empty(),
        "full repaint after overlay hide should produce diffs"
    );
}

// ---------------------------------------------------------------------------
// Surface model equivalence tests
// ---------------------------------------------------------------------------

/// Verify that the Surface-based pipeline produces identical CellGrid output
/// as the legacy view_cached()-based pipeline.
#[test]
fn test_surface_pipeline_equivalence_empty_state() {
    use kasane_core::render::ViewCache;
    use kasane_core::render::pipeline::{render_pipeline, render_pipeline_surfaces_cached};
    use kasane_core::state::DirtyFlags;
    use kasane_core::surface::SurfaceRegistry;
    use kasane_core::surface::buffer::KakouneBufferSurface;
    use kasane_core::surface::status::StatusBarSurface;

    let state = setup_state(vec![make_line("hello world"), make_line("second line")]);
    let registry = PluginRegistry::new();

    // Legacy pipeline
    let mut legacy_grid = CellGrid::new(state.cols, state.rows);
    let legacy_result = render_pipeline(&state, &registry, &mut legacy_grid);

    // Surface pipeline
    let mut surface_registry = SurfaceRegistry::new();
    surface_registry.register(Box::new(KakouneBufferSurface::new()));
    surface_registry.register(Box::new(StatusBarSurface::new()));

    let mut surface_grid = CellGrid::new(state.cols, state.rows);
    let mut cache = ViewCache::new();
    let surface_result = render_pipeline_surfaces_cached(
        &state,
        &registry,
        &surface_registry,
        &mut surface_grid,
        DirtyFlags::ALL,
        &mut cache,
        &[],
    );

    // Compare cursor positions
    assert_eq!(
        legacy_result.cursor_x, surface_result.cursor_x,
        "cursor_x mismatch"
    );
    assert_eq!(
        legacy_result.cursor_y, surface_result.cursor_y,
        "cursor_y mismatch"
    );

    // Compare cell grids
    for y in 0..state.rows {
        for x in 0..state.cols {
            let l = legacy_grid.get(x, y);
            let s = surface_grid.get(x, y);
            if let (Some(l), Some(s)) = (l, s) {
                assert_eq!(
                    l.grapheme, s.grapheme,
                    "grapheme mismatch at ({x}, {y}): legacy={:?} surface={:?}",
                    l.grapheme, s.grapheme
                );
                assert_eq!(l.face, s.face, "face mismatch at ({x}, {y})");
            }
        }
    }
}

/// Verify Surface pipeline equivalence with menu overlay.
#[test]
fn test_surface_pipeline_equivalence_with_menu() {
    use kasane_core::render::ViewCache;
    use kasane_core::render::pipeline::{render_pipeline, render_pipeline_surfaces_cached};
    use kasane_core::state::DirtyFlags;
    use kasane_core::surface::SurfaceRegistry;
    use kasane_core::surface::buffer::KakouneBufferSurface;
    use kasane_core::surface::status::StatusBarSurface;

    let mut state = setup_state(vec![make_line("hello"), make_line("world")]);
    state.apply(KakouneRequest::MenuShow {
        items: vec![make_line("item1"), make_line("item2"), make_line("item3")],
        anchor: Coord { line: 1, column: 0 },
        selected_item_face: Face::default(),
        menu_face: Face::default(),
        style: MenuStyle::Inline,
    });

    let registry = PluginRegistry::new();

    // Legacy pipeline
    let mut legacy_grid = CellGrid::new(state.cols, state.rows);
    let _legacy_result = render_pipeline(&state, &registry, &mut legacy_grid);

    // Surface pipeline
    let mut surface_registry = SurfaceRegistry::new();
    surface_registry.register(Box::new(KakouneBufferSurface::new()));
    surface_registry.register(Box::new(StatusBarSurface::new()));
    surface_registry.sync_ephemeral_surfaces(&state);

    let mut surface_grid = CellGrid::new(state.cols, state.rows);
    let mut cache = ViewCache::new();
    let _surface_result = render_pipeline_surfaces_cached(
        &state,
        &registry,
        &surface_registry,
        &mut surface_grid,
        DirtyFlags::ALL,
        &mut cache,
        &[],
    );

    // Compare cell grids
    for y in 0..state.rows {
        for x in 0..state.cols {
            let l = legacy_grid.get(x, y);
            let s = surface_grid.get(x, y);
            if let (Some(l), Some(s)) = (l, s) {
                assert_eq!(
                    l.grapheme, s.grapheme,
                    "grapheme mismatch at ({x}, {y}): legacy={:?} surface={:?}",
                    l.grapheme, s.grapheme
                );
                assert_eq!(l.face, s.face, "face mismatch at ({x}, {y})");
            }
        }
    }
}
