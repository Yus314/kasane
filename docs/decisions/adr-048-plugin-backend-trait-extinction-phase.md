# ADR-048: Plugin Backend Trait Extinction (Phase β)

**Status:** Proposed (draft, 2026-05-12). Refines and extends
ADR-046 W1-C. Targets the same wave-2 atomic PR but commits to a
more radical structural change.

### Context

ADR-046 W1-C proposed narrowing `PluginBackend` visibility to
`pub(crate)`. R2.x P6 attempted this and closed at
`#[doc(hidden)] pub` because five cross-crate consumers
(`kasane-wasm::adapter::WasmPlugin`, `kasane-tui::ReloadResourcePlugin`,
`kasane`'s four top-level builtins, `kasane::locked_wasm_provider`'s
`Box<dyn PluginBackend>` factory, the `#[kasane_plugin]` proc-macro
generated impls) all live outside `kasane-core`.

A deeper structural finding: `PluginBackend` is a **77-method trait**
whose only non-test implementer is `PluginBridge<P: Plugin>`, which
dispatches each method into the type-erased `HandlerTable`. The
`HandlerTable` already performs type erasure
(`Box<dyn Fn(&dyn PluginState, ...) -> Effects>` per handler kind).

This means `PluginBackend` is a **second erasure layer** on top of
`HandlerTable` — a 121-fn dispatch boilerplate in `bridge.rs`
(2037 LoC, 964 production) plus a 77-method trait with default
no-ops in `traits.rs` (846 LoC), totalling ≈1800 LoC of structurally
redundant code.

`PluginRuntime` (registry/mod.rs, 1037 LoC, 30+ dispatch methods)
is already the dispatch coordination layer. It iterates
`Vec<Box<dyn PluginBackend>>` and calls
`backend.on_state_changed_effects(...)` for each plugin. The
`PluginBackend` indirection collapses if `PluginRuntime` instead
holds `Vec<PluginEntry>` and dispatches directly via
`entry.table.on_state_changed.as_ref().map(|h| h(&entry.state, ...))`.

### Decision

**Delete the `PluginBackend` trait entirely.** Replace `Vec<Box<dyn PluginBackend>>`
in `PluginRuntime` with `Vec<PluginEntry>` where
`PluginEntry { id, state: Box<dyn PluginState>, table: HandlerTable, tag: PluginTag }`.
All dispatch happens via `PluginRuntime` methods that look up handlers
in `entry.table` directly.

### Migration shape

- `PluginEntry` (new): `kasane-core/src/plugin/registry/entry.rs`, ~100 LoC
- `PluginBridge<P>` (existing): demoted to a constructor —
  `PluginBridge::new<P: Plugin>(plugin: P) -> PluginEntry`. The
  `impl PluginBackend for PluginBridge` block (121 fn) deleted.
  Remaining: ~200 LoC factory.
- `traits.rs` (846 LoC): collapsed to ~150 LoC (only `Plugin`,
  `PluginState`, `KeyHandleResult`, `KeyPreDispatchResult` etc. return
  types remain; tier-narrowed `From` impls relocate to
  `effect_tiers.rs`).
- `bridge.rs` (2037 LoC, 1073 test): tests reduced to coverage of
  `PluginBridge::new` factory + ad hoc end-to-end dispatch tests
  (~300 LoC test). The `exhaustive_handler_dispatch_coverage` test
  is **deleted** — its purpose was to catch silent dispatch
  omissions in the 121-fn `PluginBridge` impl; with dispatch
  collapsed to `HandlerTable` field-presence checks, this failure
  mode is structurally eliminated.
- `PluginProvider::create()` return type: `Result<Box<dyn PluginBackend>>`
  → `Result<PluginEntry>`. Cross-crate consumers updated.
- `kasane-wasm::WasmPlugin`: rewritten to construct a `HandlerTable`
  whose erased handlers invoke the corresponding WIT exports, then
  `WasmPlugin::into_entry()` returns a `PluginEntry`. The
  `impl PluginBackend for WasmPlugin` block deleted.
- All `Plugin`-using test fixtures (≈50 sites in
  `plugin/tests/*.rs`, `event_loop/tests/*.rs`,
  `tests/plugin_integration.rs`,
  `tests/salsa_pipeline_comparison.rs`) migrated to
  `impl Plugin for X` form.

### Rationale

1. **Removes one structural layer**: the four-layer dispatch
   (event_loop → PluginRuntime → Box\<dyn PluginBackend\> →
   PluginBridge::method → HandlerTable lookup) collapses to two
   layers (event_loop → PluginRuntime → HandlerTable lookup).

2. **Eliminates the 7-site synchronisation cost** for new handler
   types: adding a new on_* extension point currently requires
   touching `HandlerRegistry::on_*`, `HandlerTable::*_handlers`,
   `PluginBridge::dispatch_*`, the `EXPECTED_HANDLER_NAMES` test,
   `PluginBackend` default impl in `traits.rs`, doc updates, and
   WIT export. Post-Phase β: only 4 sites (HandlerRegistry,
   HandlerTable, PluginRuntime dispatch method, WIT export).

3. **Inverts R2.x P6**: R2.x left `PluginBackend` at
   `#[doc(hidden)] pub` because narrowing to `pub(crate)` would
   require cross-crate migration. With backward-compatibility
   lifted, the migration is in scope; the radical answer (delete
   the trait, not narrow it) is structurally cleaner than the
   narrowing.

4. **Preserves ADR-044 tier hierarchy**: `effect_tiers.rs`
   (`KakouneSideCommand`, `KakouneSideEffects`, `ObservationEffects`,
   `ProcessCapableEffects`) is preserved. Tier-narrowed registration
   methods (`on_init_tier1<E: Into<KakouneSideEffects>>`) continue
   to enforce tier contracts at the boundary. Tier `From` impls
   relocate from `traits.rs` to `effect_tiers.rs` (their natural
   home).

### Implications

- **Net LoC delta**: production code ≈ -1900 (846 + 1800 - 150 - 200
  - 100 PluginEntry - 600 PluginRuntime expansion); test code ≈ -1000.
  Total ≈ -2900 LoC.
- **ABI 6.0.0 batched**: ships in the same atomic PR as ADR-046
  Wave 2 (WIT bump, all bundled .wasm rebuild).
- **`kasane-plugin-sdk` 0.8.0**: `PluginProvider::create()` return
  type changes; SDK plugin authors who implement the trait directly
  must adapt.
- **Documentation**: `docs/plugin-api.md` PluginBackend section
  deleted; `docs/plugin-development.md` PluginBackend examples
  rewritten as `impl Plugin`.
- **`MEMORY.md`**: R2.x program memo updated to reflect P6 outcome
  superseded by ADR-048 (deletion rather than narrowing).

### Risks

1. **`WasmPlugin::into_entry()` closure construction complexity.**
   The WIT-call-per-handler closure pattern must interop with
   wasmtime's `Store<T>` mutability requirements. Mitigation:
   PR β-prep spike validates this before committing to Phase β.

2. **`PluginRuntime` internal expansion**: the 30+ dispatch methods
   in `registry/mod.rs` absorb the dispatch logic from `bridge.rs`.
   File grows by ~600 LoC. Mitigation: split into
   `registry/dispatch_lifecycle.rs`, `registry/dispatch_input.rs`,
   etc. — mirroring the existing `handler_registry/*.rs` axis split.

3. **Test fixture migration (≈50 sites)**: Mostly mechanical
   `impl PluginBackend` → `impl Plugin`, but state-bearing fixtures
   (`StatefulPlugin`, `ShadowCursor`-adjacent tests) require manual
   verification of semantic equivalence. Mitigation: migrate one
   fixture file at a time, run targeted tests after each.

4. **Bench regression**: dispatch indirection changes from vtable
   lookup (≈2-3 inst) to field access (≈1 inst). Theoretically an
   improvement, but allocation patterns or borrow shapes could
   regress. Mitigation: PR β-prep measures iai_pipeline delta on a
   single-method conversion before full commitment.

### Alternatives considered

1. **Land ADR-046 W1-C as-written (`PluginBackend` → `pub(crate)`).**
   Rejected. R2.x P6 already attempted this and closed at
   `#[doc(hidden)] pub`. Repeating the attempt without inverting
   the cross-crate consumer pattern yields the same outcome. The
   radical answer (delete the trait) addresses the root cause.

2. **Refactor `PluginBridge` dispatch fn into macros (ADR-046 W1-E).**
   Partially done by R2.x P8 (`dispatch_state_with_default!`,
   `dispatch_inject_owner_contribution!`). Net result was +34 LoC
   (macro definitions outweighed call-site shrinkage). Continuing
   in this direction hits a structural ceiling: as long as
   `PluginBackend` has N methods, `PluginBridge` needs N dispatch
   sites, macroified or not. Mechanisation does not reduce the
   structural surface.

3. **Defer to ABI 7.0.0.** Rejected. ABI 6.0.0 is already a
   migration window; bundling Phase β with it amortises rebuild
   cost. Deferring forces a future ABI 7.0.0 bump solely for this
   refactor.

### Open questions

- **Q1**: Does `PluginEntry` need `Send + Sync` for the
  `PluginRuntime::plugins` `Vec<PluginEntry>` to support future
  parallel dispatch? Current `Box<dyn PluginBackend>` is `Send`
  only; raising to `Send + Sync` is feasible (HandlerTable
  closures already require `Send + Sync`) but defers parallel
  dispatch work.

- **Q2**: Should `PluginRuntime` split into
  `PluginCatalog` (resolution) + `PluginDispatcher` (runtime
  dispatch) at the same time? `manager.rs` (993 LoC,
  resolution) and `registry/mod.rs` (1037 LoC, dispatch) are
  already two files with two responsibilities. The rename
  (`PluginManager` → `PluginCatalog`, `PluginRuntime` →
  `PluginDispatcher`) is independently low-risk and could ship in
  Phase 4 polish.

---
