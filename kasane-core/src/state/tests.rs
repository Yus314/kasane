use super::*;
use crate::plugin::{Command, PluginRegistry};
use crate::protocol::{Face, InfoStyle, KakouneRequest, KasaneRequest, MenuStyle};
use crate::render::CellGrid;
use crate::test_utils::make_line;

#[test]
fn test_apply_draw() {
    let mut state = AppState::default();
    let flags = state.apply(KakouneRequest::Draw {
        lines: vec![make_line("hello")],
        default_face: Face::default(),
        padding_face: Face::default(),
    });
    assert!(flags.contains(DirtyFlags::BUFFER));
    assert_eq!(state.lines.len(), 1);
}

#[test]
fn test_apply_set_cursor() {
    let mut state = AppState::default();
    let flags = state.apply(KakouneRequest::SetCursor {
        mode: CursorMode::Buffer,
        coord: Coord { line: 0, column: 3 },
    });
    assert!(flags.contains(DirtyFlags::BUFFER));
    assert_eq!(state.cursor_pos.column, 3);
    assert_eq!(state.cursor_mode, CursorMode::Buffer);
}

#[test]
fn test_apply_draw_status() {
    let mut state = AppState::default();
    let flags = state.apply(KakouneRequest::DrawStatus {
        status_line: make_line(":q"),
        mode_line: make_line("insert"),
        default_face: Face::default(),
    });
    assert!(flags.contains(DirtyFlags::STATUS));
    assert_eq!(state.status_line[0].contents, ":q");
    assert_eq!(state.status_mode_line[0].contents, "insert");
}

#[test]
fn test_apply_menu_show_select_hide() {
    let mut state = AppState::default();

    state.apply(KakouneRequest::MenuShow {
        items: vec![make_line("a"), make_line("b")],
        anchor: Coord { line: 1, column: 0 },
        selected_item_face: Face::default(),
        menu_face: Face::default(),
        style: MenuStyle::Inline,
    });
    assert!(state.menu.is_some());
    assert_eq!(state.menu.as_ref().unwrap().selected, None);

    state.apply(KakouneRequest::MenuSelect { selected: 1 });
    assert_eq!(state.menu.as_ref().unwrap().selected, Some(1));

    let flags = state.apply(KakouneRequest::MenuHide);
    assert!(state.menu.is_none());
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
    assert_eq!(state.infos.len(), 1);

    let flags = state.apply(KakouneRequest::InfoHide);
    assert!(state.infos.is_empty());
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
    assert_eq!(state.infos.len(), 1);

    // Show second info (Inline at line 5) — different identity, coexists
    state.apply(KakouneRequest::InfoShow {
        title: make_line("Lint"),
        content: vec![make_line("error here")],
        anchor: Coord { line: 5, column: 0 },
        face: Face::default(),
        style: InfoStyle::Inline,
    });
    assert_eq!(state.infos.len(), 2);

    // Show info with same identity (Modal at line 0) — replaces first
    state.apply(KakouneRequest::InfoShow {
        title: make_line("Updated Help"),
        content: vec![make_line("new content")],
        anchor: Coord { line: 0, column: 0 },
        face: Face::default(),
        style: InfoStyle::Modal,
    });
    assert_eq!(state.infos.len(), 2);
    assert_eq!(state.infos[0].title[0].contents, "Updated Help");

    // Hide removes most recent
    state.apply(KakouneRequest::InfoHide);
    assert_eq!(state.infos.len(), 1);
}

#[test]
fn test_apply_set_ui_options() {
    let mut state = AppState::default();
    let mut opts = std::collections::HashMap::new();
    opts.insert("key".to_string(), "value".to_string());
    let flags = state.apply(KakouneRequest::SetUiOptions { options: opts });
    assert!(flags.contains(DirtyFlags::OPTIONS));
    assert_eq!(state.ui_options.get("key"), Some(&"value".to_string()));
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
        selected_item_face: Face::default(),
        menu_face: Face::default(),
        style: MenuStyle::Inline,
        selected: None,
        first_item: 0,
        columns: 1,
        win_height,
        menu_lines: 0, // unused in scroll logic
        max_item_width: 0,
        screen_w: 80,
    }
}

/// Helper: build a Prompt MenuState with given items, win_height, and columns.
fn make_prompt_menu(items: Vec<Line>, win_height: u16, columns: u16) -> MenuState {
    MenuState {
        items,
        anchor: Coord::default(),
        selected_item_face: Face::default(),
        menu_face: Face::default(),
        style: MenuStyle::Prompt,
        selected: None,
        first_item: 0,
        columns,
        win_height,
        menu_lines: 0,
        max_item_width: 0,
        screen_w: 80,
    }
}

/// Helper: build a Search MenuState with given items and screen_w.
fn make_search_menu(items: Vec<Line>, screen_w: u16) -> MenuState {
    MenuState {
        items,
        anchor: Coord::default(),
        selected_item_face: Face::default(),
        menu_face: Face::default(),
        style: MenuStyle::Search,
        selected: None,
        first_item: 0,
        columns: 1,
        win_height: 1,
        menu_lines: 0,
        max_item_width: 0,
        screen_w,
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

// --- TEA update() tests ---

#[test]
fn test_update_key_forwards_to_kakoune() {
    let mut state = AppState::default();
    let mut registry = PluginRegistry::new();
    let mut grid = CellGrid::new(80, 24);
    let key = crate::input::KeyEvent {
        key: crate::input::Key::Char('a'),
        modifiers: crate::input::Modifiers::empty(),
    };
    let (flags, commands) = update(&mut state, Msg::Key(key), &mut registry, &mut grid, 3);
    assert!(flags.is_empty());
    assert_eq!(commands.len(), 1);
    match &commands[0] {
        Command::SendToKakoune(req) => {
            assert_eq!(
                *req,
                crate::protocol::KasaneRequest::Keys(vec!["a".to_string()])
            );
        }
        _ => panic!("expected SendToKakoune"),
    }
}

#[test]
fn test_update_kakoune_draw() {
    let mut state = AppState::default();
    let mut registry = PluginRegistry::new();
    let mut grid = CellGrid::new(80, 24);
    let (flags, commands) = update(
        &mut state,
        Msg::Kakoune(KakouneRequest::Draw {
            lines: vec![make_line("hello")],
            default_face: Face::default(),
            padding_face: Face::default(),
        }),
        &mut registry,
        &mut grid,
        3,
    );
    assert!(flags.contains(DirtyFlags::BUFFER));
    assert!(commands.is_empty());
    assert_eq!(state.lines.len(), 1);
}

#[test]
fn test_update_focus_lost() {
    let mut state = AppState::default();
    let mut registry = PluginRegistry::new();
    let mut grid = CellGrid::new(80, 24);
    let (flags, _) = update(&mut state, Msg::FocusLost, &mut registry, &mut grid, 3);
    assert_eq!(flags, DirtyFlags::ALL);
    assert!(!state.focused);
}

#[test]
fn test_update_focus_gained() {
    let mut state = AppState::default();
    state.focused = false;
    let mut registry = PluginRegistry::new();
    let mut grid = CellGrid::new(80, 24);
    let (flags, _) = update(&mut state, Msg::FocusGained, &mut registry, &mut grid, 3);
    assert_eq!(flags, DirtyFlags::ALL);
    assert!(state.focused);
}

#[test]
fn test_update_plugin_handles_key() {
    use crate::plugin::{Plugin, PluginId};

    struct KeyPlugin;
    impl Plugin for KeyPlugin {
        fn id(&self) -> PluginId {
            PluginId("key_plugin".into())
        }
        fn handle_key(
            &mut self,
            _key: &crate::input::KeyEvent,
            _state: &AppState,
        ) -> Option<Vec<Command>> {
            Some(vec![Command::SendToKakoune(
                crate::protocol::KasaneRequest::Keys(vec!["<esc>".to_string()]),
            )])
        }
    }

    let mut state = AppState::default();
    let mut registry = PluginRegistry::new();
    registry.register(Box::new(KeyPlugin));
    let mut grid = CellGrid::new(80, 24);
    let key = crate::input::KeyEvent {
        key: crate::input::Key::Char('a'),
        modifiers: crate::input::Modifiers::empty(),
    };
    let (flags, commands) = update(&mut state, Msg::Key(key), &mut registry, &mut grid, 3);
    assert!(flags.is_empty());
    assert_eq!(commands.len(), 1);
    match &commands[0] {
        Command::SendToKakoune(req) => {
            assert_eq!(
                *req,
                crate::protocol::KasaneRequest::Keys(vec!["<esc>".to_string()])
            );
        }
        _ => panic!("expected SendToKakoune from plugin"),
    }
}

// --- Phase 3: Drag state tests ---

#[test]
fn test_drag_state_press_activates() {
    let mut state = AppState::default();
    let mut registry = PluginRegistry::new();
    let mut grid = CellGrid::new(80, 24);
    let mouse = crate::input::MouseEvent {
        kind: crate::input::MouseEventKind::Press(crate::input::MouseButton::Left),
        line: 5,
        column: 10,
    };
    update(&mut state, Msg::Mouse(mouse), &mut registry, &mut grid, 3);
    assert_eq!(
        state.drag,
        DragState::Active {
            button: crate::input::MouseButton::Left,
            start_line: 5,
            start_column: 10,
        }
    );
}

#[test]
fn test_drag_state_release_clears() {
    let mut state = AppState::default();
    state.drag = DragState::Active {
        button: crate::input::MouseButton::Left,
        start_line: 0,
        start_column: 0,
    };
    let mut registry = PluginRegistry::new();
    let mut grid = CellGrid::new(80, 24);
    let mouse = crate::input::MouseEvent {
        kind: crate::input::MouseEventKind::Release(crate::input::MouseButton::Left),
        line: 5,
        column: 10,
    };
    update(&mut state, Msg::Mouse(mouse), &mut registry, &mut grid, 3);
    assert_eq!(state.drag, DragState::None);
}

#[test]
fn test_drag_state_drag_keeps_active() {
    let mut state = AppState::default();
    state.drag = DragState::Active {
        button: crate::input::MouseButton::Left,
        start_line: 0,
        start_column: 0,
    };
    let mut registry = PluginRegistry::new();
    let mut grid = CellGrid::new(80, 24);
    let mouse = crate::input::MouseEvent {
        kind: crate::input::MouseEventKind::Drag(crate::input::MouseButton::Left),
        line: 3,
        column: 7,
    };
    let (_, commands) = update(&mut state, Msg::Mouse(mouse), &mut registry, &mut grid, 3);
    // Drag sends MouseMove
    assert_eq!(commands.len(), 1);
    match &commands[0] {
        Command::SendToKakoune(req) => {
            assert_eq!(*req, KasaneRequest::MouseMove { line: 3, column: 7 });
        }
        _ => panic!("expected SendToKakoune MouseMove"),
    }
    // Drag state remains Active
    assert!(matches!(state.drag, DragState::Active { .. }));
}

#[test]
fn test_selection_scroll_generates_two_commands() {
    let mut state = AppState::default();
    state.rows = 24;
    state.drag = DragState::Active {
        button: crate::input::MouseButton::Left,
        start_line: 5,
        start_column: 10,
    };
    let mut registry = PluginRegistry::new();
    let mut grid = CellGrid::new(80, 24);
    let mouse = crate::input::MouseEvent {
        kind: crate::input::MouseEventKind::ScrollDown,
        line: 10,
        column: 5,
    };
    let (_, commands) = update(&mut state, Msg::Mouse(mouse), &mut registry, &mut grid, 3);
    assert_eq!(commands.len(), 2, "scroll + mouse_move expected");
    // First: Scroll
    match &commands[0] {
        Command::SendToKakoune(KasaneRequest::Scroll { amount, .. }) => {
            assert_eq!(*amount, 3);
        }
        _ => panic!("expected Scroll command"),
    }
    // Second: MouseMove to edge
    match &commands[1] {
        Command::SendToKakoune(KasaneRequest::MouseMove { line, column }) => {
            assert_eq!(*line, 22); // rows - 2
            assert_eq!(*column, 5);
        }
        _ => panic!("expected MouseMove command"),
    }
}

#[test]
fn test_selection_scroll_up_edge() {
    let mut state = AppState::default();
    state.rows = 24;
    state.drag = DragState::Active {
        button: crate::input::MouseButton::Left,
        start_line: 5,
        start_column: 10,
    };
    let mut registry = PluginRegistry::new();
    let mut grid = CellGrid::new(80, 24);
    let mouse = crate::input::MouseEvent {
        kind: crate::input::MouseEventKind::ScrollUp,
        line: 10,
        column: 5,
    };
    let (_, commands) = update(&mut state, Msg::Mouse(mouse), &mut registry, &mut grid, 3);
    assert_eq!(commands.len(), 2);
    match &commands[1] {
        Command::SendToKakoune(KasaneRequest::MouseMove { line, .. }) => {
            assert_eq!(*line, 0); // edge is top
        }
        _ => panic!("expected MouseMove command"),
    }
}

#[test]
fn test_paste_produces_paste_command() {
    let mut state = AppState::default();
    let mut registry = PluginRegistry::new();
    let mut grid = CellGrid::new(80, 24);
    let (flags, commands) = update(&mut state, Msg::Paste, &mut registry, &mut grid, 3);
    assert!(flags.is_empty());
    assert_eq!(commands.len(), 1);
    assert!(matches!(commands[0], Command::Paste));
}

#[test]
fn test_pageup_intercept() {
    let mut state = AppState::default();
    state.rows = 24;
    state.cursor_pos = Coord {
        line: 10,
        column: 5,
    };
    let mut registry = PluginRegistry::new();
    let mut grid = CellGrid::new(80, 24);
    let key = crate::input::KeyEvent {
        key: crate::input::Key::PageUp,
        modifiers: crate::input::Modifiers::empty(),
    };
    let (_, commands) = update(&mut state, Msg::Key(key), &mut registry, &mut grid, 3);
    assert_eq!(commands.len(), 1);
    match &commands[0] {
        Command::SendToKakoune(KasaneRequest::Scroll {
            amount,
            line,
            column,
        }) => {
            assert_eq!(*amount, -23); // -(rows - 1)
            assert_eq!(*line, 10);
            assert_eq!(*column, 5);
        }
        _ => panic!("expected Scroll command"),
    }
}

#[test]
fn test_pagedown_intercept() {
    let mut state = AppState::default();
    state.rows = 24;
    state.cursor_pos = Coord {
        line: 10,
        column: 5,
    };
    let mut registry = PluginRegistry::new();
    let mut grid = CellGrid::new(80, 24);
    let key = crate::input::KeyEvent {
        key: crate::input::Key::PageDown,
        modifiers: crate::input::Modifiers::empty(),
    };
    let (_, commands) = update(&mut state, Msg::Key(key), &mut registry, &mut grid, 3);
    assert_eq!(commands.len(), 1);
    match &commands[0] {
        Command::SendToKakoune(KasaneRequest::Scroll { amount, .. }) => {
            assert_eq!(*amount, 23); // rows - 1
        }
        _ => panic!("expected Scroll command"),
    }
}

#[test]
fn test_pageup_with_modifier_not_intercepted() {
    let mut state = AppState::default();
    let mut registry = PluginRegistry::new();
    let mut grid = CellGrid::new(80, 24);
    let key = crate::input::KeyEvent {
        key: crate::input::Key::PageUp,
        modifiers: crate::input::Modifiers::CTRL,
    };
    let (_, commands) = update(&mut state, Msg::Key(key), &mut registry, &mut grid, 3);
    // With modifier, PageUp should be forwarded as key, not intercepted
    assert_eq!(commands.len(), 1);
    match &commands[0] {
        Command::SendToKakoune(KasaneRequest::Keys(keys)) => {
            assert_eq!(keys, &vec!["<c-pageup>".to_string()]);
        }
        _ => panic!("expected Keys command"),
    }
}

#[test]
fn test_available_height() {
    let mut state = AppState::default();
    state.rows = 24;
    assert_eq!(state.available_height(), 23);

    state.rows = 1;
    assert_eq!(state.available_height(), 0);
}
