# ADR-050: Salsa Scope Policy — Tracked-Query-Backed Inputs Only

**Status:** Accepted (2026-05-14). Amended (2026-05-14) to correct an
input classification — see "Amendment 1" below. Refines
[ADR-047](./adr-047-salsa-render-path-strategy-salsa-remains-canonical.md)
by drawing an explicit boundary inside the Salsa input surface. Closes
the δ-2.3b-iii reopening that
[`docs/roadmap/phase-gamma-delta-epsilon.md`](../roadmap/phase-gamma-delta-epsilon.md)
deferred to a "future architectural ADR".

**Amendment 1 (2026-05-14)**: the initial draft mis-classified
`DisplayDirectivesInput` as Category II based on its producer
(`sync_plugin_contributions` is the writer). Post-merge audit found
that `display_map_query` in `salsa_views/display_map.rs` is
`#[salsa::tracked]` and reads `DisplayDirectivesInput` — so the input
HAS a tracked-query consumer and is Category I per the policy. The
authoritative category boundary is **the consumer side** (does any
`#[salsa::tracked]` query depend on this input?), not the producer
side (whether the writer is observed-projection or plugin-collection).
The retirement list shrinks from 6 to 5 (see "Migration plan" below).

### Context

`kasane-core/src/salsa_inputs.rs` declared 14 `#[salsa::input]` structs.
Auditing their producers and consumers split the set into two categories:

**Category I — Tracked-query-backed inputs (8 inputs).**
`BufferInput`, `CursorInput`, `StatusInput`, `MenuInput`, `InfoInput`,
`ConfigInput`, `HistoryInput`, **`DisplayDirectivesInput`**. Consumers:
Salsa `#[salsa::tracked]` queries in `salsa_views/{buffer,status,menu,
info,display_map}.rs` derive Element trees / DisplayMap from these
inputs. **The canonical Salsa shape**: queries are pure derivations
over the input, the cache catches identical input values.

Producer split: the first seven are projected from `AppState::observed`
by `sync_inputs_from_state()`; `DisplayDirectivesInput` is imperatively
written by `sync_plugin_contributions()`. **Producer origin does not
determine category** — the consumer side does. A plugin-derived input
whose downstream is a `#[salsa::tracked]` query is still Category I
because Salsa's memoization fires on the consumer side.

**Category II — Plugin contribution inputs with no tracked consumer (6 inputs).**
`SlotContributionsInput`, `AnnotationResultInput`, `PluginOverlaysInput`,
`ContentAnnotationsInput`, `TransformPatchesInput`,
`PluginStateRevisionInput`. Producer: `sync_plugin_contributions()`
imperatively collects from the plugin runtime each frame. Consumers:
read directly from `pipeline_salsa.rs::view_sections` and the
`ContributionCache` rev-keying logic — **no `#[salsa::tracked]` query
depends on them**. The Salsa wrapper provided only the `PartialEq`
short-circuit on `set_*.to(value)`, which is unused because nothing
downstream re-runs from these inputs.

The δ-2.3b-i closure note (`ad12fa31`) recorded that the granularity
gain from extending Salsa into plugin contributions is "structurally
zero per the dependency-tracking analysis". The θ-spike (`3b0ac72b`,
2026-05-14) confirmed this empirically for `PluginOverlaysInput`:
removing the Salsa wrapper and inlining collection at the read site
showed no measurable cost change against the `delta-24` and `delta_24`
baselines (criterion `salsa_scene/warm` +1.7% within noise threshold;
iai_pipeline `iai_full_frame` +0.00163% — 12 instructions out of 738066).

### Decision

Salsa applies to Category I inputs only — those with at least one
`#[salsa::tracked]` query consumer. Inputs whose consumers are
exclusively non-tracked code pass through **revision-keyed manual
caches** (the pattern already established by `ContributionCache` after
δ-2.3b-i) or via direct parameter passing at the read site.

The five remaining Category II inputs (`PluginOverlaysInput` was
retired by `3b0ac72b` ahead of this decision) are scheduled for
retirement following the θ-spike template, each behind its own
measurement gate.

### Rationale

1. **Salsa's value proposition assumes pure-derivation queries**.
   Category II inputs have no such consumers — the imperative
   `sync_plugin_contributions()` is the actual cache miss boundary, and
   the Salsa input is a Salsa-shaped buffer with no consumer
   depending on its identity. The `PartialEq` short-circuit fires
   against nothing.

2. **The empirical signal aligns with the structural analysis**.
   `iai_full_frame` instruction count changes by 12 instructions
   (0.00163%) when `PluginOverlaysInput` is removed entirely.
   The "cache" was saving ~0.1 µs/frame in criterion measurements —
   0.05% of the 200 µs SLO. The cost was effectively nominal.

3. **Conceptual simplification is real**. The two-tier "Salsa for
   everything plugin-related" direction (δ-2's original premise)
   created an impedance mismatch: imperative collection writes
   into a Salsa input, then a non-tracked function reads it back.
   The Salsa wrapper added a vocabulary (`set_*.to()`, `db` thread)
   without adding semantics.

4. **`ContributionCache` already demonstrates the alternative**.
   δ-2.3b-i made `ContributionCache` rev-keyed on
   `PluginStateRevisionInput`. The cache lives in plain `HashMap`,
   not in Salsa. Per-plugin invalidation works without the Salsa
   layer; the Salsa input was only acting as a side-channel for
   the revision counter, which can be a plain `u64`.

### Implications

- **Category II inputs are retirement targets** following the θ-spike
  template (commit `3b0ac72b`): for each Salsa input, audit
  consumers, replace with direct parameter or revision-keyed manual
  cache, measure against `delta-24` / `delta_24` baselines, commit.
  Order is independent — each input has distinct consumers.

- **`PluginStateRevisionInput` is the boundary case**. It was added
  in δ-2.2 as the foundation for δ-2.3 / Phase ζ-2's "Salsa-tracked
  plugin queries". With this ADR, Phase ζ-2 (full tracked-query
  conversion of `contribute_to` / `decorate_*`) is **closed**: the
  granularity gain is structurally zero per the δ-2.3b-i analysis,
  and the prerequisite (`KasaneDb` plugin-runtime accessor) is no
  longer needed. `PluginStateRevisionInput` itself can collapse to
  a `u64` field on the relevant manual cache once its retirement
  is staged.

- **`bridge.generation` retirement (δ-2.3b-ii)** is unaffected —
  it remains a candidate behind a `PluginState: Hash` ADR but is
  decoupled from this Salsa-scope decision.

- **No infrastructure changes to Salsa itself** beyond the input-set
  contraction. `salsa_db`, `salsa_queries`, `salsa_views`,
  `salsa_sync` continue to serve Category I.

- **Future Salsa expansion** to additional Category I inputs (e.g.
  splitting `BufferInput` into per-line tracked queries for
  finer-grained invalidation) remains open and unaffected by this
  ADR.

### Alternatives considered

1. **Retire Salsa entirely** (the radical Π option from Round 3
   consideration). Rejected. Category I derives real value:
   `salsa_views/buffer.rs`, `salsa_views/menu.rs`,
   `salsa_views/info.rs`, `salsa_views/status.rs` produce Element
   trees from observed inputs, and the warm-cache path
   (`salsa_scene/warm` ≈ 5.9 µs vs `salsa_scene/cold` ≈ 25.6 µs)
   represents a ~20 µs/frame swing — material against the 200 µs
   SLO.

2. **Extend Salsa to plugin contributions via tracked queries**
   (δ-2.3b-iii direction). Rejected by the structural argument
   above and the measurement absence. Would require `KasaneDb`
   plugin-runtime accessor extension and per-plugin
   dependency declaration in the registry — significant
   architectural debt for zero measurable gain.

3. **Migrate Category II inputs to Salsa interned values** (a middle
   path). Rejected. Interning would help if the same value
   recurred across frames, but plugin contributions are typically
   re-collected per frame from new plugin state; interning would
   not catch identical values often enough to justify the
   complexity.

### Migration plan

Spike (`3b0ac72b`): `PluginOverlaysInput` retired. Always-recollect
strategy chosen for the spike (drops the
`any_overlay_needs_recollect()` cache gate); the bench
data shows the gate was not justified for the overlay-light bundled
workload.

Phase θ proper: retire the remaining five inputs in dependency order
— leaves first (`PluginStateRevisionInput` is read by
`ContributionCache`; `ContributionCache` becomes the direct
revision-counter owner), then siblings (`SlotContributionsInput`,
`AnnotationResultInput`, `ContentAnnotationsInput`,
`TransformPatchesInput`). Each input's retirement is a separate
commit; each measures against `delta-24` and `delta_24`. Net LoC:
estimated -250 to -500 across `salsa_inputs.rs`, `salsa_sync.rs`,
and the pipeline read sites.

`DisplayDirectivesInput` stays. The `display_map_query` tracked query
catches identical-directive frames at the boundary — a real cache hit
worth keeping (DisplayMap construction is O(N) in directive count, and
a stable buffer with stable plugin directives is the common case).

---
