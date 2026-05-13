# ADR-038: Plugin Authoring Path Consolidation

**Status**: Superseded by [ADR-039](./adr-039-plugin-path-consolidation-r2x.md) (2026-05-08).
Original status: Current (2026-05-05).

### Reconsidered context (2026-05-08)

A workspace-wide grep for narrow capability-trait consumers
(`&dyn Lifecycle | &dyn InputHandler | &dyn Contributor |
&dyn Transformer | &dyn Annotator | &dyn DisplayTransform |
&dyn Renderer | &dyn WorkspaceMember | &dyn PluginMeta |
&dyn PubSubMember | &dyn ExtensionParticipant | &dyn Io`)
returns **zero hits** outside `capability_traits.rs:108`
(itself a doc comment justifying the file).

The R1.x super-trait migration's stated value — "call sites that
only need one capability can take `&dyn CapTrait` and benefit
from a narrower trait surface" — is empirically unrealised.
`capability_traits.rs` is 1040 lines of unused infrastructure;
`impl_migrated_caps_default!` adds 21 macro sites without a
corresponding consumer benefit.

ADR-038's decision points 1 ("R1.7+ frozen, capability_traits.rs
stays as-is") and 3 ("opportunistic builtin migration") rest on
this unverified premise. ADR-039 supersedes ADR-038 with the
inverse decision: complete the consolidation by deleting
capability_traits.rs, force-migrating all 9 builtin plugins to
`Plugin + HandlerRegistry`, and making `PluginBackend` an
internal `pub(crate)` ABI consumed only by the WASM adapter.

The original context section below is preserved for audit trail.

### Context (original, 2026-05-05)

The plugin system maintains two parallel authoring paths:

1. **`Plugin` + `HandlerRegistry`** (ADR-025): authors implement a
   2-method `Plugin` trait with an associated `State` type and
   register handlers declaratively. `PluginCapabilities` are
   auto-inferred. New extension points are additive — adding a
   `r.on_X(...)` registration method does not perturb existing impl
   sites.
2. **`PluginBackend` god trait** (legacy, `kasane-core/src/plugin/traits.rs`):
   a 73-method trait with no-op defaults, implemented directly by
   the 8 built-in plugins, the WASM adapter (`WasmPlugin`), and
   ~30 test doubles.

The dispatch surface is already effectively split. `HandlerTable`
(`handler_table.rs`) carries 41 erased-handler slots and 10
vec-based handler slots — a near-1:1 mirror of `PluginBackend`'s
methods. `PluginBridge` (`bridge.rs`, ~1900 lines) re-folds the
table into `impl PluginBackend for PluginBridge` so the runtime
can treat HandlerRegistry users uniformly. The
`PluginCapabilities` bitflag cached on each `PluginSlot`
(`registry/mod.rs:63`) already gates dispatch by capability,
achieving most of "narrow-dispatch" without a trait-level split.

Two structural debts have accrued on the legacy path:

- New ADR-035 capability (`intercept_buffer_edit`), Phase 10
  `paint_inline_box`, Composable Lenses `register_lenses`, and the
  key-map trio (`compiled_key_map` / `invoke_action` /
  `refresh_key_groups`) were added directly to `PluginBackend`
  rather than as capability traits. The legacy surface keeps
  growing.
- An R1.x workstream (R1.1–R1.6 landed) splitting `PluginBackend`
  into 11 capability traits (`capability_traits.rs`) has no
  documented destination. `PluginRuntime` owns plugins as
  `Box<dyn PluginBackend>` (`registry/mod.rs:61`); without an
  owner-side decision, R1.7+ would resolve to either an umbrella
  trait (a god-trait rename) or per-capability ownership (forcing
  `Rc<RefCell<...>>` on every plugin). R1.4–R1.6 covered the three
  smallest traits (PubSub / Extension / Io, 2–3 methods each); the
  remaining six are 5–12 methods and would require macro-driven
  opt-in across 24+ impl sites without a clear payoff.

### Decision

`Plugin` + `HandlerRegistry` is the sole authoring path for
native plugins. `PluginBackend` is the internal dispatch ABI
consumed by `PluginRuntime` and the WASM adapter; it is not an
authoring surface.

1. **R1.7+ frozen.** R1.4–R1.6 stay landed. `capability_traits.rs`
   stays as-is — the blanket impls and opt-in macros are valuable
   for narrow trait views and for the three migrated traits. No
   further capability-trait migration is planned.

2. **No new `PluginBackend` methods.** New extension points are
   introduced as `HandlerRegistry::on_X(...)` registration methods
   plus the corresponding `Erased*Handler` in `HandlerTable` and
   dispatch in `PluginBridge`. The exception is a method that must
   operate on the owned trait object inside `PluginRuntime`'s
   dispatch loop (not on individual handlers); in that case the
   PR adds the HandlerRegistry registration in the same commit.

3. **Built-in migration is steady-state backlog, not a coordinated
   workstream.** The 8 built-ins
   (`BuiltinDragPlugin` / `BuiltinFoldPlugin` /
   `BuiltinMouseFallbackPlugin` / `BuiltinInputPlugin` /
   `BuiltinShadowCursorPlugin` / `BuiltinMenuPlugin` /
   `BuiltinInfoPlugin` / `BuiltinDiagnosticsPlugin`) plus
   `ProjectionStatusPlugin` migrate independently when adjacent
   work touches them. No hard deadline.

4. **Test plugins migrate opportunistically.** Plugins that
   intentionally test the legacy dispatch path (e.g.
   `LegacyAnnotatorPlugin`) keep `impl PluginBackend` directly
   with an inline comment documenting the intent.

5. **WASM adapter (`WasmPlugin`) keeps `impl PluginBackend`
   directly.** Migrating to HandlerRegistry would be a ~1600-line
   rewrite of mechanical WIT-call translations with no consumer
   benefit. Re-evaluation deferred.

6. **`PluginBackend` visibility tightening.** A follow-up audit
   downgrades the public surface (`#[doc(hidden)]` confirmation,
   prelude re-export audit, optional `pub(crate)` for cross-crate
   consumers). Tracked separately.

### Implications

- The "Plugin Redesign" phase (ADR-025–029) becomes the canonical
  authoring story. ADR-038 affirms it as the *only* path forward.
- R1.x machinery stays useful: narrow trait views via blanket
  impls remain available; the three migrated capability traits
  keep their super-trait status; opt-in macros
  (`impl_pubsub_member_default!` etc.) keep their utility.
- Built-in migration runs at low priority alongside customer-value
  workstreams (ADR-031 phase 12, ADR-032 Vello evaluation, etc.)
  rather than as a coordinated push.
- Plugin authors see one path in user-facing documentation:
  `Plugin` trait + `HandlerRegistry`.
- The 73-method legacy surface stops growing. Over time it shrinks
  (e.g. if ADR-027's R5 annotator unification ships and removes
  `annotate_line_with_ctx`).

### Acceptance Evidence

- `kasane-core/src/plugin/traits.rs` module-level doc states
  "internal ABI; new plugins use the `Plugin` trait".
- `kasane-core/src/plugin/capability_traits.rs` module-level doc
  states "R1.7+ frozen; further capability-trait migration is not
  planned".
- `CONTRIBUTING.md` Plugin API section names this ADR and forbids
  new `PluginBackend` methods outside the narrow exception.
- `roadmap.md §2.2 Backlog` carries an entry pointing to this
  ADR for the built-in migration follow-on.

### Rejected Alternatives

1. **R1.7+ continuation (full god-trait split).** Rejected:
   splitting `InputHandler` (12 methods) and `Annotator`
   (6 methods + 3 generations) across 24+ impl sites with the
   `Box<dyn PluginBackend>` owner-side question unresolved would
   either land at an umbrella trait (a rename) or fragment
   ownership. Cost large, ROI uncertain.
2. **Aggressive `PluginBackend` deletion (the original "R1.9
   strips PluginBackend").** Rejected: requires a coordinated
   rewrite of `PluginRuntime`, `PluginBridge`, `WasmPlugin`, and
   every test plugin in a single workstream. Too large for a
   "trait hygiene" justification when ADR-031 / ADR-032
   customer-value workstreams compete for the same time.
3. **Status quo + accept new methods on `PluginBackend`.**
   Rejected: extension-point accretion on the legacy surface
   negates HandlerRegistry adoption and recreates the 73-method
   debt over time.
4. **Migrate WASM adapter to HandlerRegistry as part of this
   ADR.** Rejected: ~1600-line rewrite of mechanical WIT-call
   translations with no current consumer benefit. The adapter's
   parallel-method structure is a faithful translation of the WIT
   surface; restructuring is mechanical churn.
