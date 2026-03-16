use crate::protocol::{Coord, Face, KakouneRequest, MenuStyle};
use crate::state::{AppState, DirtyFlags};
use crate::test_utils::make_line;

// --- DirtyFlags split MENU tests ---

#[test]
fn test_menu_show_returns_menu_structure() {
    let mut state = AppState::default();
    let flags = state.apply(KakouneRequest::MenuShow {
        items: vec![make_line("a")],
        anchor: Coord { line: 0, column: 0 },
        selected_item_face: Face::default(),
        menu_face: Face::default(),
        style: MenuStyle::Inline,
    });
    assert!(flags.contains(DirtyFlags::MENU_STRUCTURE));
    assert!(!flags.contains(DirtyFlags::MENU_SELECTION));
}

#[test]
fn test_menu_select_returns_menu_selection() {
    let mut state = AppState::default();
    state.apply(KakouneRequest::MenuShow {
        items: vec![make_line("a"), make_line("b")],
        anchor: Coord { line: 0, column: 0 },
        selected_item_face: Face::default(),
        menu_face: Face::default(),
        style: MenuStyle::Inline,
    });
    let flags = state.apply(KakouneRequest::MenuSelect { selected: 0 });
    assert!(flags.contains(DirtyFlags::MENU_SELECTION));
    assert!(!flags.contains(DirtyFlags::MENU_STRUCTURE));
}

#[test]
fn test_menu_hide_returns_both_menu_flags() {
    let mut state = AppState::default();
    state.apply(KakouneRequest::MenuShow {
        items: vec![make_line("a")],
        anchor: Coord { line: 0, column: 0 },
        selected_item_face: Face::default(),
        menu_face: Face::default(),
        style: MenuStyle::Inline,
    });
    let flags = state.apply(KakouneRequest::MenuHide);
    assert!(flags.contains(DirtyFlags::MENU_STRUCTURE));
    assert!(flags.contains(DirtyFlags::MENU_SELECTION));
    assert!(flags.contains(DirtyFlags::BUFFER_CONTENT));
}

#[test]
fn test_menu_composite_contains_sub_flags() {
    assert!(DirtyFlags::MENU.contains(DirtyFlags::MENU_STRUCTURE));
    assert!(DirtyFlags::MENU.contains(DirtyFlags::MENU_SELECTION));
    assert!(DirtyFlags::ALL.contains(DirtyFlags::MENU_STRUCTURE));
    assert!(DirtyFlags::ALL.contains(DirtyFlags::MENU_SELECTION));
}

#[test]
fn test_available_height() {
    let mut state = AppState::default();
    state.rows = 24;
    assert_eq!(state.available_height(), 23);

    state.rows = 1;
    assert_eq!(state.available_height(), 0);
}

// --- Line-level dirty tracking tests ---

#[test]
fn test_apply_draw_lines_dirty_single_change() {
    let mut state = AppState::default();
    state.apply(KakouneRequest::Draw {
        lines: vec![make_line("aaa"), make_line("bbb"), make_line("ccc")],
        cursor_pos: Coord::default(),
        default_face: Face::default(),
        padding_face: Face::default(),
        widget_columns: 0,
    });

    // Change only middle line
    state.apply(KakouneRequest::Draw {
        lines: vec![make_line("aaa"), make_line("BBB"), make_line("ccc")],
        cursor_pos: Coord::default(),
        default_face: Face::default(),
        padding_face: Face::default(),
        widget_columns: 0,
    });
    assert_eq!(state.lines_dirty, vec![false, true, false]);
}

#[test]
fn test_apply_draw_lines_dirty_face_change() {
    let mut state = AppState::default();
    state.apply(KakouneRequest::Draw {
        lines: vec![make_line("aaa"), make_line("bbb")],
        cursor_pos: Coord::default(),
        default_face: Face::default(),
        padding_face: Face::default(),
        widget_columns: 0,
    });

    // Same lines but different default_face → all dirty
    let new_face = Face {
        fg: crate::protocol::Color::Named(crate::protocol::NamedColor::Red),
        ..Face::default()
    };
    state.apply(KakouneRequest::Draw {
        lines: vec![make_line("aaa"), make_line("bbb")],
        cursor_pos: Coord::default(),
        default_face: new_face,
        padding_face: Face::default(),
        widget_columns: 0,
    });
    assert_eq!(state.lines_dirty, vec![true, true]);
}

#[test]
fn test_apply_draw_lines_dirty_length_change() {
    let mut state = AppState::default();
    state.apply(KakouneRequest::Draw {
        lines: vec![make_line("aaa"), make_line("bbb")],
        cursor_pos: Coord::default(),
        default_face: Face::default(),
        padding_face: Face::default(),
        widget_columns: 0,
    });

    // Different number of lines → all dirty
    state.apply(KakouneRequest::Draw {
        lines: vec![make_line("aaa"), make_line("bbb"), make_line("ccc")],
        cursor_pos: Coord::default(),
        default_face: Face::default(),
        padding_face: Face::default(),
        widget_columns: 0,
    });
    assert_eq!(state.lines_dirty, vec![true, true, true]);
}

#[test]
fn test_apply_draw_lines_dirty_no_change() {
    let mut state = AppState::default();
    state.apply(KakouneRequest::Draw {
        lines: vec![make_line("aaa"), make_line("bbb")],
        cursor_pos: Coord::default(),
        default_face: Face::default(),
        padding_face: Face::default(),
        widget_columns: 0,
    });

    // Identical draw → all clean
    state.apply(KakouneRequest::Draw {
        lines: vec![make_line("aaa"), make_line("bbb")],
        cursor_pos: Coord::default(),
        default_face: Face::default(),
        padding_face: Face::default(),
        widget_columns: 0,
    });
    assert_eq!(state.lines_dirty, vec![false, false]);
}

#[test]
fn test_apply_draw_lines_dirty_first_draw() {
    let mut state = AppState::default();
    // First draw (no prior lines) → all dirty
    state.apply(KakouneRequest::Draw {
        lines: vec![make_line("aaa"), make_line("bbb")],
        cursor_pos: Coord::default(),
        default_face: Face::default(),
        padding_face: Face::default(),
        widget_columns: 0,
    });
    assert_eq!(state.lines_dirty, vec![true, true]);
}

#[test]
fn test_menu_select_no_scroll_returns_selection_only() {
    let mut state = AppState::default();
    state.rows = 24;
    state.cols = 80;
    // 3 items fit in win_height without scrolling
    state.apply(KakouneRequest::MenuShow {
        items: vec![make_line("a"), make_line("b"), make_line("c")],
        anchor: Coord { line: 0, column: 0 },
        selected_item_face: Face::default(),
        menu_face: Face::default(),
        style: MenuStyle::Inline,
    });
    state.apply(KakouneRequest::MenuSelect { selected: 0 });

    // Moving selection within the same visible window → no scroll
    let flags = state.apply(KakouneRequest::MenuSelect { selected: 1 });
    assert!(flags.contains(DirtyFlags::MENU_SELECTION));
    assert!(!flags.contains(DirtyFlags::MENU_STRUCTURE));
}

#[test]
fn test_menu_select_with_scroll_returns_structure() {
    let mut state = AppState::default();
    state.rows = 24;
    state.cols = 80;
    // Many items: win_height will be limited, so scrolling past visible range triggers first_item change
    let items: Vec<_> = (0..30).map(|i| make_line(&format!("item{i}"))).collect();
    state.apply(KakouneRequest::MenuShow {
        items,
        anchor: Coord { line: 0, column: 0 },
        selected_item_face: Face::default(),
        menu_face: Face::default(),
        style: MenuStyle::Inline,
    });
    state.apply(KakouneRequest::MenuSelect { selected: 0 });
    let first_before = state.menu.as_ref().unwrap().first_item;

    // Select an item far enough to force scroll (beyond win_height * columns)
    let flags = state.apply(KakouneRequest::MenuSelect { selected: 25 });
    let first_after = state.menu.as_ref().unwrap().first_item;

    // first_item must have changed → MENU_STRUCTURE should be set
    assert_ne!(first_before, first_after, "scroll should have occurred");
    assert!(flags.contains(DirtyFlags::MENU_SELECTION));
    assert!(flags.contains(DirtyFlags::MENU_STRUCTURE));
}
