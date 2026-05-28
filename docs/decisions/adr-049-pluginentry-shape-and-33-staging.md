# ADR-049: `PluginEntry` Shape and β-3.3 Staging

**Status**: Proposed (2026-05-12)

### Context

[ADR-048](./adr-048-plugin-backend-trait-extinction-phase.md) committed
to deleting the `PluginBackend` trait. Phases β-1 through β-3.2 landed
the prerequisites: every native plugin lives in a `PluginBridge`,
every test fixture uses `impl Plugin`, the deprecated lifecycle
setters and the legacy `#[kasane_plugin]` macro mode are gone, and
WIT ABI 6.0.0 has shipped.

Three `impl PluginBackend for X` sites remain:

1. **`PluginBridge`** — the production translation layer that
   converts `Plugin::register()` output (HandlerTable + state) into
   the trait's vtable shape. The trait it implements is precisely the
   shape we are about to delete.
2. **`WasmPlugin`** — produces trait-method implementations by
   calling into the wasmtime store. Each PluginBackend method body is
   a WIT export invocation. ~75 method implementations total.
3. **`WidgetBackend`** — a `#[cfg(test)]`-only legacy fixture in
   `widget/backend.rs` (250 LoC) that the new `WidgetPlugin`
   (`widget/plugin.rs`) supersedes. 22 tests still call its
   PluginBackend methods directly.

The remaining work to delete the trait is not "delete the trait" — it
is *changing PluginRuntime's storage shape* so the trait is no longer
the interface plugins satisfy. ADR-048 named this shape "PluginEntry"
but did not define it.

### Decision

The replacement storage shape is **`PluginBridge` itself, with its
`impl PluginBackend` extracted as inherent methods**. There is no new
`PluginEntry` type; `PluginBridge` is renamed-in-place by removing
the trait it satisfies.

```rust
// Before β-3.3
pub(crate) enum SlotImpl {
    Native(Box<PluginBridge>),
    External(Box<dyn PluginBackend>),
}

impl PluginBackend for PluginBridge { /* ~30 methods */ }

// After β-3.3
pub(crate) struct PluginSlot {
    pub(crate) bridge: PluginBridge, // unboxed, no Option, no enum
    /* existing fields: capabilities, authorities, last_state_hash, ... */
}

impl PluginBridge {
    pub fn on_init_effects(&mut self, app: &AppView<'_>) -> Effects { /* same body */ }
    pub fn contribute_to(/*…*/) -> Option<Contribution> { /* same body */ }
    /* ~30 inherent methods */
}
```

Justification: the *behaviour* on the trait is already concentrated
in `PluginBridge` (β-1 introduced `SlotImpl` dual-storage; β-1.6 made
every per-frame dispatcher prefer the native path). The trait is the
last residue of "WasmPlugin and WidgetBackend pretend to be plugin
bridges". Once those two collapse — WasmPlugin into a function that
populates a HandlerTable, WidgetBackend into deletion — the trait has
no remaining consumers.

#### How `WasmPlugin` produces a `PluginBridge`

`WasmPluginLoader::load` already constructs the runtime state and
returns a `WasmPlugin`. The β-3.3 rewrite shifts the return type to
`PluginBridge`, with each WIT export call wrapped in a HandlerTable
closure that captures `Arc<WasmPluginShared>` by clone:

```rust
// Sketch — final spelling TBD
impl WasmPluginLoader {
    pub fn load(&self, bytes: &[u8], caps: &WasiCapabilityConfig)
        -> Result<PluginBridge, (Vec<PluginDiagnostic>, anyhow::Error)>
    {
        let shared: Arc<WasmPluginShared> = /* build wasmtime store + instance */;
        let mut registry = HandlerRegistry::<WasmPluginState>::new();

        let s = Arc::clone(&shared);
        registry.on_init_tier1(move |state, app| {
            let effects = s.call_synced_with_hash(app, "on_init_effects", |rt| {
                let api = rt.instance.kasane_plugin_plugin_api();
                Ok(convert::wit_bootstrap_effects_to_effects(
                    &api.call_on_init_effects(&mut rt.store)?,
                ))
            });
            (state.clone(), effects.into_kakoune_side())
        });
        // ... ~40 more handlers wired the same way

        Ok(PluginBridge::from_registry(registry, shared.plugin_id.clone()))
    }
}
```

`WasmPluginState` (the State associated type) wraps the existing
`Arc<WasmPluginShared>` plus any per-frame mirror state the bridge
needs. State changes propagate through the bridge's `generation`
counter that already fuels `prepare_plugin_cache`'s staleness
detection.

`set_plugin_tag` / `drain_diagnostics` / `surfaces` / etc. that are
currently PluginBackend trait methods on WasmPlugin become either:

- **HandlerTable handlers** (`declare_surfaces`, `on_init_tier1`, …)
  when they map cleanly to existing Plugin trait APIs
- **PluginBridge inherent methods** that consult bridge state
  (`set_plugin_tag` writes `self.plugin_tag`; `drain_diagnostics`
  reads `self.pending_diagnostics`)

WasmPlugin-specific concerns that have no Plugin-trait shape today —
the `WasmPluginShared::with_runtime`-style synchronised access — live
inside the closures, not on the bridge's surface.

#### Why no separate `PluginEntry` type

Three alternatives considered:

1. **New struct `PluginEntry { state, table, ... }`** — duplicates
   PluginBridge's 10 fields. Migration shuttles values from
   `PluginBridge` to `PluginEntry` and renames every call site. No
   semantic gain.

2. **Trait `PluginEntry` (the prior `Plugin` trait renamed)** —
   keeps a trait around, just under a new name. Doesn't simplify
   dispatch.

3. **Make `PluginBridge` the entry, remove its trait impl** — this
   ADR's choice. Inherent methods are static-dispatched, the
   `SlotImpl` enum collapses to a single struct, and the rename can
   be deferred indefinitely.

The `PluginBridge` name itself becomes a misnomer once nothing
bridges to a trait. A rename to `PluginEntry` or `LoadedPlugin` is a
separate mechanical PR that touches ~141 references and adds no
behaviour; defer to Phase γ polish.

### Consequences

- **Trait deletion is final**. No `#[doc(hidden)]` retention path;
  the trait disappears from `kasane-core/src/plugin/traits.rs`.
- **`SlotImpl::as_native_mut` / `::as_native` accessors removed**.
  The Phase β-1 / β-1.6 fast-path is structurally subsumed: every
  dispatch is now a direct field-and-method call on `PluginBridge`,
  no vtable, no enum match.
- **Type-erasure boundary collapses**. The `PluginBackend: Any`
  super-bound and the `Box<dyn Any>` upcast in `box_to_slot_impl`
  (registry/mod.rs) both go away.

### Staging

β-3.3 is sub-divided so each commit lands a verifiable slice:

| Sub-phase | Work | Estimated impact |
|---|---|---|
| **β-3.3a** | Delete `WidgetBackend` (`widget/backend.rs`); rewrite the 22 tests in `widget/tests.rs` to drive `WidgetPlugin` via `PluginBridge`. After this, the only `impl PluginBackend` outside `bridge.rs` is in `kasane-wasm`. | -250 LoC widget/backend.rs, +~150 LoC test rewrites; -1 trait consumer |
| **β-3.3b** | Migrate `WasmPlugin` to produce a `PluginBridge` from `WasmPluginLoader::load`. ~75 WIT-export wirings + WasmPlugin state representation. | Largest single change in β-3.3 |
| **β-3.3c** | Convert `impl PluginBackend for PluginBridge` to inherent methods; collapse `SlotImpl` enum to a single `PluginBridge` field on `PluginSlot`. Remove `as_native()` / `as_native_mut()` accessors. | ~30 method moves, dispatch sites prune |
| **β-3.3d** | Delete `PluginBackend` trait from `traits.rs`. Remove `box_to_slot_impl` and the `Any` upcast plumbing. Update `plugin_prelude.rs`. | -846 LoC traits.rs |
| **β-3.3e** | bridge.rs cleanup: collapse from ~1100 LoC to the inherent-method core (~200 LoC). Remove now-unused `IsBridgedPlugin` trait and the `is_bridge()` discriminator. | -900 LoC |

Total target: **~-2000 LoC**. β-3.3a is contained and lands first;
β-3.3b is the long pole and may itself ship in multiple commits per
handler-family.

### Rejected alternatives

1. **Keep the trait `#[doc(hidden)]` indefinitely.** Rejected because
   it leaves the dual-storage `SlotImpl` enum permanent and forfeits
   the LoC-reduction goal that motivated ADR-048.

2. **Define `PluginEntry` as a separate type.** Discussed above —
   duplicates `PluginBridge`'s fields with no semantic gain.

3. **Land β-3.3 as one atomic commit.** Rejected. Each sub-phase is
   verifiable on its own; a single ~2000 LoC commit is unreviewable.

### Open questions

- **Q1**: Should the WasmPlugin-side `Arc<WasmPluginShared>` capture
  inside HandlerTable closures be replaced by a typed
  `WasmPluginState` carried through the Plugin trait's `State`
  associated type? Default: clone the `Arc` into each closure
  (matches the pattern already used in `adapter.rs`).

- **Q2**: After β-3.3, does `PluginBridge` itself need renaming to
  `PluginEntry` / `LoadedPlugin`? The 141-site rename adds no
  behaviour. Defer to Phase γ polish.

- **Q3**: Does `IsBridgedPlugin` (`bridge.rs:960`) survive? Likely
  no — once `is_bridge()` is gone, its only consumer dies.

---

## Related Documents

- [semantics.md](../semantics.md) — Authoritative specification
- [index.md](../index.md) — Documentation entry point and architecture overview
