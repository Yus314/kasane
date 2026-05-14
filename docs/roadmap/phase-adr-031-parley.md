# Phase ADR-031 — Parley text stack migration

**Closed (2026-04-30).** Parley + swash is the production GPU text stack as of
2026-04-26; the closure cascade (PR-5a..PR-7 on `feat/parley-color-emoji-test`)
retired the public Face↔Style bridges, bumped the WIT contract to 2.0.0 with
Style-native function names, and rebuilt all bundled / fixture WASM. Phase 11
perf-tune is closed under ADR-024 acceptance (see [roadmap.md §3 Completed
Workstreams](../roadmap.md#3-completed-workstreams) below); follow-up perf
opportunities are in [roadmap.md §2.2 Backlog](../roadmap.md#22-backlog).

| Phase | Status | Notes |
|---|---|---|
| 0 — Baseline + ADR | ✅ | `baselines/pre-parley.tar.gz`; ADR-031 in [decisions.md](../decisions.md). 80×24 baseline = 53.13 µs |
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
| 11 — perf tune | ✅ Closed (ADR-024 acceptance) | Case A (`StyledLine` hash memoize) landed. Mutex-on-`StyleStore` hypothesis refuted. The `one_line_changed` gap is structurally bounded by L1-miss shape cost; absolute numbers well below the 200 µs SLO, so under ADR-024 the gap is formally accepted. Current numbers and SLO in [performance.md](../performance.md). Follow-up opportunities in [roadmap.md §2.2 Backlog](../roadmap.md#22-backlog) |
| 12 — Docs + golden image tests | Substantially complete — **1.0 critical path** (per 0.6.0 admission decision 2026-05-06) | ADR / CHANGELOG updated. CellGrid `golden_grid` (kasane-core, 10 snapshots, all green): ASCII baseline `a2ca6834`, CJK / combining / emoji `79051bb8`, cursor positioning `7c79d42a`, selection `74dcd1a0`. GUI pixel goldens (kasane-gui `golden_render`, 8 passing + 3 ignored): bootstrap `45e9ae42` on Apple M1 / macOS 26.3 / wgpu-hal 29.0.2 (reference-machine policy in `efcbb6ae`); harness landed via `94869b35` (`FrameTarget`). Remaining 3 ignored fixtures are tracked follow-ups, not Phase 12 blockers: (a) `cjk_cluster_double_width` — CI runner variance, regenerate locally with `KASANE_GOLDEN_UPDATE=1`; (b) `font_fallback_chain` — pending `render_scene_to_image` `FontConfig` override; (c) `variable_font_axes` — `Style.font_weight` not on public surface yet (ADR-031 Phase 10 Step C). Excluded from the 0.6.0 admission criteria — completion is a 1.0 prerequisite, not a 0.6.x patch driver |

Parley pipeline benchmarks: see [performance.md](../performance.md) for the
current numbers and SLO targets. (`frame_warm_24_lines`,
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
