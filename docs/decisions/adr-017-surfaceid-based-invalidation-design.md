# ADR-017: SurfaceId-Based Invalidation (Design)

**Status:** Proposed (implementation to be evaluated when Phase 5 begins)

### Background

The current `DirtyFlags` are global: Draw messages from Kakoune invalidate all Salsa inputs and SceneCache. In Phase 5 (multi-pane), pane A's Draw would unnecessarily invalidate pane B's cache.

### Proposed Design

1. **`SurfaceDirtyMap`**: Replace global `DirtyFlags` with `HashMap<SurfaceId, DirtyFlags>`
2. **Per-surface Salsa inputs**: Per-surface input structs for per-surface memoization
3. **`apply()` return type change**: `DirtyFlags` → `Vec<(SurfaceId, DirtyFlags)>`
4. **Global events**: Refresh, SetUiOptions broadcast `ALL` to all surfaces
5. **BUFFER_CURSOR split integration**: Per-surface `BUFFER_CONTENT` for inter-pane isolation

### Surface ↔ DirtyFlags Mapping

| Surface | Primary DirtyFlags |
|---------|-------------------|
| `SurfaceId::BUFFER` (per-pane) | `BUFFER_CONTENT`, `BUFFER_CURSOR` |
| `SurfaceId::STATUS` | `STATUS` |
| `SurfaceId::MENU` | `MENU_STRUCTURE`, `MENU_SELECTION` |
| `SurfaceId(INFO_BASE + i)` | `INFO` |
| Plugin surfaces | `OPTIONS` (config change) + custom |

### Compatibility with Existing Mechanisms

- RenderOrnaments surface anchor — per-surface ornament targeting. Consistent with the design
- `EffectiveSectionDeps` — extendable to per-surface deps
- `PluginSlotCache` — independent cache entries per surface

### Migration Path

1. Introduce `SurfaceDirtyMap` internally while maintaining global `DirtyFlags` as a fallback
2. In `apply()`, set flags only for the target surface for Draw; broadcast to all surfaces for others
3. Gradually migrate Salsa inputs to per-surface
4. Testing: existing `cache_soundness.rs` + `trace_equivalence.rs` guarantee single-surface equivalence

### Risks

- Plugin API compatibility: `on_state_changed(dirty: DirtyFlags)` is safest to keep as global (OR aggregation)
- Increased complexity: premature before multi-pane is implemented. Re-evaluate when Phase 5 begins
