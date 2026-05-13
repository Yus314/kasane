# ADR-016: Pipeline Equivalence Testing — Trace-Equivalence Axiom

**Status:** Superseded by ADR-047 + Phase γ-1.1 (2026-05-12). The
multi-variant equivalence model below was retired once the legacy
direct path (`render_pipeline` / `render_pipeline_direct` / `scene_render_pipeline`)
was deleted. Salsa is the only production path; T1 (determinism)
remains, T3 (cached-vs-direct) is no longer expressible. The
`trace_equivalence.rs` proptest harness was retained and reframed
around Salsa-path determinism only.

### Background

Kasane's rendering pipeline has multiple optimization variants:

1. `render_pipeline()` — full pipeline (reference implementation)
2. `render_pipeline_cached()` — Salsa-backed memoization (previously `render_pipeline_direct()` with ViewCache)
3. ~~`render_pipeline_sectioned()`~~ — removed (Salsa handles section-level memoization)
4. ~~`render_pipeline_patched()`~~ — removed (PaintPatch superseded by Salsa)
5. `scene_render_pipeline_cached()` — GPU path with SceneCache

Currently, inter-variant equivalence is verified through `debug_assert` (debug builds only) and manual tests (`cache_soundness.rs`), with the following issues:

- `cache_soundness.rs` tests only one fixed state (`rich_state()`)
- `debug_assert` is disabled in release builds
- The combination space of DirtyFlags and state mutations is wide, risking missed edge cases

### Decision

Define as a formal invariant that all pipeline variants are **observationally equivalent** for any valid `AppState` and `DirtyFlags` combination, verified through property-based testing with proptest.

**Equivalence axiom:**
```
∀ S ∈ ValidAppState, ∀ D ∈ DirtyFlags:
  render_pipeline(S) ≡ render_pipeline_direct(S, D, warm_cache(S))
                     ≡ render_pipeline_sectioned(S, D, warm_cache(S))
                     ≡ render_pipeline_patched(S, D, warm_cache(S))
```

Here `warm_cache(S)` is the cache after a full render with ALL flags on state S.

### Testing Strategy

1. **Mutation-based fuzzing**: Apply random state mutations (cursor movement, line changes, menu toggle, etc.) to `rich_state()` as a base
2. **Random DirtyFlags**: Current harness randomly generates combinations of 6 coarse categories (`BUFFER`, `STATUS`, `MENU_STRUCTURE`, `MENU_SELECTION`, `INFO`, `OPTIONS`)
3. **Warm → Mutate → Render**: Warm the cache, then apply mutations and compare rendering results with partial flags against a full render

Full Arbitrary implementation is unnecessary — the mutation-based strategy efficiently covers the combination space.

### Implementation Notes (2026-03)

- `DirtyFlags` currently has `BUFFER_CONTENT`, `BUFFER_CURSOR`, `STATUS`, `MENU_STRUCTURE`, `MENU_SELECTION`, `INFO`, `OPTIONS`
- The current `trace_equivalence.rs` strategy does not generate `BUFFER_CURSOR` independently, folding it into the coarse-grained `BUFFER` category
- Therefore, this ADR's axiom is a requirement on current semantics, and the verification harness granularity has not yet reached complete enumeration

### Preservation Mechanism

```
DirtyFlags → Salsa input sync (PartialEq early-cutoff) → per-section rebuild decision
          → SceneCache invalidation → DrawCommand regeneration decision (GPU only)
```

> **Note:** The original diagram referenced ViewCache/LayoutCache, which have been superseded by Salsa (ADR-020).

If each cache's invalidation is correct, all variants are equivalent to the reference implementation.
