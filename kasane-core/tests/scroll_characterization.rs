mod support;

use kasane_core::element::InteractiveId;
use kasane_core::input::BuiltinInputPlugin;
use kasane_core::layout::Rect;
use kasane_core::plugin::{Command, PluginBackend, PluginId};
use kasane_core::protocol::{Coord, KasaneRequest};
use kasane_core::scroll::set_smooth_scroll_enabled;
use kasane_core::state::{DirtyFlags, DragState};

use support::scroll_fixtures::{
    install_hit_region, install_info_hit_region, key_ctrl_pageup, key_pageup, make_info_state,
    mouse_press_left, mouse_scroll_down, mouse_scroll_up, registry_empty, state_80x24,
};
use support::scroll_harness::LegacyHarness;

#[test]
fn buffer_scroll_down_falls_back_to_single_scroll_request() {
    let mut harness = LegacyHarness::new(state_80x24(), registry_empty());

    let outcome = harness.dispatch_input(mouse_scroll_down(10, 5));

    assert!(outcome.dirty.is_empty());
    assert_eq!(
        outcome.requests(),
        vec![KasaneRequest::Scroll {
            amount: 3,
            line: 10,
            column: 5,
        }]
    );
}

#[test]
fn drag_scroll_down_emits_scroll_then_mouse_move_to_bottom_edge() {
    let mut state = state_80x24();
    state.drag = DragState::Active {
        button: kasane_core::input::MouseButton::Left,
        start_line: 5,
        start_column: 10,
    };
    let mut harness = LegacyHarness::new(state, registry_empty());

    let outcome = harness.dispatch_input(mouse_scroll_down(10, 5));
    let requests = outcome.requests();

    assert_eq!(requests.len(), 2);
    assert_eq!(
        requests[0],
        KasaneRequest::Scroll {
            amount: 3,
            line: 10,
            column: 5,
        }
    );
    assert_eq!(
        requests[1],
        KasaneRequest::MouseMove {
            line: 22,
            column: 5,
        }
    );
}

#[test]
fn drag_scroll_up_emits_scroll_then_mouse_move_to_top_edge() {
    let mut state = state_80x24();
    state.drag = DragState::Active {
        button: kasane_core::input::MouseButton::Left,
        start_line: 5,
        start_column: 10,
    };
    let mut harness = LegacyHarness::new(state, registry_empty());

    let outcome = harness.dispatch_input(mouse_scroll_up(10, 5));
    let requests = outcome.requests();

    assert_eq!(requests.len(), 2);
    assert_eq!(
        requests[0],
        KasaneRequest::Scroll {
            amount: -3,
            line: 10,
            column: 5,
        }
    );
    assert_eq!(requests[1], KasaneRequest::MouseMove { line: 0, column: 5 });
}

#[test]
fn info_popup_scroll_consumes_event_without_kakoune_scroll() {
    let mut state = state_80x24();
    state.infos.push(make_info_state(
        3,
        3,
        &["one", "two", "three", "four", "five", "six"],
    ));
    let registry = registry_empty();
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
    let mut harness = LegacyHarness::new(state, registry);

    let outcome = harness.dispatch_input(mouse_scroll_down(3, 3));

    assert_eq!(outcome.requests(), Vec::<KasaneRequest>::new());
    assert_eq!(harness.state.infos[0].scroll_offset, 3);
    assert_eq!(outcome.dirty, DirtyFlags::INFO);
}

#[test]
fn plugin_hit_mouse_press_consumes_before_default_mouse_forwarding() {
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

    let mut state = state_80x24();
    let mut registry = registry_empty();
    registry.register_backend(Box::new(MousePlugin));
    install_hit_region(
        &mut state,
        InteractiveId::framework(42),
        Rect {
            x: 5,
            y: 3,
            w: 8,
            h: 1,
        },
    );
    let mut harness = LegacyHarness::new(state, registry);

    let outcome = harness.dispatch_input(mouse_press_left(3, 7));

    assert!(outcome.requests().is_empty());
    assert_eq!(outcome.owner, Some(PluginId("mouse_plugin".into())));
    assert!(outcome.dirty.contains(DirtyFlags::INFO));
}

#[test]
fn pageup_builtin_emits_negative_scroll_of_available_height() {
    let mut state = state_80x24();
    state.cursor_pos = Coord {
        line: 10,
        column: 5,
    };
    let mut registry = registry_empty();
    registry.register_backend(Box::new(BuiltinInputPlugin));
    let mut harness = LegacyHarness::new(state, registry);

    let outcome = harness.dispatch_input(key_pageup());

    assert_eq!(
        outcome.requests(),
        vec![KasaneRequest::Scroll {
            amount: -23,
            line: 10,
            column: 5,
        }]
    );
}

#[test]
fn modified_pageup_is_forwarded_as_key_not_scroll() {
    let mut registry = registry_empty();
    registry.register_backend(Box::new(BuiltinInputPlugin));
    let mut harness = LegacyHarness::new(state_80x24(), registry);

    let outcome = harness.dispatch_input(key_ctrl_pageup());

    assert_eq!(
        outcome.requests(),
        vec![KasaneRequest::Keys(vec!["<c-pageup>".to_string()])]
    );
}

#[test]
fn smooth_scroll_enabled_arms_animation_without_immediate_scroll_request() {
    let mut state = state_80x24();
    set_smooth_scroll_enabled(&mut state.plugin_config, true);
    let mut registry = registry_empty();
    registry.register_backend(Box::new(BuiltinInputPlugin));
    let mut harness = LegacyHarness::new(state, registry);

    let outcome = harness.dispatch_input(mouse_scroll_down(8, 3));

    assert!(outcome.requests().is_empty());
    let plan = harness
        .runtime
        .active_plan
        .as_ref()
        .expect("smooth scroll should arm runtime plan");
    assert_eq!(plan.remaining_amount, 3);
    assert_eq!(plan.line, 8);
    assert_eq!(plan.column, 3);
}

#[test]
fn smooth_scroll_tick_emits_unit_steps_until_remaining_is_zero() {
    let mut state = state_80x24();
    set_smooth_scroll_enabled(&mut state.plugin_config, true);
    let mut registry = registry_empty();
    registry.register_backend(Box::new(BuiltinInputPlugin));
    let mut harness = LegacyHarness::new(state, registry);

    let arm = harness.dispatch_input(mouse_scroll_down(8, 3));
    assert!(arm.requests().is_empty());

    let mut requests = Vec::new();
    loop {
        let tick = harness.tick_animation();
        let emitted = tick.requests();
        if emitted.is_empty() {
            break;
        }
        requests.extend(emitted);
    }

    assert_eq!(
        requests,
        vec![
            KasaneRequest::Scroll {
                amount: 1,
                line: 8,
                column: 3,
            },
            KasaneRequest::Scroll {
                amount: 1,
                line: 8,
                column: 3,
            },
            KasaneRequest::Scroll {
                amount: 1,
                line: 8,
                column: 3,
            },
        ]
    );
    assert!(!harness.runtime.has_active_plan());
}
