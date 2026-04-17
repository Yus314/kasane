use super::*;
use kasane_core::scroll::{
    DefaultScrollCandidate, ResolvedScroll, ScrollAccumulationMode, ScrollCurve, ScrollGranularity,
    ScrollPlan, ScrollPolicyResult,
};

#[test]
fn plugin_id() {
    let plugin = load_smooth_scroll_plugin();
    assert_eq!(plugin.id().0, "smooth_scroll");
}

#[test]
fn passes_through_when_disabled() {
    let mut plugin = load_smooth_scroll_plugin();
    let state = AppState::default();
    let candidate = DefaultScrollCandidate::new(
        10,
        5,
        Modifiers::empty(),
        ScrollGranularity::Line,
        3,
        ResolvedScroll::new(3, 10, 5),
    );

    assert_eq!(
        plugin.handle_default_scroll(candidate, &AppView::new(&state)),
        None
    );
}

#[test]
fn returns_plan_when_enabled_via_settings() {
    let mut plugin = load_smooth_scroll_plugin();
    let mut state = AppState::default();
    state
        .config
        .plugin_settings
        .entry(PluginId("smooth_scroll".into()))
        .or_default()
        .insert(
            "enabled".into(),
            kasane_core::plugin::setting::SettingValue::Bool(true),
        );
    let candidate = DefaultScrollCandidate::new(
        10,
        5,
        Modifiers::empty(),
        ScrollGranularity::Line,
        3,
        ResolvedScroll::new(3, 10, 5),
    );

    assert_eq!(
        plugin.handle_default_scroll(candidate, &AppView::new(&state)),
        Some(ScrollPolicyResult::Plan(ScrollPlan::new(
            3,
            10,
            5,
            16,
            ScrollCurve::Linear,
            ScrollAccumulationMode::Add,
        )))
    );
}

#[test]
fn returns_plan_when_enabled_via_settings_negative_amount() {
    let mut plugin = load_smooth_scroll_plugin();
    let mut state = AppState::default();
    state
        .config
        .plugin_settings
        .entry(PluginId("smooth_scroll".into()))
        .or_default()
        .insert(
            "enabled".into(),
            kasane_core::plugin::setting::SettingValue::Bool(true),
        );
    let candidate = DefaultScrollCandidate::new(
        4,
        2,
        Modifiers::empty(),
        ScrollGranularity::Line,
        -3,
        ResolvedScroll::new(-3, 4, 2),
    );

    assert_eq!(
        plugin.handle_default_scroll(candidate, &AppView::new(&state)),
        Some(ScrollPolicyResult::Plan(ScrollPlan::new(
            -3,
            4,
            2,
            16,
            ScrollCurve::Linear,
            ScrollAccumulationMode::Add,
        )))
    );
}
