use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use crate::element::InteractiveId;
use crate::input::{Key, KeyEvent, Modifiers, MouseButton, MouseEvent, MouseEventKind};
use crate::layout::{Rect, build_hit_map};
use crate::plugin::{
    Command, KeyHandleResult, NullEffects, PluginId, PluginRuntime, RecordingEffects,
};
use crate::protocol::{Coord, KakouneRequest, KasaneRequest};
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
fn test_update_text_input_forwards_committed_text_to_kakoune() {
    let mut state = Box::new(AppState::default());
    let mut registry = PluginRuntime::new();

    let text = "a<b>\n日本語";
    let result = update_in_place(
        &mut state,
        Msg::TextInput(text.to_string()),
        &mut registry,
        3,
    );

    assert!(result.flags.is_empty());
    assert_eq!(result.commands.len(), 1);
    assert!(matches!(
        &result.commands[0],
        Command::InsertText(committed) if committed == text
    ));
}

#[test]
fn test_text_input_observation_and_dispatch_are_recorded() {
    let mut state = Box::new(AppState::default());
    let mut effects = RecordingEffects::default();
    let text = "かな";

    let result = update_in_place(&mut state, Msg::TextInput(text.into()), &mut effects, 3);

    assert!(result.flags.is_empty());
    assert_eq!(effects.text_input_observations, vec![text.to_string()]);
    assert_eq!(effects.text_input_dispatches, vec![text.to_string()]);
    assert!(matches!(
        &result.commands[0],
        Command::InsertText(committed) if committed == text
    ));
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
            default_style: crate::protocol::default_unresolved_style(),
            padding_style: crate::protocol::default_unresolved_style(),
            widget_columns: 0,
        }),
        &mut registry,
        3,
    );
    let flags = result.flags;
    let commands = result.commands;
    assert!(flags.contains(DirtyFlags::BUFFER));
    assert!(commands.is_empty());
    assert_eq!(state.observed.lines.len(), 1);
}

#[test]
fn test_update_focus_lost() {
    let mut state = Box::new(AppState::default());
    let mut registry = PluginRuntime::new();

    let flags = update_in_place(&mut state, Msg::FocusLost, &mut registry, 3).flags;
    assert_eq!(flags, DirtyFlags::ALL);
    assert!(!state.runtime.focused);
}

#[test]
fn test_update_focus_gained() {
    let mut state = Box::new(AppState::default());
    state.runtime.focused = false;
    let mut registry = PluginRuntime::new();

    let flags = update_in_place(&mut state, Msg::FocusGained, &mut registry, 3).flags;
    assert_eq!(flags, DirtyFlags::ALL);
    assert!(state.runtime.focused);
}

#[test]
fn test_update_plugin_handles_key() {
    struct KeyPlugin;
    impl crate::plugin::Plugin for KeyPlugin {
        type State = ();
        fn id(&self) -> PluginId {
            PluginId::from("key_plugin")
        }
        fn register(&self, r: &mut crate::plugin::HandlerRegistry<()>) {
            r.on_key(|_state, _key, _app| {
                Some((
                    (),
                    vec![Command::SendToKakoune(
                        crate::protocol::KasaneRequest::Keys(vec!["<esc>".to_string()]),
                    )],
                ))
            });
        }
    }

    let mut state = Box::new(AppState::default());
    let mut registry = PluginRuntime::new();
    registry.register(KeyPlugin);

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
    impl crate::plugin::Plugin for TransformPlugin {
        type State = ();
        fn id(&self) -> PluginId {
            PluginId::from("transform")
        }
        fn register(&self, r: &mut crate::plugin::HandlerRegistry<()>) {
            r.on_key_middleware(|state, key, _app| {
                let result = if key.key == Key::Char('a') {
                    KeyHandleResult::Transformed(KeyEvent {
                        key: Key::Char('b'),
                        modifiers: Modifiers::SHIFT,
                    })
                } else {
                    KeyHandleResult::Passthrough
                };
                (state.clone(), result)
            });
        }
    }

    let mut state = Box::new(AppState::default());
    let mut registry = PluginRuntime::new();
    registry.register(TransformPlugin);

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
    impl crate::plugin::Plugin for TransformPlugin {
        type State = ();
        fn id(&self) -> PluginId {
            PluginId::from("transform")
        }
        fn register(&self, r: &mut crate::plugin::HandlerRegistry<()>) {
            r.on_key_middleware(|state, key, _app| {
                let result = if key.key == Key::Char('a') {
                    KeyHandleResult::Transformed(KeyEvent {
                        key: Key::Char('b'),
                        modifiers: Modifiers::empty(),
                    })
                } else {
                    KeyHandleResult::Passthrough
                };
                (state.clone(), result)
            });
        }
    }

    struct ConsumePlugin;
    impl crate::plugin::Plugin for ConsumePlugin {
        type State = ();
        fn id(&self) -> PluginId {
            PluginId::from("consume")
        }
        fn register(&self, r: &mut crate::plugin::HandlerRegistry<()>) {
            r.on_key_middleware(|state, key, _app| {
                let result = if key.key == Key::Char('b') {
                    KeyHandleResult::Consumed(vec![Command::SendToKakoune(KasaneRequest::Keys(
                        vec!["consumed".to_string()],
                    ))])
                } else {
                    KeyHandleResult::Passthrough
                };
                (state.clone(), result)
            });
        }
    }

    let mut state = Box::new(AppState::default());
    let mut registry = PluginRuntime::new();
    registry.register(TransformPlugin);
    registry.register(ConsumePlugin);

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
    assert_eq!(result.source_plugin, Some(PluginId::from("consume")));
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
    impl crate::plugin::Plugin for MousePlugin {
        type State = ();
        fn id(&self) -> PluginId {
            PluginId::from("mouse_plugin")
        }
        fn register(&self, r: &mut crate::plugin::HandlerRegistry<()>) {
            r.on_handle_mouse(|_state, _event, _id, _app| {
                Some(((), vec![Command::RequestRedraw(DirtyFlags::INFO)]))
            });
        }
    }

    let mut state = Box::new(AppState::default());
    state.runtime.cols = 80;
    state.runtime.rows = 24;
    let mut registry = PluginRuntime::new();
    registry.register(MousePlugin);

    // Build a HitMap with an interactive region at (5,3)-(12,3)
    let el = crate::element::Element::Interactive {
        child: Box::new(crate::element::Element::plain_text("click me")),
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
    state.runtime.hit_map = hit_map;

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
    state.runtime.cols = 80;
    state.runtime.rows = 24;
    let mut registry = PluginRuntime::new();
    registry.register(crate::input::BuiltinDragPlugin);
    registry.register(crate::input::BuiltinMouseFallbackPlugin);
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
    impl crate::plugin::Plugin for ObserverPlugin {
        type State = ();
        fn id(&self) -> PluginId {
            PluginId::from("observer")
        }
        fn register(&self, r: &mut crate::plugin::HandlerRegistry<()>) {
            let flag = self.0.clone();
            r.on_observe_key(move |_state, _key, _app| {
                flag.store(true, Ordering::Relaxed);
            });
        }
    }

    let mut state = Box::new(AppState::default());
    let mut registry = PluginRuntime::new();
    registry.register(ObserverPlugin(observed.clone()));

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
    impl crate::plugin::Plugin for ObserverPlugin {
        type State = ();
        fn id(&self) -> PluginId {
            PluginId::from("observer")
        }
        fn register(&self, r: &mut crate::plugin::HandlerRegistry<()>) {
            let flag = self.0.clone();
            r.on_observe_key(move |_state, _key, _app| {
                flag.store(true, Ordering::Relaxed);
            });
        }
    }

    struct HandlerPlugin;
    impl crate::plugin::Plugin for HandlerPlugin {
        type State = ();
        fn id(&self) -> PluginId {
            PluginId::from("handler")
        }
        fn register(&self, r: &mut crate::plugin::HandlerRegistry<()>) {
            r.on_key(|_state, _key, _app| Some(((), Vec::<Command>::new())));
        }
    }

    let mut state = Box::new(AppState::default());
    let mut registry = PluginRuntime::new();
    registry.register(ObserverPlugin(observed.clone()));
    registry.register(HandlerPlugin);

    let key = KeyEvent {
        key: Key::Char('x'),
        modifiers: Modifiers::empty(),
    };
    let _ = update_in_place(&mut state, Msg::Key(key), &mut registry, 3);
    assert!(observed.load(Ordering::Relaxed));
}

#[test]
fn test_observe_text_input_called_for_all_plugins() {
    let observed = Arc::new(AtomicBool::new(false));

    struct ObserverPlugin(Arc<AtomicBool>);
    impl crate::plugin::Plugin for ObserverPlugin {
        type State = ();
        fn id(&self) -> PluginId {
            PluginId::from("observer")
        }
        fn register(&self, r: &mut crate::plugin::HandlerRegistry<()>) {
            let flag = self.0.clone();
            r.on_observe_text_input(move |_state, _text, _app| {
                flag.store(true, Ordering::Relaxed);
            });
        }
    }

    let mut state = Box::new(AppState::default());
    let mut registry = PluginRuntime::new();
    registry.register(ObserverPlugin(observed.clone()));

    let _ = update_in_place(&mut state, Msg::TextInput("text".into()), &mut registry, 3);
    assert!(observed.load(Ordering::Relaxed));
}

#[test]
fn test_plugin_can_handle_text_input() {
    struct TextPlugin;
    impl crate::plugin::Plugin for TextPlugin {
        type State = ();
        fn id(&self) -> PluginId {
            PluginId::from("text_plugin")
        }
        fn register(&self, r: &mut crate::plugin::HandlerRegistry<()>) {
            r.on_text_input(|_state, text, _app| {
                Some(((), vec![Command::InsertText(text.to_uppercase())]))
            });
        }
    }

    let mut state = Box::new(AppState::default());
    let mut registry = PluginRuntime::new();
    registry.register(TextPlugin);

    let result = update_in_place(&mut state, Msg::TextInput("abc".into()), &mut registry, 3);

    assert!(result.flags.is_empty());
    assert_eq!(result.source_plugin, Some(PluginId::from("text_plugin")));
    assert_eq!(result.commands.len(), 1);
    assert!(matches!(
        &result.commands[0],
        Command::InsertText(committed) if committed == "ABC"
    ));
}

#[test]
fn test_plugin_can_override_pageup() {
    struct PageUpPlugin;
    impl crate::plugin::Plugin for PageUpPlugin {
        type State = ();
        fn id(&self) -> PluginId {
            PluginId::from("pageup_override")
        }
        fn register(&self, r: &mut crate::plugin::HandlerRegistry<()>) {
            r.on_key(|_state, key, _app| {
                if key.key == Key::PageUp {
                    Some((
                        (),
                        vec![Command::SendToKakoune(KasaneRequest::Keys(vec![
                            "custom_pageup".to_string(),
                        ]))],
                    ))
                } else {
                    None
                }
            });
        }
    }

    let mut state = Box::new(AppState::default());
    let mut registry = PluginRuntime::new();
    registry.register(PageUpPlugin);

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
    impl crate::plugin::Plugin for MouseObserver {
        type State = ();
        fn id(&self) -> PluginId {
            PluginId::from("mouse_observer")
        }
        fn register(&self, r: &mut crate::plugin::HandlerRegistry<()>) {
            let flag = self.0.clone();
            r.on_observe_mouse(move |_state, _event, _app| {
                flag.store(true, Ordering::Relaxed);
            });
        }
    }

    let mut state = Box::new(AppState::default());
    state.runtime.cols = 80;
    state.runtime.rows = 24;
    let mut registry = PluginRuntime::new();
    registry.register(MouseObserver(observed.clone()));
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
    impl crate::plugin::Plugin for StateWatcher {
        type State = ();
        fn id(&self) -> PluginId {
            PluginId::from("watcher")
        }
        fn register(&self, r: &mut crate::plugin::HandlerRegistry<()>) {
            let flag = self.0.clone();
            r.on_state_changed_tier1(move |_state, _app, dirty| {
                if dirty.contains(DirtyFlags::BUFFER) {
                    flag.store(true, Ordering::Relaxed);
                }
                ((), crate::plugin::KakouneSideEffects::none())
            });
        }
    }

    let mut state = Box::new(AppState::default());
    let mut registry = PluginRuntime::new();
    registry.register(StateWatcher(called.clone()));

    let flags = update_in_place(
        &mut state,
        Msg::Kakoune(KakouneRequest::Draw {
            lines: vec![make_line("hello")],
            cursor_pos: Coord::default(),
            default_style: crate::protocol::default_unresolved_style(),
            padding_style: crate::protocol::default_unresolved_style(),
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

    impl crate::plugin::Plugin for StateWatcher {
        type State = ();
        fn id(&self) -> PluginId {
            PluginId::from("watcher-effects")
        }
        fn register(&self, r: &mut crate::plugin::HandlerRegistry<()>) {
            r.on_state_changed_tier1(|_state, _app, dirty| {
                if !dirty.contains(DirtyFlags::BUFFER) {
                    return ((), crate::plugin::KakouneSideEffects::none());
                }
                let mut effects = crate::plugin::KakouneSideEffects::redraw(DirtyFlags::STATUS);
                effects.base.scroll_plans.push(ScrollPlan {
                    total_amount: 4,
                    line: 1,
                    column: 2,
                    frame_interval_ms: 16,
                    curve: ScrollCurve::Linear,
                    accumulation: ScrollAccumulationMode::Add,
                });
                ((), effects)
            });
        }
    }

    let mut state = Box::new(AppState::default());
    let mut registry = PluginRuntime::new();
    registry.register(StateWatcher);

    let result = update_in_place(
        &mut state,
        Msg::Kakoune(KakouneRequest::Draw {
            lines: vec![make_line("hello")],
            cursor_pos: Coord::default(),
            default_style: crate::protocol::default_unresolved_style(),
            padding_style: crate::protocol::default_unresolved_style(),
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
    assert_eq!(state.runtime.cols, 120);
    assert_eq!(state.runtime.rows, 40);
    assert!(result.flags.contains(DirtyFlags::ALL));
}

// --- Display unit dispatch tests ---

#[test]
fn mouse_press_on_fold_summary_suppressed() {
    use crate::display::{DisplayDirective, DisplayMap, DisplayUnitMap};
    use crate::input::BuiltinFoldPlugin;
    use std::sync::Arc;

    let mut state = Box::new(AppState::default());
    state.runtime.cols = 80;
    state.runtime.rows = 24;
    let mut registry = PluginRuntime::new();
    registry.register(BuiltinFoldPlugin);

    // Build a non-identity display map with a fold at lines 2..5
    let directives = vec![DisplayDirective::Fold {
        range: 2..5,
        summary: vec![crate::protocol::Atom::plain("folded")],
    }];
    let dm = DisplayMap::build(10, &directives);
    let dum = DisplayUnitMap::build(&dm);
    state.runtime.display_map = Some(Arc::new(dm));
    state.runtime.display_unit_map = Some(dum);
    state.runtime.display_scroll_offset = 0;

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
        state.config.fold_toggle_state.is_expanded(&(2..5)),
        "fold range 2..5 should be expanded after click"
    );
}

#[test]
fn mouse_press_on_normal_line_forwards_to_kakoune() {
    use crate::display::{DisplayDirective, DisplayMap, DisplayUnitMap};
    use std::sync::Arc;

    let mut state = Box::new(AppState::default());
    state.runtime.cols = 80;
    state.runtime.rows = 24;
    let mut registry = PluginRuntime::new();
    registry.register(crate::input::BuiltinDragPlugin);
    registry.register(crate::input::BuiltinMouseFallbackPlugin);

    let directives = vec![DisplayDirective::Fold {
        range: 2..5,
        summary: vec![crate::protocol::Atom::plain("folded")],
    }];
    let dm = DisplayMap::build(10, &directives);
    let dum = DisplayUnitMap::build(&dm);
    state.runtime.display_map = Some(Arc::new(dm));
    state.runtime.display_unit_map = Some(dum);
    state.runtime.display_scroll_offset = 0;

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

// --- DU-3: Fold toggle integration tests ---

#[test]
fn mouse_click_fold_summary_toggles_fold_state() {
    use crate::display::{DisplayDirective, DisplayMap, DisplayUnitMap};
    use crate::input::BuiltinFoldPlugin;
    use std::sync::Arc;

    let mut state = Box::new(AppState::default());
    state.runtime.cols = 80;
    state.runtime.rows = 24;
    let mut registry = PluginRuntime::new();
    registry.register(BuiltinFoldPlugin);

    let directives = vec![DisplayDirective::Fold {
        range: 3..7,
        summary: vec![crate::protocol::Atom::plain("...")],
    }];
    let dm = DisplayMap::build(10, &directives);
    let dum = DisplayUnitMap::build(&dm);
    state.runtime.display_map = Some(Arc::new(dm));
    state.runtime.display_unit_map = Some(dum);
    state.runtime.display_scroll_offset = 0;

    // Before click: fold range not expanded
    assert!(!state.config.fold_toggle_state.is_expanded(&(3..7)));

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
    assert!(state.config.fold_toggle_state.is_expanded(&(3..7)));
}

#[test]
fn mouse_move_on_fold_summary_suppressed_without_toggle() {
    use crate::display::{DisplayDirective, DisplayMap, DisplayUnitMap};
    use std::sync::Arc;

    let mut state = Box::new(AppState::default());
    state.runtime.cols = 80;
    state.runtime.rows = 24;
    let mut registry = PluginRuntime::new();

    let directives = vec![DisplayDirective::Fold {
        range: 2..5,
        summary: vec![crate::protocol::Atom::plain("folded")],
    }];
    let dm = DisplayMap::build(10, &directives);
    let dum = DisplayUnitMap::build(&dm);
    state.runtime.display_map = Some(Arc::new(dm));
    state.runtime.display_unit_map = Some(dum);
    state.runtime.display_scroll_offset = 0;

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
        !state.config.fold_toggle_state.is_expanded(&(2..5)),
        "mouse move should not toggle fold"
    );
}

#[test]
fn fold_toggle_cleared_on_draw() {
    let mut state = AppState::default();

    // Set some fold toggle state
    state.config.fold_toggle_state.toggle(&(3..7));
    assert!(state.config.fold_toggle_state.is_expanded(&(3..7)));

    // Apply a Draw request — should clear fold toggle state
    state.apply(KakouneRequest::Draw {
        lines: vec![make_line("hello")],
        cursor_pos: Coord::default(),
        default_style: crate::protocol::default_unresolved_style(),
        padding_style: crate::protocol::default_unresolved_style(),
        widget_columns: 0,
    });

    assert!(
        !state.config.fold_toggle_state.is_expanded(&(3..7)),
        "fold toggle state should be cleared after Draw"
    );
}

// --- DU-4: Plugin dispatch tests ---

#[test]
fn fold_summary_click_dispatches_through_builtin_fold_plugin() {
    use crate::display::{DisplayDirective, DisplayMap, DisplayUnitMap};
    use crate::input::BuiltinFoldPlugin;

    let mut state = Box::new(AppState::default());
    state.runtime.cols = 80;
    state.runtime.rows = 24;
    let mut registry = PluginRuntime::new();
    registry.register(BuiltinFoldPlugin);

    let directives = vec![DisplayDirective::Fold {
        range: 2..5,
        summary: vec![crate::protocol::Atom::plain("folded")],
    }];
    let dm = DisplayMap::build(10, &directives);
    let dum = DisplayUnitMap::build(&dm);
    state.runtime.display_map = Some(Arc::new(dm));
    state.runtime.display_unit_map = Some(dum);
    state.runtime.display_scroll_offset = 0;

    let mouse = MouseEvent {
        kind: MouseEventKind::Press(MouseButton::Left),
        line: 2,
        column: 5,
        modifiers: Modifiers::empty(),
    };
    let result = update_in_place(&mut state, Msg::Mouse(mouse), &mut registry, 3);
    // BuiltinFoldPlugin returns ToggleFold for fold summary clicks
    assert!(result.flags.contains(DirtyFlags::BUFFER_CONTENT));
    assert!(state.config.fold_toggle_state.is_expanded(&(2..5)));
}

#[test]
fn fold_summary_click_recording_effects_dispatches_action() {
    use crate::display::{DisplayDirective, DisplayMap, DisplayUnitMap};

    let mut state = Box::new(AppState::default());
    state.runtime.cols = 80;
    state.runtime.rows = 24;
    let mut effects = RecordingEffects::default();

    let directives = vec![DisplayDirective::Fold {
        range: 2..5,
        summary: vec![crate::protocol::Atom::plain("folded")],
    }];
    let dm = DisplayMap::build(10, &directives);
    let dum = DisplayUnitMap::build(&dm);
    state.runtime.display_map = Some(Arc::new(dm));
    state.runtime.display_unit_map = Some(dum);
    state.runtime.display_scroll_offset = 0;

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

// (Removed) StateChangedSpawner attribution tests
// ────────────────────────────────────────────────────────────────────────
// Two `state_changed_spawn_*` tests previously exercised issue #101
// (per-plugin source attribution in state-changed batches) by emitting
// Command::SpawnProcess from on_state_changed_effects via the deprecated
// broad-Effects setter. After Phase β-3 deletes `on_state_changed` (the
// `_tier1` variant returning KakouneSideEffects is the sole surviving
// shape), this anti-pattern is structurally banned at compile time —
// the bug the test guarded against can no longer occur.
//
// Attribution for Tier-2 commands is still covered by the
// `deliver_io_event_*` / `deliver_message_*` tests, where SpawnProcess
// emission from on_update_tier2 / on_io_event_tier2 follows the same
// EffectsBatch path that originally lost the PluginId in #100.
