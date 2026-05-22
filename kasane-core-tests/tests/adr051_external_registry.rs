//! ADR-051 exit-criterion property tests.
//!
//! Covers the two FRP failure modes called out in vision §21.5 against
//! the `ExternalInputRegistry` integration shape:
//!
//! - Q18 / glitch-freedom: mid-frame commits never leak to readers
//!   before the frame-boundary drain.
//! - Q19 / bounded memory: the `Coalesce` policy holds at most one
//!   pending value regardless of how many commits arrive between
//!   drains.
//!
//! The registry itself is single-threaded; the tests exercise the
//! integration shape used by `kasane-syntax::SyntaxManager` —
//! mpsc-driven producer thread → main-thread `commit` + per-frame
//! `drain` / `clear_dirty` cycle.

use std::sync::mpsc;
use std::thread;
use std::time::{Duration, Instant};

use kasane_core::salsa_inputs::external::{BackPressurePolicy, ExternalInputRegistry};

const BURST_LEN: u32 = 5_000;

fn drive_frame<F: FnMut(&mut ExternalInputRegistry)>(
    reg: &mut ExternalInputRegistry,
    mut pre_render: F,
) {
    pre_render(reg);
    reg.drain();
    reg.clear_dirty();
}

#[test]
fn glitch_freedom_under_producer_burst() {
    // Producer thread blasts BURST_LEN distinct values through an mpsc
    // channel as quickly as it can. The main thread mirrors them into
    // the registry from a "pre_render" closure, then drains at the
    // frame boundary and reads `last`. The invariant: every observed
    // value must have been committed before the drain that surfaced it
    // — there is no mid-frame leak path.
    let (tx, rx) = mpsc::channel::<u32>();
    let producer = thread::spawn(move || {
        for v in 0..BURST_LEN {
            tx.send(v).expect("producer send");
        }
    });

    let mut reg = ExternalInputRegistry::new();
    let id = reg.register::<u32>("test", BackPressurePolicy::Coalesce);

    let mut max_committed: Option<u32> = None;
    let mut observed_history: Vec<u32> = Vec::new();
    let deadline = Instant::now() + Duration::from_secs(5);

    loop {
        let mut committed_this_frame = 0;
        drive_frame(&mut reg, |reg| {
            while let Ok(v) = rx.try_recv() {
                reg.commit(id, v);
                max_committed = Some(max_committed.map_or(v, |m| m.max(v)));
                committed_this_frame += 1;
            }
        });

        if let Some(&observed) = reg.last(id) {
            // Glitch-freedom: the observed value must be one that the
            // main thread already saw on the receive side. Anything
            // larger would mean the registry surfaced a value that
            // hadn't been committed yet.
            assert!(
                Some(observed) <= max_committed,
                "observed {observed} but max committed pre-drain was {max_committed:?}"
            );
            observed_history.push(observed);
        }

        if committed_this_frame == 0 && producer.is_finished() {
            break;
        }
        assert!(
            Instant::now() < deadline,
            "burst drain did not converge within timeout"
        );
    }

    producer.join().unwrap();

    assert_eq!(
        reg.last(id),
        Some(&(BURST_LEN - 1)),
        "final observed value must equal the producer's final commit"
    );
    assert!(
        !observed_history.is_empty(),
        "at least one frame must have surfaced a value"
    );
    // The observed sequence is monotonically non-decreasing (Coalesce
    // can skip values but cannot go backwards).
    let mut prev = observed_history[0];
    for &v in &observed_history[1..] {
        assert!(v >= prev, "observed sequence regressed: {prev} -> {v}");
        prev = v;
    }
}

#[test]
fn coalesce_pending_bounded_under_sustained_pushes() {
    // Vision §21.5 Q19: under `Coalesce`, the pending buffer is bounded
    // to one entry regardless of arrival rate. We verify both directions
    // — the invariant holds at every commit, and a final drain still
    // exposes only the most recent value.
    let mut reg = ExternalInputRegistry::new();
    let id = reg.register::<u32>("test", BackPressurePolicy::Coalesce);

    let frames = 16;
    let commits_per_frame = 1_024;

    for frame in 0..frames {
        for i in 0..commits_per_frame {
            let v = frame * commits_per_frame + i;
            reg.commit(id, v);
            assert!(
                reg.pending_len(id) <= 1,
                "pending unbounded: {} at frame={frame} i={i}",
                reg.pending_len(id)
            );
        }
        reg.drain();
        reg.clear_dirty();
        assert_eq!(reg.pending_len(id), 0, "drain must empty pending");
    }

    let final_v = frames * commits_per_frame - 1;
    assert_eq!(reg.last(id), Some(&final_v));
}

#[test]
fn is_dirty_observed_only_between_drain_and_clear() {
    // The contract used by `FrameSyncHook::post_sync` consumers: the
    // per-slot dirty flag is set during `drain` and lives until the
    // backend calls `clear_dirty`. Before `drain` and after
    // `clear_dirty`, the slot must look clean.
    let mut reg = ExternalInputRegistry::new();
    let id = reg.register::<u32>("test", BackPressurePolicy::Coalesce);

    assert!(!reg.is_dirty(id), "fresh registry must report clean");
    reg.commit(id, 7);
    assert!(
        !reg.is_dirty(id),
        "commit alone must not set the dirty flag"
    );

    reg.drain();
    assert!(
        reg.is_dirty(id),
        "drain after commit must set the dirty flag"
    );
    assert_eq!(reg.last(id), Some(&7));

    reg.clear_dirty();
    assert!(!reg.is_dirty(id), "clear_dirty must reset the dirty flag");

    // Idempotency: drain without a fresh commit must not re-set dirty.
    reg.drain();
    assert!(!reg.is_dirty(id));
}
