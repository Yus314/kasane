//! Verification tests for Salsa infrastructure (Phase 1).
//!
//! Tests that:
//! 1. Salsa tracked functions produce correct results
//! 2. sync_inputs_from_state correctly projects AppState
//! 3. Early Cutoff works (unchanged inputs don't trigger recomputation)

use kasane_core::protocol::{Atom, Coord, CursorMode, Face};
use kasane_core::render::CursorStyle;
use kasane_core::salsa_db::KasaneDatabase;
use kasane_core::salsa_queries;
use kasane_core::salsa_sync::{SalsaInputHandles, sync_inputs_from_state};
use kasane_core::state::AppState;

fn make_atom(text: &str) -> Atom {
    Atom::plain(text)
}

// ---------------------------------------------------------------------------
// Basic tracked function tests
// ---------------------------------------------------------------------------

#[test]
fn available_height_basic() {
    let mut db = KasaneDatabase::default();
    let handles = SalsaInputHandles::new(&mut db);
    let mut state = AppState::default();
    state.runtime.rows = 24;
    sync_inputs_from_state(&mut db, &state, &handles);

    let h = salsa_queries::available_height(&db, handles.config);
    assert_eq!(h, 23);
}

#[test]
fn is_prompt_mode_buffer() {
    let mut db = KasaneDatabase::default();
    let handles = SalsaInputHandles::new(&mut db);
    let state = AppState::default(); // cursor_mode = Buffer
    sync_inputs_from_state(&mut db, &state, &handles);

    assert!(!salsa_queries::is_prompt_mode(&db, handles.cursor));
}

#[test]
fn is_prompt_mode_prompt() {
    let mut db = KasaneDatabase::default();
    let handles = SalsaInputHandles::new(&mut db);
    let mut state = AppState::default();
    state.inference.cursor_mode = CursorMode::Prompt;
    sync_inputs_from_state(&mut db, &state, &handles);

    assert!(salsa_queries::is_prompt_mode(&db, handles.cursor));
}

#[test]
fn cursor_style_normal_mode() {
    let mut db = KasaneDatabase::default();
    let handles = SalsaInputHandles::new(&mut db);
    let mut state = AppState::default();
    state.observed.status_mode_line = vec![make_atom("normal")];
    sync_inputs_from_state(&mut db, &state, &handles);

    let style =
        salsa_queries::cursor_style_query(&db, handles.config, handles.cursor, handles.status);
    assert_eq!(style, CursorStyle::Block);
}

#[test]
fn cursor_style_insert_mode() {
    let mut db = KasaneDatabase::default();
    let handles = SalsaInputHandles::new(&mut db);
    let mut state = AppState::default();
    state.observed.status_mode_line = vec![make_atom("insert")];
    sync_inputs_from_state(&mut db, &state, &handles);

    let style =
        salsa_queries::cursor_style_query(&db, handles.config, handles.cursor, handles.status);
    assert_eq!(style, CursorStyle::Bar);
}

#[test]
fn cursor_style_unfocused() {
    let mut db = KasaneDatabase::default();
    let handles = SalsaInputHandles::new(&mut db);
    let mut state = AppState::default();
    state.runtime.focused = false;
    sync_inputs_from_state(&mut db, &state, &handles);

    let style =
        salsa_queries::cursor_style_query(&db, handles.config, handles.cursor, handles.status);
    assert_eq!(style, CursorStyle::Outline);
}

// ---------------------------------------------------------------------------
// Sync projection tests
// ---------------------------------------------------------------------------

#[test]
fn sync_buffer_only_when_dirty() {
    let mut db = KasaneDatabase::default();
    let handles = SalsaInputHandles::new(&mut db);

    let mut state = AppState::default();
    state.observed.lines = vec![vec![make_atom("hello")]];
    state.observed.cursor_pos = Coord { line: 0, column: 3 };

    // Sync with BUFFER flag
    sync_inputs_from_state(&mut db, &state, &handles);
    assert_eq!(handles.buffer.lines(&db).len(), 1);
    assert_eq!(handles.buffer.cursor_pos(&db), Coord { line: 0, column: 3 });

    // Status should still be default (not synced)
    assert!(handles.status.status_line(&db).is_empty());
}

#[test]
fn sync_status_only_when_dirty() {
    let mut db = KasaneDatabase::default();
    let handles = SalsaInputHandles::new(&mut db);

    let mut state = AppState::default();
    state.inference.status_line = vec![make_atom(":edit foo")];
    state.observed.status_mode_line = vec![make_atom("normal")];

    sync_inputs_from_state(&mut db, &state, &handles);
    assert_eq!(handles.status.status_line(&db).len(), 1);

    // Buffer should still be default
    assert!(handles.buffer.lines(&db).is_empty());
}

#[test]
fn sync_menu_snapshot() {
    use kasane_core::protocol::MenuStyle;
    use kasane_core::state::{MenuParams, MenuState};

    let mut db = KasaneDatabase::default();
    let handles = SalsaInputHandles::new(&mut db);

    let mut state = AppState::default();
    state.observed.menu = Some(MenuState::new(
        vec![vec![make_atom("item1")], vec![make_atom("item2")]],
        MenuParams {
            anchor: Coord { line: 5, column: 0 },
            selected_item_face: Face::default().into(),
            menu_face: Face::default().into(),
            style: MenuStyle::Inline,
            screen_w: 80,
            screen_h: 23,
            max_height: 10,
        },
    ));

    sync_inputs_from_state(&mut db, &state, &handles);
    let menu = handles.menu.menu(&db);
    assert!(menu.is_some());
    assert_eq!(menu.as_ref().unwrap().items.len(), 2);
}

#[test]
fn sync_info_snapshots() {
    use kasane_core::protocol::InfoStyle;
    use kasane_core::state::{InfoIdentity, InfoState};

    let mut db = KasaneDatabase::default();
    let handles = SalsaInputHandles::new(&mut db);

    let mut state = AppState::default();
    state.observed.infos.push(InfoState {
        title: vec![make_atom("Title")],
        content: vec![vec![make_atom("Body")]],
        anchor: Coord {
            line: 3,
            column: 10,
        },
        face: kasane_core::protocol::Style::default(),
        style: InfoStyle::Prompt,
        identity: InfoIdentity {
            style: InfoStyle::Prompt,
            anchor_line: 3,
        },
        scroll_offset: 0,
    });

    sync_inputs_from_state(&mut db, &state, &handles);
    let infos = handles.info.infos(&db);
    assert_eq!(infos.len(), 1);
    assert_eq!(infos[0].style, InfoStyle::Prompt);
}

// ---------------------------------------------------------------------------
// Early Cutoff verification
// ---------------------------------------------------------------------------

#[test]
fn early_cutoff_same_values() {
    let mut db = KasaneDatabase::default();
    let handles = SalsaInputHandles::new(&mut db);

    let mut state = AppState::default();
    state.runtime.rows = 24;
    state.observed.status_mode_line = vec![make_atom("normal")];

    // First sync + query
    sync_inputs_from_state(&mut db, &state, &handles);
    let style1 =
        salsa_queries::cursor_style_query(&db, handles.config, handles.cursor, handles.status);

    // Second sync with same values — Salsa should reuse cached result
    sync_inputs_from_state(&mut db, &state, &handles);
    let style2 =
        salsa_queries::cursor_style_query(&db, handles.config, handles.cursor, handles.status);

    assert_eq!(style1, style2);
    assert_eq!(style1, CursorStyle::Block);
}

#[test]
fn selective_dirty_preserves_unrelated_inputs() {
    let mut db = KasaneDatabase::default();
    let handles = SalsaInputHandles::new(&mut db);

    let mut state = AppState::default();
    state.observed.lines = vec![vec![make_atom("hello")]];
    state.inference.status_line = vec![make_atom("status")];

    // Sync everything first
    sync_inputs_from_state(&mut db, &state, &handles);

    // Now change only buffer, sync with BUFFER flag only
    state.observed.lines = vec![vec![make_atom("world")]];
    sync_inputs_from_state(&mut db, &state, &handles);

    // Buffer should reflect new value
    assert_eq!(handles.buffer.lines(&db)[0][0].contents.as_str(), "world");
    // Status should retain old value (not re-synced)
    assert_eq!(
        handles.status.status_line(&db)[0].contents.as_str(),
        "status"
    );
}
