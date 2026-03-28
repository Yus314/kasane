use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use crate::element::InteractiveId;
use crate::input::{Key, KeyEvent, Modifiers, MouseButton, MouseEvent, MouseEventKind};
use crate::layout::{Rect, build_hit_map};
use crate::plugin::{
    AppView, Command, Effects, KeyHandleResult, NullEffects, PluginBackend, PluginId,
    PluginRuntime, RecordingEffects,
};
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
            _state: &AppView<'_>,
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
fn test_update_key_forwards_transformed_key_to_kakoune() {
    struct TransformPlugin;
    impl PluginBackend for TransformPlugin {
        fn id(&self) -> PluginId {
            PluginId("transform".into())
        }

        fn handle_key_middleware(
            &mut self,
            key: &KeyEvent,
            _state: &AppView<'_>,
        ) -> KeyHandleResult {
            if key.key == Key::Char('a') {
                KeyHandleResult::Transformed(KeyEvent {
                    key: Key::Char('b'),
                    modifiers: Modifiers::SHIFT,
                })
            } else {
                KeyHandleResult::Passthrough
            }
        }
    }

    let mut state = Box::new(AppState::default());
    let mut registry = PluginRuntime::new();
    registry.register_backend(Box::new(TransformPlugin));

    let result = update_in_place(
        &mut state,
        Msg::Key(KeyEvent {
            key: Key::Char('a'),
            modifiers: Modifiers::empty(),
        }),
        &mut registry,
        3,
    );

    assert!(result.flags.is_empty());
    assert_eq!(result.source_plugin, None);
    assert_eq!(result.commands.len(), 1);
    match &result.commands[0] {
        Command::SendToKakoune(KasaneRequest::Keys(keys)) => {
            assert_eq!(keys, &vec!["B".to_string()]);
        }
        _ => panic!("expected transformed key to be forwarded"),
    }
}

#[test]
fn test_update_key_transformed_then_consumed_by_next_plugin() {
    struct TransformPlugin;
    impl PluginBackend for TransformPlugin {
        fn id(&self) -> PluginId {
            PluginId("transform".into())
        }

        fn handle_key_middleware(
            &mut self,
            key: &KeyEvent,
            _state: &AppView<'_>,
        ) -> KeyHandleResult {
            if key.key == Key::Char('a') {
                KeyHandleResult::Transformed(KeyEvent {
                    key: Key::Char('b'),
                    modifiers: Modifiers::empty(),
                })
            } else {
                KeyHandleResult::Passthrough
            }
        }
    }

    struct ConsumePlugin;
    impl PluginBackend for ConsumePlugin {
        fn id(&self) -> PluginId {
            PluginId("consume".into())
        }

        fn handle_key_middleware(
            &mut self,
            key: &KeyEvent,
            _state: &AppView<'_>,
        ) -> KeyHandleResult {
            if key.key == Key::Char('b') {
                KeyHandleResult::Consumed(vec![Command::SendToKakoune(KasaneRequest::Keys(vec![
                    "consumed".to_string(),
                ]))])
            } else {
                KeyHandleResult::Passthrough
            }
        }
    }

    let mut state = Box::new(AppState::default());
    let mut registry = PluginRuntime::new();
    registry.register_backend(Box::new(TransformPlugin));
    registry.register_backend(Box::new(ConsumePlugin));

    let result = update_in_place(
        &mut state,
        Msg::Key(KeyEvent {
            key: Key::Char('a'),
            modifiers: Modifiers::empty(),
        }),
        &mut registry,
        3,
    );

    assert!(result.flags.is_empty());
    assert_eq!(result.source_plugin, Some(PluginId("consume".into())));
    assert_eq!(result.commands.len(), 1);
    match &result.commands[0] {
        Command::SendToKakoune(KasaneRequest::Keys(keys)) => {
            assert_eq!(keys, &vec!["consumed".to_string()]);
        }
        _ => panic!("expected consumer command"),
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
            _state: &AppView<'_>,
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
        id: InteractiveId::framework(42),
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
        fn observe_key(&mut self, _key: &KeyEvent, _state: &AppView<'_>) {
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
        fn observe_key(&mut self, _key: &KeyEvent, _state: &AppView<'_>) {
            self.0.store(true, Ordering::Relaxed);
        }
    }

    struct HandlerPlugin;
    impl PluginBackend for HandlerPlugin {
        fn id(&self) -> PluginId {
            PluginId("handler".into())
        }
        fn handle_key(&mut self, _key: &KeyEvent, _state: &AppView<'_>) -> Option<Vec<Command>> {
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
        fn handle_key(&mut self, key: &KeyEvent, _state: &AppView<'_>) -> Option<Vec<Command>> {
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
        fn observe_mouse(&mut self, _event: &MouseEvent, _state: &AppView<'_>) {
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
        fn on_state_changed_effects(&mut self, _state: &AppView<'_>, dirty: DirtyFlags) -> Effects {
            if dirty.contains(DirtyFlags::BUFFER) {
                self.0.store(true, Ordering::Relaxed);
            }
            Effects::default()
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

        fn on_state_changed_effects(&mut self, _state: &AppView<'_>, dirty: DirtyFlags) -> Effects {
            if !dirty.contains(DirtyFlags::BUFFER) {
                return Effects::default();
            }
            Effects {
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

// --- PluginEffects trait tests ---

#[test]
fn update_key_with_null_effects_passes_through_to_kakoune() {
    let mut state = Box::new(AppState::default());
    let mut effects = NullEffects;
    let key = KeyEvent {
        key: Key::Char('a'),
        modifiers: Modifiers::empty(),
    };
    let result = update_in_place(&mut state, Msg::Key(key), &mut effects, 3);
    assert_eq!(result.commands.len(), 1);
    assert!(matches!(result.commands[0], Command::SendToKakoune(_)));
}

#[test]
fn update_key_records_observations() {
    let mut state = Box::new(AppState::default());
    let mut effects = RecordingEffects::default();
    let key = KeyEvent {
        key: Key::Char('x'),
        modifiers: Modifiers::empty(),
    };
    update_in_place(&mut state, Msg::Key(key.clone()), &mut effects, 3);
    assert_eq!(effects.key_observations.len(), 1);
    assert_eq!(effects.key_dispatches.len(), 1);
    assert_eq!(effects.key_observations[0], key);
}

#[test]
fn update_resize_with_null_effects_sends_resize_command() {
    let mut state = Box::new(AppState::default());
    let mut effects = NullEffects;
    let result = update_in_place(
        &mut state,
        Msg::Resize {
            cols: 120,
            rows: 40,
        },
        &mut effects,
        3,
    );
    assert_eq!(state.cols, 120);
    assert_eq!(state.rows, 40);
    assert!(result.flags.contains(DirtyFlags::ALL));
}

// --- Display unit dispatch tests ---

#[test]
fn mouse_press_on_fold_summary_suppressed() {
    use crate::display::{DisplayDirective, DisplayMap, DisplayUnitMap};
    use std::sync::Arc;

    let mut state = Box::new(AppState::default());
    state.cols = 80;
    state.rows = 24;
    let mut registry = PluginRuntime::new();

    // Build a non-identity display map with a fold at lines 2..5
    let directives = vec![DisplayDirective::Fold {
        range: 2..5,
        summary: vec![crate::protocol::Atom {
            face: Face::default(),
            contents: "folded".into(),
        }],
    }];
    let dm = DisplayMap::build(10, &directives);
    let dum = DisplayUnitMap::build(&dm);
    state.display_map = Some(Arc::new(dm));
    state.display_unit_map = Some(dum);
    state.display_scroll_offset = 0;

    // Click on display line 2 = fold summary (ReadOnly)
    let mouse = MouseEvent {
        kind: MouseEventKind::Press(MouseButton::Left),
        line: 2,
        column: 5,
        modifiers: Modifiers::empty(),
    };
    let result = update_in_place(&mut state, Msg::Mouse(mouse), &mut registry, 3);
    // Fold summary press triggers ToggleFold (DU-3): returns BUFFER_CONTENT, no commands
    assert!(
        result.commands.is_empty(),
        "fold summary click should not forward to Kakoune"
    );
    assert!(
        result.flags.contains(DirtyFlags::BUFFER_CONTENT),
        "fold summary press should trigger redraw via fold toggle"
    );
    // Fold range should now be expanded in fold_toggle_state
    assert!(
        state.fold_toggle_state.is_expanded(&(2..5)),
        "fold range 2..5 should be expanded after click"
    );
}

#[test]
fn mouse_press_on_normal_line_forwards_to_kakoune() {
    use crate::display::{DisplayDirective, DisplayMap, DisplayUnitMap};
    use std::sync::Arc;

    let mut state = Box::new(AppState::default());
    state.cols = 80;
    state.rows = 24;
    let mut registry = PluginRuntime::new();

    let directives = vec![DisplayDirective::Fold {
        range: 2..5,
        summary: vec![crate::protocol::Atom {
            face: Face::default(),
            contents: "folded".into(),
        }],
    }];
    let dm = DisplayMap::build(10, &directives);
    let dum = DisplayUnitMap::build(&dm);
    state.display_map = Some(Arc::new(dm));
    state.display_unit_map = Some(dum);
    state.display_scroll_offset = 0;

    // Click on display line 0 = buffer line 0 (Normal)
    let mouse = MouseEvent {
        kind: MouseEventKind::Press(MouseButton::Left),
        line: 0,
        column: 5,
        modifiers: Modifiers::empty(),
    };
    let result = update_in_place(&mut state, Msg::Mouse(mouse), &mut registry, 3);
    assert_eq!(
        result.commands.len(),
        1,
        "normal line click should forward to Kakoune"
    );
    assert!(matches!(result.commands[0], Command::SendToKakoune(_)));
}

#[test]
fn mouse_press_on_virtual_text_suppressed() {
    use crate::display::{DisplayDirective, DisplayMap, DisplayUnitMap};
    use std::sync::Arc;

    let mut state = Box::new(AppState::default());
    state.cols = 80;
    state.rows = 24;
    let mut registry = PluginRuntime::new();

    let directives = vec![DisplayDirective::InsertAfter {
        after: 0,
        content: vec![crate::protocol::Atom {
            face: Face::default(),
            contents: "virtual".into(),
        }],
    }];
    let dm = DisplayMap::build(3, &directives);
    let dum = DisplayUnitMap::build(&dm);
    state.display_map = Some(Arc::new(dm));
    state.display_unit_map = Some(dum);
    state.display_scroll_offset = 0;

    // Display: [buf(0), virtual, buf(1), buf(2)]
    // Click on display line 1 = virtual text (ReadOnly)
    let mouse = MouseEvent {
        kind: MouseEventKind::Press(MouseButton::Left),
        line: 1,
        column: 5,
        modifiers: Modifiers::empty(),
    };
    let result = update_in_place(&mut state, Msg::Mouse(mouse), &mut registry, 3);
    assert!(
        result.commands.is_empty(),
        "virtual text click should be suppressed"
    );
}

// --- DU-3: Fold toggle integration tests ---

#[test]
fn mouse_click_fold_summary_toggles_fold_state() {
    use crate::display::{DisplayDirective, DisplayMap, DisplayUnitMap};
    use std::sync::Arc;

    let mut state = Box::new(AppState::default());
    state.cols = 80;
    state.rows = 24;
    let mut registry = PluginRuntime::new();

    let directives = vec![DisplayDirective::Fold {
        range: 3..7,
        summary: vec![crate::protocol::Atom {
            face: Face::default(),
            contents: "...".into(),
        }],
    }];
    let dm = DisplayMap::build(10, &directives);
    let dum = DisplayUnitMap::build(&dm);
    state.display_map = Some(Arc::new(dm));
    state.display_unit_map = Some(dum);
    state.display_scroll_offset = 0;

    // Before click: fold range not expanded
    assert!(!state.fold_toggle_state.is_expanded(&(3..7)));

    // Click on fold summary line (display line 3)
    let mouse = MouseEvent {
        kind: MouseEventKind::Press(MouseButton::Left),
        line: 3,
        column: 0,
        modifiers: Modifiers::empty(),
    };
    let result = update_in_place(&mut state, Msg::Mouse(mouse), &mut registry, 3);

    assert!(result.flags.contains(DirtyFlags::BUFFER_CONTENT));
    assert!(result.commands.is_empty());
    assert!(state.fold_toggle_state.is_expanded(&(3..7)));
}

#[test]
fn mouse_move_on_fold_summary_suppressed_without_toggle() {
    use crate::display::{DisplayDirective, DisplayMap, DisplayUnitMap};
    use std::sync::Arc;

    let mut state = Box::new(AppState::default());
    state.cols = 80;
    state.rows = 24;
    let mut registry = PluginRuntime::new();

    let directives = vec![DisplayDirective::Fold {
        range: 2..5,
        summary: vec![crate::protocol::Atom {
            face: Face::default(),
            contents: "folded".into(),
        }],
    }];
    let dm = DisplayMap::build(10, &directives);
    let dum = DisplayUnitMap::build(&dm);
    state.display_map = Some(Arc::new(dm));
    state.display_unit_map = Some(dum);
    state.display_scroll_offset = 0;

    // Mouse move (not press) on fold summary — should suppress but NOT toggle
    let mouse = MouseEvent {
        kind: MouseEventKind::Move,
        line: 2,
        column: 5,
        modifiers: Modifiers::empty(),
    };
    let result = update_in_place(&mut state, Msg::Mouse(mouse), &mut registry, 3);

    assert!(result.commands.is_empty());
    assert!(result.flags.is_empty());
    assert!(
        !state.fold_toggle_state.is_expanded(&(2..5)),
        "mouse move should not toggle fold"
    );
}

#[test]
fn fold_toggle_cleared_on_draw() {
    let mut state = AppState::default();

    // Set some fold toggle state
    state.fold_toggle_state.toggle(&(3..7));
    assert!(state.fold_toggle_state.is_expanded(&(3..7)));

    // Apply a Draw request — should clear fold toggle state
    state.apply(KakouneRequest::Draw {
        lines: vec![make_line("hello")],
        cursor_pos: Coord::default(),
        default_face: Face::default(),
        padding_face: Face::default(),
        widget_columns: 0,
    });

    assert!(
        !state.fold_toggle_state.is_expanded(&(3..7)),
        "fold toggle state should be cleared after Draw"
    );
}

// --- DU-4: Plugin dispatch tests ---

#[test]
fn fold_summary_click_dispatches_through_null_effects() {
    use crate::display::{DisplayDirective, DisplayMap, DisplayUnitMap};

    let mut state = Box::new(AppState::default());
    state.cols = 80;
    state.rows = 24;
    let mut effects = NullEffects;

    let directives = vec![DisplayDirective::Fold {
        range: 2..5,
        summary: vec![crate::protocol::Atom {
            face: Face::default(),
            contents: "folded".into(),
        }],
    }];
    let dm = DisplayMap::build(10, &directives);
    let dum = DisplayUnitMap::build(&dm);
    state.display_map = Some(Arc::new(dm));
    state.display_unit_map = Some(dum);
    state.display_scroll_offset = 0;

    let mouse = MouseEvent {
        kind: MouseEventKind::Press(MouseButton::Left),
        line: 2,
        column: 5,
        modifiers: Modifiers::empty(),
    };
    let result = update_in_place(&mut state, Msg::Mouse(mouse), &mut effects, 3);
    // NullEffects returns Pass for dispatch_navigation_action,
    // so fallback fold toggle should still work
    assert!(result.flags.contains(DirtyFlags::BUFFER_CONTENT));
    assert!(state.fold_toggle_state.is_expanded(&(2..5)));
}

#[test]
fn fold_summary_click_recording_effects_dispatches_action() {
    use crate::display::{DisplayDirective, DisplayMap, DisplayUnitMap};

    let mut state = Box::new(AppState::default());
    state.cols = 80;
    state.rows = 24;
    let mut effects = RecordingEffects::default();

    let directives = vec![DisplayDirective::Fold {
        range: 2..5,
        summary: vec![crate::protocol::Atom {
            face: Face::default(),
            contents: "folded".into(),
        }],
    }];
    let dm = DisplayMap::build(10, &directives);
    let dum = DisplayUnitMap::build(&dm);
    state.display_map = Some(Arc::new(dm));
    state.display_unit_map = Some(dum);
    state.display_scroll_offset = 0;

    let mouse = MouseEvent {
        kind: MouseEventKind::Press(MouseButton::Left),
        line: 2,
        column: 5,
        modifiers: Modifiers::empty(),
    };
    let _result = update_in_place(&mut state, Msg::Mouse(mouse), &mut effects, 3);
    // RecordingEffects should have recorded the navigation action dispatch
    assert_eq!(
        effects.navigation_action_dispatches.len(),
        1,
        "should dispatch one navigation action"
    );
    let (ref unit, ref action) = effects.navigation_action_dispatches[0];
    assert_eq!(unit.role, crate::display::unit::SemanticRole::FoldSummary);
    assert_eq!(*action, crate::display::NavigationAction::ToggleFold,);
}
