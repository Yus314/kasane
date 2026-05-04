//! ADR-035 §2 — production-path integration test for the
//! `HistoryInput` Salsa sync.
//!
//! Validates that `sync_inputs_from_state` correctly threads
//! `AppState::history`'s ring + version into the `HistoryInput`
//! Salsa input, so the Time-aware Salsa queries
//! (`text_at_time`, `selection_at_time`,
//! `display_directives_at_time`) operate on the same backend the
//! `apply()` auto-commit hook writes to.

use std::sync::Arc;

use compact_str::CompactString;

use kasane_core::history::{HistoryBackend, Time};
use kasane_core::protocol::{Atom, Coord, KakouneRequest, UnresolvedStyle};
use kasane_core::salsa_db::KasaneDatabase;
use kasane_core::salsa_queries::{display_directives_at_time, selection_at_time, text_at_time};
use kasane_core::salsa_sync::{SalsaInputHandles, sync_inputs_from_state};
use kasane_core::state::AppState;

fn atom(s: &str) -> Atom {
    Atom::with_style(
        CompactString::from(s),
        kasane_core::protocol::Style::default(),
    )
}

fn fresh_state() -> AppState {
    let mut state = AppState::default();
    state.runtime.rows = 5;
    state.runtime.cols = 80;
    state
}

fn draw(lines: Vec<Vec<Atom>>) -> KakouneRequest {
    KakouneRequest::Draw {
        lines,
        cursor_pos: Coord { line: 0, column: 0 },
        default_style: Arc::new(UnresolvedStyle::default()),
        padding_style: Arc::new(UnresolvedStyle::default()),
        widget_columns: 0,
    }
}

#[test]
fn sync_threads_history_backend_into_salsa() {
    let mut db = KasaneDatabase::default();
    let inputs = SalsaInputHandles::new(&mut db);

    let mut state = fresh_state();
    state.apply(draw(vec![vec![atom("hello world")]]));

    sync_inputs_from_state(&mut db, &state, &inputs);

    // After sync, text_at_time(Time::Now) reads from the SAME ring
    // that AppState::apply just committed to.
    let text = text_at_time(&db, inputs.buffer, inputs.history, Time::Now)
        .expect("Time::Now after apply + sync");
    assert_eq!(&*text, "hello world");

    // And Time::At(current_version) returns the same payload.
    let current_v = state.history.current_version();
    let past_text = text_at_time(&db, inputs.buffer, inputs.history, Time::At(current_v))
        .expect("Time::At current");
    assert_eq!(&*past_text, "hello world");
}

#[test]
fn sync_invalidates_time_now_when_version_advances() {
    let mut db = KasaneDatabase::default();
    let inputs = SalsaInputHandles::new(&mut db);

    let mut state = fresh_state();

    // First commit through apply.
    state.apply(draw(vec![vec![atom("first")]]));
    sync_inputs_from_state(&mut db, &state, &inputs);
    let v_first = state.history.current_version();

    let s_first = selection_at_time(&db, inputs.history, Time::Now);
    assert!(s_first.is_some(), "Time::Now after first sync");

    // Second commit + resync. selection_at_time(Time::Now) should
    // now reflect the new version.
    state.apply(draw(vec![vec![atom("second")]]));
    sync_inputs_from_state(&mut db, &state, &inputs);
    let v_second = state.history.current_version();
    assert_ne!(v_first, v_second);

    // Past version still resolvable.
    let past = selection_at_time(&db, inputs.history, Time::At(v_first));
    assert!(past.is_some(), "Time::At(past) still resolves");
}

#[test]
fn display_directives_at_time_works_on_synced_state() {
    let mut db = KasaneDatabase::default();
    let inputs = SalsaInputHandles::new(&mut db);

    let mut state = fresh_state();
    state.apply(draw(vec![vec![atom("plain")]]));
    sync_inputs_from_state(&mut db, &state, &inputs);

    // With default-style atoms, the heuristic detects no selection,
    // so the synthesised display has zero leaves.
    let nd = display_directives_at_time(&db, inputs.history, Time::Now);
    assert!(nd.leaves.is_empty());
}

#[test]
fn sync_keeps_history_backend_arc_shared_with_appstate() {
    // The Arc threaded through the Salsa input must point to the
    // same allocation as AppState::history — otherwise commits made
    // through state.history wouldn't be visible to Salsa queries.
    let mut db = KasaneDatabase::default();
    let inputs = SalsaInputHandles::new(&mut db);
    let state = fresh_state();
    sync_inputs_from_state(&mut db, &state, &inputs);

    let salsa_ring = inputs.history.backend(&db);
    assert!(
        Arc::ptr_eq(&salsa_ring, &state.history),
        "Salsa HistoryInput's backend must be the same Arc as AppState::history",
    );
}
