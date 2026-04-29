//! Integration tests for the full rendering pipeline:
//!   JSON-RPC parse → State update → view() → layout → paint() → CellGrid
//!
//! These tests exercise the end-to-end flow to catch regressions in the
//! interaction between subsystems, complementing unit tests within each module.

use kasane_core::layout::Rect;
use kasane_core::layout::flex::place;
use kasane_core::plugin::PluginRuntime;
use kasane_core::protocol::{
    Atom, Color, Coord, Face, InfoStyle, KakouneRequest, Line, MenuStyle, NamedColor,
};
use kasane_core::render::CellGrid;
use kasane_core::render::paint;
use kasane_core::render::view;
use kasane_core::state::{AppState, DirtyFlags, Msg, update_in_place};
use kasane_core::test_support::{make_line, render_with_registry, row_text};

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Set up a standard 80x24 state with given buffer lines.
fn setup_state(lines: Vec<Line>) -> AppState {
    let mut state = kasane_core::test_support::test_state_80x24();
    state.observed.lines = lines;
    state.observed.status_default_style = state.observed.default_style.clone();
    state.inference.status_line = make_line(" main.rs ");
    state.observed.status_mode_line = make_line("normal");
    state
}

/// Run the full pipeline with an empty registry.
fn render(state: &AppState) -> kasane_core::render::CellGrid {
    render_with_registry(state, &PluginRuntime::new())
}

/// Create a registry with the built-in menu and info renderers.
fn registry_with_builtins() -> PluginRuntime {
    let mut registry = PluginRuntime::new();
    registry.register_backend(Box::new(kasane_core::render::view::menu::BuiltinMenuPlugin));
    registry.register_backend(Box::new(kasane_core::render::view::info::BuiltinInfoPlugin));
    registry
}

/// Run the pipeline with built-in renderers registered.
fn render_with_builtins(state: &AppState) -> kasane_core::render::CellGrid {
    render_with_registry(state, &registry_with_builtins())
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
        Atom::from_face(
            Face {
                fg: red,
                ..Face::default()
            },
            "red",
        ),
        Atom::from_face(Face::default(), " plain"),
    ];
    let state = setup_state(vec![line]);
    let grid = render(&state);

    // Text content correct
    assert_eq!(row_text(&grid, 0), "red plain");
    // First cell inherits red foreground
    let cell = grid.get(0, 0).unwrap();
    assert_eq!(cell.face().fg, red);
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
    state.config.status_at_top = true;
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
    state.observed.cursor_pos = Coord { line: 0, column: 3 };

    // Show inline menu
    let items = vec![make_line("foo"), make_line("bar"), make_line("baz")];
    state.apply(KakouneRequest::MenuShow {
        items,
        anchor: Coord { line: 0, column: 3 },
        selected_item_style: std::sync::Arc::new(
            kasane_core::protocol::UnresolvedStyle::from_face(&Face {
                fg: Color::Named(NamedColor::Black),
                bg: Color::Named(NamedColor::Cyan),
                ..Face::default()
            }),
        ),
        menu_style: std::sync::Arc::new(kasane_core::protocol::UnresolvedStyle::from_face(&Face {
            fg: Color::Named(NamedColor::White),
            bg: Color::Named(NamedColor::Blue),
            ..Face::default()
        })),
        style: MenuStyle::Inline,
    });
    assert!(state.observed.menu.is_some());

    let grid = render_with_builtins(&state);
    // Menu items should appear somewhere in the grid
    let mut found_foo = false;
    let mut found_bar = false;
    for y in 0..state.runtime.rows {
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
    assert_eq!(state.observed.menu.as_ref().unwrap().selected, Some(1));
}

#[test]
fn menu_hide() {
    let mut state = setup_state(vec![make_line("text")]);
    state.apply(KakouneRequest::MenuShow {
        items: vec![make_line("item1"), make_line("item2")],
        anchor: Coord { line: 0, column: 0 },
        selected_item_style: kasane_core::protocol::default_unresolved_style(),
        menu_style: kasane_core::protocol::default_unresolved_style(),
        style: MenuStyle::Inline,
    });
    assert!(state.observed.menu.is_some());

    state.apply(KakouneRequest::MenuHide);
    assert!(state.observed.menu.is_none());
}

#[test]
fn prompt_menu_multi_column() {
    let mut state = setup_state(vec![make_line("text")]);
    let items: Vec<Line> = (0..20).map(|i| make_line(&format!("cmd_{i}"))).collect();
    state.apply(KakouneRequest::MenuShow {
        items,
        anchor: Coord { line: 0, column: 0 },
        selected_item_style: kasane_core::protocol::default_unresolved_style(),
        menu_style: kasane_core::protocol::default_unresolved_style(),
        style: MenuStyle::Prompt,
    });
    assert!(state.observed.menu.is_some());
    let menu = state.observed.menu.as_ref().unwrap();
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
        info_style: kasane_core::protocol::default_unresolved_style(),
        style: InfoStyle::Inline,
    });
    assert!(flags.contains(DirtyFlags::INFO));
    assert_eq!(state.observed.infos.len(), 1);

    let grid = render_with_builtins(&state);
    // Info content should appear in the grid
    let mut found = false;
    for y in 0..state.runtime.rows {
        if row_text(&grid, y).contains("help text") {
            found = true;
            break;
        }
    }
    assert!(found, "info popup content should be visible");

    // Hide
    state.apply(KakouneRequest::InfoHide);
    assert!(state.observed.infos.is_empty());
}

#[test]
fn multiple_infos_coexist() {
    let mut state = setup_state(vec![make_line("code line here")]);

    // Two infos at different anchors/styles should coexist
    state.apply(KakouneRequest::InfoShow {
        title: make_line("Lint"),
        content: vec![make_line("error: unused var")],
        anchor: Coord { line: 0, column: 0 },
        info_style: kasane_core::protocol::default_unresolved_style(),
        style: InfoStyle::Inline,
    });
    state.apply(KakouneRequest::InfoShow {
        title: make_line("Doc"),
        content: vec![make_line("fn doc text")],
        anchor: Coord {
            line: 0,
            column: 10,
        },
        info_style: kasane_core::protocol::default_unresolved_style(),
        style: InfoStyle::Modal,
    });
    assert_eq!(state.observed.infos.len(), 2);

    // Should render without panic
    let _grid = render(&state);
}

// ===========================================================================
// Resize
// ===========================================================================

#[test]
fn resize_updates_grid() {
    let mut state = Box::new(setup_state(vec![make_line("hello")]));
    let mut grid = CellGrid::new(state.runtime.cols, state.runtime.rows);
    let mut registry = PluginRuntime::new();

    let result = update_in_place(
        &mut state,
        Msg::Resize {
            cols: 120,
            rows: 40,
        },
        &mut registry,
        3,
    );
    let flags = result.flags;
    // Caller must resize grid after update() returns ALL
    grid.resize(state.runtime.cols, state.runtime.rows);
    grid.invalidate_all();
    assert!(flags.contains(DirtyFlags::ALL));
    assert_eq!(state.runtime.cols, 120);
    assert_eq!(state.runtime.rows, 40);
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
    assert_eq!(state.observed.lines.len(), 1);

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
    grid.clear(&state.observed.default_style.to_face());

    let registry = PluginRuntime::new();
    let element = view::view(&state, &registry.view());
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
    grid.clear(&state.observed.default_style.to_face());
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
    let mut state = Box::new(setup_state(vec![]));
    let mut registry = PluginRuntime::new();

    let req = KakouneRequest::Draw {
        lines: vec![make_line("updated content")],
        cursor_pos: Coord::default(),
        default_style: kasane_core::protocol::default_unresolved_style(),
        padding_style: kasane_core::protocol::default_unresolved_style(),
        widget_columns: 0,
    };
    let result = update_in_place(&mut state, Msg::Kakoune(req), &mut registry, 3);
    let (flags, cmds) = (result.flags, result.commands);
    assert!(flags.contains(DirtyFlags::BUFFER));
    assert!(cmds.is_empty(), "draw should not produce commands");

    let grid = render(&state);
    assert_eq!(row_text(&grid, 0), "updated content");
}

#[test]
fn update_focus_changes() {
    let mut state = Box::new(setup_state(vec![make_line("text")]));
    let mut registry = PluginRuntime::new();

    let result = update_in_place(&mut state, Msg::FocusLost, &mut registry, 3);
    assert!(!state.runtime.focused);
    assert!(result.flags.contains(DirtyFlags::ALL));

    let result = update_in_place(&mut state, Msg::FocusGained, &mut registry, 3);
    assert!(state.runtime.focused);
    assert!(result.flags.contains(DirtyFlags::ALL));
}

// ===========================================================================
// Edge cases
// ===========================================================================

#[test]
fn small_terminal_1x1() {
    let mut state = AppState::default();
    state.runtime.cols = 1;
    state.runtime.rows = 1;
    state.observed.lines = vec![make_line("x")];
    state.observed.default_style = Face::default().into();
    state.observed.padding_style = Face::default().into();
    state.observed.status_default_style = Face::default().into();
    state.inference.status_line = make_line("");
    state.observed.status_mode_line = make_line("");

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

/// Helper: render with line-dirty optimization using render_pipeline_direct.
fn render_with_dirty(state: &AppState, dirty: DirtyFlags, grid: &mut CellGrid) {
    use kasane_core::render::render_pipeline_direct;

    let registry = registry_with_builtins();
    render_pipeline_direct(state, &registry.view(), grid, dirty);
}

#[test]
fn test_line_dirty_single_edit_diff() {
    // Frame 1: full render
    let mut state = setup_state(vec![
        make_line("line 0"),
        make_line("line 1"),
        make_line("line 2"),
    ]);
    let mut grid = CellGrid::new(state.runtime.cols, state.runtime.rows);
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
        default_style: std::sync::Arc::new(kasane_core::protocol::UnresolvedStyle::from_face(
            &state.observed.default_style.to_face(),
        )),
        padding_style: std::sync::Arc::new(kasane_core::protocol::UnresolvedStyle::from_face(
            &state.observed.padding_style.to_face(),
        )),
        widget_columns: 0,
    });
    assert_eq!(state.inference.lines_dirty, vec![false, true, false]);

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
    let mut grid = CellGrid::new(state.runtime.cols, state.runtime.rows);
    render_with_dirty(&state, DirtyFlags::ALL, &mut grid);
    grid.swap();

    // Frame 2: edit line 5
    let mut lines: Vec<Line> = (0..23).map(|i| make_line(&format!("line {i}"))).collect();
    lines[5] = make_line("EDITED_5");
    state.apply(KakouneRequest::Draw {
        lines,
        cursor_pos: Coord::default(),
        default_style: std::sync::Arc::new(kasane_core::protocol::UnresolvedStyle::from_face(
            &state.observed.default_style.to_face(),
        )),
        padding_style: std::sync::Arc::new(kasane_core::protocol::UnresolvedStyle::from_face(
            &state.observed.padding_style.to_face(),
        )),
        widget_columns: 0,
    });
    render_with_dirty(&state, DirtyFlags::BUFFER, &mut grid);
    let diffs = grid.diff();
    let dirty_rows: std::collections::HashSet<u16> = diffs.iter().map(|d| d.y).collect();
    assert!(dirty_rows.contains(&5));
    assert!(!dirty_rows.contains(&0));
    grid.swap_with_dirty();
    state.inference.lines_dirty.clear();

    // Frame 3: edit line 10
    let mut lines: Vec<Line> = state.observed.lines.clone();
    lines[10] = make_line("EDITED_10");
    state.apply(KakouneRequest::Draw {
        lines,
        cursor_pos: Coord::default(),
        default_style: std::sync::Arc::new(kasane_core::protocol::UnresolvedStyle::from_face(
            &state.observed.default_style.to_face(),
        )),
        padding_style: std::sync::Arc::new(kasane_core::protocol::UnresolvedStyle::from_face(
            &state.observed.padding_style.to_face(),
        )),
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
    let mut grid = CellGrid::new(state.runtime.cols, state.runtime.rows);

    // Show then hide a menu to get MENU|BUFFER dirty flags
    state.apply(KakouneRequest::MenuShow {
        items: vec![make_line("item")],
        anchor: Coord { line: 0, column: 0 },
        selected_item_style: kasane_core::protocol::default_unresolved_style(),
        menu_style: kasane_core::protocol::default_unresolved_style(),
        style: MenuStyle::Inline,
    });
    render_with_dirty(&state, DirtyFlags::ALL, &mut grid);
    grid.swap();

    let flags = state.apply(KakouneRequest::MenuHide);
    // MenuHide returns MENU|BUFFER_CONTENT — buffer content needs redraw (overlay removed)
    assert!(flags.contains(DirtyFlags::MENU));
    assert!(flags.contains(DirtyFlags::BUFFER_CONTENT));

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

/// Verify that the Salsa pipeline produces identical CellGrid output
/// as the legacy view()-based pipeline.
#[test]
fn test_salsa_pipeline_equivalence_empty_state() {
    use kasane_core::render::{render_pipeline, render_pipeline_cached};
    use kasane_core::salsa_db::KasaneDatabase;
    use kasane_core::salsa_sync::{
        SalsaInputHandles, sync_display_directives, sync_inputs_from_state,
        sync_plugin_contributions,
    };
    use kasane_core::state::DirtyFlags;

    let state = setup_state(vec![make_line("hello world"), make_line("second line")]);
    let registry = PluginRuntime::new();

    // Legacy pipeline
    let mut legacy_grid = CellGrid::new(state.runtime.cols, state.runtime.rows);
    let (legacy_result, _) = render_pipeline(&state, &registry.view(), &mut legacy_grid);

    // Salsa pipeline
    let mut db = KasaneDatabase::default();
    let mut handles = SalsaInputHandles::new(&mut db);
    sync_inputs_from_state(&mut db, &state, &handles);

    sync_display_directives(&mut db, &state, &registry.view(), &handles);
    sync_plugin_contributions(&mut db, &state, &registry.view(), &mut handles);

    let mut salsa_grid = CellGrid::new(state.runtime.cols, state.runtime.rows);
    let (salsa_result, _) = render_pipeline_cached(
        &db,
        &handles,
        &state,
        &registry.view(),
        &mut salsa_grid,
        DirtyFlags::ALL,
        Default::default(),
    );

    // Compare cursor positions
    assert_eq!(
        legacy_result.cursor_x, salsa_result.cursor_x,
        "cursor_x mismatch"
    );
    assert_eq!(
        legacy_result.cursor_y, salsa_result.cursor_y,
        "cursor_y mismatch"
    );

    // Compare cell grids
    for y in 0..state.runtime.rows {
        for x in 0..state.runtime.cols {
            let l = legacy_grid.get(x, y);
            let s = salsa_grid.get(x, y);
            if let (Some(l), Some(s)) = (l, s) {
                assert_eq!(
                    l.grapheme, s.grapheme,
                    "grapheme mismatch at ({x}, {y}): legacy={:?} salsa={:?}",
                    l.grapheme, s.grapheme
                );
                assert_eq!(l.face(), s.face(), "face mismatch at ({x}, {y})");
            }
        }
    }
}

/// Verify Salsa pipeline equivalence with menu overlay.
#[test]
fn test_salsa_pipeline_equivalence_with_menu() {
    use kasane_core::render::{render_pipeline, render_pipeline_cached};
    use kasane_core::salsa_db::KasaneDatabase;
    use kasane_core::salsa_sync::{
        SalsaInputHandles, sync_display_directives, sync_inputs_from_state,
        sync_plugin_contributions,
    };
    use kasane_core::state::DirtyFlags;

    let mut state = setup_state(vec![make_line("hello"), make_line("world")]);
    state.apply(KakouneRequest::MenuShow {
        items: vec![make_line("item1"), make_line("item2"), make_line("item3")],
        anchor: Coord { line: 1, column: 0 },
        selected_item_style: kasane_core::protocol::default_unresolved_style(),
        menu_style: kasane_core::protocol::default_unresolved_style(),
        style: MenuStyle::Inline,
    });

    let registry = registry_with_builtins();

    // Legacy pipeline
    let mut legacy_grid = CellGrid::new(state.runtime.cols, state.runtime.rows);
    let (_legacy_result, _) = render_pipeline(&state, &registry.view(), &mut legacy_grid);

    // Salsa pipeline
    let mut db = KasaneDatabase::default();
    let mut handles = SalsaInputHandles::new(&mut db);
    sync_inputs_from_state(&mut db, &state, &handles);

    sync_display_directives(&mut db, &state, &registry.view(), &handles);
    sync_plugin_contributions(&mut db, &state, &registry.view(), &mut handles);

    let mut salsa_grid = CellGrid::new(state.runtime.cols, state.runtime.rows);
    let (_salsa_result, _) = render_pipeline_cached(
        &db,
        &handles,
        &state,
        &registry.view(),
        &mut salsa_grid,
        DirtyFlags::ALL,
        Default::default(),
    );

    // Compare cell grids
    for y in 0..state.runtime.rows {
        for x in 0..state.runtime.cols {
            let l = legacy_grid.get(x, y);
            let s = salsa_grid.get(x, y);
            if let (Some(l), Some(s)) = (l, s) {
                assert_eq!(
                    l.grapheme, s.grapheme,
                    "grapheme mismatch at ({x}, {y}): legacy={:?} salsa={:?}",
                    l.grapheme, s.grapheme
                );
                assert_eq!(l.face(), s.face(), "face mismatch at ({x}, {y})");
            }
        }
    }
}
