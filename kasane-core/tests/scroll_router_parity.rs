mod support;

use kasane_core::element::InteractiveId;
use kasane_core::input::BuiltinInputPlugin;
use kasane_core::layout::Rect;
use kasane_core::plugin::{Command, PluginBackend, PluginId};
use kasane_core::scroll::ScrollOwner;
use kasane_core::state::DirtyFlags;
use support::scroll_fixtures::{
    install_hit_region, install_info_hit_region, key_pageup, make_info_state, mouse_press_left,
    mouse_scroll_down, mouse_scroll_up, registry_empty, state_80x24,
};
use support::scroll_harness::{
    LegacyHarness, NewHarness, TraceOutcome, TraceStep, assert_same_flags, assert_same_requests,
    assert_same_visible_state,
};

fn legacy_outcome(trace: &[TraceStep]) -> TraceOutcome {
    let mut harness = LegacyHarness::new(state_80x24(), registry_empty());
    harness.run_trace(trace)
}

fn new_outcome(trace: &[TraceStep]) -> TraceOutcome {
    let mut harness = NewHarness::new(state_80x24(), registry_empty());
    harness.run_trace(trace)
}

#[test]
fn parity_single_buffer_scroll_without_plugins() {
    let trace = vec![TraceStep::Input(mouse_scroll_down(10, 5))];
    let legacy = legacy_outcome(&trace);
    let new = new_outcome(&trace);

    assert_same_requests(&legacy, &new);
    assert_same_flags(&legacy, &new);
    assert_same_visible_state(&legacy, &new);
    assert_eq!(new.emitted[0].owner, None);
    let _expected_owner = ScrollOwner::Policy;
}

#[test]
fn parity_drag_scroll_trace() {
    let mut state = state_80x24();
    state.drag = kasane_core::state::DragState::Active {
        button: kasane_core::input::MouseButton::Left,
        start_line: 5,
        start_column: 10,
    };
    let new_state = state.clone();
    let trace = vec![
        TraceStep::Input(mouse_scroll_down(10, 5)),
        TraceStep::Input(mouse_scroll_up(10, 5)),
    ];
    let mut harness = LegacyHarness::new(state, registry_empty());
    let legacy = harness.run_trace(&trace);
    let mut new_harness = NewHarness::new(new_state, registry_empty());
    let new = new_harness.run_trace(&trace);

    assert_same_requests(&legacy, &new);
    assert_same_flags(&legacy, &new);
    assert_same_visible_state(&legacy, &new);
}

#[test]
fn parity_info_popup_scroll_trace() {
    let mut state = state_80x24();
    state.infos.push(make_info_state(
        3,
        3,
        &["one", "two", "three", "four", "five", "six"],
    ));
    let mut new_state = state.clone();
    let legacy_registry = registry_empty();
    install_info_hit_region(
        &mut state,
        0,
        Rect {
            x: 2,
            y: 2,
            w: 12,
            h: 4,
        },
    );
    let new_registry = registry_empty();
    install_info_hit_region(
        &mut new_state,
        0,
        Rect {
            x: 2,
            y: 2,
            w: 12,
            h: 4,
        },
    );
    let trace = vec![
        TraceStep::Input(mouse_scroll_down(3, 3)),
        TraceStep::Input(mouse_scroll_down(10, 5)),
    ];
    let mut harness = LegacyHarness::new(state, legacy_registry);
    let legacy = harness.run_trace(&trace);
    let mut new_harness = NewHarness::new(new_state, new_registry);
    let new = new_harness.run_trace(&trace);

    assert_same_requests(&legacy, &new);
    assert_same_flags(&legacy, &new);
    assert_same_visible_state(&legacy, &new);
}

#[test]
fn parity_plugin_hit_then_miss_trace() {
    struct MousePlugin;
    impl PluginBackend for MousePlugin {
        fn id(&self) -> PluginId {
            PluginId("mouse_plugin".into())
        }

        fn handle_mouse(
            &mut self,
            _event: &kasane_core::input::MouseEvent,
            _id: InteractiveId,
            _state: &kasane_core::plugin::AppView<'_>,
        ) -> Option<Vec<Command>> {
            Some(vec![Command::RequestRedraw(DirtyFlags::INFO)])
        }
    }

    let trace = vec![
        TraceStep::Input(mouse_press_left(3, 7)),
        TraceStep::Input(mouse_press_left(12, 30)),
    ];
    let mut state = state_80x24();
    let mut legacy_registry = registry_empty();
    legacy_registry.register_backend(Box::new(MousePlugin));
    install_hit_region(
        &mut state,
        InteractiveId(42),
        Rect {
            x: 5,
            y: 3,
            w: 8,
            h: 1,
        },
    );
    let mut new_state = state.clone();
    let mut new_registry = registry_empty();
    new_registry.register_backend(Box::new(MousePlugin));
    install_hit_region(
        &mut new_state,
        InteractiveId(42),
        Rect {
            x: 5,
            y: 3,
            w: 8,
            h: 1,
        },
    );

    let mut legacy_harness = LegacyHarness::new(state, legacy_registry);
    let legacy = legacy_harness.run_trace(&trace);
    let mut new_harness = NewHarness::new(new_state, new_registry);
    let new = new_harness.run_trace(&trace);

    assert_same_requests(&legacy, &new);
    assert_same_flags(&legacy, &new);
    assert_same_visible_state(&legacy, &new);
}

#[test]
fn parity_pageup_trace() {
    let trace = vec![TraceStep::Input(key_pageup())];
    let mut state = state_80x24();
    state.cursor_pos = kasane_core::protocol::Coord {
        line: 10,
        column: 5,
    };
    let mut legacy_registry = registry_empty();
    legacy_registry.register_backend(Box::new(BuiltinInputPlugin));
    let mut new_registry = registry_empty();
    new_registry.register_backend(Box::new(BuiltinInputPlugin));
    let mut legacy_harness = LegacyHarness::new(state.clone(), legacy_registry);
    let legacy = legacy_harness.run_trace(&trace);
    let mut new_harness = NewHarness::new(state, new_registry);
    let new = new_harness.run_trace(&trace);

    assert_same_requests(&legacy, &new);
    assert_same_flags(&legacy, &new);
    assert_same_visible_state(&legacy, &new);
}
