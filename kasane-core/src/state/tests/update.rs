use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use crate::element::InteractiveId;
use crate::input::{Key, KeyEvent, Modifiers, MouseButton, MouseEvent, MouseEventKind};
use crate::layout::{Rect, build_hit_map};
use crate::plugin::{Command, PluginBackend, PluginId, PluginRuntime, RuntimeEffects};
use crate::protocol::{Coord, Face, KakouneRequest, KasaneRequest};
use crate::scroll::{ScrollAccumulationMode, ScrollCurve, ScrollPlan};
use crate::state::update::{Msg, update_in_place};
use crate::state::{AppState, DirtyFlags};
use crate::test_utils::make_line;

#[test]
fn test_update_key_forwards_to_kakoune() {
    let mut state = Box::new(AppState::default());
    let mut registry = PluginRuntime::new();

    let key = crate::input::KeyEvent {
        key: crate::input::Key::Char('a'),
        modifiers: crate::input::Modifiers::empty(),
    };
    let result = update_in_place(&mut state, Msg::Key(key), &mut registry, 3);
    let flags = result.flags;
    let commands = result.commands;
    assert!(flags.is_empty());
    assert_eq!(commands.len(), 1);
    match &commands[0] {
        Command::SendToKakoune(req) => {
            assert_eq!(
                req,
                &crate::protocol::KasaneRequest::Keys(vec!["a".to_string()])
            );
        }
        _ => panic!("expected SendToKakoune"),
    }
}

#[test]
fn test_update_kakoune_draw() {
    let mut state = Box::new(AppState::default());
    let mut registry = PluginRuntime::new();

    let result = update_in_place(
        &mut state,
        Msg::Kakoune(KakouneRequest::Draw {
            lines: vec![make_line("hello")],
            cursor_pos: Coord::default(),
            default_face: Face::default(),
            padding_face: Face::default(),
            widget_columns: 0,
        }),
        &mut registry,
        3,
    );
    let flags = result.flags;
    let commands = result.commands;
    assert!(flags.contains(DirtyFlags::BUFFER));
    assert!(commands.is_empty());
    assert_eq!(state.lines.len(), 1);
}

#[test]
fn test_update_focus_lost() {
    let mut state = Box::new(AppState::default());
    let mut registry = PluginRuntime::new();

    let flags = update_in_place(&mut state, Msg::FocusLost, &mut registry, 3).flags;
    assert_eq!(flags, DirtyFlags::ALL);
    assert!(!state.focused);
}

#[test]
fn test_update_focus_gained() {
    let mut state = Box::new(AppState::default());
    state.focused = false;
    let mut registry = PluginRuntime::new();

    let flags = update_in_place(&mut state, Msg::FocusGained, &mut registry, 3).flags;
    assert_eq!(flags, DirtyFlags::ALL);
    assert!(state.focused);
}

#[test]
fn test_update_plugin_handles_key() {
    struct KeyPlugin;
    impl PluginBackend for KeyPlugin {
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

    let mut state = Box::new(AppState::default());
    let mut registry = PluginRuntime::new();
    registry.register_backend(Box::new(KeyPlugin));

    let key = crate::input::KeyEvent {
        key: crate::input::Key::Char('a'),
        modifiers: crate::input::Modifiers::empty(),
    };
    let result = update_in_place(&mut state, Msg::Key(key), &mut registry, 3);
    let flags = result.flags;
    let commands = result.commands;
    assert!(flags.is_empty());
    assert_eq!(commands.len(), 1);
    match &commands[0] {
        Command::SendToKakoune(req) => {
            assert_eq!(
                req,
                &crate::protocol::KasaneRequest::Keys(vec!["<esc>".to_string()])
            );
        }
        _ => panic!("expected SendToKakoune from plugin"),
    }
}

#[test]
fn test_update_mouse_routes_to_plugin() {
    struct MousePlugin;
    impl PluginBackend for MousePlugin {
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

    let mut state = Box::new(AppState::default());
    state.cols = 80;
    state.rows = 24;
    let mut registry = PluginRuntime::new();
    registry.register_backend(Box::new(MousePlugin));

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
    state.hit_map = hit_map;

    let mouse = MouseEvent {
        kind: MouseEventKind::Press(MouseButton::Left),
        line: 3,
        column: 7,
        modifiers: Modifiers::empty(),
    };
    let result = update_in_place(&mut state, Msg::Mouse(mouse), &mut registry, 3);
    let flags = result.flags;
    let commands = result.commands;
    // Plugin handled the mouse event and returned RequestRedraw(INFO)
    assert!(flags.contains(DirtyFlags::INFO));
    assert!(commands.is_empty()); // RequestRedraw was extracted
}

#[test]
fn test_update_mouse_miss_forwards_to_kakoune() {
    let mut state = Box::new(AppState::default());
    state.cols = 80;
    state.rows = 24;
    let mut registry = PluginRuntime::new();
    // Empty HitMap (no interactive regions)

    let mouse = MouseEvent {
        kind: MouseEventKind::Press(MouseButton::Left),
        line: 5,
        column: 10,
        modifiers: Modifiers::empty(),
    };
    let result = update_in_place(&mut state, Msg::Mouse(mouse), &mut registry, 3);
    let flags = result.flags;
    let commands = result.commands;
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
    impl PluginBackend for ObserverPlugin {
        fn id(&self) -> PluginId {
            PluginId("observer".into())
        }
        fn observe_key(&mut self, _key: &KeyEvent, _state: &AppState) {
            self.0.store(true, Ordering::Relaxed);
        }
    }

    let mut state = Box::new(AppState::default());
    let mut registry = PluginRuntime::new();
    registry.register_backend(Box::new(ObserverPlugin(observed.clone())));

    let key = KeyEvent {
        key: Key::Char('x'),
        modifiers: Modifiers::empty(),
    };
    let _ = update_in_place(&mut state, Msg::Key(key), &mut registry, 3);
    assert!(observed.load(Ordering::Relaxed));
}

#[test]
fn test_observe_key_called_even_when_plugin_handles() {
    // observe_key should be called for all plugins before handle_key
    let observed = Arc::new(AtomicBool::new(false));

    struct ObserverPlugin(Arc<AtomicBool>);
    impl PluginBackend for ObserverPlugin {
        fn id(&self) -> PluginId {
            PluginId("observer".into())
        }
        fn observe_key(&mut self, _key: &KeyEvent, _state: &AppState) {
            self.0.store(true, Ordering::Relaxed);
        }
    }

    struct HandlerPlugin;
    impl PluginBackend for HandlerPlugin {
        fn id(&self) -> PluginId {
            PluginId("handler".into())
        }
        fn handle_key(&mut self, _key: &KeyEvent, _state: &AppState) -> Option<Vec<Command>> {
            Some(vec![])
        }
    }

    let mut state = Box::new(AppState::default());
    let mut registry = PluginRuntime::new();
    registry.register_backend(Box::new(ObserverPlugin(observed.clone())));
    registry.register_backend(Box::new(HandlerPlugin));

    let key = KeyEvent {
        key: Key::Char('x'),
        modifiers: Modifiers::empty(),
    };
    let _ = update_in_place(&mut state, Msg::Key(key), &mut registry, 3);
    assert!(observed.load(Ordering::Relaxed));
}

#[test]
fn test_plugin_can_override_pageup() {
    struct PageUpPlugin;
    impl PluginBackend for PageUpPlugin {
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

    let mut state = Box::new(AppState::default());
    let mut registry = PluginRuntime::new();
    registry.register_backend(Box::new(PageUpPlugin));

    let key = KeyEvent {
        key: Key::PageUp,
        modifiers: Modifiers::empty(),
    };
    let commands = update_in_place(&mut state, Msg::Key(key), &mut registry, 3).commands;
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
    impl PluginBackend for MouseObserver {
        fn id(&self) -> PluginId {
            PluginId("mouse_observer".into())
        }
        fn observe_mouse(&mut self, _event: &MouseEvent, _state: &AppState) {
            self.0.store(true, Ordering::Relaxed);
        }
    }

    let mut state = Box::new(AppState::default());
    state.cols = 80;
    state.rows = 24;
    let mut registry = PluginRuntime::new();
    registry.register_backend(Box::new(MouseObserver(observed.clone())));
    // No interactive regions → hit_test returns None

    let mouse = MouseEvent {
        kind: MouseEventKind::Press(MouseButton::Left),
        line: 5,
        column: 10,
        modifiers: Modifiers::empty(),
    };
    let _ = update_in_place(&mut state, Msg::Mouse(mouse), &mut registry, 3);
    assert!(observed.load(Ordering::Relaxed));
}

#[test]
fn test_on_state_changed_dispatched_in_kakoune_msg() {
    let called = Arc::new(AtomicBool::new(false));

    struct StateWatcher(Arc<AtomicBool>);
    impl PluginBackend for StateWatcher {
        fn id(&self) -> PluginId {
            PluginId("watcher".into())
        }
        fn on_state_changed_effects(
            &mut self,
            _state: &AppState,
            dirty: DirtyFlags,
        ) -> RuntimeEffects {
            if dirty.contains(DirtyFlags::BUFFER) {
                self.0.store(true, Ordering::Relaxed);
            }
            RuntimeEffects::default()
        }
    }

    let mut state = Box::new(AppState::default());
    let mut registry = PluginRuntime::new();
    registry.register_backend(Box::new(StateWatcher(called.clone())));

    let flags = update_in_place(
        &mut state,
        Msg::Kakoune(KakouneRequest::Draw {
            lines: vec![make_line("hello")],
            cursor_pos: Coord::default(),
            default_face: Face::default(),
            padding_face: Face::default(),
            widget_columns: 0,
        }),
        &mut registry,
        3,
    )
    .flags;
    assert!(flags.contains(DirtyFlags::BUFFER));
    assert!(called.load(Ordering::Relaxed));
}

#[test]
fn test_on_state_changed_effects_return_scroll_plans() {
    struct StateWatcher;

    impl PluginBackend for StateWatcher {
        fn id(&self) -> PluginId {
            PluginId("watcher-effects".into())
        }

        fn on_state_changed_effects(
            &mut self,
            _state: &AppState,
            dirty: DirtyFlags,
        ) -> RuntimeEffects {
            if !dirty.contains(DirtyFlags::BUFFER) {
                return RuntimeEffects::default();
            }
            RuntimeEffects {
                redraw: DirtyFlags::STATUS,
                commands: vec![],
                scroll_plans: vec![ScrollPlan {
                    total_amount: 4,
                    line: 1,
                    column: 2,
                    frame_interval_ms: 16,
                    curve: ScrollCurve::Linear,
                    accumulation: ScrollAccumulationMode::Add,
                }],
            }
        }
    }

    let mut state = Box::new(AppState::default());
    let mut registry = PluginRuntime::new();
    registry.register_backend(Box::new(StateWatcher));

    let result = update_in_place(
        &mut state,
        Msg::Kakoune(KakouneRequest::Draw {
            lines: vec![make_line("hello")],
            cursor_pos: Coord::default(),
            default_face: Face::default(),
            padding_face: Face::default(),
            widget_columns: 0,
        }),
        &mut registry,
        3,
    );

    assert!(result.flags.contains(DirtyFlags::BUFFER));
    assert!(result.flags.contains(DirtyFlags::STATUS));
    assert_eq!(result.scroll_plans.len(), 1);
    assert_eq!(result.scroll_plans[0].total_amount, 4);
}
