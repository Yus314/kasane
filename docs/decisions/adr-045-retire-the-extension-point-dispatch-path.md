# ADR-045: Retire the Extension-Point Dispatch Path

**Status:** Partially landed (commit `cbf17f4c`). Rust dispatch deleted;
the WIT `evaluate-extension` guest export remains in
`kasane:plugin@5.0.0` until the next major ABI bump, where the wire
removal is batched per [ADR-046](./adr-046-wit-abi-600-batched-retirement.md).

### Context

The plugin-defined extension-point API
([ADR-029](./adr-029-topic-based-pubsub-and-plugin-defined-extension-points.md))
added per-frame composition (`Merge` / `FirstWins` / `Chain`) so plugins
could define typed extension points that other plugins contribute to.
After a roadmap audit (commit `4e646603`, R3.x admission-criteria
cleanup), the in-tree producer count was zero: no host code, no example
plugin, and no test outside the dispatch infrastructure itself called
`define_extension` or `on_extension`. The four integration tests that
referenced the API were dispatch-witness tests, not consumer tests.

The dispatch path carried ~600 LoC of machinery (per-frame composition
engine, type-erased handler tables, host-side bridge dispatch, WASM
adapter dispatch, manifest-driven metadata-only definitions) in service
of a zero-consumer feature, and held the WIT `evaluate-extension`
export in the load-bearing surface that has to be preserved across
minor bumps.

### Decision

Retire the Rust dispatch path immediately; defer the WIT wire-level
removal to the next major ABI bump. Specifically:

- Delete `PluginRuntime::evaluate_extensions` and the
  `Merge` / `FirstWins` / `Chain` composition logic in
  `kasane-core/src/plugin/registry/mod.rs`.
- Delete `PluginBackend::{extension_definitions, evaluate_extension}`
  and the impls in `PluginBridge` and `WasmPlugin`.
- Delete `HandlerRegistry::{define_extension,
  define_extension_with_handler, on_extension}` and the
  `HandlerTable::{extension_definitions, extension_contributions}`
  fields.
- Delete the five extension-point types (`ExtensionDefinition`,
  `ExtensionContribution`, `ExtensionResults`, `ExtensionOutput`,
  `CompositionRule`).
- Delete the four extension-point integration tests in
  `kasane-core/src/plugin/tests/registry.rs`.
- Delete `WasmPlugin::{extensions_consumed, extension_defs}` shared
  fields and the manifest → `metadata_only` translation in
  `kasane-wasm/src/lib.rs`.
- Keep `ExtensionPointId` as a typed wrapper. The plugin manifest
  schema still parses `handlers.extensions_defined` and
  `handlers.extensions_consumed` into this shape so existing
  `kasane-plugin.toml` files continue to parse cleanly even though the
  metadata has no runtime effect.
- Keep the WIT `evaluate-extension` guest export until the next major
  ABI bump (ADR-046). Per `docs/abi-versioning.md`, removing a WIT
  function is a major bump (`6.0.0`); doing it standalone would
  invalidate every shipped `.wasm` for a single-line wire gain. The
  host no longer dispatches to this export, so guests compiled against
  `kasane:plugin@5.0.0` keep their generated stubs without functional
  consequence.

### Rationale

**Producer-count audit dominates.** ADR-029 originally introduced
extension points speculatively, anticipating overlay-renderer plugins
that would consume them. Two years on, the actual overlay renderers
(menu, info) ship as built-in plugins and use direct
`PluginBackend::render_menu_overlay` / `render_info_overlays` hooks.
Pub/sub
([`r.subscribe` / `r.publish_typed`](./adr-029-topic-based-pubsub-and-plugin-defined-extension-points.md))
absorbed the per-frame inter-plugin coordination niche, and slot
contribution covered the rest. No production producer materialised.

**Splitting the retirement into Rust-now / WIT-later minimises wasm
rebuild churn.** A standalone WIT-only major bump for one export would
force all bundled and example `.wasm` blobs to rebuild, plus every
external plugin author to bump SDK. Bundling the WIT removal with the
next major (ADR-046) amortises that cost.

**Manifest schema kept stable for parse compatibility.** External
plugin packages may have already declared
`handlers.extensions_defined` / `handlers.extensions_consumed` in
their `kasane-plugin.toml`. Dropping the fields from the manifest
parser would force a manifest schema rev, which is more user-visible
than the runtime no-op the fields now have. Schema rev is deferred to
ADR-046's discretion.

### Implications

- **CHANGELOG:** Breaking change entry. Native plugins that called
  `HandlerRegistry::define_extension` / `on_extension` no longer
  compile. WASM plugins are unaffected at the wire level until
  ADR-046 ships.
- **`docs/migration/0.7-to-0.8.md`:** Migration cookbook section
  covering the `define_extension` → `r.subscribe` / `r.publish_typed`
  rewrite recipe.
- **`docs/plugin-api.md`:** Removed the "Extension Point" subsection
  from §1; manifest reference still documents the metadata fields as
  inert.
- **Roadmap:** §2.2 backlog entry updated to flag partial completion
  and forward-reference ADR-046 for the WIT-side finish.
- **WIT 6.0.0 batch (ADR-046):** delete `evaluate-extension` export,
  delete `ExtensionPointId` re-export, drop manifest fields (optional).

### Alternatives considered

1. **Full retirement in one ABI 6.0.0 bump now.** Rejected. F-1b's
   WIT removal is one line; bumping major for it standalone wastes
   the migration window. Bundling with the W1 (Tier-1 ABI completion)
   batch is strictly more efficient.

2. **Keep dispatch wired and document as "ready for consumers".**
   Rejected. ADR-029's two-year lookback found zero consumers; another
   maintenance cycle costs ~600 LoC of carried complexity for no
   observed value. The retirement reverses cleanly if a future
   consumer materialises (the design is well-documented in the
   git history at HEAD~).

3. **Delete `ExtensionPointId` and the manifest fields immediately.**
   Rejected. Manifest schema rev is more user-visible than the
   runtime no-op. Defer to ADR-046's coordinated migration.

---
