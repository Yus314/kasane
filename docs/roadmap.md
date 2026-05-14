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

**All in-flight refactor programs are closed.** Per-phase closure notes
live in [`docs/roadmap/`](./roadmap/):

- [phase-adr-031-parley.md](./roadmap/phase-adr-031-parley.md) — Parley
  text stack migration (closed 2026-04-30)
- [phase-r2x-plugin-path.md](./roadmap/phase-r2x-plugin-path.md) — Plugin
  Path Consolidation R2.x (closed 2026-05-10)
- [phase-adr-044-tier-hierarchy.md](./roadmap/phase-adr-044-tier-hierarchy.md)
  — Handler → Effect Tier Hierarchy (closed 2026-05-11)
- [phase-r3x-cleanup.md](./roadmap/phase-r3x-cleanup.md) — R3.x
  admission-criteria cleanup
- [phase-alpha-beta.md](./roadmap/phase-alpha-beta.md) — PluginBackend
  extinction program (closed 2026-05-12)
- [phase-gamma-delta-epsilon.md](./roadmap/phase-gamma-delta-epsilon.md)
  — Structural cascade γ + δ + ε (closed 2026-05-14)

Current bench baseline: `delta-24` (criterion) / `delta_24` (iai). See
`MEMORY.md` or [performance.md](./performance.md) for re-measurement
discipline.

Deferred to a future ADR (each blocked on a design decision, an upstream Plugin-API change, or a baseline measurement that has to land in its own PR):

- [ADR-045](decisions/adr-045-retire-the-extension-point-dispatch-path.md): Retire `extension-point` API — **partially landed** (commit `cbf17f4c`). Rust dispatch deleted (-575 LoC); the WIT `evaluate-extension` guest export still ships in `kasane:plugin@5.0.0`. **Scheduled for Phase α-1 completion** (subsumes ADR-046 F-1b).
- [ADR-046](decisions/adr-046-wit-abi-600-batched-retirement.md): WIT ABI 6.0.0 — Batched Retirement — **proposed (draft)**. **Superseded in shape by ADR-048** (Phase β escalates W1-C from narrowing to deletion). The two-wave batched-retirement structure is preserved; Wave 2 atomic PR is now Phase β-4.
- Salsa-input annotation `Arc<Vec<…>>` interning (host-side `.clone()` → `Arc::clone()` requires changing `AnnotationResult` field types, which is the plugin-facing surface).
- `PluginBackend` proc-macro generation — superseded: `PluginBackend` is extinct after Phase β-3.3d. The remaining handler-dispatch boilerplate (HandlerRegistry setters / HandlerTable erased types / `PluginBridge` dispatch sites — the 4-place manual sync rule) is now scheduled as Phase γ-3 (`#[handler_table]` DSL).

### 2.2 Backlog

| Workstream | Notes |
|---|---|
| External plugin candidates | indent guides, clickable links, built-in splits, floating panels, code folding, display-line navigation, URL detection, region-specific text policy, etc. |
| Plugin ABI 4.0+4.1 — fully landed (2026-05-11) | From the sprout dogfooding tracker (Issue #81). [ADR-041](./decisions/adr-041-eval-command-in-session-ready-command.md) **Decided** (`dd2fbe3a`): `eval-command(string)` added to `session-ready-command`; ABI 3.0.0 → 4.0.0. [ADR-042](./decisions/adr-042-command-error-event-via-infoshow-marker-attribution.md) **Decided** (`178eeedd` Phase A + `858581db` Step 1 + `cfc13952` Step 2 + `4eb241ca` Step 3): `command-error` record + `on-command-error-effects` export + host-side marker recognition + `[handlers] command_error_observability` opt-in for auto-wrap; ABI 4.0.0 → 4.1.0. All bundled / fixture / example WASM rebuilt against `kasane:plugin@4.1.0`. |
| Composable Lenses | **Complete (2026-05-04)** — `kasane_core::lens` with `Lens` trait, `LensId`, `LensRegistry`; opt-in `CacheStrategy::{None, PerBuffer, PerLine}` (cache module hashes once per frame for `PerBuffer`, per-line for `PerLine`; bundled lenses opt in to `PerLine` with optimised `display_line` overrides). WIT surface (`lens-declaration` + `lens-cache-strategy` + `declare-lenses` / `lens-display` / `lens-display-line` exports): WASM plugins declare lenses via the manifest-style `declare-lenses` export; the host's `WasmPlugin::register_lenses_into(registry)` iterates declarations and registers `WasmLensAdapter` instances. **Auto-wired lifecycle**: `PluginRuntime::sync_lenses(registry)` drops stale-plugin lens entries and re-registers from each live plugin via `PluginBackend::register_lenses` trait method (no-op default; WASM impl wraps the inherent register). Wired into TUI `lib.rs` + `event_handler.rs` and GUI `app/mod.rs` after every initialize / reload — embedders no longer orchestrate per-plugin. Optional follow-up: more bundled example lenses (mixed-indent warning, tab marker, etc.). |
| Session-affine plugin surfaces | Plugin API for declaring session affinity on `surfaces()` return values. No consumer exists yet; deferred until a plugin requires it |
| Element ↔ §2.6 P(X) synchronisation regression test | Mechanise the §15.1 sync obligation between `Element` variants and the polynomial functor P(X) in semantics §2.6, so variant additions force a semantics update. See semantics §13.16 |
| Semantic Zoom Phase 3 | Per-pane zoom (requires plugin instance state) |
| Semantic Zoom Phase 4 | WIT extension (WASM plugins define custom zoom strategies) |
| Semantic Zoom Phase 5 | Level 5 MAP (module dependency graph display) |
| GPU hardware stencil clipping | Activate the existing `depth_stencil.rs` infrastructure (stencil_write_increment / stencil_write_decrement). Defer until a UI feature requires non-rectangular clipping (e.g. rounded `Container` border radius) |
| Vello GPU rendering re-evaluation (ADR-032) | Spike + trait abstraction + golden image tests. External triggers for re-opening: (a) Vello ≥ 1.0 stable release, (b) Glifo published to crates.io ≥ 0.2, (c) spike `frame_warm_24_lines` ≤ 70 µs at 80×24. ADR-032 in [decisions.md](./decisions.md). The `GpuBackend` trait and `GpuPrimitive::Path` variant are landed *independently* of any adoption decision (decision-grade artefacts). **W2 progress (2026-05-01)**: ADR-032 augmented with §Non-Spike Decision Factors (7 sub-sections); `FrameTarget` enum + `SceneRenderer::render_to_target` landed in `kasane-gui::gpu::scene_renderer`; `GpuState::surface` is `Option`; `tests/golden_render.rs` drives SceneRenderer headlessly via `FrameTarget::View` (`monochrome_grid` fixture pinned). Per-frame Scene-encode allocation baseline recorded at 583 allocs / 89.7 KB / 27 DrawCommands (80×24, see [performance.md](./performance.md#scene-encoding-allocations-adr-032-w5-input)) — feeds ADR-032 §Spike Measurement Matrix. **2026-05-01 ADR-032 textual amendments** (added by author execution of "Vello adoption next-action plan"): §Spike Measurement Matrix gained 4 rows (incremental warm frame, hybrid CPU strip share, actual LOC retired, adapter LOC introduced); §Decision Gates gained pre-W5 baseline-freeze and W3-closing degradation-policy-spec rows; §Non-Spike Decision Factors expanded from 7 to 9 (parallel-paint future closure, Linebender alignment metric); §Rejected Alternatives expanded from 5 to 9 (Forma, custom compute strip, Glifo-only Mode A1, Glifo-only Mode A2); §Implications gained the dual-stack rule (`WgpuBackend` not deleted until Vello 1.0); §Spike Findings gained a 12-required-fields template + verdict-routing rule. **Baseline freeze active** — see ADR-031 post-closure perf opportunities entry below for the suspended item (3) reopen triggers during the W5 measurement window. |
| ADR-031 post-closure perf opportunities | (1) ✅ `StyledLine` allocation reuse (`StyledLineScratch` threaded through SceneRenderer). Current numbers and the host-normalised re-measurement record live in [performance.md](./performance.md). (2) `atom_styles: Vec<Arc<Style>>` — **rejected**. Per-line `atom_styles` is built fresh from interned `Atom.style: Arc<UnresolvedStyle>` (the B-wide intern point); post-resolve `Arc` would only deduplicate when two atoms across different lines produce identical resolved `Style`, and the StyleRun merger in `styled_line.rs:160-181` already collapses identical-style adjacency within a line. Reopen only if profiling shows post-resolve `Style::clone` as a hot allocation. (3) Sub-line word/cluster shape cache — the only structural lever against per-L1-miss shape cost. SLO has 3.5× headroom; deliberately deferred. **Reopen triggers** (any one suffices): (a) `parley_pipeline/one_line_changed` exceeds the threshold documented in performance.md, (b) an ADR-032 Vello spike confirms the shape stage remains the dominant CPU cost, (c) 200×60 warm exceeds 50 % of the SLO (i.e. linear-scaling assumption breaks). **Frozen during the W5 measurement window** (declared 2026-05-01, cross-referenced from [`decisions.md` ADR-032 §Decision Gates "Pre-W5" row](./decisions/adr-032-gpu-rendering-strategy-vello-evaluation-framework.md)): the (a)/(b)/(c) reopen triggers are *suspended* for the duration of W5 spike preparation and execution so that ADR-032 §Spike Measurement Matrix readings compare against a stable baseline rather than a moving target. The suspension expires automatically when ADR-032 §Spike Findings is finalised with a verdict (Accepted / Deferred / Rejected). If a self-optimisation lands during the suspension window despite this rule, it invalidates pre-self-opt W5 measurements and the matrix must be recomputed against the new baseline. The freeze does not block (1)'s "re-measure `parley_pipeline/one_line_changed`" follow-up (a measurement against the existing baseline, not a baseline-moving change). |
| ADR-031 post-closure visibility tightening | ✅ Closed 2026-05-08 (Plan B execution under R2.x P7+P9). Step (a) prelude-routing (`protocol::wire` submodule, `d2d4384`), step (c) Style migration of all production sites, and `Atom::from_wire` demotion to `pub(crate)` all landed across 8 PRs. `WireFace` is now `#[doc(hidden)] pub` and removed from `plugin_prelude`; plugins observe `final_*` flags via `UnresolvedStyle` instead. The remaining external `WireFace` consumers are JSON wire-format encoders in benches (`kasane-tui benches/backend.rs`) and WIT round-trip tests (`kasane-wasm convert/tests`) — these legitimately mirror the on-the-wire JSON layout. Step (b) (full `pub(in crate::protocol)` visibility downgrade) is not pursued: it would require either moving JSON helpers into `kasane-core::test_support` (cross-crate refactor) or duplicating the struct in those crates — neither is justified given `#[doc(hidden)]` already hides `WireFace` from the rendered API surface |
| ADR-031 Phase 10 pixel goldens | Subpixel positioning (4-step quantisation), variable font axes, rich underlines (curly/dotted/dashed/double), and InlineBox text flow — all landed as features in Phase 10 with unit-level coverage at `shaper.rs` / `layout_cache.rs` / `styled_line.rs`, but no GPU pixel snapshot pins the final rendered output. Originally deferred under ADR-032 W2 per the `tests/golden_grid.rs:14-22` rationale (W2's `SceneRenderer::render_inner` surface-decoupling is the prerequisite refactor). **Tracked separately here** so this work is not gated on the Vello triggers (a/b/c above) — the goldens themselves are valuable regardless of Vello adoption. Path forward: (a) ✅ `SceneRenderer::render_inner` decoupled via `FrameTarget` enum (2026-05-01); `tests/golden_render.rs` drives SceneRenderer headlessly with the `monochrome_grid` smoke fixture pinned. (b) Add Phase 10 feature snapshots (subpixel / variable font / curly underline / InlineBox / RTL) — each follows the `monochrome_grid` template, requires a GPU environment for first-run snapshot bootstrap (`KASANE_GOLDEN_UPDATE=1`) |
| ~~Plugin authoring path consolidation (ADR-038)~~ | **Superseded** by ADR-039 (2026-05-08) — see [phase-r2x-plugin-path.md](./roadmap/phase-r2x-plugin-path.md) for the executed program. ADR-038's freeze rested on the unverified premise that `capability_traits.rs` had narrow-trait consumers; a workspace grep returned zero. R2.x reverses the freeze and executes the consolidation. |
| WIT 2.x text-metrics bundle | Deferred surface for per-cluster advance queries, per-cluster letter-spacing writeback, face-id parameters, font-variation metadata, and bidi overrides. Originally proposed as `get-advance-em` family in #105; deferred because the host's layout pipeline is cell-grid (unicode-width) — exposing Parley cluster advances to plugins without first migrating the layout pipeline yields coordinate data plugins cannot reconcile with the host's column model. Gated on a precursor "Parley-accurate layout" ADR. Cell-grid alternatives ship today via [#108](https://github.com/Yus314/kasane/issues/108) `get-display-cells` and [#109](https://github.com/Yus314/kasane/issues/109) font/cell metrics. Tracked in [`docs/roadmap/wit-2.x-text-metrics.md`](./roadmap/wit-2.x-text-metrics.md). |

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
