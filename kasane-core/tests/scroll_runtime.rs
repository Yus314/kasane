mod support;

use kasane_core::protocol::KasaneRequest;
use kasane_core::scroll::{ScrollAccumulationMode, ScrollCurve, ScrollPlan};
use support::scroll_fixtures::{mouse_scroll_down, registry_empty, state_80x24};
use support::scroll_harness::NewHarness;

#[test]
fn runtime_does_not_emit_before_initial_resize() {
    let plan = ScrollPlan::new(
        3,
        10,
        5,
        16,
        ScrollCurve::Linear,
        ScrollAccumulationMode::Add,
    );
    let mut new_harness = NewHarness::new(state_80x24(), registry_empty());
    new_harness.forced_scroll_policy = Some(kasane_core::scroll::ScrollPolicyResult::Plan(plan));
    new_harness.dispatch_input(mouse_scroll_down(10, 5));
    let tick = new_harness.tick_runtime();
    assert!(tick.requests().is_empty());
    assert!(new_harness.runtime.has_active_plan());
}

#[test]
fn runtime_emits_after_initial_resize_is_completed() {
    let plan = ScrollPlan::new(
        9,
        10,
        5,
        16,
        ScrollCurve::Linear,
        ScrollAccumulationMode::Add,
    );
    let mut harness = NewHarness::new(state_80x24(), registry_empty());
    harness.forced_scroll_policy = Some(kasane_core::scroll::ScrollPolicyResult::Plan(plan));
    harness.dispatch_input(mouse_scroll_down(10, 5));
    harness.runtime.complete_initial_resize();
    let tick = harness.tick_runtime();
    assert_eq!(
        tick.requests(),
        vec![KasaneRequest::Scroll {
            amount: 1,
            line: 10,
            column: 5,
        }]
    );
}

#[test]
fn runtime_cancels_plan_on_session_switch() {
    let plan = ScrollPlan::new(
        9,
        10,
        5,
        16,
        ScrollCurve::Linear,
        ScrollAccumulationMode::Replace,
    );
    let mut harness = NewHarness::new(state_80x24(), registry_empty());
    harness.forced_scroll_policy = Some(kasane_core::scroll::ScrollPolicyResult::Plan(plan));
    harness.dispatch_input(mouse_scroll_down(10, 5));
    harness.runtime.complete_initial_resize();
    harness.runtime.advance_generation();
    let tick = harness.tick_runtime();
    assert!(tick.requests().is_empty());
    assert!(!harness.runtime.has_active_plan());
}

#[test]
fn runtime_drops_stale_ticks_by_generation() {
    let plan = ScrollPlan::new(
        9,
        10,
        5,
        16,
        ScrollCurve::Instant,
        ScrollAccumulationMode::Add,
    );
    let mut harness = NewHarness::new(state_80x24(), registry_empty());
    harness.forced_scroll_policy = Some(kasane_core::scroll::ScrollPolicyResult::Plan(plan));
    harness.dispatch_input(mouse_scroll_down(10, 5));
    harness.runtime.complete_initial_resize();
    harness.runtime.advance_generation();
    let tick = harness.tick_runtime();
    assert!(tick.requests().is_empty());
    assert!(!harness.runtime.has_active_plan());
}

#[test]
fn runtime_never_overshoots_total_delta() {
    let plan = ScrollPlan::new(
        2,
        10,
        5,
        16,
        ScrollCurve::Linear,
        ScrollAccumulationMode::Add,
    );
    let mut harness = NewHarness::new(state_80x24(), registry_empty());
    harness.forced_scroll_policy = Some(kasane_core::scroll::ScrollPolicyResult::Plan(plan));
    harness.dispatch_input(mouse_scroll_down(10, 5));
    harness.runtime.complete_initial_resize();

    let first = harness.tick_runtime();
    let second = harness.tick_runtime();
    let third = harness.tick_runtime();

    assert_eq!(
        first.requests(),
        vec![KasaneRequest::Scroll {
            amount: 1,
            line: 10,
            column: 5,
        }]
    );
    assert_eq!(
        second.requests(),
        vec![KasaneRequest::Scroll {
            amount: 1,
            line: 10,
            column: 5,
        }]
    );
    assert!(third.requests().is_empty());
}
