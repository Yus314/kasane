//! Plugin diagnostics as an [`ExternalInputRegistry`] source.
//!
//! Each frame, after the plugin runtime drains pending diagnostics, the
//! per-frame snapshot is committed under [`PLUGIN_DIAGNOSTIC_SLOT`].
//! This routes a sparse / bursty / multi-source workload (multiple
//! plugins surfacing lifecycle and runtime diagnostics in the same
//! frame) through the §4.1.8 push-to-set / pull-to-derive discipline,
//! validating that the abstraction holds for a source whose event shape
//! differs from the dense single-source buffer-lines path.
//!
//! The snapshot is shared by reference-counted slice so commit-time
//! cloning between the existing tracing / overlay consumers and the
//! registry slot stays O(1) regardless of burst size.
//!
//! [`ExternalInputRegistry`]: super::external::ExternalInputRegistry

use std::sync::Arc;

use crate::plugin::PluginDiagnostic;

/// Per-frame plugin diagnostic snapshot committed to the
/// `plugin.diagnostics` registry slot.
///
/// `Arc<[T]>` rather than `Vec<T>`: the same value backs the registry,
/// the tracing reporter, and the overlay scheduler in the same frame;
/// the refcount avoids a deep copy on each consumer.
pub type PluginDiagnosticBurst = Arc<[PluginDiagnostic]>;

/// Slot name used at registration. Diagnostic-only; the registry uses
/// numeric ids for lookup.
pub const PLUGIN_DIAGNOSTIC_SLOT: &str = "plugin.diagnostics";

#[cfg(test)]
mod tests {
    use super::*;
    use crate::plugin::{PluginDiagnostic, PluginId};
    use crate::salsa_inputs::external::{BackPressurePolicy, ExternalInputRegistry};

    fn diag(id: &str) -> PluginDiagnostic {
        PluginDiagnostic::runtime_error(PluginId::from(id), "step", "boom")
    }

    #[test]
    fn burst_round_trips_through_registry() {
        let mut reg = ExternalInputRegistry::new();
        let slot = reg.register::<PluginDiagnosticBurst>(
            PLUGIN_DIAGNOSTIC_SLOT,
            BackPressurePolicy::Coalesce,
        );

        let burst: PluginDiagnosticBurst = vec![diag("alpha"), diag("beta")].into();
        reg.commit(slot, burst.clone());

        // Glitch-freedom: pre-drain reads see no value.
        assert!(reg.last(slot).is_none());

        reg.drain();
        let observed = reg
            .last(slot)
            .expect("committed burst observable post-drain");
        assert_eq!(observed.len(), 2);
        assert!(reg.is_dirty(slot));

        reg.clear_dirty();
        assert!(!reg.is_dirty(slot));
    }

    #[test]
    fn empty_burst_can_be_committed_and_observed() {
        // A frame with no diagnostics may still want to publish an empty
        // snapshot to advance a "diagnostics-quiet" signal. Verify the
        // empty case round-trips without special-casing at the call
        // site.
        let mut reg = ExternalInputRegistry::new();
        let slot = reg.register::<PluginDiagnosticBurst>(
            PLUGIN_DIAGNOSTIC_SLOT,
            BackPressurePolicy::Coalesce,
        );

        let empty: PluginDiagnosticBurst = Vec::<PluginDiagnostic>::new().into();
        reg.commit(slot, empty);
        reg.drain();

        let observed = reg.last(slot).expect("empty burst still observable");
        assert!(observed.is_empty());
    }

    #[test]
    fn coalesce_keeps_last_burst_under_intra_frame_bursts() {
        // A producer that publishes multiple snapshots before the frame
        // drain (e.g. mid-frame plugin failure cascade emitting two
        // separate batches) sees only the most recent at the consumer.
        // This matches the §4.1.8 "as-of-drain" semantics for sources
        // with snapshot-shaped values.
        let mut reg = ExternalInputRegistry::new();
        let slot = reg.register::<PluginDiagnosticBurst>(
            PLUGIN_DIAGNOSTIC_SLOT,
            BackPressurePolicy::Coalesce,
        );

        let first: PluginDiagnosticBurst = vec![diag("first")].into();
        let second: PluginDiagnosticBurst = vec![diag("second-a"), diag("second-b")].into();
        reg.commit(slot, first);
        reg.commit(slot, second);
        reg.drain();

        let observed = reg.last(slot).unwrap();
        assert_eq!(observed.len(), 2);
        assert!(matches!(
            observed[0].target,
            crate::plugin::PluginDiagnosticTarget::Plugin(ref id) if id.as_str() == "second-a"
        ));
    }
}
