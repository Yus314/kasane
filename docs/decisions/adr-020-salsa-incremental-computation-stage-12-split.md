# ADR-020: Salsa Incremental Computation — Stage 1/2 Split

**Status:** Decided

### Background

Kasane's rendering pipeline previously used a multi-layer caching system (ViewCache, LayoutCache, SceneCache, PaintPatch) driven by manual `DirtyFlags` bitmask tracking. While effective — achieving ~49μs CPU per frame at 80×24 — the system had accumulated complexity:

1. **Manual invalidation bookkeeping**: Each view function had to declare its `DirtyFlags` dependencies (BUILD_BASE_DEPS, BUILD_MENU_SECTION_DEPS, etc.), verified at compile time by the `#[kasane::component(deps(...))]` macro. Adding new state fields required updating both `DirtyFlags` and all dependency declarations.

2. **Cache coherence by convention**: `ViewCache`, `SceneCache`, and `LayoutCache` each duplicated the invalidation logic (which flags invalidate which cache section), with correctness relying on manual alignment rather than structural guarantees.

3. **Plugin interaction complexity**: `PluginSlotCache` used its own two-level cache (L1: state_hash, L3: slot_deps) independent of the view caching system, requiring separate `prepare_plugin_cache()` calls before rendering.

The Salsa incremental computation framework (v0.26.0) offers automatic dependency tracking and memoization, potentially replacing the manual invalidation bookkeeping while preserving the pipeline's performance characteristics.

### Decision

Adopt a **Stage 1 / Stage 2 split** architecture where:

- **Stage 1 (Salsa tracked)**: Pure Element generation from protocol state. Salsa automatically tracks dependencies and memoizes results. No plugin interaction.
- **Stage 2 (hybrid)**: Plugin contributions, transforms, and annotations collected imperatively from `PluginRuntime` (which uses `RefCell` interior mutability), then stored as Salsa inputs. Pure transform patches (`ElementPatch`) and per-plugin contribution results are set as Salsa inputs with `PartialEq` early-cutoff, enabling downstream memoization when plugin outputs are stable across frames. Impure patches (Custom, ModifyAnchor) fall back to imperative application.

Salsa is a mandatory dependency. The legacy Surface-based pipeline (`pipeline_surface.rs`, `SurfaceViewSource`) has been removed; all rendering uses the Salsa path exclusively.

### Architecture

Stage 1 uses 7 Salsa input structs: 6 grouped by protocol message boundary (Buffer, Cursor, Status, Menu, Info, Config) plus `TransformPatchesInput` for pre-collected pure transform patches. Four tracked view functions produce Element trees from these inputs. Stage 2 collects plugin outputs imperatively and writes them into additional Salsa inputs (`SlotContributionsInput`, `AnnotationResultInput`, `PluginOverlaysInput`, `DisplayDirectivesInput`, `TransformPatchesInput`) via `sync_plugin_contributions()`, `sync_display_directives()`, and `sync_transform_patches()`. Each input uses `PartialEq` early-cutoff for fine-grained memoization. The `ContributionCache` (per-plugin per-slot caching) is owned by `SalsaInputHandles`, consolidating all sync-phase state. The legacy manual caching infrastructure (ViewCache, LayoutCache, PaintPatch) has been removed; `SalsaViewSource` is the sole implementation. `SceneCache` remains as a GPU-path auxiliary cache.

For implementation details (input structs, tracked functions, pipeline variants, file mapping), see the source code in `kasane-core/src/salsa_sync.rs`, `kasane-core/src/salsa_inputs.rs`, and `kasane-core/src/render/pipeline_salsa.rs`.

### Trade-offs

1. **Now fully replacive**: The Salsa layer adds ~11-13μs of cache-hit overhead (5-6 tracked functions × ~2.2μs each), which is negligible relative to the 4167μs frame budget at 240fps. The legacy caching infrastructure (`ViewCache`, `LayoutCache`, `PaintPatch`) has been fully removed. Only `SceneCache` remains as a GPU-path auxiliary cache for per-section `DrawCommand` reuse.

2. **Plugin boundary is hybrid**: Plugins with `RefCell` interior mutability cannot participate directly in Salsa's dependency graph. The epoch-based bridge detects when plugin outputs *might* have changed, and the sync phase re-collects outputs into Salsa inputs. However, pure transform patches (`ElementPatch` with `PartialEq`) and contribution results benefit from Salsa's early-cutoff: when a plugin's output is unchanged across frames, downstream revalidation is skipped.

3. **Legacy pipeline removed**: The `salsa_pipeline_comparison.rs` test suite verifies correctness of the Salsa path against reference outputs.

4. **`no_eq` on all view functions**: Although `Element` implements `PartialEq`, the tracked view functions use `no_eq` because no downstream tracked functions depend on their outputs. Output-level equality checks would add comparison cost without benefit. This means a cache miss on any input *will* propagate to all callers, even if the output happens to be identical. This is acceptable because the tracked functions are leaf-level (no further tracked functions depend on their Element output).

### Testing

`kasane-core/tests/salsa_pipeline_comparison.rs` — 15 tests verifying cell-by-cell grid equivalence between legacy and Salsa pipelines across scenarios including:

- Base states (empty, buffer content, status bar, menu variants, info popups)
- Plugin contributions (slot, transform, annotation, gutter)
- Combined plugin scenarios

### Future Considerations

- If the pipeline is deepened (e.g., layout or composition as tracked functions), remove `no_eq` annotations to enable output-level early-cutoff (`Element` already implements `PartialEq`)
- When Phase 5 (multi-pane) introduces `SurfaceDirtyMap`, the Salsa input sync can be extended to per-surface granularity
- Plugin purity contracts (future): plugins that opt into pure `fn(&AppState) -> Element` could become tracked functions, eliminating the epoch bridge for those plugins
