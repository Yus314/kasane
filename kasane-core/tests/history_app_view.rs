//! ADR-035 §2 plugin-perspective integration test.
//!
//! Simulates how a plugin would consume the time-aware accessors via
//! `AppView`, the read-only projection that plugin handlers receive.
//! Demonstrates the end-to-end path:
//!
//!   Kakoune protocol → AppState::apply → history commit → AppView::text_at

use std::sync::Arc;

use compact_str::CompactString;
use kasane_core::history::{HistoryBackend, Time};
use kasane_core::plugin::AppView;
use kasane_core::protocol::{Atom, Coord, KakouneRequest, UnresolvedStyle};
use kasane_core::state::AppState;

fn atom(s: &str) -> Atom {
    Atom::with_style(
        CompactString::from(s),
        kasane_core::protocol::Style::default(),
    )
}

fn default_style() -> Arc<UnresolvedStyle> {
    Arc::new(UnresolvedStyle::default())
}

fn draw(lines: Vec<Vec<Atom>>) -> KakouneRequest {
    KakouneRequest::Draw {
        lines,
        cursor_pos: Coord { line: 0, column: 0 },
        default_style: default_style(),
        padding_style: default_style(),
        widget_columns: 0,
    }
}

fn fresh_state() -> AppState {
    let mut state = AppState::default();
    state.runtime.rows = 5;
    state.runtime.cols = 80;
    state
}

#[test]
fn plugin_reads_current_text_via_app_view() {
    let mut state = fresh_state();
    state.apply(draw(vec![vec![atom("hello world")]]));

    // Plugin handler perspective: receives an AppView, reads via the
    // ergonomic time-aware accessor.
    let view = AppView::new(&state);
    let text = view.text_at(Time::Now).expect("Time::Now after apply");
    assert_eq!(&*text, "hello world");
}

#[test]
fn plugin_reads_past_version_via_app_view() {
    let mut state = fresh_state();

    state.apply(draw(vec![vec![atom("revision-1")]]));
    let v1 = state.history.current_version();

    state.apply(draw(vec![vec![atom("revision-2")]]));
    let v2 = state.history.current_version();

    state.apply(draw(vec![vec![atom("revision-3")]]));

    let view = AppView::new(&state);
    assert_eq!(view.text_at(Time::Now).as_deref(), Some("revision-3"));
    assert_eq!(view.text_at(Time::At(v2)).as_deref(), Some("revision-2"));
    assert_eq!(view.text_at(Time::At(v1)).as_deref(), Some("revision-1"));
}

#[test]
fn plugin_inspects_history_metadata_via_app_view() {
    let mut state = fresh_state();

    let view = AppView::new(&state);
    let initial_current = view.history().current_version();
    let initial_earliest = view.history().earliest_version();
    assert_eq!(initial_current, initial_earliest);

    let _ = view;
    state.apply(draw(vec![vec![atom("a")]]));
    state.apply(draw(vec![vec![atom("b")]]));
    state.apply(draw(vec![vec![atom("c")]]));

    let view = AppView::new(&state);
    let after_current = view.history().current_version();
    let after_earliest = view.history().earliest_version();

    // After 3 commits the current version has advanced; earliest is
    // still the first committed version (no eviction at default
    // capacity).
    assert!(after_current > initial_current);
    assert_eq!(after_earliest.0, 0);
    assert_eq!(after_current.0, 2);
}

#[test]
fn plugin_can_iterate_versions_in_range() {
    let mut state = fresh_state();
    state.apply(draw(vec![vec![atom("v0")]]));
    state.apply(draw(vec![vec![atom("v1")]]));
    state.apply(draw(vec![vec![atom("v2")]]));

    let view = AppView::new(&state);
    let earliest = view.history().earliest_version();
    let current = view.history().current_version();

    // Plugin walks from earliest to current and pulls each text.
    let mut texts = Vec::new();
    let mut v = earliest;
    while v <= current {
        if let Some(t) = view.text_at(Time::At(v)) {
            texts.push((*t).to_string());
        }
        v.0 += 1;
    }

    assert_eq!(texts, vec!["v0", "v1", "v2"]);
}

#[test]
fn empty_state_returns_none_via_app_view() {
    let state = fresh_state();
    let view = AppView::new(&state);
    assert_eq!(view.text_at(Time::Now), None);
}
