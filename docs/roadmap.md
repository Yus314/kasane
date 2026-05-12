# Implementation Roadmap

This document tracks **what is currently incomplete and what ships next** in Kasane.

## 1. Scope of This Document

This document is limited to the following three concerns.

- Currently open / active workstreams
- Next deliverable
- Delegation targets for backlog / upstream dependencies

The following are NOT the responsibility of this document.

- Explanation of current semantics
- Detailed specification of the shared Plugin API
- Lengthy design explanations of native escape hatches
- Detailed history of completed phases (see [decisions.md](./decisions.md) for ADR records)

For detailed design rationale, see [decisions.md](./decisions.md); for current semantics, see [semantics.md](./semantics.md);
for the current specification from a plugin's perspective, see
[plugin-api.md](./plugin-api.md); for performance numbers and implementation status, see
[performance.md](./performance.md).

## 2. Current Priorities

### 2.1 Now

**ADR-031 Parley text stack migration — Closed (2026-04-30).** Parley
+ swash is the production GPU text stack as of 2026-04-26; the
closure cascade (PR-5a..PR-7 on `feat/parley-color-emoji-test`)
retired the public Face↔Style bridges, bumped the WIT contract to
2.0.0 with Style-native function names, and rebuilt all bundled /
fixture WASM. Phase 11 perf-tune is closed under ADR-024 acceptance
(see "Closed workstreams" below); follow-up perf opportunities are in
[Backlog](#22-backlog).

| Phase | Status | Notes |
|---|---|---|
| 0 — Baseline + ADR | ✅ | `baselines/pre-parley.tar.gz`; ADR-031 in [decisions.md](./decisions.md). 80×24 baseline = 53.13 µs |
| 1a — Style + Brush types | ✅ | Coexists with `Face`; `Atom::style()` bridge |
| 1b–d — `Atom { face, contents }` migration | ✅ | B-wide commit `98592a47` carries `Arc<UnresolvedStyle>` directly on `Atom`; mutex-on-`StyleStore` retired |
| 2 — kasane-core type migration | ✅ | Phase A.3 cascade landed (commits `0388a6f5`–`9266c5ed`); `final_*` resolution flags consumed at the protocol boundary |
| 3 — TUI `TerminalStyle` (design-δ) | ✅ | `TerminalStyle` moved from `kasane-tui` to `kasane-core::render::terminal_style`; `Cell.face: Face` → `Cell.style: TerminalStyle` (Copy, ~50 bytes, SGR-emit-ready); backend reads `cell.style` directly retiring per-cell projection; GUI cell renderer reads `cell.style.fg/bg/reverse` directly |
| B3 commits 1-5 — plugin extension points de-Faced | ✅ | `KakouneRequest`, `ElementStyle`, `BackgroundLayer`, `CellDecoration`, `ElementPatch::ModifyStyle/WrapContainer{style}` migrated from `Face` to `Arc<UnresolvedStyle>` / `Style`; `Cell::with_face_mut`/`set_face` retired in favour of `Cell::with_style_mut`. 9 commits in `057a67d2..05c0be16`. Bench post-merge: warm 64.4 µs (−0.8%), one_line_changed 81.6 µs (−2.6%) |
| B3 Style-native cascade (PR-1..PR-3c) | ✅ | 5 PRs on `feat/parley-color-emoji-test`: `54a466b7` ColorResolver round-trip, `34f30e54` Theme `set_style`/`get_style`/`resolve(_, &Style)→Style`, `7815e3c2` view/* + salsa_views + builders helpers, `eba04c4a` grid + paint API → `&TerminalStyle` + `put_line_with_base(_, _, _, _, base_style: Option<&Style>)`, `6ce6e75b` scene_renderer helpers consume `&Style` (decorations read `DecorationStyle` enum, honour `TextDecoration.thickness` override). The Atom test cascade was already complete pre-branch (`75439f1f` + `3724556f`); the 13 remaining `Atom::from_face` callsites are wire-aware (cursor_face FINAL_FG, detect_cursors, parser, test_support::wire) |
| Closure cascade (PR-5a..PR-7) | ✅ | 6 PRs on `feat/parley-color-emoji-test`: `04aa9fa3` Truth Style-native, `093f5516` BufferRefParams + `make_secondary_cursor_style` (Brush::linear_blend) + cursor blend rewrite, `16266fd1` `Cell::face` / `Atom::face` / `emit_sgr_diff(Face)` deleted + `Style::from_face`/`to_face`/`From<Face>`/`TerminalStyle::from_face` `#[doc(hidden)]`, `571bff58` WIT 2.0.0 + 12 bundled / fixture WASM rebuild + 6 host-state function renames + collision-resolving `get-menu-style` (string) → `get-menu-mode`, `c87699d0` `Atom::from_face` → `Atom::from_wire`. `UnresolvedStyle::from_face` / `to_face` retained (wire parser path); `Face` / `Color` / `Attributes` are `#[doc(hidden)]`. The full `Face` → `WireFace` rename + `pub(in crate::protocol)` visibility downgrade was scoped out — the host crates (kasane-wasm convert layer, kasane-tui / kasane-gui benches and diagnostics) still consume Face directly; a future PR may complete it once those sites migrate to Style |
| 4 — WIT plugin ABI redesign | ✅ | Tier A `a5ef9f56` (brush/style/inline-box, ABI 1.0.0) + Tier B `8f281f52` (SDK macros + helpers + 5 templates) |
| 5 — Bundled WASM plugins rebuild | ✅ | All 10 examples + 6 bundled + 12 fixtures rebuilt against `kasane:plugin@2.0.0` (`571bff58`, post-closure); originally landed `f4df0762` against 1.0.0; `cargo test -p kasane-wasm` 188/0 |
| 6 — `parley_text` facade + cargo deps | ✅ | parley 0.9 + swash 0.2.7 |
| 7 — Parley shaper + L1 `LayoutCache` | ✅ | `Arc<ParleyLayout>`, content/style/font_size key |
| 8 — swash rasteriser + L2/L3 caches | ✅ | LRU + etagere atlas, mask + color split |
| 9 — `SceneRenderer` Parley path | ✅ | All four DrawCommand text variants routed through Parley |
| 9b Step 4c — L2 cache refactor + frame-epoch eviction | ✅ | Same-frame entries protected from eviction |
| 10 — Rich underlines (font metrics) | ✅ | `RunMetrics::underline_offset/size` drives quad geometry |
| 10 — RTL hit_test, InlineBox host paint, Variable font | ✅ | Host paint extension point landed (Phase 10 Step 2-renderer A–D, `26e392a8`–`a019a169`). `define_plugin!` `paint_inline_box(box_id) { body }` macro section parser landed; bundled WASM plugins can override paint. `PluginView::paint_inline_box` enforces recursion depth (≤ 8) + cycle detection thread-locally with once-only error logging. RTL/combining-mark/ZWJ-emoji/trailing-position hit_test coverage added. The bundled `color-preview` plugin (`68c7ece`, rebuilt against WIT 2.0.0 in `571bff5`) is the Phase 10 paint_inline_box worked example end-to-end: `display()` emits `inline_box(...)` directives with a `(line_idx, color_idx)` round-trippable `box_id`; `paint_inline_box(box_id)` decodes back to state and returns a single-cell solid swatch. Variable-font feature surface remains contracted-but-unused; no consumer-driven need has surfaced yet |
| 11 — cosmic-text removal | ✅ | ~1900 LOC dropped; deps gone |
| 11 — perf tune | ✅ Closed (ADR-024 acceptance) | Case A (`StyledLine` hash memoize) landed. Mutex-on-`StyleStore` hypothesis refuted. The `one_line_changed` gap is structurally bounded by L1-miss shape cost; absolute numbers well below the 200 µs SLO, so under ADR-024 the gap is formally accepted. Current numbers and SLO in [performance.md](./performance.md). Follow-up opportunities in §2.2 Backlog |
| 12 — Docs + golden image tests | Substantially complete — **1.0 critical path** (per 0.6.0 admission decision 2026-05-06) | ADR / CHANGELOG updated. CellGrid `golden_grid` (kasane-core, 10 snapshots, all green): ASCII baseline `a2ca6834`, CJK / combining / emoji `79051bb8`, cursor positioning `7c79d42a`, selection `74dcd1a0`. GUI pixel goldens (kasane-gui `golden_render`, 8 passing + 3 ignored): bootstrap `45e9ae42` on Apple M1 / macOS 26.3 / wgpu-hal 29.0.2 (reference-machine policy in `efcbb6ae`); harness landed via `94869b35` (`FrameTarget`). Remaining 3 ignored fixtures are tracked follow-ups, not Phase 12 blockers: (a) `cjk_cluster_double_width` — CI runner variance, regenerate locally with `KASANE_GOLDEN_UPDATE=1`; (b) `font_fallback_chain` — pending `render_scene_to_image` `FontConfig` override; (c) `variable_font_axes` — `Style.font_weight` not on public surface yet (ADR-031 Phase 10 Step C). Excluded from the 0.6.0 admission criteria — completion is a 1.0 prerequisite, not a 0.6.x patch driver |

Parley pipeline benchmarks: see [performance.md](./performance.md)
for the current numbers and SLO targets. (`frame_warm_24_lines`,
`frame_one_line_changed_24_lines`, `shape_warm`,
`salsa_scaling/full_frame/80x24`.)

Open follow-up debts surfaced during the Phase 5 landing (2026-04-29). Most resolved in the design-δ round:
- ✅ L1 `LayoutCache` test coverage: `bg` / `underline` / `reverse` / `dim` / decoration colour / decoration thickness / strikethrough colour now pinned by negative tests in `layout_cache.rs`.
- ✅ GPU atlas pressure: `glyphs_dropped_atlas_full` counter (`raster_cache.rs:103-107`) reports drops with a once-per-process warn guard; the SLO entry is in `docs/performance.md`.
- ✅ `ResolvedParleyStyle` `italic: bool` + `oblique: bool` — replaced by `SlantKind` enum (`6cc6558c`).
- ✅ `atom_to_wit` (`kasane-wasm/src/convert/mod.rs`) now uses `style_to_wit(&a.style_resolved_default())` directly; the legacy `Style::from_face(&a.face())` round-trip is gone.
- ✅ `text-decoration.thickness` physical-pixel unit — documented in `semantics.md` ("Decoration thickness unit").
- ✅ GPU color pipeline sRGB bypass — documented in `semantics.md` ("Brush colour space").
- Pending: ShadowCursor × InlineBox boundary semantics — landed in `semantics.md` ("InlineBox boundary against ShadowCursor"); a runtime assertion that drops/diagnoses overlap is still on the backlog.

**Plugin Path Consolidation (R2.x) — opened 2026-05-08.**
[ADR-039](./decisions.md#adr-039-plugin-path-consolidation-r2x)
supersedes ADR-038. A workspace-wide grep confirmed
`capability_traits.rs` (1040 LoC) has zero narrow-trait
consumers; the R1.x super-trait migration is dead architecture.
ADR-039 reverses ADR-038's freeze and defines a 12-PR program
to:

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

**R2.x program closed (2026-05-10).** All 12 PRs landed: P0–P11. P7 was expanded into the 8-PR Plan B cascade (`7020bc52..62a793c0`); P8 closed at reduced scope (consistency win, not LoC); P6 closed at `#[doc(hidden)] pub`; P9 closed at `pub(crate)`. `bridge.rs` 1900→700 LoC target deferred (cookie-cutter exhausted; further reduction requires structural changes). Total wall-clock: 2 days (2026-05-08 → 2026-05-10).

**Handler → Effect Tier Hierarchy ([ADR-044](./decisions.md#adr-044-handler--effect-tier-hierarchy)) — Closed (2026-05-11).** Phase A (host-side tier projections + tier-typed `HandlerRegistry` setters across 11 lifecycle handlers) and Phase B (WIT 5.0.0 tier-typed exports + SDK `define_plugin!` routing) both landed; the dual-export migration channel from B-2 (`on-state-changed-tier1-effects`) was collapsed into single tier-typed signatures at B-5 (`7edd615d`). All in-tree WASM blobs (`kasane-wasm/{bundled,fixtures,guests}/*.wasm` + `examples/wasm/*`) rebuilt against `kasane:plugin@5.0.0`. ABI 4.x plugins are rejected at load with a pointer to [`docs/migration/0.6-to-0.7.md`](./migration/0.6-to-0.7.md) §8.3. Five exports are now tier-typed: `on-state-changed-effects` / `on-command-error-effects` / `on-subscription` return `kakoune-side-effects` (T1); `on-io-event-effects` / `update-effects` return `process-capable-effects` (T2). Shipped in 0.7.0 (`43924376`) tracked under [#102](https://github.com/Yus314/kasane/issues/102).

**R3.x admission-criteria cleanup (in flight).** Post-R2.x audit of LLM-assisted refactoring candidates produced a verified-via-grep punch list. Net result: −741 LoC, 2899 tests green, clippy --features gui green. Landed items:

- `EffectFootprint` + `compute_transitive_footprints` + tests deleted (ADR-030 Level 5 artefact; 0 production readers confirmed by workspace grep). decisions.md ADR-030 §Level 5 note updated.
- `Element::ResolvedSlot` + `Element::SlotPlaceholder` (placeholder retained, ResolvedSlot collapsed) replaced by `Element::Flex { slot: Option<FlexSlotMetadata>, .. }`. Removes duplicated measure / place / walk dispatch arms across `layout/flex.rs`, `layout/hit_test.rs`, `layout/hit_map.rs`, `render/walk.rs`, `render/cursor.rs`, `render/pipeline_salsa.rs`, `plugin/bridge.rs`, `surface/resolve.rs`, `kasane-wasm/src/host.rs`, plus `bin/element_probe.rs` and `surface_probe` tests. semantics.md §2.6 P(X) functor synchronised.
- `*PreDispatchResult` enums collapsed: `KeyPreDispatchResult<Cmd = Command>`, `MousePreDispatchResult<Cmd = Command>`, `TextInputPreDispatchResult<Cmd = Command>` with `KakouneSide*` as type aliases (ADR-044 tier-1 names preserved, duplicate enum bodies retired).
- `restart_required_diff()` rewritten as declarative `RESTART_REQUIRED_FIELDS: &[(&str, FieldDiffersFn)]` table.
- `depth_stencil.rs` lost `stencil_write_increment` + `stencil_write_decrement` (no callers; the `pipeline_depth_stencil` builder remains, wired into `image_pipeline` / `quad_pipeline` / `scene_renderer` and confirmed in active use).
- Dead-code reaping: `kasane/src/builtins/{info,menu}.rs` (one-line re-export stubs), `MirrorBufferSurface` alias, `ShadowRenderInfo` + `EditableSynthetic.shadow_override` placeholder, `WorkspaceNode::any_child` + `find_in_children`, `WidgetBackend::{from_widgets,reload_from_widgets}`, `CoreSettingRegistry::keys`, unused `WireFace` bench imports.
- Performance numbers consolidated to [performance.md](./performance.md); roadmap rows and the ADR-031 perf-tune table cite the single source instead of duplicating Phase-11-era figures.

**Refactor program — Phase α (cleanup) + Phase β (PluginBackend extinction) — opened 2026-05-12.**
After four deep refactor analysis passes, the next program targets the
~3000 LoC structural redundancy between `PluginBackend` (846 LoC trait,
77 methods) and `HandlerTable` (already-erased dispatch table).
`PluginRuntime` already coordinates dispatch; `PluginBackend` is a
second erasure layer that can be deleted entirely. Backward
compatibility is intentionally lifted for this program.

Decisions taken at program open:

- [ADR-047](decisions.md#adr-047-salsa-render-path-strategy--salsa-remains-canonical) **Accepted (2026-05-12)**: Salsa render path is canonical (production reachability confirmed via static trace from `kasane-tui/src/lib.rs:598` and `kasane-gui/src/app/render.rs:117`). The "Salsa lacks plugin transforms" hypothesis from `project_plugin_extensibility_gaps.md` was stale — the gap was resolved 2026-03-27. No Salsa infrastructure changes.
- [ADR-048](decisions.md#adr-048-plugin-backend-trait-extinction-phase-β) **Proposed (2026-05-12)**: Refines and extends ADR-046 W1-C. Rather than narrowing `PluginBackend` to `pub(crate)` (R2.x P6 closed at `#[doc(hidden)] pub` for cross-crate reasons), delete the trait entirely. `PluginRuntime` holds `Vec<PluginEntry>` directly. Net LoC −2900 (production −1900, test −1000).

| Phase | Status | Notes |
|---|---|---|
| 0.1 — Bench baselines | 🟡 in flight | `cargo bench --bench rendering_pipeline -- --save-baseline phase0` for comparison after each subsequent PR |
| 0.2 — Salsa path static analysis | ✅ 2026-05-12 | Static trace confirmed canonical; `MEMORY.md` index for extensibility-gap memo updated to reflect resolved state |
| 0.3 — ADR-047 / ADR-048 drafts | ✅ 2026-05-12 (`abcac0f0`) | Both ADRs landed in `decisions.md` |
| α-1 partial — Rust-side ADR-045 cleanup | ✅ 2026-05-12 (`3dee4e9a`) | 6 files, -86 / +5 LoC. `extension_point.rs` deleted; `CapabilityDescriptor.extensions_*` fields, `plugin_prelude::ExtensionPointId`, and `kasane-plugin-package` manifest schema cleaned. WIT `evaluate-extension` export + `kasane-plugin-sdk-macros::defaults::evaluate_extension` default impl deferred to Phase β-4 (WIT 6.0.0 bundle) |
| α-2 — Migrate `BuiltinDiagnosticsPlugin` to `Plugin` trait | ✅ 2026-05-12 (`4eee0984`) | Last production `impl PluginBackend` outside `kasane-wasm` (`kasane/src/builtins/diagnostics.rs:21`); switched to `r.on_overlay` registration |
| α-3 prep — internal fixtures + v2 macro migrated to tier-typed setters | ✅ 2026-05-12 (`38b3dfe6` + `581fcc70`) | `state.rs` test plugins, `handler_registry/tests.rs`, `tests/registry.rs PublisherPlugin`, and `plugin_v2_basic` trybuild fixture all switched to `_tier1`/`_tier2`. v2 macro emission now generates `on_init_tier1` / `on_session_ready_tier1` / `on_state_changed_tier1` / `on_io_event_tier2` |
| α-3 deletion — delete 7 deprecated setters | ✅ 2026-05-12 (`119c8051`) | Landed as Phase β-3.1. `on_init`/`on_session_ready`/`on_state_changed`/`on_io_event`/`on_process_task`/`on_process_task_streaming`/`on_update` all gone (-222 LoC from `lifecycle.rs`). `AllHandlersPlugin` exhaustive fixture migrated to tier1/tier2 variants; `StateChangedSpawner` attribution tests deleted since the SpawnProcess-from-state-changed anti-pattern is now structurally banned at the type level. |
| α-4 — Delete legacy `#[kasane_plugin]` macro mode | ✅ 2026-05-12 (`2241fbaf`) | Landed as Phase β-3.2. `expand_kasane_plugin` + 9 `gen_*_impl` helpers + 4 attr filters deleted (-418 LoC from `kasane-macros/src/plugin.rs`). 7 legacy trybuild fixtures + `external_plugin.rs` deleted; 4 macro-emitted plugins in `plugin_integration.rs` rewritten as manual `impl Plugin`. The macro now compile-errors on no-argument usage with a migration hint. |
| α-5 — Extract `handler_registry/mod.rs` test block | ✅ 2026-05-12 (`fa3aae3a`) | 999→382 LoC; `tests.rs` sibling (616 LoC). 41 tests verified |
| β-prep — One-method dispatch spike + iai_pipeline measurement | ✅ 2026-05-12 (`d14a0684`) | GO verdict: dispatch_overhead bench confirms 9ns vtable savings (~0.14% per frame). Phase β's primary value is structural simplification (-2900 LoC), not perf |
| β-1 — Architecture B: SlotImpl enum dual-storage | ✅ 2026-05-12 (`543c51e6`) | `PluginSlot.backend: SlotImpl { Native(Box<PluginBridge>) \| External(Box<dyn PluginBackend>) }`. `PluginBackend::is_bridge()` discriminator + Rust 1.86 trait-object upcast for Box→PluginBridge downcast at register_backend. Deref preserves all 141 call sites. Perf-neutral vs phase0 (p>0.05) |
| β-1.5 — Native fast-path in `notify_state_changed_batch` | ✅ 2026-05-12 (`e956270f`) | First dispatcher branching via `SlotImpl::as_native_mut()`. PluginBridge calls direct concrete method (no vtable); External falls back to `Box<dyn>` path. Pattern proven |
| β-1.6 — Expand fast-path to remaining per-frame dispatchers | ✅ 2026-05-12 (`07fba70b`) | Added `SlotImpl::as_native()` (shared borrow), then mirrored the β-1.5 branch across every remaining per-frame dispatcher in `registry/mod.rs` (prepare_plugin_cache, init_all_batch, notify_active_session_ready_batch, evaluate_pubsub, notify_workspace_changed, shutdown_all, deliver_io/command_error/message_batch, start_process_task, sync_lenses, drain_all_diagnostics) and the view-collection sites in `registry/collection/{ornaments,contributions,transforms}.rs`. Perf-neutral vs phase0 (p=0.80) |
| β-2 — Migrate test fixtures to `impl Plugin` | ✅ 2026-05-12 | 78 of 81 sites migrated across ~20 commits. 6 new `HandlerRegistry` APIs added en route (`declare_surfaces`, `declare_workspace_request`, `deny_process_spawn`, `declare_authorities`, `declare_display_priority`, `declare_lenses`). One auto-inferred capability bug fixed (`observe_key`/`observe_mouse` now contribute to `INPUT_HANDLER`). Remaining 3 `impl PluginBackend` sites — `PluginBridge`, `WasmPlugin`, legacy-tests `WidgetBackend` — disappear structurally in β-3/β-4. All 1931 lib tests pass; perf-neutral vs phase0 |
| β-3.1 — Delete 7 deprecated handler setters (α-3 bundled) | ✅ 2026-05-12 (`119c8051`) | -340 LoC. Removed `on_init`/`on_session_ready`/`on_state_changed`/`on_io_event`/`on_process_task`/`on_process_task_streaming`/`on_update`. SpawnProcess-from-state-changed anti-pattern (#100/#101) now compile-time banned. |
| β-3.2 — Delete legacy `#[kasane_plugin]` macro mode (α-4 bundled) | ✅ 2026-05-12 (`2241fbaf`) | -925 LoC net. `expand_kasane_plugin` gone; 4 macro-emitted plugins rewritten as manual `impl Plugin`; 7 trybuild fixtures + external_plugin.rs deleted. |
| β-3.3a — Delete production WidgetBackend; widget tests use a shim | ✅ 2026-05-12 (`2bae1af2`) | -470 LoC `widget/backend.rs` (legacy `impl PluginBackend` aggregator gone). 22 `backend_*` tests preserved via a test-only `WidgetBackend` shim in `widget/tests.rs` that delegates to `PluginRuntime` + new `first_*_for_test` helpers. Only 2 `impl PluginBackend` sites remain (`PluginBridge`, `WasmPlugin`). |
| β-3.3b.1 — WasmPlugin lifecycle handlers via `impl Plugin` | ✅ 2026-05-12 | Added `impl Plugin for WasmPlugin` (type State = ()) registering 6 lifecycle handlers (`on_init_tier1`, `on_session_ready_tier1`, `on_state_changed_tier1`, `on_io_event_tier2`, `on_workspace_changed`, `on_shutdown`) into a `HandlerRegistry`. Closures capture `Arc<WasmPluginShared>` and reuse the existing `call_synced_with_hash` machinery. New tier-typed converters in `kasane-wasm::convert::command` (`wit_*_to_kakoune_side_effects[_with]`, `wit_process_capable_effects_to_process_capable_effects_with`) plus `#[doc(hidden)] KakouneSideCommand::from_command_unchecked` / `ProcessCommand::from_command_unchecked` backdoors used only at the WIT boundary. PluginBackend impl is left intact — it remains the live runtime path until the loader-flip in β-3.3b.12. New structural test `wasm_plugin_constructs_plugin_bridge` pins the trait-shape fit. |
| β-3.3b.2 — WasmPlugin input observers via `impl Plugin` | ✅ 2026-05-12 | `observe_key` / `observe_mouse` / `observe_drop` registered through `on_observe_key` / `on_observe_mouse` / `on_observe_drop`. Closures preserve the existing `INPUT_HANDLER` / `DROP_HANDLER` capability gates and reuse `WasmPluginShared::call_synced`. `observe_text_input` is intentionally absent — no `observe-text-input` WIT export exists today. |
| β-3.3b.3 — WasmPlugin input handlers via `impl Plugin` | ✅ 2026-05-12 | `handle_key` / `handle_key_middleware` / `handle_mouse` / `handle_drop` / `handle_default_scroll` registered through `on_key` / `on_key_middleware` / `on_handle_mouse` / `on_drop` / `on_default_scroll`. Closures preserve `with_runtime` vs `call_synced` selection per existing trait method (key + default_scroll do conditional state-hash refresh inside `with_runtime`; mouse/drop use `call_synced` without hash refresh). `handle_text_input` is intentionally absent — no `handle-text-input` WIT export. |
| β-3.3b.4 — WasmPlugin input dispatch helpers via `impl Plugin` | ✅ 2026-05-12 | Three new HandlerRegistry APIs land alongside the migration: `declare_key_map(CompiledKeyMap)` (install a pre-built key map; counterpart to the builder-based `on_key_map`), `on_refresh_key_groups(Fn(&S, &AppView, &mut CompiledKeyMap))`, and `on_invoke_action(Fn(&S, &str, &KeyEvent, &AppView) -> (S, KeyResponse))`. WasmPlugin's `compiled_key_map` / `refresh_key_groups` / `invoke_action` migrate to these setters; the WIT-built map is cloned into the registry at `register()` time, group activation re-queries `is_group_active` per dispatch, and `invoke_action` preserves the existing `with_runtime` + plugin_tag + state-hash refresh + diagnostic-on-trap shape. |
| β-3.3b.5 — WasmPlugin view / contribute / transform via `impl Plugin` | ✅ 2026-05-12 | New `on_contribute_any` HandlerRegistry API + `ContributeAnyEntry` + `contribute_any_handler` field on `HandlerTable` cover the case where the underlying contract dispatches contributions for arbitrary slots — primarily WASM plugins. `PluginBridge::contribute_to` consults slot-specific handlers first, then the any-handler fallback. WasmPlugin's `contribute_to` migrates to `on_contribute_any`; `transform_patch` migrates to `on_transform(priority, …)` with the WIT-supplied priority queried at `register()` time and baked into the `TransformEntry`; `transform_menu_item` migrates to `on_menu_transform`. The legacy full-rewrite `transform` WIT export is intentionally not migrated — `PluginBridge::transform` auto-derives by applying the registered patch to the subject. The bridge's `exhaustive_handler_dispatch_coverage` test gains `contribute_any` so future regressions surface. |
| β-3.3b.6 — WasmPlugin annotations + ornaments via `impl Plugin` | ✅ 2026-05-12 | New `on_annotate_line` HandlerRegistry API + `annotate_line_handler` field cover the monolithic line-annotation case where the WIT `annotate-line` export returns all parts (gutter / background / inline / virtual text) in one call. `PluginBridge::has_decomposed_annotations` now derives from whether `annotate_line_handler` is set; when set, `annotate_line_with_ctx` is dispatched through the closure (with `inject_owner` applied to gutter elements). Migrating WasmPlugin via `on_annotate_line` instead of decomposing avoids a 5× WIT round-trip per annotated line. `render_ornaments` and `paint_inline_box` also wired via existing `on_render_ornaments` / `on_paint_inline_box` setters. Exhaustive coverage test gained `annotate_line`. |
| β-3.3b.7 — WasmPlugin display + projections via `impl Plugin` | ✅ 2026-05-12 | `display_directives` → `on_display`; `unified_display` → `on_display_unified` (registered only when the WIT `display` export was probed at construction); `projection_directives` → one `define_projection(descriptor, …)` call per cached descriptor. The `display_directive_priority` trait method always returned 0 (no WIT export), so no `declare_display_priority` call is needed — the registry's default is 0. `has_unified_display` derives automatically from registration. No new HandlerRegistry APIs. |
| β-3.3b.8 — WasmPlugin navigation + overlay + edit intercept via `impl Plugin` | ✅ 2026-05-12 | `navigation_policy` → `on_navigation_policy` (gated on `NAVIGATION_POLICY` cap so plugins without the WIT export skip registration entirely; bridge then returns `None` matching the trait method's early-return); `navigation_action` → `on_navigation_action` (gated on `NAVIGATION_ACTION`; closure returns the raw `ActionResult`, bridge collapses `Pass` → `None`); `contribute_overlay_with_ctx` → `on_overlay` (preserves the `plugin_id: PluginId(String::new())` placeholder the trait emitted); `intercept_buffer_edit` → `on_buffer_edit_intercept` (`BufferEditVerdict::default()` = `PassThrough` is the on-trap fallback). No new HandlerRegistry APIs. |
| β-3.3b.9 — WasmPlugin persistence + workspace via `impl Plugin` | ✅ 2026-05-12 | Two new opaque-bytes HandlerRegistry APIs land alongside the migration: `on_persist_state(Fn(&S) -> Option<Vec<u8>>)` and `on_restore_state(Fn(&S, &[u8]) -> bool)`. PluginBridge gains overrides that read the matching handler-table fields (parallel to the existing `workspace_save` / `workspace_restore` JSON path). WasmPlugin's `persist_state` / `restore_state` migrate to these setters; `surfaces` migrates to `declare_surfaces` (factory queries the WIT export at preflight time, matching the trait method's per-call shape). `workspace_request` is skipped — `WasmPlugin` does not override the trait default of `None`. |
| β-3.3b.10-11 — WasmPlugin handler-family migrations | pending | Remaining per-family migrations (process tasks + pubsub + lens, static metadata + cleanup) populating the same `register()`. See [`memory/project_phase_beta_3_3b_substaging.md`](../) for the breakdown. |
| β-3.3b.12 — WasmPluginLoader return-type flip | pending | After all handler families land, switch `WasmPluginLoader::load` to return `PluginBridge` and delete `impl PluginBackend for WasmPlugin`. |
| β-3.3c-e — Trait deletion + bridge collapse | pending | Convert `impl PluginBackend for PluginBridge` to inherent methods, collapse `SlotImpl` to a single `PluginBridge` field, delete the `PluginBackend` trait, collapse `bridge.rs` 964→200. Target: ~-2000 LoC total. |
| β-4 — WIT 6.0.0 ABI bump + bundled WASM rebuild | ✅ partial 2026-05-12 (`7cd460a5`) | WIT 6.0.0 lands: `evaluate-extension` export deleted, `HOST_ABI_VERSION` bumped across host + manifest scaffolding, all 13 example + 13 fixture + 6 bundled .wasm artifacts rebuilt against the new SDK, docs updated. WasmPlugin `into_entry()` rewrite is deferred — it is structurally part of β-3.3 (the trait it produces an entry for does not yet have a non-PluginBackend shape) |
| β-5 — Documentation rewrite (`plugin-api.md`, `plugin-development.md`, `migration/0.7-to-0.8.md`) | pending | `.claude/rules/plugin-docs.md` sweep at ABI ship time |

Phase α/β supersedes ADR-046 W1-A through W1-F as the implementation
shape (ADR-046's wave structure is preserved; only the W1-C scope
escalates from narrowing to deletion). Total wall-clock estimate:
**17-23 days** (Salsa B retire eliminated from the original
plan once ADR-047 closed; crate-split deferred to a future program).

Deferred to a future ADR (each blocked on a design decision, an upstream Plugin-API change, or a baseline measurement that has to land in its own PR):

- [ADR-045](decisions.md#adr-045-retire-the-extension-point-dispatch-path): Retire `extension-point` API — **partially landed** (commit `cbf17f4c`). Rust dispatch deleted (-575 LoC); the WIT `evaluate-extension` guest export still ships in `kasane:plugin@5.0.0`. **Scheduled for Phase α-1 completion** (subsumes ADR-046 F-1b).
- [ADR-046](decisions.md#adr-046-wit-abi-600--batched-retirement): WIT ABI 6.0.0 — Batched Retirement — **proposed (draft)**. **Superseded in shape by ADR-048** (Phase β escalates W1-C from narrowing to deletion). The two-wave batched-retirement structure is preserved; Wave 2 atomic PR is now Phase β-4.
- Salsa-input annotation `Arc<Vec<…>>` interning (host-side `.clone()` → `Arc::clone()` requires changing `AnnotationResult` field types, which is the plugin-facing surface).
- `#[deprecated]` lifecycle scaffolding (`on_init` / `on_session_ready` / `on_state_changed` / `on_io_event` / `on_process_task` / `on_process_task_streaming` / `on_update`) — sole live consumer is the `#[kasane::plugin]` proc-macro, which cannot emit tier-typed setters without compile-time return-type inference; needs a coordinated macro / handler-registry redesign.
- `PluginBackend` proc-macro generation (the R2.x P8 "1900 → ~700 LoC bridge.rs" follow-up).
- The verification log itself is the ground truth: of 9 LLM-shortlisted candidates, 3 were correct (EffectFootprint, ResolvedSlot, PreDispatchResult), 6 were rejected after grep (RecoveryWitness, depth_stencil scope, migration §8.3 gap, Extension Points "dead code", transform-parser duplication, plugin-model absorption) — the verify-before-cut rule is the load-bearing discipline.

### 2.2 Backlog

| Workstream | Notes |
|---|---|
| External plugin candidates | indent guides, clickable links, built-in splits, floating panels, code folding, display-line navigation, URL detection, region-specific text policy, etc. |
| Plugin ABI 4.0+4.1 — fully landed (2026-05-11) | From the sprout dogfooding tracker (Issue #81). [ADR-041](./decisions.md#adr-041-eval-command-in-session-ready-command) **Decided** (`dd2fbe3a`): `eval-command(string)` added to `session-ready-command`; ABI 3.0.0 → 4.0.0. [ADR-042](./decisions.md#adr-042-command-error-event-via-info_show-marker-attribution) **Decided** (`178eeedd` Phase A + `858581db` Step 1 + `cfc13952` Step 2 + `4eb241ca` Step 3): `command-error` record + `on-command-error-effects` export + host-side marker recognition + `[handlers] command_error_observability` opt-in for auto-wrap; ABI 4.0.0 → 4.1.0. All bundled / fixture / example WASM rebuilt against `kasane:plugin@4.1.0`. |
| Composable Lenses | **Complete (2026-05-04)** — `kasane_core::lens` with `Lens` trait, `LensId`, `LensRegistry`; opt-in `CacheStrategy::{None, PerBuffer, PerLine}` (cache module hashes once per frame for `PerBuffer`, per-line for `PerLine`; bundled lenses opt in to `PerLine` with optimised `display_line` overrides). WIT surface (`lens-declaration` + `lens-cache-strategy` + `declare-lenses` / `lens-display` / `lens-display-line` exports): WASM plugins declare lenses via the manifest-style `declare-lenses` export; the host's `WasmPlugin::register_lenses_into(registry)` iterates declarations and registers `WasmLensAdapter` instances. **Auto-wired lifecycle**: `PluginRuntime::sync_lenses(registry)` drops stale-plugin lens entries and re-registers from each live plugin via `PluginBackend::register_lenses` trait method (no-op default; WASM impl wraps the inherent register). Wired into TUI `lib.rs` + `event_handler.rs` and GUI `app/mod.rs` after every initialize / reload — embedders no longer orchestrate per-plugin. Optional follow-up: more bundled example lenses (mixed-indent warning, tab marker, etc.). |
| Session-affine plugin surfaces | Plugin API for declaring session affinity on `surfaces()` return values. No consumer exists yet; deferred until a plugin requires it |
| Element ↔ §2.6 P(X) synchronisation regression test | Mechanise the §15.1 sync obligation between `Element` variants and the polynomial functor P(X) in semantics §2.6, so variant additions force a semantics update. See semantics §13.16 |
| Semantic Zoom Phase 3 | Per-pane zoom (requires plugin instance state) |
| Semantic Zoom Phase 4 | WIT extension (WASM plugins define custom zoom strategies) |
| Semantic Zoom Phase 5 | Level 5 MAP (module dependency graph display) |
| GPU hardware stencil clipping | Activate the existing `depth_stencil.rs` infrastructure (stencil_write_increment / stencil_write_decrement). Defer until a UI feature requires non-rectangular clipping (e.g. rounded `Container` border radius) |
| Vello GPU rendering re-evaluation (ADR-032) | Spike + trait abstraction + golden image tests. External triggers for re-opening: (a) Vello ≥ 1.0 stable release, (b) Glifo published to crates.io ≥ 0.2, (c) spike `frame_warm_24_lines` ≤ 70 µs at 80×24. ADR-032 in [decisions.md](./decisions.md). The `GpuBackend` trait and `GpuPrimitive::Path` variant are landed *independently* of any adoption decision (decision-grade artefacts). **W2 progress (2026-05-01)**: ADR-032 augmented with §Non-Spike Decision Factors (7 sub-sections); `FrameTarget` enum + `SceneRenderer::render_to_target` landed in `kasane-gui::gpu::scene_renderer`; `GpuState::surface` is `Option`; `tests/golden_render.rs` drives SceneRenderer headlessly via `FrameTarget::View` (`monochrome_grid` fixture pinned). Per-frame Scene-encode allocation baseline recorded at 583 allocs / 89.7 KB / 27 DrawCommands (80×24, see [performance.md](./performance.md#scene-encoding-allocations-adr-032-w5-input)) — feeds ADR-032 §Spike Measurement Matrix. **2026-05-01 ADR-032 textual amendments** (added by author execution of "Vello adoption next-action plan"): §Spike Measurement Matrix gained 4 rows (incremental warm frame, hybrid CPU strip share, actual LOC retired, adapter LOC introduced); §Decision Gates gained pre-W5 baseline-freeze and W3-closing degradation-policy-spec rows; §Non-Spike Decision Factors expanded from 7 to 9 (parallel-paint future closure, Linebender alignment metric); §Rejected Alternatives expanded from 5 to 9 (Forma, custom compute strip, Glifo-only Mode A1, Glifo-only Mode A2); §Implications gained the dual-stack rule (`WgpuBackend` not deleted until Vello 1.0); §Spike Findings gained a 12-required-fields template + verdict-routing rule. **Baseline freeze active** — see ADR-031 post-closure perf opportunities entry below for the suspended item (3) reopen triggers during the W5 measurement window. |
| ADR-031 post-closure perf opportunities | (1) ✅ `StyledLine` allocation reuse (`StyledLineScratch` threaded through SceneRenderer). Current numbers and the host-normalised re-measurement record live in [performance.md](./performance.md). (2) `atom_styles: Vec<Arc<Style>>` — **rejected**. Per-line `atom_styles` is built fresh from interned `Atom.style: Arc<UnresolvedStyle>` (the B-wide intern point); post-resolve `Arc` would only deduplicate when two atoms across different lines produce identical resolved `Style`, and the StyleRun merger in `styled_line.rs:160-181` already collapses identical-style adjacency within a line. Reopen only if profiling shows post-resolve `Style::clone` as a hot allocation. (3) Sub-line word/cluster shape cache — the only structural lever against per-L1-miss shape cost. SLO has 3.5× headroom; deliberately deferred. **Reopen triggers** (any one suffices): (a) `parley_pipeline/one_line_changed` exceeds the threshold documented in performance.md, (b) an ADR-032 Vello spike confirms the shape stage remains the dominant CPU cost, (c) 200×60 warm exceeds 50 % of the SLO (i.e. linear-scaling assumption breaks). **Frozen during the W5 measurement window** (declared 2026-05-01, cross-referenced from [`decisions.md` ADR-032 §Decision Gates "Pre-W5" row](./decisions.md#adr-032-gpu-rendering-strategy--vello-evaluation-framework)): the (a)/(b)/(c) reopen triggers are *suspended* for the duration of W5 spike preparation and execution so that ADR-032 §Spike Measurement Matrix readings compare against a stable baseline rather than a moving target. The suspension expires automatically when ADR-032 §Spike Findings is finalised with a verdict (Accepted / Deferred / Rejected). If a self-optimisation lands during the suspension window despite this rule, it invalidates pre-self-opt W5 measurements and the matrix must be recomputed against the new baseline. The freeze does not block (1)'s "re-measure `parley_pipeline/one_line_changed`" follow-up (a measurement against the existing baseline, not a baseline-moving change). |
| ADR-031 post-closure visibility tightening | ✅ Closed 2026-05-08 (Plan B execution under R2.x P7+P9). Step (a) prelude-routing (`protocol::wire` submodule, `d2d4384`), step (c) Style migration of all production sites, and `Atom::from_wire` demotion to `pub(crate)` all landed across 8 PRs. `WireFace` is now `#[doc(hidden)] pub` and removed from `plugin_prelude`; plugins observe `final_*` flags via `UnresolvedStyle` instead. The remaining external `WireFace` consumers are JSON wire-format encoders in benches (`kasane-tui benches/backend.rs`) and WIT round-trip tests (`kasane-wasm convert/tests`) — these legitimately mirror the on-the-wire JSON layout. Step (b) (full `pub(in crate::protocol)` visibility downgrade) is not pursued: it would require either moving JSON helpers into `kasane-core::test_support` (cross-crate refactor) or duplicating the struct in those crates — neither is justified given `#[doc(hidden)]` already hides `WireFace` from the rendered API surface |
| ADR-031 Phase 10 pixel goldens | Subpixel positioning (4-step quantisation), variable font axes, rich underlines (curly/dotted/dashed/double), and InlineBox text flow — all landed as features in Phase 10 with unit-level coverage at `shaper.rs` / `layout_cache.rs` / `styled_line.rs`, but no GPU pixel snapshot pins the final rendered output. Originally deferred under ADR-032 W2 per the `tests/golden_grid.rs:14-22` rationale (W2's `SceneRenderer::render_inner` surface-decoupling is the prerequisite refactor). **Tracked separately here** so this work is not gated on the Vello triggers (a/b/c above) — the goldens themselves are valuable regardless of Vello adoption. Path forward: (a) ✅ `SceneRenderer::render_inner` decoupled via `FrameTarget` enum (2026-05-01); `tests/golden_render.rs` drives SceneRenderer headlessly with the `monochrome_grid` smoke fixture pinned. (b) Add Phase 10 feature snapshots (subpixel / variable font / curly underline / InlineBox / RTL) — each follows the `monochrome_grid` template, requires a GPU environment for first-run snapshot bootstrap (`KASANE_GOLDEN_UPDATE=1`) |
| ~~Plugin authoring path consolidation (ADR-038)~~ | **Superseded** by ADR-039 (2026-05-08) — see §2.1 "Plugin Path Consolidation (R2.x)" entry. ADR-038's freeze rested on the unverified premise that `capability_traits.rs` had narrow-trait consumers; a workspace grep returned zero. R2.x reverses the freeze and executes the consolidation. |

## 3. Completed Workstreams

### 3.1 Display transformation (P-032)

ADR-030 Levels 1–6 complete. The formal observed/policy separation workstream is finished. For level-by-level details, see [decisions.md ADR-030](./decisions.md).

### 3.2 Semantic Zoom (Phases 0–2)

Complete. The `kasane.semantic-zoom` structural projection provides 6 zoom levels (0 Raw → 4 Skeleton) via `DisplayDirective`s generated through the display pipeline. Two strategy paths:

- **Indent-based fallback** (`indent_strategy.rs`): works on viewport lines using leading whitespace heuristics. No external dependencies.
- **Syntax-aware** (`syntax_strategy.rs` + `kasane-syntax` crate): uses tree-sitter via `SyntaxProvider` trait. Feature-gated via `--features syntax`. Bundled declaration queries for Rust, Python, Go, TypeScript.

## 4. Phase Status Summary

All implementation phases are complete.

| Phase | Primary objective | Notes |
|---|---|---|
| Phase 0 | Development environment and CI foundation | project bootstrap |
| Phase 1 | MVP (TUI core features + declarative UI foundation) | Element + TEA + basic slots |
| Phase 2 | Enhanced floating windows + plugin foundation | |
| Phase 3 | Input, clipboard, and scroll enhancements | |
| Phase G | GUI backend | DecorationPipeline, image element GPU pipeline + texture cache, line-shaping cache, glyph atlas grow via copy_texture_to_texture, sRGB color pipeline correction |
| Phase W | WASM plugin runtime foundation | plugin manifest, settings API, precompiled cache |
| Phase 4 | Shared Plugin API validation | Proof artifacts for public extension points |
| Phase 5 | Surface / Workspace / multi-pane foundation | Session/surface, multi-pane split/focus/routing, pane layout persistence |
| Phase P | Plugin I/O foundation | P-1 / P-2 / P-3 |
| Plugin Redesign | HandlerRegistry, ElementPatch, annotation decomposition, per-plugin invalidation, pub/sub, extension points, WASM capability inference, proc macro v2 | ADR-025 through ADR-029 |

## 5. Items Separated to Upstream Dependencies

The following items are not tracked in this roadmap; [upstream-dependencies.md](./upstream-dependencies.md) is the source of truth.

- D-001: Startup info retention
- D-002: Auxiliary display for off-screen cursors / selections
- P-001: Overlay composition (full version)
- P-010 / P-011: Supplemental area contributions (full version)
- D-004: Completeness of right-side navigation UI

## 6. Update Rules

This document is updated when:

- Priorities among `Now` / `Backlog` change
- Deliverables or completion criteria for an open workstream change
- A workstream completes and moves to §3
- The source of truth for the tracker is moved to another document

## 7. Related Documents

- [upstream-dependencies.md](./upstream-dependencies.md) — Upstream blockers
- [semantics.md](./semantics.md) — Current semantics authority (referenced by backlog entries for gap identifiers)
- [plugin-api.md](./plugin-api.md) — Current API from a plugin's perspective
- [plugin-development.md](./plugin-development.md) — Practical guide for plugin authoring
- [performance.md](./performance.md) — Performance implementation progress
