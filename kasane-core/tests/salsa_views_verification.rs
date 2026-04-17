//! Verification tests for Salsa tracked view functions (Phase 2-1).
//!
//! Tests that:
//! 1. Pure tracked view functions produce structurally correct Element trees
//! 2. Memoization works (same inputs → cached result without re-execution)
//! 3. Menu and info view functions handle edge cases correctly

use kasane_core::element::{Element, OverlayAnchor};
use kasane_core::protocol::{Atom, Coord, Face, InfoStyle, MenuStyle};
use kasane_core::salsa_db::KasaneDatabase;
use kasane_core::salsa_sync::{SalsaInputHandles, sync_inputs_from_state};
use kasane_core::salsa_views;
use kasane_core::state::{AppState, InfoIdentity, InfoState, MenuParams, MenuState};

fn make_atom(text: &str) -> Atom {
    Atom {
        face: Face::default(),
        contents: text.into(),
    }
}

// ---------------------------------------------------------------------------
// Status bar tracked function
// ---------------------------------------------------------------------------

#[test]
fn pure_status_element_basic() {
    let mut db = KasaneDatabase::default();
    let handles = SalsaInputHandles::new(&mut db);
    let mut state = AppState::default();
    state.inference.status_line = vec![make_atom(":edit foo.rs")];
    state.observed.status_mode_line = vec![make_atom("normal")];
    sync_inputs_from_state(&mut db, &state, &handles);

    let element = salsa_views::pure_status_element(&db, handles.status);

    // Should be a Flex row with 2 children (status_line + mode_line)
    match &element {
        Element::Flex { children, .. } => {
            assert_eq!(children.len(), 2, "expected 2 children in status row");
        }
        other => panic!(
            "expected Flex element, got {:?}",
            std::mem::discriminant(other)
        ),
    }
}

#[test]
fn pure_status_element_empty_mode() {
    let mut db = KasaneDatabase::default();
    let handles = SalsaInputHandles::new(&mut db);
    let mut state = AppState::default();
    state.inference.status_line = vec![make_atom(":edit foo.rs")];
    // No mode line → only 1 child
    sync_inputs_from_state(&mut db, &state, &handles);

    let element = salsa_views::pure_status_element(&db, handles.status);
    match &element {
        Element::Flex { children, .. } => {
            assert_eq!(children.len(), 1, "expected 1 child when mode line empty");
        }
        other => panic!(
            "expected Flex element, got {:?}",
            std::mem::discriminant(other)
        ),
    }
}

#[test]
fn pure_status_memoization() {
    let mut db = KasaneDatabase::default();
    let handles = SalsaInputHandles::new(&mut db);
    let mut state = AppState::default();
    state.inference.status_line = vec![make_atom("status")];
    state.observed.status_mode_line = vec![make_atom("normal")];
    sync_inputs_from_state(&mut db, &state, &handles);

    let e1 = salsa_views::pure_status_element(&db, handles.status);
    // Query again without changing inputs — should return cached result
    let e2 = salsa_views::pure_status_element(&db, handles.status);
    // Same structure (PartialEq)
    assert_eq!(e1, e2);
}

// ---------------------------------------------------------------------------
// Buffer tracked function
// ---------------------------------------------------------------------------

#[test]
fn pure_buffer_element_basic() {
    let mut db = KasaneDatabase::default();
    let handles = SalsaInputHandles::new(&mut db);
    let mut state = AppState::default();
    state.runtime.rows = 24;
    sync_inputs_from_state(&mut db, &state, &handles);

    let element = salsa_views::pure_buffer_element(&db, handles.config);
    match &element {
        Element::BufferRef {
            line_range,
            line_backgrounds,
            ..
        } => {
            assert_eq!(line_range.len(), 23, "available_height = rows - 1");
            assert!(line_backgrounds.is_none());
        }
        other => panic!(
            "expected BufferRef, got {:?}",
            std::mem::discriminant(other)
        ),
    }
}

// ---------------------------------------------------------------------------
// Menu tracked function
// ---------------------------------------------------------------------------

#[test]
fn pure_menu_overlay_none_when_empty() {
    let mut db = KasaneDatabase::default();
    let handles = SalsaInputHandles::new(&mut db);
    let state = AppState::default(); // no menu
    sync_inputs_from_state(&mut db, &state, &handles);

    let overlay = salsa_views::pure_menu_overlay(&db, handles.menu, handles.config);
    assert!(overlay.is_none());
}

#[test]
fn pure_menu_overlay_inline() {
    let mut db = KasaneDatabase::default();
    let handles = SalsaInputHandles::new(&mut db);
    let mut state = AppState::default();
    state.runtime.cols = 80;
    state.runtime.rows = 24;
    state.observed.menu = Some(MenuState::new(
        vec![
            vec![make_atom("item1")],
            vec![make_atom("item2")],
            vec![make_atom("item3")],
        ],
        MenuParams {
            anchor: Coord {
                line: 5,
                column: 10,
            },
            selected_item_face: Face::default(),
            menu_face: Face::default(),
            style: MenuStyle::Inline,
            screen_w: 80,
            screen_h: 23,
            max_height: 10,
        },
    ));
    sync_inputs_from_state(&mut db, &state, &handles);

    let overlay = salsa_views::pure_menu_overlay(&db, handles.menu, handles.config);
    assert!(overlay.is_some(), "inline menu should produce overlay");
    let o = overlay.unwrap();
    match &o.anchor {
        OverlayAnchor::Absolute { w, h, .. } => {
            assert!(*w > 0);
            assert!(*h > 0);
        }
        other => panic!("expected Absolute anchor, got {:?}", other),
    }
}

#[test]
fn pure_menu_overlay_prompt() {
    let mut db = KasaneDatabase::default();
    let handles = SalsaInputHandles::new(&mut db);
    let mut state = AppState::default();
    state.runtime.cols = 80;
    state.runtime.rows = 24;
    state.observed.menu = Some(MenuState::new(
        vec![vec![make_atom("cmd1")], vec![make_atom("cmd2")]],
        MenuParams {
            anchor: Coord { line: 0, column: 0 },
            selected_item_face: Face::default(),
            menu_face: Face::default(),
            style: MenuStyle::Prompt,
            screen_w: 80,
            screen_h: 23,
            max_height: 10,
        },
    ));
    sync_inputs_from_state(&mut db, &state, &handles);

    let overlay = salsa_views::pure_menu_overlay(&db, handles.menu, handles.config);
    assert!(overlay.is_some(), "prompt menu should produce overlay");
}

#[test]
fn pure_menu_overlay_search() {
    let mut db = KasaneDatabase::default();
    let handles = SalsaInputHandles::new(&mut db);
    let mut state = AppState::default();
    state.runtime.cols = 80;
    state.runtime.rows = 24;
    state.observed.menu = Some(MenuState::new(
        vec![vec![make_atom("match1")], vec![make_atom("match2")]],
        MenuParams {
            anchor: Coord { line: 0, column: 0 },
            selected_item_face: Face::default(),
            menu_face: Face::default(),
            style: MenuStyle::Search,
            screen_w: 80,
            screen_h: 23,
            max_height: 10,
        },
    ));
    sync_inputs_from_state(&mut db, &state, &handles);

    let overlay = salsa_views::pure_menu_overlay(&db, handles.menu, handles.config);
    assert!(overlay.is_some(), "search menu should produce overlay");
    // Search menu is positioned at status row - 1
    let o = overlay.unwrap();
    if let OverlayAnchor::Absolute { h, .. } = o.anchor {
        assert_eq!(h, 1, "search menu should be 1 row tall");
    }
}

// ---------------------------------------------------------------------------
// Info tracked function
// ---------------------------------------------------------------------------

#[test]
fn pure_info_overlays_empty() {
    let mut db = KasaneDatabase::default();
    let handles = SalsaInputHandles::new(&mut db);
    let state = AppState::default(); // no infos
    sync_inputs_from_state(&mut db, &state, &handles);

    let overlays = salsa_views::pure_info_overlays(
        &db,
        handles.info,
        handles.menu,
        handles.buffer,
        handles.config,
    );
    assert!(overlays.is_empty());
}

#[test]
fn pure_info_overlays_single_modal() {
    let mut db = KasaneDatabase::default();
    let handles = SalsaInputHandles::new(&mut db);
    let mut state = AppState::default();
    state.runtime.cols = 80;
    state.runtime.rows = 24;
    state.observed.infos.push(InfoState {
        title: vec![make_atom("Help")],
        content: vec![vec![make_atom("Line 1")], vec![make_atom("Line 2")]],
        anchor: Coord {
            line: 5,
            column: 10,
        },
        face: Face::default(),
        style: InfoStyle::Modal,
        identity: InfoIdentity {
            style: InfoStyle::Modal,
            anchor_line: 5,
        },
        scroll_offset: 0,
    });
    sync_inputs_from_state(&mut db, &state, &handles);

    let overlays = salsa_views::pure_info_overlays(
        &db,
        handles.info,
        handles.menu,
        handles.buffer,
        handles.config,
    );
    assert_eq!(overlays.len(), 1, "should produce 1 info overlay");

    // Check that it has Interactive wrapper and correct style
    let (style, overlay) = &overlays[0];
    assert_eq!(*style, InfoStyle::Modal);
    match &overlay.element {
        Element::Interactive { .. } => {}
        other => panic!(
            "expected Interactive wrapper, got {:?}",
            std::mem::discriminant(other)
        ),
    }
}

#[test]
fn pure_info_overlays_multiple() {
    let mut db = KasaneDatabase::default();
    let handles = SalsaInputHandles::new(&mut db);
    let mut state = AppState::default();
    state.runtime.cols = 80;
    state.runtime.rows = 24;
    for i in 0..3u32 {
        state.observed.infos.push(InfoState {
            title: vec![make_atom(&format!("Info {i}"))],
            content: vec![vec![make_atom(&format!("Content {i}"))]],
            anchor: Coord {
                line: (i * 5) as i32,
                column: 0,
            },
            face: Face::default(),
            style: InfoStyle::Inline,
            identity: InfoIdentity {
                style: InfoStyle::Inline,
                anchor_line: i * 5,
            },
            scroll_offset: 0,
        });
    }
    sync_inputs_from_state(&mut db, &state, &handles);

    let overlays = salsa_views::pure_info_overlays(
        &db,
        handles.info,
        handles.menu,
        handles.buffer,
        handles.config,
    );
    assert_eq!(overlays.len(), 3, "should produce 3 info overlays");
}

#[test]
fn pure_info_overlays_prompt_style() {
    let mut db = KasaneDatabase::default();
    let handles = SalsaInputHandles::new(&mut db);
    let mut state = AppState::default();
    state.runtime.cols = 80;
    state.runtime.rows = 24;
    state.observed.infos.push(InfoState {
        title: vec![make_atom("Assistant")],
        content: vec![
            vec![make_atom("Welcome to Kakoune!")],
            vec![make_atom("Press :q to quit.")],
        ],
        anchor: Coord {
            line: 10,
            column: 0,
        },
        face: Face::default(),
        style: InfoStyle::Prompt,
        identity: InfoIdentity {
            style: InfoStyle::Prompt,
            anchor_line: 10,
        },
        scroll_offset: 0,
    });
    sync_inputs_from_state(&mut db, &state, &handles);

    let overlays = salsa_views::pure_info_overlays(
        &db,
        handles.info,
        handles.menu,
        handles.buffer,
        handles.config,
    );
    assert_eq!(overlays.len(), 1, "should produce 1 prompt info overlay");
}

// ---------------------------------------------------------------------------
// Memoization: changing unrelated inputs doesn't recompute
// ---------------------------------------------------------------------------

#[test]
fn menu_memoization_across_buffer_changes() {
    let mut db = KasaneDatabase::default();
    let handles = SalsaInputHandles::new(&mut db);
    let mut state = AppState::default();
    state.runtime.cols = 80;
    state.runtime.rows = 24;
    state.observed.menu = Some(MenuState::new(
        vec![vec![make_atom("item")]],
        MenuParams {
            anchor: Coord { line: 3, column: 0 },
            selected_item_face: Face::default(),
            menu_face: Face::default(),
            style: MenuStyle::Inline,
            screen_w: 80,
            screen_h: 23,
            max_height: 10,
        },
    ));
    sync_inputs_from_state(&mut db, &state, &handles);

    let m1 = salsa_views::pure_menu_overlay(&db, handles.menu, handles.config);

    // Change buffer content only — menu should use cached result
    state.observed.lines = vec![vec![make_atom("hello world")]];
    sync_inputs_from_state(&mut db, &state, &handles);

    let m2 = salsa_views::pure_menu_overlay(&db, handles.menu, handles.config);
    assert_eq!(
        m1, m2,
        "menu overlay should be unchanged after buffer-only update"
    );
}
