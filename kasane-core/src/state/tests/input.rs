use crate::plugin::{Command, PluginRegistry};
use crate::protocol::{Coord, KasaneRequest};
use crate::render::CellGrid;
use crate::state::update::{Msg, update};
use crate::state::{AppState, DragState};

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
        modifiers: crate::input::Modifiers::empty(),
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
        modifiers: crate::input::Modifiers::empty(),
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
        modifiers: crate::input::Modifiers::empty(),
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
        modifiers: crate::input::Modifiers::empty(),
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
        modifiers: crate::input::Modifiers::empty(),
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
