# Phase R3.x — Admission-criteria cleanup

Post-R2.x audit of LLM-assisted refactoring candidates produced a
verified-via-grep punch list. Net result: −741 LoC, 2899 tests green, clippy
`--features gui` green. Landed items:

- `EffectFootprint` + `compute_transitive_footprints` + tests deleted
  (ADR-030 Level 5 artefact; 0 production readers confirmed by workspace
  grep). decisions.md ADR-030 §Level 5 note updated.
- `Element::ResolvedSlot` + `Element::SlotPlaceholder` (placeholder
  retained, ResolvedSlot collapsed) replaced by
  `Element::Flex { slot: Option<FlexSlotMetadata>, .. }`. Removes
  duplicated measure / place / walk dispatch arms across `layout/flex.rs`,
  `layout/hit_test.rs`, `layout/hit_map.rs`, `render/walk.rs`,
  `render/cursor.rs`, `render/pipeline_salsa.rs`, `plugin/bridge.rs`,
  `surface/resolve.rs`, `kasane-wasm/src/host.rs`, plus
  `bin/element_probe.rs` and `surface_probe` tests. semantics.md §2.6 P(X)
  functor synchronised.
- `*PreDispatchResult` enums collapsed: `KeyPreDispatchResult<Cmd = Command>`,
  `MousePreDispatchResult<Cmd = Command>`,
  `TextInputPreDispatchResult<Cmd = Command>` with `KakouneSide*` as type
  aliases (ADR-044 tier-1 names preserved, duplicate enum bodies retired).
- `restart_required_diff()` rewritten as declarative
  `RESTART_REQUIRED_FIELDS: &[(&str, FieldDiffersFn)]` table.
- `depth_stencil.rs` lost `stencil_write_increment` +
  `stencil_write_decrement` (no callers; the `pipeline_depth_stencil`
  builder remains, wired into `image_pipeline` / `quad_pipeline` /
  `scene_renderer` and confirmed in active use).
- Dead-code reaping: `kasane/src/builtins/{info,menu}.rs` (one-line
  re-export stubs), `MirrorBufferSurface` alias, `ShadowRenderInfo` +
  `EditableSynthetic.shadow_override` placeholder, `WorkspaceNode::any_child`
  + `find_in_children`, `WidgetBackend::{from_widgets,reload_from_widgets}`,
  `CoreSettingRegistry::keys`, unused `WireFace` bench imports.
- Performance numbers consolidated to [performance.md](../performance.md);
  roadmap rows and the ADR-031 perf-tune table cite the single source
  instead of duplicating Phase-11-era figures.
