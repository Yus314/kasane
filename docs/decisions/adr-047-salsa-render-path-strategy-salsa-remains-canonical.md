# ADR-047: Salsa Render Path Strategy — Salsa Remains Canonical

**Status:** Accepted (2026-05-12). Closes the open question raised in
`memory/project_plugin_extensibility_gaps.md` and pre-empts a
"retire Salsa" alternative considered during refactor planning.

### Context

Refactor planning surfaced the question of whether the Salsa-backed
render entry (`pipeline_salsa.rs`, 588 LoC + `salsa_views/` 746 LoC +
`salsa_sync.rs` 529 LoC + `salsa_queries.rs` 518 LoC + `salsa_inputs.rs`
208 LoC ≈ 2600 LoC infrastructure) is pulling its weight, or whether
it represents half-finished memoization that could be retired in
favour of the simpler `pipeline.rs::render_pipeline` path.

The hypothesis "Salsa lacks plugin transform integration" originated
from a 2026-03-22 finding in `project_plugin_extensibility_gaps.md`.
That gap was **resolved on 2026-03-27**: both menu (via fallback to
the non-Salsa builder when `MENU_TRANSFORM`-capable plugins are
present) and info (via style-specific transform targets) now flow
plugin transforms through the Salsa path.

### Decision

Salsa remains the canonical render path. `pipeline_salsa.rs::render_pipeline_cached`
is called from `kasane-tui/src/lib.rs:598` and
`kasane-gui/src/app/render.rs:117` as the production entry.

**Phase γ-1.1 closure (2026-05-12)**: the `pipeline.rs::render_pipeline`
/ `render_pipeline_direct` / `scene_render_pipeline` paths (and the
`DirectViewSource` fallback) — preserved by this ADR for the legacy
parity tests — were deleted along with `tests/salsa_pipeline_comparison.rs`.
The shared core (`PreparedFrame`, `render_cached_core`,
`scene_render_core`, `populate_inline_box_paint_commands`, private
helpers) absorbed into `pipeline_salsa.rs`. The `ViewSource` trait
disappeared with its only non-Salsa implementer.

No infrastructure changes to Salsa itself. The cost (≈2600 LoC,
salsa proc-macro build time) is amortised by the existing memoization
hit rate.

### Rationale

1. **Production reachability**: trace-equivalent static analysis
   confirms both TUI and GUI backends call the Salsa wrapper as their
   exclusive production entry. Removing it would require redesigning
   both backend entry points and forfeiting all memoization.

2. **Gap memo is stale**: the headline claim "Salsa path lacks plugin
   transforms" was true on 2026-03-22 and is false from 2026-03-27.
   `pipeline_salsa.rs:171, 202, 321, 366` all invoke
   `registry.apply_transform_chain*` or
   `apply_transform_chain_hierarchical` and
   `salsa_sync.rs:282 sync_plugin_contributions()` feeds plugin
   contributions into Salsa inputs each frame.

3. **Three-stage pipeline is the right architecture**: Stage 1 (pure
   Salsa-memoized rendering of builtin views), Stage 2 (plugin
   contributions stored as Salsa inputs), Stage 3 (imperative
   transform application). Replacing with a single direct path would
   collapse stages 1 and 2 into per-frame recomputation.

### Implications

- `MEMORY.md` index entry for `project_plugin_extensibility_gaps.md`
  is updated to reflect resolved status (was stale).
- Refactor work on the plugin path (Phase α/β in
  `docs/roadmap.md`) does not touch Salsa infrastructure.
- Future work to extend Salsa memoization to plugin transforms
  themselves (full Stage 3 memoization) remains open but is **not
  required** — the current three-stage split satisfies ADR-024
  perceptual imperceptibility.

### Alternatives considered

1. **Retire Salsa entirely, fold the legacy direct path back into the
   production entry.** Rejected. Both production backends depend on
   `render_pipeline_cached`. Forfeits memoization without measurable
   gain. (The mirror move — keeping Salsa and deleting the legacy
   direct path — became Phase γ-1.1; see "Phase γ-1.1 closure" above.)

2. **Extend Salsa to memoize Stage 3 transforms.** Deferred. Would
   require making plugin state Salsa-input-compatible
   (`Eq + Hash` requirement), forcing API contract change on
   `PluginState`. Not justified absent measured perf regression.

---
