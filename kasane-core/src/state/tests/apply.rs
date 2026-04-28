use crate::protocol::{
    Attributes, Coord, CursorMode, Face, InfoStyle, KakouneRequest, MenuStyle, StatusStyle,
};
use crate::state::{AppState, DirtyFlags, MenuState};
use crate::test_utils::make_line;

use crate::protocol::{Atom, Color, Line, NamedColor};

#[test]
fn test_apply_draw() {
    let mut state = AppState::default();
    let flags = state.apply(KakouneRequest::Draw {
        lines: vec![make_line("hello")],
        cursor_pos: Coord { line: 0, column: 3 },
        default_face: Face::default(),
        padding_face: Face::default(),
        widget_columns: 0,
    });
    assert!(flags.contains(DirtyFlags::BUFFER));
    assert_eq!(state.observed.lines.len(), 1);
}

#[test]
fn test_draw_updates_cursor_pos() {
    let mut state = AppState::default();
    state.apply(KakouneRequest::Draw {
        lines: vec![make_line("hello")],
        cursor_pos: Coord {
            line: 5,
            column: 10,
        },
        default_face: Face::default(),
        padding_face: Face::default(),
        widget_columns: 0,
    });
    assert_eq!(
        state.observed.cursor_pos,
        Coord {
            line: 5,
            column: 10
        }
    );
}

#[test]
fn test_draw_stores_widget_columns() {
    let mut state = AppState::default();
    state.apply(KakouneRequest::Draw {
        lines: vec![make_line("hello")],
        cursor_pos: Coord::default(),
        default_face: Face::default(),
        padding_face: Face::default(),
        widget_columns: 3,
    });
    assert_eq!(state.observed.widget_columns, 3);
}

#[test]
fn test_draw_status_derives_cursor_mode_prompt() {
    let mut state = AppState::default();
    let flags = state.apply(KakouneRequest::DrawStatus {
        prompt: make_line(":"),
        content: make_line("quit"),
        content_cursor_pos: 4,
        mode_line: make_line("normal"),
        default_face: Face::default(),
        style: StatusStyle::Command,
    });
    assert!(flags.contains(DirtyFlags::STATUS));
    assert!(flags.contains(DirtyFlags::BUFFER_CURSOR)); // mode changed
    assert_eq!(state.inference.cursor_mode, CursorMode::Prompt);
    assert_eq!(state.observed.status_content_cursor_pos, 4);
}

#[test]
fn test_draw_status_derives_cursor_mode_buffer() {
    let mut state = AppState::default();
    let flags = state.apply(KakouneRequest::DrawStatus {
        prompt: make_line(""),
        content: make_line(""),
        content_cursor_pos: -1,
        mode_line: make_line("normal"),
        default_face: Face::default(),
        style: StatusStyle::Status,
    });
    assert!(flags.contains(DirtyFlags::STATUS));
    assert!(!flags.contains(DirtyFlags::BUFFER)); // mode unchanged (already Buffer)
    assert_eq!(state.inference.cursor_mode, CursorMode::Buffer);
}

#[test]
fn test_apply_draw_status() {
    let mut state = AppState::default();
    let flags = state.apply(KakouneRequest::DrawStatus {
        prompt: make_line(":"),
        content: make_line("q"),
        content_cursor_pos: 1,
        mode_line: make_line("insert"),
        default_face: Face::default(),
        style: StatusStyle::Command,
    });
    assert!(flags.contains(DirtyFlags::STATUS));
    assert_eq!(state.observed.status_prompt[0].contents, ":");
    assert_eq!(state.observed.status_content[0].contents, "q");
    // Combined status_line = prompt + content
    assert_eq!(state.inference.status_line[0].contents, ":");
    assert_eq!(state.inference.status_line[1].contents, "q");
    assert_eq!(state.observed.status_mode_line[0].contents, "insert");
}

#[test]
fn test_apply_menu_show_select_hide() {
    let mut state = AppState::default();

    state.apply(KakouneRequest::MenuShow {
        items: vec![make_line("a"), make_line("b")],
        anchor: Coord { line: 1, column: 0 },
        selected_item_face: Face::default().into(),
        menu_face: Face::default().into(),
        style: MenuStyle::Inline,
    });
    assert!(state.observed.menu.is_some());
    assert_eq!(state.observed.menu.as_ref().unwrap().selected, None);

    state.apply(KakouneRequest::MenuSelect { selected: 1 });
    assert_eq!(state.observed.menu.as_ref().unwrap().selected, Some(1));

    let flags = state.apply(KakouneRequest::MenuHide);
    assert!(state.observed.menu.is_none());
    assert!(flags.contains(DirtyFlags::MENU));
}

#[test]
fn test_apply_info_show_hide() {
    let mut state = AppState::default();

    state.apply(KakouneRequest::InfoShow {
        title: make_line("Help"),
        content: vec![make_line("content")],
        anchor: Coord { line: 0, column: 0 },
        face: Face::default(),
        style: InfoStyle::Modal,
    });
    assert_eq!(state.observed.infos.len(), 1);

    let flags = state.apply(KakouneRequest::InfoHide);
    assert!(state.observed.infos.is_empty());
    assert!(flags.contains(DirtyFlags::INFO));
}

#[test]
fn test_apply_multiple_infos() {
    let mut state = AppState::default();

    // Show first info (Modal at line 0)
    state.apply(KakouneRequest::InfoShow {
        title: make_line("Help"),
        content: vec![make_line("content1")],
        anchor: Coord { line: 0, column: 0 },
        face: Face::default(),
        style: InfoStyle::Modal,
    });
    assert_eq!(state.observed.infos.len(), 1);

    // Show second info (Inline at line 5) — different identity, coexists
    state.apply(KakouneRequest::InfoShow {
        title: make_line("Lint"),
        content: vec![make_line("error here")],
        anchor: Coord { line: 5, column: 0 },
        face: Face::default(),
        style: InfoStyle::Inline,
    });
    assert_eq!(state.observed.infos.len(), 2);

    // Show info with same identity (Modal at line 0) — replaces first
    state.apply(KakouneRequest::InfoShow {
        title: make_line("Updated Help"),
        content: vec![make_line("new content")],
        anchor: Coord { line: 0, column: 0 },
        face: Face::default(),
        style: InfoStyle::Modal,
    });
    assert_eq!(state.observed.infos.len(), 2);
    assert_eq!(state.observed.infos[0].title[0].contents, "Updated Help");

    // Hide removes most recent
    state.apply(KakouneRequest::InfoHide);
    assert_eq!(state.observed.infos.len(), 1);
}

#[test]
fn test_apply_set_ui_options() {
    let mut state = AppState::default();
    let mut opts = std::collections::HashMap::new();
    opts.insert("key".to_string(), "value".to_string());
    let flags = state.apply(KakouneRequest::SetUiOptions {
        options: opts.clone(),
    });
    assert!(flags.contains(DirtyFlags::OPTIONS));
    assert_eq!(
        state.observed.ui_options.get("key"),
        Some(&"value".to_string())
    );

    // Same options again → no dirty flags (change detection)
    let flags = state.apply(KakouneRequest::SetUiOptions { options: opts });
    assert_eq!(flags, DirtyFlags::empty());
}

#[test]
fn test_apply_refresh() {
    let mut state = AppState::default();
    let flags = state.apply(KakouneRequest::Refresh { force: true });
    assert_eq!(flags, DirtyFlags::ALL);

    let flags = state.apply(KakouneRequest::Refresh { force: false });
    assert!(flags.contains(DirtyFlags::BUFFER));
    assert!(flags.contains(DirtyFlags::STATUS));
}

/// Helper: build an Inline MenuState with given items and win_height.
fn make_inline_menu(items: Vec<Line>, win_height: u16) -> MenuState {
    MenuState {
        items,
        anchor: Coord::default(),
        selected_item_face: Face::default().into(),
        menu_face: Face::default().into(),
        style: MenuStyle::Inline,
        selected: None,
        first_item: 0,
        columns: 1,
        win_height,
        menu_lines: 0, // unused in scroll logic
        max_item_width: 0,
        screen_w: 80,
        columns_split: None,
    }
}

/// Helper: build a Prompt MenuState with given items, win_height, and columns.
fn make_prompt_menu(items: Vec<Line>, win_height: u16, columns: u16) -> MenuState {
    MenuState {
        items,
        anchor: Coord::default(),
        selected_item_face: Face::default().into(),
        menu_face: Face::default().into(),
        style: MenuStyle::Prompt,
        selected: None,
        first_item: 0,
        columns,
        win_height,
        menu_lines: 0,
        max_item_width: 0,
        screen_w: 80,
        columns_split: None,
    }
}

/// Helper: build a Search MenuState with given items and screen_w.
fn make_search_menu(items: Vec<Line>, screen_w: u16) -> MenuState {
    MenuState {
        items,
        anchor: Coord::default(),
        selected_item_face: Face::default().into(),
        menu_face: Face::default().into(),
        style: MenuStyle::Search,
        selected: None,
        first_item: 0,
        columns: 1,
        win_height: 1,
        menu_lines: 0,
        max_item_width: 0,
        screen_w,
        columns_split: None,
    }
}

#[test]
fn test_select_column_scroll_down() {
    // 5 items, win_height=3 → stride=3, so items 0-2 are col 0, items 3-4 are col 1
    let items: Vec<Line> = (0..5).map(|i| make_line(&format!("item{i}"))).collect();
    let mut menu = make_inline_menu(items, 3);

    // Select item 0: stays in col 0, first_item stays 0
    menu.select(0);
    assert_eq!(menu.first_item, 0);

    // Select item 3: moves to col 1, first_item should scroll to 3
    menu.select(3);
    assert_eq!(menu.first_item, 3);
}

#[test]
fn test_select_column_scroll_up() {
    let items: Vec<Line> = (0..6).map(|i| make_line(&format!("item{i}"))).collect();
    let mut menu = make_inline_menu(items, 3);

    // Scroll forward to col 1
    menu.select(3);
    assert_eq!(menu.first_item, 3);

    // Select item 1: back in col 0, first_item should scroll back to 0
    menu.select(1);
    assert_eq!(menu.first_item, 0);
}

#[test]
fn test_select_prompt_multi_column() {
    // 12 items, win_height=3, columns=2 → stride=3
    // col 0: items 0-2, col 1: items 3-5, col 2: items 6-8, col 3: items 9-11
    // Visible: 2 columns at a time
    let items: Vec<Line> = (0..12).map(|i| make_line(&format!("item{i}"))).collect();
    let mut menu = make_prompt_menu(items, 3, 2);

    // Select item 6 (col 2): needs to scroll since only 2 cols visible
    // col 2 becomes the leftmost visible column → first_item = 2*3 = 6
    menu.select(6);
    assert_eq!(menu.first_item, 6);

    // Select item 9 (col 3): already visible (cols 2-3 shown), no scroll
    menu.select(9);
    assert_eq!(menu.first_item, 6);
}

#[test]
fn test_select_search_stateless() {
    // Items: "aa" (2), "bb" (2), "cc" (2), "dd" (2), "ee" (2)
    // Each takes width+1 = 3 in search bar
    // screen_w = 15 → available width = 15 - 3 = 12
    // Cumulative: aa=3, bb=6, cc=9, dd=12, ee=15 (exceeds 12)
    let items: Vec<Line> = ["aa", "bb", "cc", "dd", "ee"]
        .iter()
        .map(|s| make_line(s))
        .collect();

    // Path A: select directly to item 4
    let mut menu_a = make_search_menu(items.clone(), 15);
    menu_a.select(4);

    // Path B: select 0, then 1, ..., then 4
    let mut menu_b = make_search_menu(items, 15);
    for i in 0..=4 {
        menu_b.select(i);
    }

    // Stateless: same selected → same first_item regardless of path
    assert_eq!(menu_a.first_item, menu_b.first_item);
    assert_eq!(menu_a.selected, Some(4));
    assert_eq!(menu_b.selected, Some(4));
}

#[test]
fn test_select_search_fits_in_width() {
    // Items: "a" (1), "b" (1), "c" (1) → each takes 2 (width+1)
    // screen_w = 80 → available = 77, total = 6, fits easily
    let items: Vec<Line> = ["a", "b", "c"].iter().map(|s| make_line(s)).collect();
    let mut menu = make_search_menu(items, 80);

    menu.select(2);
    assert_eq!(menu.first_item, 0);
}

#[test]
fn test_select_out_of_range_resets() {
    let items: Vec<Line> = (0..3).map(|i| make_line(&format!("item{i}"))).collect();
    let mut menu = make_inline_menu(items, 3);

    // Select valid item first
    menu.select(1);
    assert_eq!(menu.selected, Some(1));

    // Select -1 → resets
    menu.select(-1);
    assert_eq!(menu.selected, None);
    assert_eq!(menu.first_item, 0);

    // Select beyond length → resets
    menu.select(1);
    menu.select(3);
    assert_eq!(menu.selected, None);
    assert_eq!(menu.first_item, 0);
}

// --- secondary cursor extraction tests ---

/// Helper: create a cursor atom (FINAL_FG + REVERSE).
fn cursor_atom(s: &str) -> Atom {
    Atom::from_face(
        Face {
            fg: Color::Named(NamedColor::White),
            bg: Color::Default,
            underline: Color::Default,
            attributes: Attributes::FINAL_FG | Attributes::REVERSE,
        },
        s,
    )
}

/// Helper: create a normal (non-cursor) atom.
fn normal_atom(s: &str) -> Atom {
    Atom::from_face(Face::default(), s)
}

#[test]
fn test_draw_extracts_secondary_cursors_multiple() {
    let mut state = AppState::default();

    // Line: "hello" (5 chars) + cursor "w" at col 5 + "orld" (4 chars)
    // + another cursor at col 10 (the "!" char)
    let line = vec![
        normal_atom("hello"),
        cursor_atom("w"),
        normal_atom("orld"),
        cursor_atom("!"),
    ];

    // Primary cursor at line 0, column 5 (from draw message)
    state.apply(KakouneRequest::Draw {
        lines: vec![line],
        cursor_pos: Coord { line: 0, column: 5 },
        default_face: Face::default(),
        padding_face: Face::default(),
        widget_columns: 0,
    });

    assert_eq!(state.inference.cursor_count, 2);
    // Primary at (0, 5) is excluded; secondary at (0, 10) remains
    assert_eq!(state.inference.secondary_cursors.len(), 1);
    assert_eq!(
        state.inference.secondary_cursors[0],
        Coord {
            line: 0,
            column: 10
        }
    );
}

#[test]
fn test_draw_single_cursor_no_secondary() {
    let mut state = AppState::default();

    let line = vec![cursor_atom("h"), normal_atom("ello")];

    state.apply(KakouneRequest::Draw {
        lines: vec![line],
        cursor_pos: Coord { line: 0, column: 0 },
        default_face: Face::default(),
        padding_face: Face::default(),
        widget_columns: 0,
    });

    assert_eq!(state.inference.cursor_count, 1);
    assert!(state.inference.secondary_cursors.is_empty());
}

#[test]
fn test_draw_no_cursors() {
    let mut state = AppState::default();
    let line = vec![normal_atom("hello world")];

    state.apply(KakouneRequest::Draw {
        lines: vec![line],
        cursor_pos: Coord::default(),
        default_face: Face::default(),
        padding_face: Face::default(),
        widget_columns: 0,
    });

    // cursor_pos is always provided by Kakoune, so at least the primary
    // cursor is assumed to exist even when no atom carries cursor attributes.
    assert_eq!(state.inference.cursor_count, 1);
    assert!(state.inference.secondary_cursors.is_empty());
}

#[test]
fn test_draw_cjk_column_width() {
    let mut state = AppState::default();

    // "漢字" is 4 display columns, then cursor at col 4
    let line = vec![normal_atom("漢字"), cursor_atom("x")];

    // Primary cursor at column 4 (after two CJK chars = 4 display columns)
    state.apply(KakouneRequest::Draw {
        lines: vec![line],
        cursor_pos: Coord { line: 0, column: 4 },
        default_face: Face::default(),
        padding_face: Face::default(),
        widget_columns: 0,
    });

    assert_eq!(state.inference.cursor_count, 1);
    assert!(state.inference.secondary_cursors.is_empty());
}

#[test]
fn test_draw_secondary_cursors_multiline() {
    let mut state = AppState::default();

    let line0 = vec![cursor_atom("a"), normal_atom("bc")];
    let line1 = vec![normal_atom("de"), cursor_atom("f")];

    state.apply(KakouneRequest::Draw {
        lines: vec![line0, line1],
        cursor_pos: Coord { line: 0, column: 0 },
        default_face: Face::default(),
        padding_face: Face::default(),
        widget_columns: 0,
    });

    assert_eq!(state.inference.cursor_count, 2);
    assert_eq!(state.inference.secondary_cursors.len(), 1);
    assert_eq!(
        state.inference.secondary_cursors[0],
        Coord { line: 1, column: 2 }
    );
}
