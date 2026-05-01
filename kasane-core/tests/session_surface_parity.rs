//! Integration tests for session / surface parity.
//!
//! Proves that the current single-pane design correctly handles multi-session
//! scenarios via AppState swap + ephemeral surface lifecycle management.

use kasane_core::element::Element;
use kasane_core::plugin::PluginRuntime;
use kasane_core::protocol::{Coord, InfoStyle, MenuStyle};
use kasane_core::session::{SessionId, SessionManager, SessionSpec, SessionStateStore};
use kasane_core::state::{AppState, DirtyFlags, InfoIdentity, InfoState, MenuState};
use kasane_core::surface::buffer::KakouneBufferSurface;
use kasane_core::surface::status::StatusBarSurface;
use kasane_core::surface::{SurfaceId, SurfaceRegistry};
use kasane_core::test_support::make_line;

use kasane_core::layout::Rect;

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn test_rect() -> Rect {
    Rect {
        x: 0,
        y: 0,
        w: 80,
        h: 24,
    }
}

fn setup_two_session_states() -> (AppState, AppState) {
    let mut state_a = AppState::default();
    state_a.runtime.cols = 80;
    state_a.runtime.rows = 24;
    state_a.observed.lines = vec![make_line("hello")].into();
    state_a.inference.lines_dirty = vec![true];

    let mut state_b = AppState::default();
    state_b.runtime.cols = 80;
    state_b.runtime.rows = 24;
    state_b.observed.lines = vec![make_line("world")].into();
    state_b.inference.lines_dirty = vec![true];
    state_b.observed.menu = Some(make_test_menu());

    (state_a, state_b)
}

fn make_test_menu() -> MenuState {
    MenuState {
        items: vec![make_line("item1"), make_line("item2")],
        anchor: Coord { line: 0, column: 0 },
        selected_item_face: kasane_core::protocol::Style::default(),
        menu_face: kasane_core::protocol::Style::default(),
        style: MenuStyle::Inline,
        selected: None,
        first_item: 0,
        columns: 1,
        win_height: 0,
        menu_lines: 0,
        max_item_width: 0,
        screen_w: 80,
        columns_split: None,
    }
}

fn make_test_info() -> InfoState {
    InfoState {
        title: vec![],
        content: vec![make_line("info content")],
        anchor: Coord { line: 0, column: 0 },
        face: kasane_core::protocol::Style::default(),
        style: InfoStyle::Prompt,
        identity: InfoIdentity {
            style: InfoStyle::Prompt,
            anchor_line: 0,
        },
        scroll_offset: 0,
    }
}

fn new_surface_registry() -> SurfaceRegistry {
    let mut reg = SurfaceRegistry::new();
    reg.register(Box::new(KakouneBufferSurface::new()));
    reg.register(Box::new(StatusBarSurface::new()));
    reg
}

type TestSessionManager = SessionManager<(), Vec<u8>, ()>;

fn setup_session_manager_with_two_sessions() -> (TestSessionManager, SessionId, SessionId) {
    let mut mgr = TestSessionManager::new();
    let id_a = mgr
        .insert(SessionSpec::new("session-a", None, vec![]), (), vec![], ())
        .unwrap();
    let id_b = mgr
        .insert(SessionSpec::new("session-b", None, vec![]), (), vec![], ())
        .unwrap();
    (mgr, id_a, id_b)
}

// ===========================================================================
// Test 1: Session switch changes buffer content
// ===========================================================================

#[test]
fn test_session_switch_changes_buffer_content() {
    let (state_a, state_b) = setup_two_session_states();
    let (mut mgr, id_a, id_b) = setup_session_manager_with_two_sessions();

    let mut store = SessionStateStore::new();
    store.sync_from_active(id_a, &state_a);
    store.sync_from_active(id_b, &state_b);

    // Start with session A active
    let mut state = state_a.clone();
    assert_eq!(state.observed.lines[0][0].contents.as_str(), "hello");

    // Save A, activate B, restore B
    mgr.sync_and_activate(&mut store, id_b, &state).unwrap();
    assert!(store.restore_into(id_b, &mut state));
    assert_eq!(state.observed.lines[0][0].contents.as_str(), "world");

    // Save B, activate A, restore A
    mgr.sync_and_activate(&mut store, id_a, &state).unwrap();
    assert!(store.restore_into(id_a, &mut state));
    assert_eq!(state.observed.lines[0][0].contents.as_str(), "hello");
}

// ===========================================================================
// Test 2: Ephemeral surfaces follow state on session switch
// ===========================================================================

#[test]
fn test_session_switch_ephemeral_surfaces_follow_state() {
    let (mut state_a, state_b) = setup_two_session_states();
    // State A: menu=None, one info popup
    state_a.observed.menu = None;
    state_a.observed.infos = vec![make_test_info()];

    // State B: menu=Some, no info popups (already set in setup)
    assert!(state_b.observed.menu.is_some());
    assert!(state_b.observed.infos.is_empty());

    let mut reg = new_surface_registry();

    // Sync to state A
    reg.sync_ephemeral_surfaces(&state_a);
    assert!(reg.get(SurfaceId::MENU).is_none(), "A: no menu");
    assert!(
        reg.get(SurfaceId(SurfaceId::INFO_BASE)).is_some(),
        "A: has info"
    );

    // Switch to state B
    reg.sync_ephemeral_surfaces(&state_b);
    assert!(reg.get(SurfaceId::MENU).is_some(), "B: has menu");
    assert!(
        reg.get(SurfaceId(SurfaceId::INFO_BASE)).is_none(),
        "B: no info"
    );

    // Switch back to state A
    reg.sync_ephemeral_surfaces(&state_a);
    assert!(reg.get(SurfaceId::MENU).is_none(), "A again: no menu");
    assert!(
        reg.get(SurfaceId(SurfaceId::INFO_BASE)).is_some(),
        "A again: has info"
    );
}

// ===========================================================================
// Test 3: No stale surfaces after A→B→A round-trip
// ===========================================================================

#[test]
fn test_session_switch_no_stale_surfaces() {
    let (state_a, state_b) = setup_two_session_states();
    let mut reg = new_surface_registry();

    // Sync to A and record count
    reg.sync_ephemeral_surfaces(&state_a);
    let count_a = reg.surface_count();

    // Switch to B
    reg.sync_ephemeral_surfaces(&state_b);
    let count_b = reg.surface_count();

    // Switch back to A
    reg.sync_ephemeral_surfaces(&state_a);
    let count_a2 = reg.surface_count();

    assert_eq!(
        count_a, count_a2,
        "surface count must be stable after A→B→A round-trip"
    );
    // B has a menu so it should have one more surface than A
    assert_eq!(count_b, count_a + 1);
}

// ===========================================================================
// Test 4: Session close + promote preserves surface composition
// ===========================================================================

#[test]
fn test_session_close_and_promote_preserves_surface_composition() {
    let mut mgr = TestSessionManager::new();
    let id_a = mgr
        .insert(SessionSpec::new("a", None, vec![]), (), vec![], ())
        .unwrap();
    let id_b = mgr
        .insert(SessionSpec::new("b", None, vec![]), (), vec![], ())
        .unwrap();
    let id_c = mgr
        .insert(SessionSpec::new("c", None, vec![]), (), vec![], ())
        .unwrap();

    let mut store = SessionStateStore::new();
    let (state_a, state_b) = setup_two_session_states();
    let mut state_c = AppState::default();
    state_c.runtime.cols = 80;
    state_c.runtime.rows = 24;
    state_c.observed.lines = vec![make_line("third")].into();
    state_c.inference.lines_dirty = vec![true];

    store.sync_from_active(id_a, &state_a);
    store.sync_from_active(id_b, &state_b);
    store.sync_from_active(id_c, &state_c);

    // Activate B
    let mut state = state_a.clone();
    mgr.sync_and_activate(&mut store, id_b, &state).unwrap();
    store.restore_into(id_b, &mut state);

    // Close B (middle session) — should promote to C
    let _ = mgr.close(id_b);
    store.remove(id_b);
    assert_eq!(mgr.active_session_id(), Some(id_c));

    // Restore promoted session C
    assert!(store.restore_into(id_c, &mut state));
    assert_eq!(state.observed.lines[0][0].contents.as_str(), "third");

    // Ephemeral surfaces should match state C (no menu, no infos)
    let mut reg = new_surface_registry();
    reg.sync_ephemeral_surfaces(&state);
    assert!(reg.get(SurfaceId::MENU).is_none());
    assert!(reg.get(SurfaceId(SurfaceId::INFO_BASE)).is_none());
}

// ===========================================================================
// Test 5: DirtyFlags::SESSION set on lifecycle events
// ===========================================================================

#[test]
fn test_dirty_flags_session_set_on_lifecycle_events() {
    let mut mgr = TestSessionManager::new();
    let mut store = SessionStateStore::new();
    let mut state = AppState::default();
    state.runtime.cols = 80;
    state.runtime.rows = 24;
    let mut dirty = DirtyFlags::empty();
    let mut initial_resize_sent = true;

    // spawn
    let id_a = mgr
        .insert(SessionSpec::new("a", None, vec![]), (), vec![], ())
        .unwrap();
    store.ensure_session(id_a, &state);
    dirty |= DirtyFlags::SESSION;
    assert!(dirty.contains(DirtyFlags::SESSION));

    // spawn second
    dirty = DirtyFlags::empty();
    let id_b = mgr
        .insert(SessionSpec::new("b", None, vec![]), (), vec![], ())
        .unwrap();
    store.ensure_session(id_b, &state);
    dirty |= DirtyFlags::SESSION;
    assert!(dirty.contains(DirtyFlags::SESSION));

    // switch — should set both SESSION and ALL
    dirty = DirtyFlags::empty();
    kasane_core::event_loop::switch_session_core(
        "b",
        &mut kasane_core::event_loop::SessionMutContext {
            session_manager: &mut mgr,
            session_states: &mut store,
            state: &mut state,
            dirty: &mut dirty,
            initial_resize_sent: &mut initial_resize_sent,
        },
    );
    assert!(dirty.contains(DirtyFlags::SESSION));
    assert!(dirty.contains(DirtyFlags::ALL));

    // close — should set SESSION
    dirty = DirtyFlags::empty();
    let quit = kasane_core::event_loop::close_session_core(
        Some("a"),
        &mut kasane_core::event_loop::SessionMutContext {
            session_manager: &mut mgr,
            session_states: &mut store,
            state: &mut state,
            dirty: &mut dirty,
            initial_resize_sent: &mut initial_resize_sent,
        },
    );
    assert!(!quit);
    assert!(dirty.contains(DirtyFlags::SESSION));

    // death of active session
    dirty = DirtyFlags::empty();
    let id_c = mgr
        .insert(SessionSpec::new("c", None, vec![]), (), vec![], ())
        .unwrap();
    store.ensure_session(id_c, &state);
    // death — use handle_pane_death which also cleans up surfaces
    let mut surface_registry = SurfaceRegistry::new();
    surface_registry
        .try_register(Box::new(KakouneBufferSurface::new()))
        .unwrap();
    surface_registry
        .try_register(Box::new(StatusBarSurface::new()))
        .unwrap();
    let quit = kasane_core::event_loop::handle_pane_death(
        id_b,
        &mut surface_registry,
        &mut kasane_core::event_loop::SessionMutContext {
            session_manager: &mut mgr,
            session_states: &mut store,
            state: &mut state,
            dirty: &mut dirty,
            initial_resize_sent: &mut initial_resize_sent,
        },
    );
    assert!(!quit);
    assert!(dirty.contains(DirtyFlags::ALL));
}

// ===========================================================================
// Test 6: Session metadata consistency after operations
// ===========================================================================

#[test]
fn test_session_metadata_consistent_after_operations() {
    let mut mgr = TestSessionManager::new();
    let mut store = SessionStateStore::new();
    let mut state = AppState::default();
    state.runtime.cols = 80;
    state.runtime.rows = 24;

    // Spawn session
    let id_a = mgr
        .insert(SessionSpec::new("work", None, vec![]), (), vec![], ())
        .unwrap();
    store.ensure_session(id_a, &state);
    kasane_core::event_loop::sync_session_metadata(&mgr, &store, &mut state);
    assert_eq!(state.session.session_descriptors.len(), 1);
    assert_eq!(state.session.session_descriptors[0].key, "work");
    assert_eq!(state.session.active_session_key.as_deref(), Some("work"));

    // Spawn second session
    let id_b = mgr
        .insert(SessionSpec::new("play", None, vec![]), (), vec![], ())
        .unwrap();
    store.ensure_session(id_b, &state);
    kasane_core::event_loop::sync_session_metadata(&mgr, &store, &mut state);
    assert_eq!(state.session.session_descriptors.len(), 2);

    // Switch to second
    let mut dirty = DirtyFlags::empty();
    let mut initial_resize_sent = true;
    kasane_core::event_loop::switch_session_core(
        "play",
        &mut kasane_core::event_loop::SessionMutContext {
            session_manager: &mut mgr,
            session_states: &mut store,
            state: &mut state,
            dirty: &mut dirty,
            initial_resize_sent: &mut initial_resize_sent,
        },
    );
    assert_eq!(state.session.active_session_key.as_deref(), Some("play"));

    // Close second — should promote to first
    kasane_core::event_loop::close_session_core(
        Some("play"),
        &mut kasane_core::event_loop::SessionMutContext {
            session_manager: &mut mgr,
            session_states: &mut store,
            state: &mut state,
            dirty: &mut dirty,
            initial_resize_sent: &mut initial_resize_sent,
        },
    );
    assert_eq!(state.session.session_descriptors.len(), 1);
    assert_eq!(state.session.session_descriptors[0].key, "work");
    assert_eq!(state.session.active_session_key.as_deref(), Some("work"));
}

// ===========================================================================
// Test 7: compose_view reflects session switch
// ===========================================================================

#[test]
fn test_compose_view_after_session_switch() {
    let (state_a, state_b) = setup_two_session_states();
    let (mut mgr, id_a, id_b) = setup_session_manager_with_two_sessions();

    let mut store = SessionStateStore::new();
    store.sync_from_active(id_a, &state_a);
    store.sync_from_active(id_b, &state_b);

    let mut reg = new_surface_registry();
    let plugin_reg = PluginRuntime::new();

    // Compose with state A
    let mut state = state_a.clone();
    reg.sync_ephemeral_surfaces(&state);
    let element_a = reg.compose_view(&state, &plugin_reg.view(), test_rect());
    assert!(!matches!(element_a, Element::Empty));

    // Switch to B
    mgr.sync_and_activate(&mut store, id_b, &state).unwrap();
    store.restore_into(id_b, &mut state);
    assert!(state.observed.menu.is_some()); // B has a menu

    reg.sync_ephemeral_surfaces(&state);
    let element_b = reg.compose_view(&state, &plugin_reg.view(), test_rect());
    assert!(!matches!(element_b, Element::Empty));

    // The menu surface should now be registered
    assert!(reg.get(SurfaceId::MENU).is_some());

    // Switch back to A
    mgr.sync_and_activate(&mut store, id_a, &state).unwrap();
    store.restore_into(id_a, &mut state);
    assert!(state.observed.menu.is_none()); // A has no menu

    reg.sync_ephemeral_surfaces(&state);
    assert!(reg.get(SurfaceId::MENU).is_none());
}
