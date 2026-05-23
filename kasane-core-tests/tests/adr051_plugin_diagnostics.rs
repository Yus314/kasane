//! ADR-051 chunk 3c (second source): plugin diagnostics through
//! [`ExternalInputRegistry`].
//!
//! Validates that the registry abstraction holds for a workload
//! materially different from the dense single-source buffer-lines path
//! it was first proven against:
//!
//! - **multi-source:** multiple plugins emit diagnostics in the same
//!   frame; the snapshot delivered to the slot is the union.
//! - **bursty:** a single frame can carry tens of diagnostics from a
//!   failure cascade; the slot's `Coalesce` policy holds the latest
//!   snapshot whole.
//! - **glitch-freedom:** mid-frame commits do not surface to readers
//!   until the frame-boundary drain.
//!
//! These cover DDD-CST vision §19 Decision Point 1' against a source
//! shape the §4.1.8 push/pull discipline had not yet been stressed
//! against.

use std::sync::Arc;
use std::sync::mpsc;
use std::thread;
use std::time::{Duration, Instant};

use kasane_core::plugin::{PluginDiagnostic, PluginDiagnosticTarget, PluginId};
use kasane_core::salsa_inputs::diagnostics::{PLUGIN_DIAGNOSTIC_SLOT, PluginDiagnosticBurst};
use kasane_core::salsa_inputs::external::{BackPressurePolicy, ExternalInputRegistry};

fn diag_for(plugin: &str, method: &str) -> PluginDiagnostic {
    PluginDiagnostic::runtime_error(PluginId::from(plugin), method, "boom")
}

fn register_slot(
    reg: &mut ExternalInputRegistry,
) -> kasane_core::salsa_inputs::external::ExternalInputId<PluginDiagnosticBurst> {
    reg.register::<PluginDiagnosticBurst>(PLUGIN_DIAGNOSTIC_SLOT, BackPressurePolicy::Coalesce)
}

#[test]
fn multi_source_burst_visible_atomically_post_drain() {
    // Six plugins each contribute one diagnostic to the per-frame
    // snapshot. The aggregator (mirrors `PluginRuntime::drain_all_diagnostics`)
    // unions them into a single Vec, which becomes one commit. After
    // drain, the snapshot is visible as a whole — no partial fan-in,
    // no per-plugin slot.
    let mut reg = ExternalInputRegistry::new();
    let slot = register_slot(&mut reg);

    let aggregated: Vec<PluginDiagnostic> = (0..6)
        .map(|i| diag_for(&format!("plugin-{i}"), "on_state_changed"))
        .collect();
    let burst: PluginDiagnosticBurst = aggregated.into();
    reg.commit(slot, burst);

    // Pre-drain: nothing observable. Plugin-side runtime cannot observe
    // diagnostics it has not yet finished emitting for the frame.
    assert!(reg.last(slot).is_none());

    reg.drain();
    let observed = reg.last(slot).expect("post-drain snapshot present");
    assert_eq!(observed.len(), 6);
    assert!(reg.is_dirty(slot));
}

#[test]
fn intra_frame_cascade_collapses_to_latest_snapshot() {
    // A failure cascade may commit several snapshots within a single
    // frame (e.g. one batch of lifecycle errors followed by another of
    // runtime errors). `Coalesce` preserves the latest whole snapshot;
    // the earlier ones are superseded. Consumers downstream of the
    // registry slot see one consistent value per frame.
    let mut reg = ExternalInputRegistry::new();
    let slot = register_slot(&mut reg);

    let early: PluginDiagnosticBurst = vec![diag_for("plugin-a", "on_init")].into();
    let later: PluginDiagnosticBurst = vec![
        diag_for("plugin-b", "on_state_changed"),
        diag_for("plugin-c", "on_state_changed"),
        diag_for("plugin-d", "on_state_changed"),
    ]
    .into();

    reg.commit(slot, early);
    reg.commit(slot, later);
    reg.drain();

    let observed = reg.last(slot).expect("snapshot");
    assert_eq!(observed.len(), 3);
    let ids: Vec<&str> = observed
        .iter()
        .map(|d| match &d.target {
            PluginDiagnosticTarget::Plugin(id) => id.as_str(),
            _ => "<provider>",
        })
        .collect();
    assert_eq!(ids, vec!["plugin-b", "plugin-c", "plugin-d"]);
}

#[test]
fn glitch_freedom_under_producer_burst() {
    // Vision §21.5 Q18: producer-side burst whose snapshots straddle a
    // frame boundary. The registry's "as-of-drain" guarantee means
    // observers never see a snapshot whose commit had not yet been
    // mirrored by the main thread before the drain that surfaced it.
    const FRAMES: u32 = 64;
    const PLUGINS_PER_BURST: u32 = 5;

    let (tx, rx) = mpsc::channel::<PluginDiagnosticBurst>();
    let producer = thread::spawn(move || {
        for frame in 0..FRAMES {
            let burst: PluginDiagnosticBurst = (0..PLUGINS_PER_BURST)
                .map(|i| diag_for(&format!("p-{i}"), &format!("frame-{frame}")))
                .collect::<Vec<_>>()
                .into();
            tx.send(burst).expect("producer send");
        }
    });

    let mut reg = ExternalInputRegistry::new();
    let slot = register_slot(&mut reg);

    let mut max_committed_frame: Option<u32> = None;
    let mut observed_frames: Vec<u32> = Vec::new();
    let deadline = Instant::now() + Duration::from_secs(5);

    loop {
        let mut committed_this_frame = 0;
        while let Ok(burst) = rx.try_recv() {
            // Extract the frame stamp from the first diagnostic's method
            // string — that's the producer-side identity we wire
            // backwards from the observed snapshot.
            let frame: u32 = burst[0]
                .kind_method_for_test()
                .strip_prefix("frame-")
                .and_then(|s| s.parse().ok())
                .expect("frame stamp parseable");
            reg.commit(slot, burst);
            max_committed_frame = Some(max_committed_frame.map_or(frame, |m| m.max(frame)));
            committed_this_frame += 1;
        }

        reg.drain();
        if let Some(observed) = reg.last(slot) {
            let frame: u32 = observed[0]
                .kind_method_for_test()
                .strip_prefix("frame-")
                .and_then(|s| s.parse().ok())
                .expect("frame stamp parseable");
            // The observed snapshot must correspond to a commit the
            // main thread already drained — never larger than the max
            // committed before this drain.
            assert!(
                Some(frame) <= max_committed_frame,
                "observed frame {frame} but max committed pre-drain was {max_committed_frame:?}"
            );
            observed_frames.push(frame);
        }
        reg.clear_dirty();

        if committed_this_frame == 0 && producer.is_finished() {
            break;
        }
        assert!(
            Instant::now() < deadline,
            "burst drain did not converge within timeout"
        );
    }

    producer.join().unwrap();

    let final_observed = reg.last(slot).expect("at least one snapshot survived");
    assert_eq!(final_observed.len() as u32, PLUGINS_PER_BURST);
    assert!(
        !observed_frames.is_empty(),
        "at least one frame must have surfaced a snapshot"
    );
    // Snapshot stream is monotonically non-decreasing in producer-frame
    // (Coalesce may skip frames, never go backwards).
    let mut prev = observed_frames[0];
    for &v in &observed_frames[1..] {
        assert!(v >= prev, "observed sequence regressed: {prev} -> {v}");
        prev = v;
    }
}

/// Test-only accessor: extract the method string from a
/// `RuntimeError` diagnostic. Mirrors what production code reads via
/// the kind enum directly; encapsulated here so the burst-frame
/// identity is the single concern of the test.
trait DiagMethodForTest {
    fn kind_method_for_test(&self) -> &str;
}

impl DiagMethodForTest for PluginDiagnostic {
    fn kind_method_for_test(&self) -> &str {
        match &self.kind {
            kasane_core::plugin::PluginDiagnosticKind::RuntimeError { method } => method.as_str(),
            _ => panic!("test diagnostic must be RuntimeError-kind"),
        }
    }
}

#[test]
fn arc_clones_share_underlying_storage() {
    // The commit path moves an Arc into the registry while tracing and
    // overlay consumers retain their own Arc to the same allocation.
    // Verify the strong count semantics so a future refactor that
    // breaks Arc-sharing (e.g. converting to Vec mid-pipeline) fails
    // this test rather than silently degrading hot-path perf.
    let mut reg = ExternalInputRegistry::new();
    let slot = register_slot(&mut reg);

    let burst: PluginDiagnosticBurst =
        vec![diag_for("plugin-x", "step"), diag_for("plugin-y", "step")].into();
    let shared_for_consumers = burst.clone();
    assert_eq!(Arc::strong_count(&burst), 2);

    reg.commit(slot, burst);
    // Registry holds one strong ref; consumer copy still alive.
    assert_eq!(Arc::strong_count(&shared_for_consumers), 2);

    reg.drain();
    let observed = reg.last(slot).expect("post-drain");
    // The slot's `last` returns a borrow; cloning produces another Arc
    // pointing at the same storage.
    let _ = observed.iter().count();
    assert_eq!(Arc::strong_count(&shared_for_consumers), 2);
}
