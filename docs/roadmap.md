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

**ADR-031 Parley text stack migration** — Parley + swash is the production
GPU text stack as of 2026-04-26. cosmic-text and the glyphon-derived
`text_pipeline` are gone; the `KASANE_TEXT_BACKEND` opt-in is removed.
Several follow-up phases remain pending — they cover the protocol-side
`Style` rollout and WIT/plugin ABI work, which run independently of the
GPU swap.

| Phase | Status | Notes |
|---|---|---|
| 0 — Baseline + ADR | ✅ | `baselines/pre-parley.tar.gz`; ADR-031 in [decisions.md](./decisions.md). 80×24 baseline = 53.13 µs |
| 1a — Style + Brush types | ✅ | Coexists with `Face`; `Atom::style()` bridge |
| 1b–d — `Atom { face, contents }` migration | Pending | 58 files × 468 occurrences cascade |
| 2 — kasane-core type migration | Pending | Depends on 1b–d |
| 3 — TUI `TerminalStyle` | Pending | Depends on 2 |
| 4 — WIT plugin ABI redesign | Pending | Independent of 1–3; cascades to 5 |
| 5 — Bundled WASM plugins rebuild | Pending | 10 plugins + native examples |
| 6 — `parley_text` facade + cargo deps | ✅ | parley 0.9 + swash 0.2.7 |
| 7 — Parley shaper + L1 `LayoutCache` | ✅ | `Arc<ParleyLayout>`, content/style/font_size key |
| 8 — swash rasteriser + L2/L3 caches | ✅ | LRU + etagere atlas, mask + color split |
| 9 — `SceneRenderer` Parley path | ✅ | All four DrawCommand text variants routed through Parley |
| 9b Step 4c — L2 cache refactor + frame-epoch eviction | ✅ | Same-frame entries protected from eviction |
| 10 — Rich underlines (font metrics) | ✅ | `RunMetrics::underline_offset/size` drives quad geometry |
| 10 — RTL hit_test, InlineBox, Variable font | Pending | Glyph-accurate paragraph hit_test wires next |
| 11 — cosmic-text removal | ✅ | ~1900 LOC dropped; deps gone |
| 11 — perf tune | Pending | Re-baseline pending; target: ≤ 70 µs warm 80×24 frame |
| 12 — Docs + golden image tests | In progress | ADR / CHANGELOG updated; goldens pending |

Parley pipeline benchmarks (post-Phase-11 baseline, 2026-04-26):
- `frame_warm_24_lines`: 63.8 µs (within ≤ 70 µs target)
- `frame_one_line_changed_24_lines`: 83.3 µs (typing pattern; +19% over target — perf-tune candidate)
- Core `salsa_scaling/full_frame/80x24`: 49.2 µs (backend-agnostic; unchanged)

### 2.2 Backlog

| Workstream | Notes |
|---|---|
| External plugin candidates | indent guides, clickable links, built-in splits, floating panels, code folding, display-line navigation, URL detection, region-specific text policy, etc. |
| Session-affine plugin surfaces | Plugin API for declaring session affinity on `surfaces()` return values. No consumer exists yet; deferred until a plugin requires it |
| Element ↔ §2.6 P(X) synchronisation regression test | Mechanise the §15.1 sync obligation between `Element` variants and the polynomial functor P(X) in semantics §2.6, so variant additions force a semantics update. See semantics §13.16 |
| Semantic Zoom Phase 3 | Per-pane zoom (requires plugin instance state) |
| Semantic Zoom Phase 4 | WIT extension (WASM plugins define custom zoom strategies) |
| Semantic Zoom Phase 5 | Level 5 MAP (module dependency graph display) |
| GPU hardware stencil clipping | Activate the existing `depth_stencil.rs` infrastructure (stencil_write_increment / stencil_write_decrement). Defer until a UI feature requires non-rectangular clipping (e.g. rounded `Container` border radius) |
| Vello GPU rendering re-evaluation (ADR-032) | Spike + trait abstraction + golden image tests. External triggers for re-opening: (a) Vello ≥ 1.0 stable release, (b) Glifo published to crates.io ≥ 0.2, (c) spike `frame_warm_24_lines` ≤ 70 µs at 80×24. ADR-032 in [decisions.md](./decisions.md). The `GpuBackend` trait and `GpuPrimitive::Path` variant are landed *independently* of any adoption decision (decision-grade artefacts) |

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
