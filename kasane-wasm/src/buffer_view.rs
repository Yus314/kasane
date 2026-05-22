//! Host-side `buffer-view` capability resource (ADR-052 chunk 1).
//!
//! The `buffer-view` resource is the first WIT capability resource in
//! Kasane. A plugin acquires a handle via `open-buffer-view` and reads
//! buffer lines through methods on the handle; the WIT resource type
//! is the unforgeability guarantee (no Wasm-side construction path).
//!
//! Chunk 1 ships the resource shape only — there is no broker yet, no
//! attenuation, and no scope tracking. Every `open-buffer-view` call
//! succeeds when a buffer is focused and yields a `BufferViewRep` that
//! delegates to the same line data as `host-state::get-lines-text`.
//! ADR-052 chunks 2–3 add the `CapabilityBroker` and manifest gating;
//! chunk 4 wires `define_plugin!` to declare the capability.

use wasmtime::component::Resource;

use crate::bindings;
use crate::host::HostState;

/// Per-handle host-side state for a `buffer-view` capability.
///
/// Chunk 1 carries no fields — the handle identity *is* the
/// authority, and the line-text method reads through to the host's
/// shared `HostState::lines` snapshot. Later chunks attach a scope
/// (e.g. line range from APL attenuation per ADR-056) here.
pub struct BufferViewRep;

impl bindings::kasane::plugin::host_capabilities::HostBufferView for HostState {
    fn get_lines_text(
        &mut self,
        self_: Resource<BufferViewRep>,
        start: u32,
        end: u32,
    ) -> Vec<String> {
        if self.table.get(&self_).is_err() {
            return Vec::new();
        }
        let len = self.lines.len();
        let s = (start as usize).min(len);
        let e = (end as usize).min(len);
        if s >= e {
            return Vec::new();
        }
        self.lines[s..e]
            .iter()
            .map(|line| line.iter().map(|atom| atom.contents.as_str()).collect())
            .collect()
    }

    fn drop(&mut self, rep: Resource<BufferViewRep>) -> wasmtime::Result<()> {
        self.table.delete(rep)?;
        Ok(())
    }
}

impl bindings::kasane::plugin::host_capabilities::Host for HostState {
    fn open_buffer_view(
        &mut self,
    ) -> Result<Resource<BufferViewRep>, bindings::kasane::plugin::host_capabilities::OpenError>
    {
        // ADR-052 chunk 3: manifest-declared bound enforced here, once
        // per acquisition. The hot-path method calls bypass the broker
        // — they pay only the resource-table lookup cost.
        if !self.capability_broker.allows_service("buffer") {
            return Err(bindings::kasane::plugin::host_capabilities::OpenError::Denied);
        }
        // `open-error::unavailable` is reserved for host-side failures
        // beyond authority (e.g. resource-table exhaustion). Focus
        // state is intentionally not gated here — the underlying
        // `lines` snapshot is what `host-state::get-lines-text` reads
        // through, and that accessor does not require focus either.
        self.table
            .push(BufferViewRep)
            .map_err(|_| bindings::kasane::plugin::host_capabilities::OpenError::Unavailable)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::bindings::kasane::plugin::host_capabilities::{Host, HostBufferView, OpenError};
    use crate::broker::CapabilityBroker;
    use kasane_core::protocol::Atom as ProtocolAtom;
    use kasane_plugin_package::manifest::{
        CapabilitiesSection, PluginManifest, PluginSection, ServiceDeclaration,
    };

    fn line(text: &str) -> Vec<ProtocolAtom> {
        vec![ProtocolAtom::with_style(
            text,
            kasane_core::protocol::Style::default(),
        )]
    }

    fn manifest_declaring(services: &[&str]) -> PluginManifest {
        PluginManifest {
            manifest_version: None,
            plugin: PluginSection {
                id: "test".into(),
                abi_version: "6.5.0".into(),
            },
            capabilities: CapabilitiesSection {
                wasi: Vec::new(),
                env_vars: Vec::new(),
                services: services
                    .iter()
                    .map(|n| ServiceDeclaration { name: (*n).into() })
                    .collect(),
            },
            authorities: Default::default(),
            handlers: Default::default(),
            view: Default::default(),
            settings: Default::default(),
        }
    }

    fn host_with_buffer_capability() -> HostState {
        let mut host = HostState::default();
        host.capability_broker = CapabilityBroker::from_manifest(&manifest_declaring(&["buffer"]));
        host
    }

    #[test]
    fn open_buffer_view_yields_handle_when_declared() {
        let mut host = host_with_buffer_capability();
        let r = host.open_buffer_view();
        assert!(r.is_ok());
    }

    #[test]
    fn open_buffer_view_denied_without_manifest_declaration() {
        let mut host = HostState::default();
        match host.open_buffer_view() {
            Err(OpenError::Denied) => {}
            other => panic!("expected Denied, got {other:?}"),
        }
    }

    #[test]
    fn get_lines_text_through_handle_reads_lines() {
        let mut host = host_with_buffer_capability();
        host.lines = std::sync::Arc::new(vec![line("alpha"), line("beta"), line("gamma")]);
        let handle = host.open_buffer_view().expect("open should succeed");
        let got = host.get_lines_text(handle, 0, 3);
        assert_eq!(got, vec!["alpha", "beta", "gamma"]);
    }

    #[test]
    fn drop_revokes_handle_and_subsequent_calls_return_empty() {
        let mut host = host_with_buffer_capability();
        host.lines = std::sync::Arc::new(vec![line("alpha")]);
        let handle = host.open_buffer_view().expect("open should succeed");
        // Clone the resource handle so we can call drop and still attempt a
        // post-drop access (the rep id is the same, but the table entry is
        // gone — subsequent lookups must fail closed).
        let handle_id = handle.rep();
        host.drop(handle).expect("drop should succeed");
        let stale: Resource<BufferViewRep> = Resource::new_own(handle_id);
        let got = host.get_lines_text(stale, 0, 1);
        assert!(got.is_empty(), "post-drop handle must return empty");
    }

    // -- ADR-052 §Exit criterion: unforgeability property tests --
    //
    // The unforgeability claim has two layers:
    //
    //   1. *Type-level*: the bindgen-generated `BufferView` is an empty
    //      Rust enum (`pub enum BufferView {}`); no guest-side
    //      construction path exists. A property test cannot exercise
    //      "the guest forges one" because the enum has no inhabitants.
    //
    //   2. *Runtime-level*: even if a guest were able to lower an
    //      arbitrary u32 to `Resource<BufferView>` (which it cannot
    //      under wit-bindgen), the host-side methods must fail closed
    //      because the resource id has no entry in `HostState::table`.
    //      That's what the property test below exercises: an unbounded
    //      sweep over forged rep ids, asserting `get-lines-text`
    //      returns empty rather than reading buffer state.
    //
    // ADR-056 (APL attenuation) end-to-end tests are deferred: APL
    // predicates are a separate ADR and require additional WIT
    // surface (`buffer-view.attenuate(predicate)`). The intended shape
    // is `buf.attenuate(line_range(0, 100))` returning a handle whose
    // `get-lines-text` reads outside [0, 100) return empty. See
    // `docs/decisions/adr-056-attenuation-predicate-language.md`.

    proptest::proptest! {
        #[test]
        fn forged_handle_ids_never_read_buffer(
            rep_id in proptest::num::u32::ANY,
            start in 0u32..1024,
            end in 0u32..1024,
        ) {
            let mut host = host_with_buffer_capability();
            host.lines = std::sync::Arc::new(vec![
                line("alpha"), line("beta"), line("gamma"),
            ]);
            // A forged Resource<BufferViewRep> that was never returned
            // by `open-buffer-view`: the rep id is arbitrary, so it has
            // no table entry. `get-lines-text` must return empty
            // regardless of `(start, end)` — never the underlying
            // `lines` snapshot.
            let forged: Resource<BufferViewRep> = Resource::new_own(rep_id);
            let got = host.get_lines_text(forged, start, end);
            proptest::prop_assert!(
                got.is_empty(),
                "forged handle id={rep_id} produced non-empty read: {got:?}"
            );
        }

        #[test]
        fn undeclared_broker_denies_every_open(
            // No matter how many open attempts the plugin makes, an
            // empty broker (no `[[capabilities.services]]` in the
            // manifest) must `Denied` each one. The host never
            // allocates a handle.
            attempts in 1usize..32,
        ) {
            let mut host = HostState::default();
            for _ in 0..attempts {
                match host.open_buffer_view() {
                    Err(OpenError::Denied) => {}
                    other => proptest::prop_assert!(
                        false,
                        "expected Denied on every attempt, got {other:?}"
                    ),
                }
            }
        }
    }
}
