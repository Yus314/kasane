# Phase ADR-044 — Handler → Effect Tier Hierarchy

**Closed (2026-05-11).** [ADR-044](../decisions/adr-044-handler-effect-tier-hierarchy.md)
landed in two phases:

- **Phase A** — host-side tier projections + tier-typed `HandlerRegistry`
  setters across 11 lifecycle handlers
- **Phase B** — WIT 5.0.0 tier-typed exports + SDK `define_plugin!` routing

The dual-export migration channel from B-2 (`on-state-changed-tier1-effects`)
was collapsed into single tier-typed signatures at B-5 (`7edd615d`). All
in-tree WASM blobs (`kasane-wasm/{bundled,fixtures,guests}/*.wasm` +
`examples/wasm/*`) were rebuilt against `kasane:plugin@5.0.0`. ABI 4.x plugins
are rejected at load with a pointer to
[`docs/migration/0.6-to-0.7.md`](../migration/0.6-to-0.7.md) §8.3.

Five exports are now tier-typed:

- `on-state-changed-effects` / `on-command-error-effects` / `on-subscription`
  → return `kakoune-side-effects` (Tier 1)
- `on-io-event-effects` / `update-effects` → return `process-capable-effects`
  (Tier 2)

Shipped in 0.7.0 (`43924376`); tracked under
[#102](https://github.com/Yus314/kasane/issues/102).
