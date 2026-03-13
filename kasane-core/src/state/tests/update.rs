use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use crate::element::InteractiveId;
use crate::input::{Key, KeyEvent, Modifiers, MouseButton, MouseEvent, MouseEventKind};
use crate::layout::{Rect, build_hit_map};
use crate::plugin::{Command, Plugin, PluginId, PluginRegistry};
use crate::protocol::{Coord, Face, KakouneRequest, KasaneRequest};
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
            cursor_pos: Coord::default(),
            default_face: Face::default(),
            padding_face: Face::default(),
            widget_columns: 0,
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
        modifiers: Modifiers::empty(),
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
        modifiers: Modifiers::empty(),
    };
    let (flags, commands) = update(&mut state, Msg::Mouse(mouse), &mut registry, &mut grid, 3);
    assert!(flags.is_empty());
    // Should have been forwarded to Kakoune as a mouse press
    assert_eq!(commands.len(), 1);
    assert!(matches!(commands[0], Command::SendToKakoune(_)));
}

// --- Input observation tests ---

#[test]
fn test_observe_key_called_for_all_plugins() {
    let observed = Arc::new(AtomicBool::new(false));

    struct ObserverPlugin(Arc<AtomicBool>);
    impl Plugin for ObserverPlugin {
        fn id(&self) -> PluginId {
            PluginId("observer".into())
        }
        fn observe_key(&mut self, _key: &KeyEvent, _state: &AppState) {
            self.0.store(true, Ordering::Relaxed);
        }
    }

    let mut state = AppState::default();
    let mut registry = PluginRegistry::new();
    registry.register(Box::new(ObserverPlugin(observed.clone())));
    let mut grid = CellGrid::new(80, 24);
    let key = KeyEvent {
        key: Key::Char('x'),
        modifiers: Modifiers::empty(),
    };
    let _ = update(&mut state, Msg::Key(key), &mut registry, &mut grid, 3);
    assert!(observed.load(Ordering::Relaxed));
}

#[test]
fn test_observe_key_called_even_when_plugin_handles() {
    // observe_key should be called for all plugins before handle_key
    let observed = Arc::new(AtomicBool::new(false));

    struct ObserverPlugin(Arc<AtomicBool>);
    impl Plugin for ObserverPlugin {
        fn id(&self) -> PluginId {
            PluginId("observer".into())
        }
        fn observe_key(&mut self, _key: &KeyEvent, _state: &AppState) {
            self.0.store(true, Ordering::Relaxed);
        }
    }

    struct HandlerPlugin;
    impl Plugin for HandlerPlugin {
        fn id(&self) -> PluginId {
            PluginId("handler".into())
        }
        fn handle_key(&mut self, _key: &KeyEvent, _state: &AppState) -> Option<Vec<Command>> {
            Some(vec![])
        }
    }

    let mut state = AppState::default();
    let mut registry = PluginRegistry::new();
    registry.register(Box::new(ObserverPlugin(observed.clone())));
    registry.register(Box::new(HandlerPlugin));
    let mut grid = CellGrid::new(80, 24);
    let key = KeyEvent {
        key: Key::Char('x'),
        modifiers: Modifiers::empty(),
    };
    let _ = update(&mut state, Msg::Key(key), &mut registry, &mut grid, 3);
    assert!(observed.load(Ordering::Relaxed));
}

#[test]
fn test_plugin_can_override_pageup() {
    struct PageUpPlugin;
    impl Plugin for PageUpPlugin {
        fn id(&self) -> PluginId {
            PluginId("pageup_override".into())
        }
        fn handle_key(&mut self, key: &KeyEvent, _state: &AppState) -> Option<Vec<Command>> {
            if key.key == Key::PageUp {
                Some(vec![Command::SendToKakoune(KasaneRequest::Keys(vec![
                    "custom_pageup".to_string(),
                ]))])
            } else {
                None
            }
        }
    }

    let mut state = AppState::default();
    let mut registry = PluginRegistry::new();
    registry.register(Box::new(PageUpPlugin));
    let mut grid = CellGrid::new(80, 24);
    let key = KeyEvent {
        key: Key::PageUp,
        modifiers: Modifiers::empty(),
    };
    let (_, commands) = update(&mut state, Msg::Key(key), &mut registry, &mut grid, 3);
    assert_eq!(commands.len(), 1);
    match &commands[0] {
        Command::SendToKakoune(KasaneRequest::Keys(keys)) => {
            assert_eq!(keys[0], "custom_pageup");
        }
        _ => panic!("expected custom PageUp handler"),
    }
}

#[test]
fn test_observe_mouse_called_without_hit_test() {
    let observed = Arc::new(AtomicBool::new(false));

    struct MouseObserver(Arc<AtomicBool>);
    impl Plugin for MouseObserver {
        fn id(&self) -> PluginId {
            PluginId("mouse_observer".into())
        }
        fn observe_mouse(&mut self, _event: &MouseEvent, _state: &AppState) {
            self.0.store(true, Ordering::Relaxed);
        }
    }

    let mut state = AppState::default();
    state.cols = 80;
    state.rows = 24;
    let mut registry = PluginRegistry::new();
    registry.register(Box::new(MouseObserver(observed.clone())));
    // No interactive regions → hit_test returns None
    let mut grid = CellGrid::new(80, 24);
    let mouse = MouseEvent {
        kind: MouseEventKind::Press(MouseButton::Left),
        line: 5,
        column: 10,
        modifiers: Modifiers::empty(),
    };
    let _ = update(&mut state, Msg::Mouse(mouse), &mut registry, &mut grid, 3);
    assert!(observed.load(Ordering::Relaxed));
}

#[test]
fn test_on_state_changed_dispatched_in_kakoune_msg() {
    let called = Arc::new(AtomicBool::new(false));

    struct StateWatcher(Arc<AtomicBool>);
    impl Plugin for StateWatcher {
        fn id(&self) -> PluginId {
            PluginId("watcher".into())
        }
        fn on_state_changed(&mut self, _state: &AppState, dirty: DirtyFlags) -> Vec<Command> {
            if dirty.contains(DirtyFlags::BUFFER) {
                self.0.store(true, Ordering::Relaxed);
            }
            vec![]
        }
    }

    let mut state = AppState::default();
    let mut registry = PluginRegistry::new();
    registry.register(Box::new(StateWatcher(called.clone())));
    let mut grid = CellGrid::new(80, 24);
    let (flags, _) = update(
        &mut state,
        Msg::Kakoune(KakouneRequest::Draw {
            lines: vec![make_line("hello")],
            cursor_pos: Coord::default(),
            default_face: Face::default(),
            padding_face: Face::default(),
            widget_columns: 0,
        }),
        &mut registry,
        &mut grid,
        3,
    );
    assert!(flags.contains(DirtyFlags::BUFFER));
    assert!(called.load(Ordering::Relaxed));
}
