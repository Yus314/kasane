//! ADR-035 §2 round-trip integration test.
//!
//! Exercises the AppState ↔ HistoryBackend ↔ Time three-way wiring
//! end-to-end:
//!
//! 1. Construct an `AppState` (default: fresh `InMemoryRing`).
//! 2. Commit a sequence of snapshots via `commit_snapshot`.
//! 3. Read each snapshot back via `text_at(Time::At(v))`.
//! 4. Confirm `text_at(Time::Now)` returns the latest snapshot.
//! 5. Validate FIFO eviction by overflowing a small ring.

use std::sync::Arc;

use kasane_core::history::{InMemoryRing, Time, VersionId};
use kasane_core::state::AppState;
use kasane_core::state::selection::{BufferId, BufferVersion};
use kasane_core::state::selection_set::SelectionSet;

fn buf() -> BufferId {
    BufferId::new("history-roundtrip-test")
}

fn empty_sel(v: BufferVersion) -> SelectionSet {
    SelectionSet::empty(buf(), v)
}

#[test]
fn empty_state_returns_none_for_now() {
    let state = AppState::default();
    assert_eq!(state.text_at(Time::Now), None);
}

#[test]
fn empty_state_returns_none_for_at_zero() {
    let state = AppState::default();
    assert_eq!(state.text_at(Time::At(VersionId(0))), None);
}

#[test]
fn commit_then_query_at_returns_payload() {
    let state = AppState::default();
    let v = state.commit_snapshot(
        buf(),
        BufferVersion::INITIAL,
        Arc::from("hello"),
        empty_sel(BufferVersion::INITIAL),
    );
    let text = state.text_at(Time::At(v)).expect("snapshot exists");
    assert_eq!(&*text, "hello");
}

#[test]
fn commit_then_query_now_returns_latest() {
    let state = AppState::default();
    state.commit_snapshot(
        buf(),
        BufferVersion(0),
        Arc::from("first"),
        empty_sel(BufferVersion(0)),
    );
    state.commit_snapshot(
        buf(),
        BufferVersion(1),
        Arc::from("second"),
        empty_sel(BufferVersion(1)),
    );
    let v3 = state.commit_snapshot(
        buf(),
        BufferVersion(2),
        Arc::from("third"),
        empty_sel(BufferVersion(2)),
    );

    let now = state.text_at(Time::Now).expect("now exists");
    assert_eq!(&*now, "third");

    let at = state.text_at(Time::At(v3)).expect("at(v3) exists");
    assert_eq!(&*at, "third");
}

#[test]
fn three_snapshots_round_trip_independently() {
    let state = AppState::default();
    let v0 = state.commit_snapshot(
        buf(),
        BufferVersion(0),
        Arc::from("alpha"),
        empty_sel(BufferVersion(0)),
    );
    let v1 = state.commit_snapshot(
        buf(),
        BufferVersion(1),
        Arc::from("beta"),
        empty_sel(BufferVersion(1)),
    );
    let v2 = state.commit_snapshot(
        buf(),
        BufferVersion(2),
        Arc::from("gamma"),
        empty_sel(BufferVersion(2)),
    );

    assert_eq!(state.text_at(Time::At(v0)).as_deref(), Some("alpha"));
    assert_eq!(state.text_at(Time::At(v1)).as_deref(), Some("beta"));
    assert_eq!(state.text_at(Time::At(v2)).as_deref(), Some("gamma"));
}

#[test]
fn future_version_is_none() {
    let state = AppState::default();
    state.commit_snapshot(
        buf(),
        BufferVersion(0),
        Arc::from("x"),
        empty_sel(BufferVersion(0)),
    );
    assert_eq!(state.text_at(Time::At(VersionId(99))), None);
}

#[test]
fn cloned_state_shares_history() {
    // History is Arc-shared by design — clone-and-mutate of speculative
    // state copies still appends to the same history.
    let state = AppState::default();
    let cloned = state.clone();

    let v = cloned.commit_snapshot(
        buf(),
        BufferVersion(0),
        Arc::from("via-clone"),
        empty_sel(BufferVersion(0)),
    );

    assert_eq!(state.text_at(Time::At(v)).as_deref(), Some("via-clone"));
    assert_eq!(cloned.text_at(Time::At(v)).as_deref(), Some("via-clone"));
}

#[test]
fn small_ring_evicts_oldest_after_overflow() {
    // Construct an AppState with a small custom ring (replacing the
    // default 256-slot ring) so we can witness eviction.
    let mut state = AppState::default();
    state.history = Arc::new(InMemoryRing::with_capacity(2));

    let v0 = state.commit_snapshot(
        buf(),
        BufferVersion(0),
        Arc::from("a"),
        empty_sel(BufferVersion(0)),
    );
    let v1 = state.commit_snapshot(
        buf(),
        BufferVersion(1),
        Arc::from("b"),
        empty_sel(BufferVersion(1)),
    );
    let v2 = state.commit_snapshot(
        buf(),
        BufferVersion(2),
        Arc::from("c"),
        empty_sel(BufferVersion(2)),
    );

    // v0 evicted by FIFO.
    assert_eq!(state.text_at(Time::At(v0)), None);
    assert_eq!(state.text_at(Time::At(v1)).as_deref(), Some("b"));
    assert_eq!(state.text_at(Time::At(v2)).as_deref(), Some("c"));
    assert_eq!(state.text_at(Time::Now).as_deref(), Some("c"));
}

#[test]
fn debug_output_is_bounded() {
    // Confirm the manual Debug impl on InMemoryRing renders a compact
    // summary rather than dumping every snapshot. Sanity check only —
    // the precise format is not part of the contract.
    let state = AppState::default();
    for i in 0..50 {
        state.commit_snapshot(
            buf(),
            BufferVersion(i),
            Arc::from(format!("v{}", i)),
            empty_sel(BufferVersion(i)),
        );
    }
    let dbg = format!("{:?}", state.history);
    assert!(dbg.contains("InMemoryRing"));
    assert!(dbg.contains("len"));
    assert!(
        dbg.len() < 500,
        "Debug output too large: {} bytes",
        dbg.len()
    );
}
