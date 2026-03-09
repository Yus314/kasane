use crate::element::InteractiveId;
use crate::input::{MouseButton, MouseEvent, MouseEventKind};
use crate::layout::{Rect, build_hit_map};
use crate::plugin::{Command, Plugin, PluginId, PluginRegistry};
use crate::protocol::{Face, KakouneRequest};
use crate::render::CellGrid;
use crate::state::update::{Msg, update};
use crate::state::{AppState, DirtyFlags};
use crate::test_utils::make_line;

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

#[test]
fn test_update_mouse_routes_to_plugin() {
    struct MousePlugin;
    impl Plugin for MousePlugin {
        fn id(&self) -> PluginId {
            PluginId("mouse_plugin".into())
        }
        fn handle_mouse(
            &mut self,
            _event: &MouseEvent,
            _id: InteractiveId,
            _state: &AppState,
        ) -> Option<Vec<Command>> {
            Some(vec![Command::RequestRedraw(DirtyFlags::INFO)])
        }
    }

    let mut state = AppState::default();
    state.cols = 80;
    state.rows = 24;
    let mut registry = PluginRegistry::new();
    registry.register(Box::new(MousePlugin));

    // Build a HitMap with an interactive region at (5,3)-(12,3)
    let el = crate::element::Element::Interactive {
        child: Box::new(crate::element::Element::text("click me", Face::default())),
        id: InteractiveId(42),
    };
    let area = Rect {
        x: 5,
        y: 3,
        w: 8,
        h: 1,
    };
    let layout = crate::layout::flex::place(&el, area, &state);
    let hit_map = build_hit_map(&el, &layout);
    registry.set_hit_map(hit_map);

    let mut grid = CellGrid::new(80, 24);
    let mouse = MouseEvent {
        kind: MouseEventKind::Press(MouseButton::Left),
        line: 3,
        column: 7,
    };
    let (flags, commands) = update(&mut state, Msg::Mouse(mouse), &mut registry, &mut grid, 3);
    // Plugin handled the mouse event and returned RequestRedraw(INFO)
    assert!(flags.contains(DirtyFlags::INFO));
    assert!(commands.is_empty()); // RequestRedraw was extracted
}

#[test]
fn test_update_mouse_miss_forwards_to_kakoune() {
    let mut state = AppState::default();
    state.cols = 80;
    state.rows = 24;
    let mut registry = PluginRegistry::new();
    // Empty HitMap (no interactive regions)
    let mut grid = CellGrid::new(80, 24);
    let mouse = MouseEvent {
        kind: MouseEventKind::Press(MouseButton::Left),
        line: 5,
        column: 10,
    };
    let (flags, commands) = update(&mut state, Msg::Mouse(mouse), &mut registry, &mut grid, 3);
    assert!(flags.is_empty());
    // Should have been forwarded to Kakoune as a mouse press
    assert_eq!(commands.len(), 1);
    assert!(matches!(commands[0], Command::SendToKakoune(_)));
}
