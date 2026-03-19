mod support;

use kasane_core::protocol::KasaneRequest;
use kasane_core::scroll::{
    ResolvedScroll, ScrollAccumulationMode, ScrollConsumption, ScrollCurve, ScrollPlan,
    ScrollPolicyResult,
};
use support::scroll_fixtures::{mouse_scroll_down, registry_empty, state_80x24};
use support::scroll_harness::{LegacyHarness, NewHarness, TraceStep};

#[test]
fn pass_delegates_to_legacy_fallback() {
    let trace = vec![TraceStep::Input(mouse_scroll_down(10, 5))];
    let mut harness = LegacyHarness::new(state_80x24(), registry_empty());
    let legacy = harness.run_trace(&trace);
    let mut new_harness = NewHarness::new(state_80x24(), registry_empty());
    new_harness.forced_scroll_policy = Some(ScrollPolicyResult::Pass);
    let new = new_harness.run_trace(&trace);
    let result = ScrollPolicyResult::Pass;
    assert_eq!(result.consumption(), ScrollConsumption::Pass);
    assert_eq!(
        support::scroll_harness::flatten_requests(&legacy),
        support::scroll_harness::flatten_requests(&new)
    );
}

#[test]
fn suppress_blocks_fallback_and_emits_no_scroll() {
    let result = ScrollPolicyResult::Suppress;
    assert_eq!(result.consumption(), ScrollConsumption::Suppress);
    let mut harness = NewHarness::new(state_80x24(), registry_empty());
    harness.forced_scroll_policy = Some(result);
    let outcome = harness.dispatch_input(mouse_scroll_down(10, 5));
    assert!(outcome.requests().is_empty());
}

#[test]
fn immediate_emits_exactly_one_scroll_request() {
    let result = ScrollPolicyResult::Immediate(ResolvedScroll::new(3, 10, 5));
    assert_eq!(result.consumption(), ScrollConsumption::Immediate);
    let mut harness = NewHarness::new(state_80x24(), registry_empty());
    harness.forced_scroll_policy = Some(result);
    let outcome = harness.dispatch_input(mouse_scroll_down(10, 5));
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
fn plan_produces_no_immediate_request_when_host_executed() {
    let result = ScrollPolicyResult::Plan(ScrollPlan::new(
        9,
        10,
        5,
        16,
        ScrollCurve::Linear,
        ScrollAccumulationMode::Add,
    ));
    assert_eq!(result.consumption(), ScrollConsumption::Plan);
    let mut harness = NewHarness::new(state_80x24(), registry_empty());
    harness.forced_scroll_policy = Some(result);
    let outcome = harness.dispatch_input(mouse_scroll_down(10, 5));
    assert!(outcome.requests().is_empty());
    assert!(harness.runtime.has_active_plan());
}

#[test]
fn plan_total_delta_is_conserved() {
    let plan = ScrollPlan::new(
        9,
        10,
        5,
        16,
        ScrollCurve::Linear,
        ScrollAccumulationMode::Add,
    );
    let mut harness = NewHarness::new(state_80x24(), registry_empty());
    harness.forced_scroll_policy = Some(ScrollPolicyResult::Plan(plan));
    let arm = harness.dispatch_input(mouse_scroll_down(10, 5));
    assert!(arm.requests().is_empty());
    harness.runtime.complete_initial_resize();

    let mut total = 0;
    while harness.runtime.has_active_plan() {
        let tick = harness.tick_runtime();
        for request in tick.requests() {
            match request {
                KasaneRequest::Scroll { amount, .. } => total += amount,
                other => panic!("unexpected request: {other:?}"),
            }
        }
    }

    assert_eq!(total, 9);
}

#[test]
fn plan_terminates_for_finite_delta() {
    let plan = ScrollPlan::new(
        9,
        10,
        5,
        16,
        ScrollCurve::Linear,
        ScrollAccumulationMode::Replace,
    );
    let mut harness = NewHarness::new(state_80x24(), registry_empty());
    harness.forced_scroll_policy = Some(ScrollPolicyResult::Plan(plan));
    harness.dispatch_input(mouse_scroll_down(10, 5));
    harness.runtime.complete_initial_resize();

    for _ in 0..16 {
        let _ = harness.tick_runtime();
        if !harness.runtime.has_active_plan() {
            return;
        }
    }

    panic!("finite scroll plan should terminate");
}
