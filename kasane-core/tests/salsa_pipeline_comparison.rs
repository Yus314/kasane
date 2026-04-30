//! Parallel comparison tests for Salsa vs legacy rendering pipelines (Phase 2-4, Phase 3).
//!
//! These tests verify that the Salsa-backed pipeline produces identical
//! CellGrid output to the legacy PluginViewSource pipeline across a variety
//! of AppState configurations, including with active plugins.

use kasane_core::element::Element;
use kasane_core::plugin::{
    AnnotateContext, AppView, BackgroundLayer, BlendMode, ContribSizeHint, ContributeContext,
    Contribution, LineAnnotation, PluginBackend, PluginCapabilities, PluginId, PluginRuntime,
    SlotId, TransformContext, TransformTarget,
};
use kasane_core::protocol::{Atom, Color, Coord, InfoStyle, MenuStyle, NamedColor, WireFace};
use kasane_core::render::{CellGrid, render_pipeline, render_pipeline_cached};
use kasane_core::salsa_db::KasaneDatabase;
use kasane_core::salsa_sync::{
    SalsaInputHandles, sync_display_directives, sync_inputs_from_state, sync_plugin_contributions,
};
use kasane_core::state::{AppState, DirtyFlags, InfoIdentity, InfoState, MenuParams, MenuState};
use kasane_core::test_support::{assert_grids_equal, test_state_80x24};

/// Create a PluginRuntime with the built-in menu and info renderers registered.
fn registry_with_builtins() -> PluginRuntime {
    let mut registry = PluginRuntime::new();
    registry.register_backend(Box::new(kasane_core::render::view::menu::BuiltinMenuPlugin));
    registry.register_backend(Box::new(kasane_core::render::view::info::BuiltinInfoPlugin));
    registry
}

fn make_atom(text: &str) -> Atom {
    Atom::plain(text)
}

/// Render with legacy pipeline and return the grid.
fn render_legacy(state: &AppState, registry: &PluginRuntime) -> CellGrid {
    let mut grid = CellGrid::new(state.runtime.cols, state.runtime.rows);
    grid.clear(&kasane_core::render::TerminalStyle::from_style(
        &state.observed.default_style,
    ));
    render_pipeline(state, &registry.view(), &mut grid);
    grid
}

/// Render with Salsa pipeline and return the grid.
fn render_salsa(
    state: &AppState,
    registry: &PluginRuntime,
    db: &KasaneDatabase,
    handles: &SalsaInputHandles,
) -> CellGrid {
    let mut grid = CellGrid::new(state.runtime.cols, state.runtime.rows);
    grid.clear(&kasane_core::render::TerminalStyle::from_style(
        &state.observed.default_style,
    ));
    render_pipeline_cached(
        db,
        handles,
        state,
        &registry.view(),
        &mut grid,
        DirtyFlags::ALL,
        Default::default(),
    );
    grid
}

/// Set up Salsa database and sync all inputs including plugin contributions.
fn setup_salsa_with_plugins(
    state: &AppState,
    registry: &PluginRuntime,
) -> (KasaneDatabase, SalsaInputHandles) {
    let mut db = KasaneDatabase::default();
    let mut handles = SalsaInputHandles::new(&mut db);
    sync_inputs_from_state(&mut db, state, &handles);

    sync_display_directives(&mut db, state, &registry.view(), &handles);
    sync_plugin_contributions(&mut db, state, &registry.view(), &mut handles);
    (db, handles)
}

/// Set up Salsa database and sync state (no plugins).
fn setup_salsa(state: &AppState) -> (KasaneDatabase, SalsaInputHandles) {
    let registry = PluginRuntime::new();
    setup_salsa_with_plugins(state, &registry)
}

// ---------------------------------------------------------------------------
// Comparison tests
// ---------------------------------------------------------------------------

#[test]
fn compare_empty_state() {
    let state = test_state_80x24();
    let registry = PluginRuntime::new();
    let (db, handles) = setup_salsa(&state);

    let legacy = render_legacy(&state, &registry);
    let salsa = render_salsa(&state, &registry, &db, &handles);

    assert_grids_equal(&salsa, &legacy, "empty state");
}

#[test]
fn compare_with_buffer_content() {
    let mut state = test_state_80x24();
    state.observed.lines = vec![
        vec![make_atom("fn main() {")],
        vec![make_atom("    println!(\"hello\");")],
        vec![make_atom("}")],
    ];
    state.observed.cursor_pos = Coord { line: 1, column: 4 };
    let registry = PluginRuntime::new();
    let (db, handles) = setup_salsa(&state);

    let legacy = render_legacy(&state, &registry);
    let salsa = render_salsa(&state, &registry, &db, &handles);

    assert_grids_equal(&salsa, &legacy, "buffer content");
}

#[test]
fn compare_with_status_line() {
    let mut state = test_state_80x24();
    state.inference.status_line = vec![make_atom(" :edit foo.rs ")];
    state.observed.status_mode_line = vec![make_atom(" normal ")];
    let registry = PluginRuntime::new();
    let (db, handles) = setup_salsa(&state);

    let legacy = render_legacy(&state, &registry);
    let salsa = render_salsa(&state, &registry, &db, &handles);

    assert_grids_equal(&salsa, &legacy, "status line");
}

#[test]
fn compare_with_inline_menu() {
    let mut state = test_state_80x24();
    state.observed.lines = vec![vec![make_atom("hello")]];
    state.observed.menu = Some(MenuState::new(
        vec![
            vec![make_atom("item_one")],
            vec![make_atom("item_two")],
            vec![make_atom("item_three")],
        ],
        MenuParams {
            anchor: Coord { line: 1, column: 5 },
            selected_item_face: kasane_core::protocol::Style::default(),
            menu_face: kasane_core::protocol::Style::default(),
            style: MenuStyle::Inline,
            screen_w: 80,
            screen_h: 23,
            max_height: 10,
        },
    ));
    let registry = registry_with_builtins();
    let (db, handles) = setup_salsa(&state);

    let legacy = render_legacy(&state, &registry);
    let salsa = render_salsa(&state, &registry, &db, &handles);

    assert_grids_equal(&salsa, &legacy, "inline menu");
}

#[test]
fn compare_with_search_menu() {
    let mut state = test_state_80x24();
    state.observed.menu = Some(MenuState::new(
        vec![vec![make_atom("match1")], vec![make_atom("match2")]],
        MenuParams {
            anchor: Coord { line: 0, column: 0 },
            selected_item_face: kasane_core::protocol::Style::default(),
            menu_face: kasane_core::protocol::Style::default(),
            style: MenuStyle::Search,
            screen_w: 80,
            screen_h: 23,
            max_height: 10,
        },
    ));
    let registry = registry_with_builtins();
    let (db, handles) = setup_salsa(&state);

    let legacy = render_legacy(&state, &registry);
    let salsa = render_salsa(&state, &registry, &db, &handles);

    assert_grids_equal(&salsa, &legacy, "search menu");
}

#[test]
fn compare_with_info_modal() {
    let mut state = test_state_80x24();
    state.observed.infos.push(InfoState {
        title: vec![make_atom("Help")],
        content: vec![
            vec![make_atom("Line 1: some content")],
            vec![make_atom("Line 2: more content")],
        ],
        anchor: Coord {
            line: 5,
            column: 10,
        },
        face: kasane_core::protocol::Style::default(),
        style: InfoStyle::Modal,
        identity: InfoIdentity {
            style: InfoStyle::Modal,
            anchor_line: 5,
        },
        scroll_offset: 0,
    });
    let registry = registry_with_builtins();
    let (db, handles) = setup_salsa(&state);

    let legacy = render_legacy(&state, &registry);
    let salsa = render_salsa(&state, &registry, &db, &handles);

    assert_grids_equal(&salsa, &legacy, "info modal");
}

#[test]
fn compare_with_multiple_infos() {
    let mut state = test_state_80x24();
    state.observed.infos.push(InfoState {
        title: vec![make_atom("Info A")],
        content: vec![vec![make_atom("Content A")]],
        anchor: Coord { line: 3, column: 0 },
        face: kasane_core::protocol::Style::default(),
        style: InfoStyle::Inline,
        identity: InfoIdentity {
            style: InfoStyle::Inline,
            anchor_line: 3,
        },
        scroll_offset: 0,
    });
    state.observed.infos.push(InfoState {
        title: vec![make_atom("Info B")],
        content: vec![vec![make_atom("Content B")]],
        anchor: Coord {
            line: 8,
            column: 20,
        },
        face: kasane_core::protocol::Style::default(),
        style: InfoStyle::Inline,
        identity: InfoIdentity {
            style: InfoStyle::Inline,
            anchor_line: 8,
        },
        scroll_offset: 0,
    });
    let registry = registry_with_builtins();
    let (db, handles) = setup_salsa(&state);

    let legacy = render_legacy(&state, &registry);
    let salsa = render_salsa(&state, &registry, &db, &handles);

    assert_grids_equal(&salsa, &legacy, "multiple infos");
}

#[test]
fn compare_memoization_consistency() {
    let mut state = test_state_80x24();
    state.observed.lines = vec![vec![make_atom("hello")]];
    state.inference.status_line = vec![make_atom("status")];
    state.observed.status_mode_line = vec![make_atom("normal")];
    let registry = PluginRuntime::new();
    let (mut db, mut handles) = setup_salsa(&state);

    // First render
    let salsa1 = render_salsa(&state, &registry, &db, &handles);

    // Change only buffer, re-sync, render again
    state.observed.lines = vec![vec![make_atom("world")]];
    sync_inputs_from_state(&mut db, &state, &handles);

    sync_display_directives(&mut db, &state, &registry.view(), &handles);
    sync_plugin_contributions(&mut db, &state, &registry.view(), &mut handles);

    let legacy2 = render_legacy(&state, &registry);
    let salsa2 = render_salsa(&state, &registry, &db, &handles);

    assert_grids_equal(&salsa2, &legacy2, "after buffer change");
    // Also verify it actually changed
    assert_ne!(
        salsa1.get(0, 0).map(|c| c.grapheme.as_str()),
        salsa2.get(0, 0).map(|c| c.grapheme.as_str()),
        "buffer content should have changed"
    );
}

// ---------------------------------------------------------------------------
// Mock plugins for Phase 3 comparison tests
// ---------------------------------------------------------------------------

/// Plugin that contributes a fixed-width element to BUFFER_LEFT (e.g., line numbers).
struct BufferLeftPlugin;

impl PluginBackend for BufferLeftPlugin {
    fn id(&self) -> PluginId {
        PluginId("test_buffer_left".into())
    }

    fn capabilities(&self) -> PluginCapabilities {
        PluginCapabilities::CONTRIBUTOR
    }

    fn contribute_to(
        &self,
        region: &SlotId,
        _state: &kasane_core::plugin::AppView<'_>,
        _ctx: &ContributeContext,
    ) -> Option<Contribution> {
        if region == &SlotId::BUFFER_LEFT {
            Some(Contribution {
                element: Element::plain_text("LN"),
                priority: 0,
                size_hint: ContribSizeHint::Auto,
            })
        } else {
            None
        }
    }
}

/// Plugin that contributes to STATUS_RIGHT.
struct StatusRightPlugin;

impl PluginBackend for StatusRightPlugin {
    fn id(&self) -> PluginId {
        PluginId("test_status_right".into())
    }

    fn capabilities(&self) -> PluginCapabilities {
        PluginCapabilities::CONTRIBUTOR
    }

    fn contribute_to(
        &self,
        region: &SlotId,
        _state: &kasane_core::plugin::AppView<'_>,
        _ctx: &ContributeContext,
    ) -> Option<Contribution> {
        if region == &SlotId::STATUS_RIGHT {
            Some(Contribution {
                element: Element::plain_text("[RS]"),
                priority: 0,
                size_hint: ContribSizeHint::Auto,
            })
        } else {
            None
        }
    }
}

/// Plugin that wraps the buffer element with a banner line.
struct BufferTransformPlugin;

impl PluginBackend for BufferTransformPlugin {
    fn id(&self) -> PluginId {
        PluginId("test_buffer_transform".into())
    }

    fn capabilities(&self) -> PluginCapabilities {
        PluginCapabilities::TRANSFORMER
    }

    fn transform(
        &self,
        target: &kasane_core::plugin::TransformTarget,
        subject: kasane_core::plugin::TransformSubject,
        _state: &kasane_core::plugin::AppView<'_>,
        _ctx: &TransformContext,
    ) -> kasane_core::plugin::TransformSubject {
        if *target == TransformTarget::BUFFER {
            subject.map_element(|element| {
                Element::column(vec![
                    kasane_core::element::FlexChild::fixed(Element::text(
                        "~banner~",
                        WireFace::default(),
                    )),
                    kasane_core::element::FlexChild::flexible(element, 1.0),
                ])
            })
        } else {
            subject
        }
    }
}

/// Plugin that adds a line background highlight to line 0.
struct LineHighlightPlugin;

impl PluginBackend for LineHighlightPlugin {
    fn id(&self) -> PluginId {
        PluginId("test_line_highlight".into())
    }

    fn capabilities(&self) -> PluginCapabilities {
        PluginCapabilities::ANNOTATOR
    }

    fn annotate_line_with_ctx(
        &self,
        line: usize,
        _state: &kasane_core::plugin::AppView<'_>,
        _ctx: &AnnotateContext,
    ) -> Option<LineAnnotation> {
        if line == 0 {
            Some(LineAnnotation {
                left_gutter: None,
                right_gutter: None,
                background: Some(BackgroundLayer {
                    style: kasane_core::protocol::Style::from_face(&WireFace {
                        bg: Color::Named(NamedColor::Blue),
                        ..WireFace::default()
                    }),
                    z_order: 0,
                    blend: BlendMode::Opaque,
                }),
                priority: 0,
                inline: None,
                virtual_text: vec![],
            })
        } else {
            None
        }
    }
}

/// Plugin that contributes a left gutter element per line.
struct GutterPlugin;

impl PluginBackend for GutterPlugin {
    fn id(&self) -> PluginId {
        PluginId("test_gutter".into())
    }

    fn capabilities(&self) -> PluginCapabilities {
        PluginCapabilities::ANNOTATOR
    }

    fn annotate_line_with_ctx(
        &self,
        line: usize,
        _state: &kasane_core::plugin::AppView<'_>,
        _ctx: &AnnotateContext,
    ) -> Option<LineAnnotation> {
        let num = format!("{:>3}", line + 1);
        Some(LineAnnotation {
            left_gutter: Some(Element::plain_text(&num)),
            right_gutter: None,
            background: None,
            priority: 0,
            inline: None,
            virtual_text: vec![],
        })
    }
}

// ---------------------------------------------------------------------------
// Phase 3: Plugin comparison tests
// ---------------------------------------------------------------------------

#[test]
fn compare_with_buffer_left_plugin() {
    let mut state = test_state_80x24();
    state.observed.lines = vec![vec![make_atom("hello world")]];
    let mut registry = PluginRuntime::new();
    registry.register_backend(Box::new(BufferLeftPlugin));
    registry.init_all(&AppView::new(&state));
    registry.prepare_plugin_cache(DirtyFlags::ALL);
    let (db, handles) = setup_salsa_with_plugins(&state, &registry);

    let legacy = render_legacy(&state, &registry);
    let salsa = render_salsa(&state, &registry, &db, &handles);

    assert_grids_equal(&salsa, &legacy, "buffer_left plugin");
}

#[test]
fn compare_with_status_right_plugin() {
    let mut state = test_state_80x24();
    state.inference.status_line = vec![make_atom("main.rs")];
    state.observed.status_mode_line = vec![make_atom("normal")];
    let mut registry = PluginRuntime::new();
    registry.register_backend(Box::new(StatusRightPlugin));
    registry.init_all(&AppView::new(&state));
    registry.prepare_plugin_cache(DirtyFlags::ALL);
    let (db, handles) = setup_salsa_with_plugins(&state, &registry);

    let legacy = render_legacy(&state, &registry);
    let salsa = render_salsa(&state, &registry, &db, &handles);

    assert_grids_equal(&salsa, &legacy, "status_right plugin");
}

#[test]
fn compare_with_buffer_transform_plugin() {
    let mut state = test_state_80x24();
    state.observed.lines = vec![vec![make_atom("line 0")], vec![make_atom("line 1")]];
    let mut registry = PluginRuntime::new();
    registry.register_backend(Box::new(BufferTransformPlugin));
    registry.init_all(&AppView::new(&state));
    registry.prepare_plugin_cache(DirtyFlags::ALL);
    let (db, handles) = setup_salsa_with_plugins(&state, &registry);

    let legacy = render_legacy(&state, &registry);
    let salsa = render_salsa(&state, &registry, &db, &handles);

    assert_grids_equal(&salsa, &legacy, "buffer transform plugin");
}

#[test]
fn compare_with_line_highlight_plugin() {
    let mut state = test_state_80x24();
    state.observed.lines = vec![
        vec![make_atom("highlighted line")],
        vec![make_atom("normal line")],
    ];
    let mut registry = PluginRuntime::new();
    registry.register_backend(Box::new(LineHighlightPlugin));
    registry.init_all(&AppView::new(&state));
    registry.prepare_plugin_cache(DirtyFlags::ALL);
    let (db, handles) = setup_salsa_with_plugins(&state, &registry);

    let legacy = render_legacy(&state, &registry);
    let salsa = render_salsa(&state, &registry, &db, &handles);

    assert_grids_equal(&salsa, &legacy, "line highlight plugin");
}

#[test]
fn compare_with_gutter_plugin() {
    let mut state = test_state_80x24();
    state.observed.lines = vec![
        vec![make_atom("fn main() {")],
        vec![make_atom("    println!(\"hello\");")],
        vec![make_atom("}")],
    ];
    let mut registry = PluginRuntime::new();
    registry.register_backend(Box::new(GutterPlugin));
    registry.init_all(&AppView::new(&state));
    registry.prepare_plugin_cache(DirtyFlags::ALL);
    let (db, handles) = setup_salsa_with_plugins(&state, &registry);

    let legacy = render_legacy(&state, &registry);
    let salsa = render_salsa(&state, &registry, &db, &handles);

    assert_grids_equal(&salsa, &legacy, "gutter plugin");
}

#[test]
fn compare_with_multiple_plugins() {
    let mut state = test_state_80x24();
    state.observed.lines = vec![
        vec![make_atom("fn main() {")],
        vec![make_atom("    println!(\"hello\");")],
        vec![make_atom("}")],
    ];
    state.inference.status_line = vec![make_atom("main.rs")];
    state.observed.status_mode_line = vec![make_atom("normal")];
    let mut registry = PluginRuntime::new();
    registry.register_backend(Box::new(GutterPlugin));
    registry.register_backend(Box::new(BufferLeftPlugin));
    registry.register_backend(Box::new(StatusRightPlugin));
    registry.register_backend(Box::new(LineHighlightPlugin));
    registry.init_all(&AppView::new(&state));
    registry.prepare_plugin_cache(DirtyFlags::ALL);
    let (db, handles) = setup_salsa_with_plugins(&state, &registry);

    let legacy = render_legacy(&state, &registry);
    let salsa = render_salsa(&state, &registry, &db, &handles);

    assert_grids_equal(&salsa, &legacy, "multiple plugins");
}

#[test]
fn compare_with_plugins_and_menu() {
    let mut state = test_state_80x24();
    state.observed.lines = vec![vec![make_atom("hello")]];
    state.observed.menu = Some(MenuState::new(
        vec![vec![make_atom("item_one")], vec![make_atom("item_two")]],
        MenuParams {
            anchor: Coord { line: 1, column: 5 },
            selected_item_face: kasane_core::protocol::Style::default(),
            menu_face: kasane_core::protocol::Style::default(),
            style: MenuStyle::Inline,
            screen_w: 80,
            screen_h: 23,
            max_height: 10,
        },
    ));
    let mut registry = registry_with_builtins();
    registry.register_backend(Box::new(GutterPlugin));
    registry.register_backend(Box::new(StatusRightPlugin));
    registry.init_all(&AppView::new(&state));
    registry.prepare_plugin_cache(DirtyFlags::ALL);
    let (db, handles) = setup_salsa_with_plugins(&state, &registry);

    let legacy = render_legacy(&state, &registry);
    let salsa = render_salsa(&state, &registry, &db, &handles);

    assert_grids_equal(&salsa, &legacy, "plugins with menu");
}

// ---------------------------------------------------------------------------
// Menu + info overlay combination tests
// ---------------------------------------------------------------------------

fn make_info_state(anchor_line: i32, anchor_col: i32, style: InfoStyle) -> InfoState {
    InfoState {
        title: vec![make_atom("Info")],
        content: vec![
            vec![make_atom("Info line 1")],
            vec![make_atom("Info line 2")],
        ],
        anchor: Coord {
            line: anchor_line,
            column: anchor_col,
        },
        face: kasane_core::protocol::Style::default(),
        style,
        identity: InfoIdentity {
            style,
            anchor_line: anchor_line as u32,
        },
        scroll_offset: 0,
    }
}

fn make_menu_state() -> MenuState {
    MenuState::new(
        vec![
            vec![make_atom("item_one")],
            vec![make_atom("item_two")],
            vec![make_atom("item_three")],
        ],
        MenuParams {
            anchor: Coord { line: 1, column: 5 },
            selected_item_face: kasane_core::protocol::Style::default(),
            menu_face: kasane_core::protocol::Style::default(),
            style: MenuStyle::Inline,
            screen_w: 80,
            screen_h: 23,
            max_height: 10,
        },
    )
}

#[test]
fn compare_menu_and_info_simultaneous() {
    let mut state = test_state_80x24();
    state.observed.lines = vec![vec![make_atom("hello world")]];
    state.observed.menu = Some(make_menu_state());
    state
        .observed
        .infos
        .push(make_info_state(5, 10, InfoStyle::Modal));
    let registry = registry_with_builtins();
    let (db, handles) = setup_salsa(&state);

    let legacy = render_legacy(&state, &registry);
    let salsa = render_salsa(&state, &registry, &db, &handles);

    assert_grids_equal(&salsa, &legacy, "menu and info simultaneous");
}

#[test]
fn compare_menu_appears_while_info_visible() {
    let mut state = test_state_80x24();
    state.observed.lines = vec![vec![make_atom("hello world")]];
    state
        .observed
        .infos
        .push(make_info_state(3, 0, InfoStyle::Inline));

    let registry = registry_with_builtins();
    let (mut db, mut handles) = setup_salsa(&state);

    // Render with only info visible
    let legacy_info_only = render_legacy(&state, &registry);
    let salsa_info_only = render_salsa(&state, &registry, &db, &handles);
    assert_grids_equal(
        &salsa_info_only,
        &legacy_info_only,
        "info only (before menu)",
    );

    // Now add a menu and re-render
    state.observed.menu = Some(make_menu_state());
    sync_inputs_from_state(&mut db, &state, &handles);

    sync_display_directives(&mut db, &state, &registry.view(), &handles);
    sync_plugin_contributions(&mut db, &state, &registry.view(), &mut handles);

    let legacy_both = render_legacy(&state, &registry);
    let salsa_both = render_salsa(&state, &registry, &db, &handles);
    assert_grids_equal(&salsa_both, &legacy_both, "menu appears while info visible");
}

#[test]
fn compare_menu_disappears_while_info_visible() {
    let mut state = test_state_80x24();
    state.observed.lines = vec![vec![make_atom("hello world")]];
    state.observed.menu = Some(make_menu_state());
    state
        .observed
        .infos
        .push(make_info_state(3, 0, InfoStyle::Inline));

    let registry = registry_with_builtins();
    let (mut db, mut handles) = setup_salsa(&state);

    // Render with both menu and info
    let legacy_both = render_legacy(&state, &registry);
    let salsa_both = render_salsa(&state, &registry, &db, &handles);
    assert_grids_equal(&salsa_both, &legacy_both, "menu + info (before removal)");

    // Remove the menu
    state.observed.menu = None;
    sync_inputs_from_state(&mut db, &state, &handles);

    sync_display_directives(&mut db, &state, &registry.view(), &handles);
    sync_plugin_contributions(&mut db, &state, &registry.view(), &mut handles);

    let legacy_info_only = render_legacy(&state, &registry);
    let salsa_info_only = render_salsa(&state, &registry, &db, &handles);
    assert_grids_equal(
        &salsa_info_only,
        &legacy_info_only,
        "menu disappears while info visible",
    );
}
