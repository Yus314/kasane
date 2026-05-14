# Phase R2.x — Plugin Path Consolidation

**Opened 2026-05-08, closed 2026-05-10.**
[ADR-039](../decisions/adr-039-plugin-path-consolidation-r2x.md) supersedes
ADR-038. A workspace-wide grep confirmed `capability_traits.rs` (1040 LoC) has
zero narrow-trait consumers; the R1.x super-trait migration is dead
architecture. ADR-039 reverses ADR-038's freeze and defines a 12-PR program to:

- Migrate all 9 builtin plugins to `Plugin + HandlerRegistry`
  (~525 LoC of `impl PluginBackend` rewritten).
- Delete `capability_traits.rs` (1040 LoC) and the
  `impl_migrated_caps_default!` macro across 21 sites.
- Reduce `PluginBackend` to internal `pub(crate)` ABI consumed
  only by `PluginRuntime` and the WASM adapter.
- Delete transitional APIs unblocked by builtin migration:
  `has_decomposed_annotations`, `annotate_line_with_ctx`,
  `Atom::from_wire`, `WireFace` public visibility,
  `#[deprecated] PluginRegistry` alias.
- Mechanise Bridge dispatch (`bridge.rs`: 1900 → ~700 LoC).
- Split 3 large modules (shadow_cursor, registry/collection,
  handler_registry) along natural axes.
- Contract `kasane-core` public module surface from 28 to ~12.

| Phase | Status | Notes |
|---|---|---|
| P0 — ADR-039 + roadmap entry | ✅ 2026-05-08 (`6484224a`) | This entry; ADR-038 marked Superseded |
| P1-prep — HandlerRegistry pre-dispatch hooks | ✅ 2026-05-08 (`ad9e4588`) | Added `on_key_pre_dispatch` / `on_mouse_pre_dispatch` / `on_text_input_pre_dispatch` / `on_mouse_fallback`. Discovery: HandlerRegistry was missing these, blocking P1a/P1c |
| P1a — Input builtins (4) | ✅ 2026-05-08 (`65726e12`) | BuiltinInputPlugin, BuiltinDragPlugin, BuiltinFoldPlugin, BuiltinMouseFallbackPlugin |
| P1b — Render builtins (2) | ✅ 2026-05-08 (`8cd345e7`) | BuiltinInfoPlugin + BuiltinMenuPlugin; `on_render_menu_overlay` / `on_render_info_overlays` signatures gained `&PluginView` |
| P1c — BuiltinShadowCursorPlugin | ✅ 2026-05-08 (`bb52cd35`) | Largest builtin (255 LoC `impl PluginBackend`); manual smoke gate cleared |
| P1d — ProjectionStatusPlugin | ✅ 2026-05-08 (`5a80dbce`) | |
| P2 — Vestigial deletes | ✅ 2026-05-08 (`c4836223`) | `#[deprecated] PluginRegistry` alias removed; shadow_cursor docstring rewrite |
| P3 — Delete capability_traits.rs | ✅ 2026-05-08 (`17bfea90`) | 30 files, +65/−1210 LoC. 7 super-trait methods moved onto `PluginBackend`; `#[kasane::plugin]` proc macro no longer emits the scaffolding |
| P4 — Delete `has_decomposed_annotations` + `annotate_line_with_ctx` | ✅ 2026-05-08 (`ed314b83`) — **reduced scope** | Bridge's joiner (61 LoC) deleted. Trait-level `has_decomposed_annotations` retained: WIT `annotate-line` export still relies on it. Full deletion blocked on WIT 4.0 ABI bump (out of scope per ADR-039 §Rejected #2) |
| P5 — `PluginCapabilities` bitflag scope reduction | ✅ 2026-05-08 (`8245a3cc`) | Dropped unused `VIRTUAL_EDIT` and `TEXT_INPUT_PRE_DISPATCH` bits |
| P6 — `PluginBackend` visibility tightening | ✅ 2026-05-08 — **closed at `#[doc(hidden)] pub`** | Already achieved by P3 (`traits.rs:128`). True `pub(crate)` is not viable: `kasane-wasm::adapter`, `kasane-tui::event_handler`, `kasane`'s 4 builtins, `kasane-macros` proc macro, and `locked_wasm_provider`'s factory all hold `impl PluginBackend` / `dyn PluginBackend` outside `kasane-core`. `pub(crate)` would require migrating ~7 sites including a 1000+ LoC WASM adapter — out of the 0.5-day P6 budget; defer to a future ABI-extraction workstream if surfaced |
| P7 — `WireFace` full visibility downgrade | ✅ 2026-05-08 (Plan B execution) | 8-PR cascade migrating ~200 occurrences of `WireFace { ... }` literals + `face: WireFace` fields to `Style`. Endpoints: `Element::text(Style)`, diagnostics overlay primitives, `ColorResolver`, IME state, bench/test fixtures, WIT bridge, ornament types (`CursorEffectOrn` / `SurfaceOrn` / `ResolvedSurfaceOrn`), `ContainerPaintInfo`, `Command::RegisterThemeTokens`, `DisplayDirective::StyleInline`/`StyleLine`, `InlineOp::Style`. `WireFace` is `#[doc(hidden)] pub` (not in plugin_prelude); the JSON wire format helpers in `kasane-tui benches` and `kasane-wasm convert/tests` retain access for round-trip testing |
| P8 — Bridge dispatch full mechanisation | ✅ 2026-05-10 (`e5d679cb`) — **reduced scope** | Added `dispatch_state_with_default!` (covers 8 state-mutating handlers with non-Effects/non-Option returns) and `dispatch_inject_owner_contribution!` (covers `contribute_to` and `contribute_overlay_with_ctx`). The original ADR-039 estimate (`1900 → ~700 LoC`) was over-ambitious: tests account for ~1000 lines, the impl block ~600. Net LoC: +34 (macro definitions outweigh callsite shrinkage). Win is consistency / extensibility, not LoC. `decorate_gutter` retains its explicit form (priority tuple). |
| P9 — `Atom::from_wire` delete | ✅ 2026-05-08 (Plan B PR7) | Demoted from `pub` to `pub(crate)` (the `final_*`-preserving constructor stays internal to the protocol parser and `test_support::wire`'s cursor fixtures). ~60 callers migrated to `Atom::with_style(_, Style::from_face(&face))` for non-cursor cases |
| P10a — `state/shadow_cursor.rs` split | ✅ 2026-05-08 (`24c6e1f7`) | Extracted `keyboard.rs` + `commit.rs`; mod.rs keeps types + tests + the Plugin |
| P10b — `registry/collection.rs` split | ✅ 2026-05-08 (`39df9817`) | 6 axes: contributions / transforms / annotations / display / overlays / ornaments |
| P10c — `handler_registry.rs` split | ✅ 2026-05-08 (`77cbb40d`) | 6 axes: lifecycle / input / render / transform / decoration / extension |
| P11 — kasane-core public surface contraction | ✅ 2026-05-08 (`21439d27`) — **reduced scope** | 4 modules contracted: `salsa_inputs` → `pub(crate)`; `salsa_queries`/`salsa_views`/`display_algebra` → `#[doc(hidden)] pub` (have integration test/bench consumers). Effective rendered surface: 28 → 23. Backends consume more modules than the original 12-target assumed |

**R2.x program closed (2026-05-10).** All 12 PRs landed: P0–P11. P7 was
expanded into the 8-PR Plan B cascade (`7020bc52..62a793c0`); P8 closed at
reduced scope (consistency win, not LoC); P6 closed at `#[doc(hidden)] pub`;
P9 closed at `pub(crate)`. `bridge.rs` 1900→700 LoC target deferred
(cookie-cutter exhausted; further reduction requires structural changes).
Total wall-clock: 2 days (2026-05-08 → 2026-05-10).
