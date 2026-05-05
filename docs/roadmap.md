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
| 11 — perf tune | ✅ Closed (ADR-024 acceptance) | Case A (`StyledLine` hash memoize, 2026-04-29) landed: warm 64.9 µs ✓, one_line_changed 83.8 µs (+19.7 %). Mutex-on-`StyleStore` hypothesis refuted. Post-closure (`feat/parley-color-emoji-test`): warm 63.3 µs, one_line_changed ~83 µs. The +18 % `one_line_changed` gap is structurally bounded by `shape_warm = 13.58 µs` per L1 miss; the absolute number is well below the 200 µs SLO and the 4.17 ms 240-Hz scanout, so under ADR-024 the gap is formally accepted. Follow-up perf opportunities (StyledLine alloc reuse, sub-line shape cache, atom_styles `Vec<Arc<Style>>`) listed in §2.2 Backlog |
| 12 — Docs + golden image tests | In progress | ADR / CHANGELOG updated; CellGrid `golden_grid` 80×24 ASCII baseline pinned (`a2ca6834`); CJK / cursor / selection golden coverage pending |

Parley pipeline benchmarks (post Phase 11 case A, 2026-04-29):
- `frame_warm_24_lines`: 64.9 µs (−2.3% vs 2026-04-29 pre-memoize, ≤ 70 µs target ✓)
- `frame_one_line_changed_24_lines`: 83.8 µs (typing pattern; +19.7% over target — gap is structurally bounded by `shape_warm` cost, see [performance.md §Parley-only baseline](./performance.md))
- `shape_warm`: 13.58 µs (unchanged — fundamental Parley re-shape cost)
- Core `salsa_scaling/full_frame/80x24`: 49.2 µs (backend-agnostic; unchanged)

Open follow-up debts surfaced during the Phase 5 landing (2026-04-29). Most resolved in the design-δ round:
- ✅ L1 `LayoutCache` test coverage: `bg` / `underline` / `reverse` / `dim` / decoration colour / decoration thickness / strikethrough colour now pinned by negative tests in `layout_cache.rs`.
- ✅ GPU atlas pressure: `glyphs_dropped_atlas_full` counter (`raster_cache.rs:103-107`) reports drops with a once-per-process warn guard; the SLO entry is in `docs/performance.md`.
- ✅ `ResolvedParleyStyle` `italic: bool` + `oblique: bool` — replaced by `SlantKind` enum (`6cc6558c`).
- ✅ `atom_to_wit` (`kasane-wasm/src/convert/mod.rs`) now uses `style_to_wit(&a.style_resolved_default())` directly; the legacy `Style::from_face(&a.face())` round-trip is gone.
- ✅ `text-decoration.thickness` physical-pixel unit — documented in `semantics.md` ("Decoration thickness unit").
- ✅ GPU color pipeline sRGB bypass — documented in `semantics.md` ("Brush colour space").
- Pending: ShadowCursor × InlineBox boundary semantics — landed in `semantics.md` ("InlineBox boundary against ShadowCursor"); a runtime assertion that drops/diagnoses overlap is still on the backlog.

### 2.2 Backlog

| Workstream | Notes |
|---|---|
| External plugin candidates | indent guides, clickable links, built-in splits, floating panels, code folding, display-line navigation, URL detection, region-specific text policy, etc. |
| Composable Lenses | **Complete (2026-05-04)** — `kasane_core::lens` with `Lens` trait, `LensId`, `LensRegistry`; opt-in `CacheStrategy::{None, PerBuffer, PerLine}` (cache module hashes once per frame for `PerBuffer`, per-line for `PerLine`; bundled lenses opt in to `PerLine` with optimised `display_line` overrides). WIT surface (`lens-declaration` + `lens-cache-strategy` + `declare-lenses` / `lens-display` / `lens-display-line` exports): WASM plugins declare lenses via the manifest-style `declare-lenses` export; the host's `WasmPlugin::register_lenses_into(registry)` iterates declarations and registers `WasmLensAdapter` instances. **Auto-wired lifecycle**: `PluginRuntime::sync_lenses(registry)` drops stale-plugin lens entries and re-registers from each live plugin via `PluginBackend::register_lenses` trait method (no-op default; WASM impl wraps the inherent register). Wired into TUI `lib.rs` + `event_handler.rs` and GUI `app/mod.rs` after every initialize / reload — embedders no longer orchestrate per-plugin. Optional follow-up: more bundled example lenses (mixed-indent warning, tab marker, etc.). |
| Session-affine plugin surfaces | Plugin API for declaring session affinity on `surfaces()` return values. No consumer exists yet; deferred until a plugin requires it |
| Element ↔ §2.6 P(X) synchronisation regression test | Mechanise the §15.1 sync obligation between `Element` variants and the polynomial functor P(X) in semantics §2.6, so variant additions force a semantics update. See semantics §13.16 |
| Semantic Zoom Phase 3 | Per-pane zoom (requires plugin instance state) |
| Semantic Zoom Phase 4 | WIT extension (WASM plugins define custom zoom strategies) |
| Semantic Zoom Phase 5 | Level 5 MAP (module dependency graph display) |
| GPU hardware stencil clipping | Activate the existing `depth_stencil.rs` infrastructure (stencil_write_increment / stencil_write_decrement). Defer until a UI feature requires non-rectangular clipping (e.g. rounded `Container` border radius) |
| Vello GPU rendering re-evaluation (ADR-032) | Spike + trait abstraction + golden image tests. External triggers for re-opening: (a) Vello ≥ 1.0 stable release, (b) Glifo published to crates.io ≥ 0.2, (c) spike `frame_warm_24_lines` ≤ 70 µs at 80×24. ADR-032 in [decisions.md](./decisions.md). The `GpuBackend` trait and `GpuPrimitive::Path` variant are landed *independently* of any adoption decision (decision-grade artefacts). **W2 progress (2026-05-01)**: ADR-032 augmented with §Non-Spike Decision Factors (7 sub-sections); `FrameTarget` enum + `SceneRenderer::render_to_target` landed in `kasane-gui::gpu::scene_renderer`; `GpuState::surface` is `Option`; `tests/golden_render.rs` drives SceneRenderer headlessly via `FrameTarget::View` (`monochrome_grid` fixture pinned). Per-frame Scene-encode allocation baseline recorded at 583 allocs / 89.7 KB / 27 DrawCommands (80×24, see [performance.md](./performance.md#scene-encoding-allocations-adr-032-w5-input)) — feeds ADR-032 §Spike Measurement Matrix. **2026-05-01 ADR-032 textual amendments** (added by author execution of "Vello adoption next-action plan"): §Spike Measurement Matrix gained 4 rows (incremental warm frame, hybrid CPU strip share, actual LOC retired, adapter LOC introduced); §Decision Gates gained pre-W5 baseline-freeze and W3-closing degradation-policy-spec rows; §Non-Spike Decision Factors expanded from 7 to 9 (parallel-paint future closure, Linebender alignment metric); §Rejected Alternatives expanded from 5 to 9 (Forma, custom compute strip, Glifo-only Mode A1, Glifo-only Mode A2); §Implications gained the dual-stack rule (`WgpuBackend` not deleted until Vello 1.0); §Spike Findings gained a 12-required-fields template + verdict-routing rule. **Baseline freeze active** — see ADR-031 post-closure perf opportunities entry below for the suspended item (3) reopen triggers during the W5 measurement window. |
| ADR-031 post-closure perf opportunities | (1) ✅ `StyledLine` allocation reuse (`StyledLineScratch` threaded through SceneRenderer; landed post-closure, lifted warm 80×24 from 63.3 µs → 56.7 µs). **Follow-up open**: re-measure `parley_pipeline/one_line_changed` against the post-Scratch baseline — the ADR-031 closure recorded 81.6 µs vs 64.4 µs pre-Scratch warm, so the ~7 µs Scratch saving may have already absorbed part of the +18 % gap that closure formally attributed to `shape_warm`. (2) `atom_styles: Vec<Arc<Style>>` — **rejected (2026-05)**. Per-line `atom_styles` is built fresh from interned `Atom.style: Arc<UnresolvedStyle>` (the B-wide intern point); post-resolve `Arc` would only deduplicate when two atoms across different lines produce identical resolved `Style`, and the StyleRun merger in `styled_line.rs:160-181` already collapses identical-style adjacency within a line. Reopen only if profiling shows post-resolve `Style::clone` as a hot allocation. (3) Sub-line word/cluster shape cache — the only structural lever against `shape_warm = 13.58 µs` per L1 miss. SLO has 3.5× headroom (current warm 56.7 µs vs 200 µs); deliberately deferred. **Reopen triggers** (any one suffices): (a) `parley_pipeline/one_line_changed` > 100 µs at 80×24, (b) an ADR-032 Vello spike confirms the shape stage remains the dominant CPU cost, (c) 200×60 warm exceeds 50 % of the 200 µs SLO (i.e. linear-scaling assumption breaks). **Frozen during the W5 measurement window** (declared 2026-05-01, cross-referenced from [`decisions.md` ADR-032 §Decision Gates "Pre-W5" row](./decisions.md#adr-032-gpu-rendering-strategy--vello-evaluation-framework)): the (a)/(b)/(c) reopen triggers are *suspended* for the duration of W5 spike preparation and execution so that ADR-032 §Spike Measurement Matrix readings compare against a stable baseline rather than a moving target. The suspension expires automatically when ADR-032 §Spike Findings is finalised with a verdict (Accepted / Deferred / Rejected). If a self-optimisation lands during the suspension window despite this rule, it invalidates pre-self-opt W5 measurements and the matrix must be recomputed against the new baseline. The freeze does not block (1)'s "re-measure `parley_pipeline/one_line_changed`" follow-up (a measurement against the existing baseline, not a baseline-moving change). |
| ADR-031 post-closure visibility tightening | `WireFace` rename ✅ landed post-closure (workspace-wide sed). `pub(in crate::protocol)` visibility downgrade still **blocked** on the plugin_prelude re-export (`plugin_prelude.rs:50-53`) — native plugins legitimately consume `WireFace` for `detect_cursors`-style wire-aware paths, plus 22 external workspace files (kasane-wasm convert, kasane-gui colors / ime / diagnostics, kasane-tui diagnostics, kasane builtins, benches and macro tests) hold `WireFace` directly. Path forward: (a) introduce `kasane_core::protocol::wire` submodule with `pub(crate)` `WireFace` and explicit `pub use` of just the wire-helper-friendly subset, (b) migrate all 22 external sites + the prelude consumer set to `Style`, (c) downgrade. `#[doc(hidden)]` already keeps `WireFace` invisible from the rendered API surface in the meantime |
| ADR-031 Phase 10 pixel goldens | Subpixel positioning (4-step quantisation), variable font axes, rich underlines (curly/dotted/dashed/double), and InlineBox text flow — all landed as features in Phase 10 with unit-level coverage at `shaper.rs` / `layout_cache.rs` / `styled_line.rs`, but no GPU pixel snapshot pins the final rendered output. Originally deferred under ADR-032 W2 per the `tests/golden_grid.rs:14-22` rationale (W2's `SceneRenderer::render_inner` surface-decoupling is the prerequisite refactor). **Tracked separately here** so this work is not gated on the Vello triggers (a/b/c above) — the goldens themselves are valuable regardless of Vello adoption. Path forward: (a) ✅ `SceneRenderer::render_inner` decoupled via `FrameTarget` enum (2026-05-01); `tests/golden_render.rs` drives SceneRenderer headlessly with the `monochrome_grid` smoke fixture pinned. (b) Add Phase 10 feature snapshots (subpixel / variable font / curly underline / InlineBox / RTL) — each follows the `monochrome_grid` template, requires a GPU environment for first-run snapshot bootstrap (`KASANE_GOLDEN_UPDATE=1`) |
| Plugin authoring path consolidation (ADR-038) | Governance entry: `Plugin` + `HandlerRegistry` (ADR-025) is the sole authoring path; `PluginBackend` is the internal dispatch ABI consumed by `PluginRuntime` and `WasmPlugin`. R1.7+ capability-trait migration is frozen. New extension points are added via `HandlerRegistry::on_X(...)` registrations, not via new `PluginBackend` methods. Built-in plugins (`BuiltinDragPlugin`, `BuiltinFoldPlugin`, `BuiltinMouseFallbackPlugin`, `BuiltinInputPlugin`, `BuiltinShadowCursorPlugin`, `BuiltinMenuPlugin`, `BuiltinInfoPlugin`, `BuiltinDiagnosticsPlugin`, `ProjectionStatusPlugin`) migrate to `Plugin` + `HandlerRegistry` opportunistically when adjacent work touches them — no coordinated push. WASM adapter (`WasmPlugin`) keeps `impl PluginBackend` directly. See [ADR-038](./decisions.md#adr-038-plugin-authoring-path-consolidation) for rationale and rejected alternatives. |

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
