use crate::protocol::{Atom, Coord, CursorMode, InfoStyle, MenuStyle};
use crate::state::{AppState, InfoIdentity, InfoState, MenuParams, MenuState};

#[test]
fn test_visible_line_range_empty() {
    let state = AppState::default();
    assert_eq!(state.visible_line_range(), 0..0);
}

#[test]
fn test_visible_line_range_with_lines() {
    let mut state = AppState::default();
    state.observed.lines = vec![vec![], vec![], vec![]];
    assert_eq!(state.visible_line_range(), 0..3);
}

#[test]
fn test_buffer_line_count() {
    let mut state = AppState::default();
    assert_eq!(state.buffer_line_count(), 0);
    state.observed.lines = vec![vec![], vec![]];
    assert_eq!(state.buffer_line_count(), 2);
}

#[test]
fn test_has_menu() {
    let mut state = AppState::default();
    assert!(!state.has_menu());
    state.observed.menu = Some(MenuState::new(
        vec![vec![Atom::plain("a")]],
        MenuParams {
            anchor: Coord::default(),
            selected_item_face: crate::protocol::Style::default(),
            menu_face: crate::protocol::Style::default(),
            style: MenuStyle::Inline,
            screen_w: 80,
            screen_h: 24,
            max_height: 10,
        },
    ));
    assert!(state.has_menu());
}

#[test]
fn test_has_info() {
    let mut state = AppState::default();
    assert!(!state.has_info());
    state.observed.infos.push(InfoState {
        title: vec![],
        content: vec![],
        anchor: Coord::default(),
        style: InfoStyle::Prompt,
        face: crate::protocol::Style::default(),
        identity: InfoIdentity {
            style: InfoStyle::Prompt,
            anchor_line: 0,
        },
        scroll_offset: 0,
    });
    assert!(state.has_info());
}

#[test]
fn test_is_prompt_mode() {
    let mut state = AppState::default();
    assert!(!state.is_prompt_mode()); // default is Buffer
    state.inference.cursor_mode = CursorMode::Prompt;
    assert!(state.is_prompt_mode());
}
