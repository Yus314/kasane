# ADR-032: GPU Rendering Strategy — Vello Evaluation Framework

**Status:** Proposed (2026-04-28). This ADR establishes a re-evaluation framework for Vello adoption; it does **not** commit to migration. The decision artefact (§Spike Findings) is filled in by a 5-day timeboxed spike. The current GUI stack (winit + wgpu + Parley + swash; ADR-031) remains the production renderer until and unless this ADR is updated to "Accepted with adoption plan".

**Re-evaluates (does not supersede):** [ADR-014](./adr-014-gui-technology-stack-winit-wgpu-glyphon.md) §14-1's rejection of Vello. ADR-031's closing note "Vello adoption is unblocked, not committed" is the proximate hand-off into this ADR.

### Context

ADR-014 (2024) rejected Vello with three blockers: (i) no glyph cache (vector path rendering every frame), (ii) requires compute shaders, (iii) unstable API (3-5 month break cycles). ADR-031 (2026-04-26) migrated text from cosmic-text to Parley + swash and explicitly left the door open: *"Vello adoption is unblocked, not committed."*

Two of the three ADR-014 blockers have started to soften during 2025-2026 Q1:

1. **Glyph cache.** The `parley_draw` crate has been renamed **Glifo** and moved into the Vello repository, providing atlas-based glyph caching with `render_to_atlas` / `write_to_atlas` APIs. The "vector-path-per-frame" assumption in ADR-014 no longer holds for the canonical Linebender path.
2. **Compute shader requirement.** **Vello Hybrid** (beta as of 2026 Q1) provides a GPU/CPU mixed path that does not require pure compute shaders, expanding hardware support to GPUs that lack robust compute pipelines.
3. **API stability.** Still unresolved. Vello is at 0.8.0 (pre-1.0); Linebender has not announced a 1.0 timeline. This is the remaining ADR-014 blocker as of this writing.

Independently, the cost of *staying* with the hand-rolled wgpu stack is non-trivial: ~11.5 K LOC (Rust + WGSL), 5 RenderPipeline objects, 8 WGSL shaders, a 3-tier glyph cache (~1.5 K LOC), and **zero golden-image regression tests**. Recent activity shows 16 of 25 GPU-layer commits were bug fixes, indicating ongoing maintenance load.

The strategic question is not *"adopt or not"* in isolation but *"when, at what granularity, behind what abstraction"*. This ADR formalises that framing.

### Decision

Run a four-workstream evaluation framework that produces decision-grade information without committing to adoption:

| Workstream | Output | Adoption-conditional? |
|---|---|---|
| **W1** ADR-032 (this document) | Decision framework + §Spike Findings | No (artifact independent of outcome) |
| **W2** Golden image test infrastructure | Visual regression harness for `kasane-gui` | **No** — pays off regardless of Vello outcome |
| **W3** `GpuBackend` trait | Backend-agnostic boundary, with `BackendCapabilities` for negotiation | **No** — pure additive refactor; current `WgpuBackend` is the only impl |
| **W4** Roadmap entry | Decision triggers visible in `roadmap.md` §2.2 | No |
| **W5** `kasane-vello-spike` (5-day timebox) | Performance, parity, memory data for ADR-032 §Spike Findings | Spike crate stays out of `members` if findings are negative |

W1, W2, W4 run from day 1. W3 must precede W5. W5 has hard halt gates (see §Decision Gates).

**Vector path API surface.** During W3 implementation we discovered that `GpuPrimitive` (`kasane-gui/src/gpu/scene_graph.rs`) is *not* wired into the production rendering path — `SceneRenderer` consumes `&[DrawCommand]` (kasane-core) directly, and `GpuPrimitive` is exercised only by unit tests and the dormant `SceneBuilder::from_draw_commands` helper. Adding a `Path` variant to a non-load-bearing enum would not pin any production-relevant API. The decision is therefore to:

1. Land [`BackendCapabilities::supports_paths`](#) (boolean, currently `false` for `WgpuBackend`) as the negotiation surface for callers that may one day emit vector contributions.
2. Defer the actual `DrawCommand::DrawPath` (or equivalent) variant addition to **the adoption work** that follows a positive W5 spike. This avoids introducing dead code in `kasane-core` and avoids colliding with ADR-031 Phases 2–5, which still churn `DrawCommand`-adjacent types.
3. Land `BackendCapabilities::degradation_policy` (enum `Reject | Skip | FallbackToTui`) as the contract for plugin contributions whose primitives exceed the active backend's capability set. This is decision-grade independent of Vello: today the rejection path is undefined, so any future capability-gated primitive (paths, blur, gradients) requires this contract to exist *before* the primitive ships, not after.

   **Visible behaviour for each value** (specified here so the enum is not dead-code semantics): a plugin contribution carrying a primitive that exceeds `BackendCapabilities` is processed as follows.
   - `Reject`: the contribution is dropped and a `PluginDiagnostic { severity: Warning, kind: BackendCapabilityRejected }` is emitted (single fire per (plugin_id, primitive_kind) per session). The frame proceeds without the contribution; no placeholder pixel is rendered. Default for plugins that did not opt into a degradation strategy.
   - `Skip`: identical to `Reject` but suppresses the diagnostic. Reserved for plugins that emit best-effort decoration where silence is the user's preferred outcome (e.g. an inline-image preview that would simply be absent on a TUI build).
   - `FallbackToTui`: the contribution is re-rendered through the TUI translation path of the same primitive. Defined only for primitives with a TUI analogue (path → ASCII frame, gradient → solid centre colour); a primitive without one degrades to `Reject` and emits the diagnostic. Used when a plugin author requires *some* visible artefact in any backend.

   The `degradation_policy` value is read from `BackendCapabilities` per frame; the policy is per-backend, not per-plugin. Plugins that need policy override negotiate it through plugin-emitted hints (out of ADR scope; future ADR addresses if a need surfaces). Note: this is **not** the same contract as ADR-033 §Plugin Failure Semantics, which governs handler panics/traps. ADR-033 fires when the plugin *crashes*; degradation_policy fires when the plugin *succeeds but the backend cannot represent the output*. Both can fire on the same frame for different plugins; they share no state.

### Spike Measurement Matrix

The spike (W5) produces the following data points. Each row has a target and a halt trigger; a halt trigger fires at the day-2 checkpoint (early termination preserves remaining timebox).

**Targets below are calibrated to the 2026-04-29 Linux x86_64 reference host.** When the spike runs on a different machine class, apply the *Reference machine policy* below the table — relative-to-host-baseline interpretation overrides absolute targets where the host is > 1.5× off reference.

| Metric | Target (Linux ref) | Halt trigger (Linux ref) |
|---|---|---|
| 80×24 warm frame | ≤ 70 µs | > 100 µs at Day 2 |
| Cursor-only frame | ≤ 20 µs | > 60 µs |
| Color emoji DSSIM vs swash | ≤ 0.01 | > 0.05 |
| Variable font axis change cost | ≤ 2× swash | > 5× → flag, continue |
| Resident GPU memory | ≤ 1.5× current atlas | > 3× → flag |
| Per-frame CPU heap allocations during Scene encode | ≤ 245 (1.5× of 163 baseline @ 80×24 post-CompactString optimisation, see [performance.md §Scene Encoding Allocations](./performance.md#scene-encoding-allocations-adr-032-w5-input)) | > 489 (3×) → flag |
| Vello + Glifo clean build time | ≤ +60 s | > +180 s → flag |
| 80×24 incremental warm frame (Salsa hit, 1-line-changed) | ≤ 35 µs | > 60 µs → flag (full Scene re-encode penalty exceeds Salsa benefit) |
| Hybrid CPU strip rasterisation share of `total_warm` | ≤ 20 % (durable choice) | > 50 % (stepping-stone — record `vello`-compute Day 4 retry as required) |
| Actual LOC retired from `kasane-gui/src/gpu/` (file-by-file) | ≥ 2,400 (Mode A2 expectation) | < 1,200 → flag (LOC win below ecosystem-coupling cost threshold) |
| Glifo adapter LOC introduced (if Mode A1 path taken) | ≤ 400 | > 800 → flag (LOC win negated; reconsider Glifo-only rejection) |

The 80×24 warm-frame target intentionally matches ADR-031's Phase 11 target (≤ 70 µs) — Vello must clear the same bar Parley + swash already cleared.

#### Reference machine policy (added 2026-05-02)

The absolute µs targets above are calibrated against the **2026-04-29 Linux x86_64 reference host** documented in [`docs/performance.md` §Parley-only baseline](./performance.md). Cross-machine measurement on Apple M1 / macOS 26.3 (recorded in [`docs/performance.md` §Cross-machine baseline](./performance.md)) shows the ratio between hosts is **strongly bench-dependent**: M1 is faster on shape and cold-frame paths, but **3.5–4.4× slower on warm-frame paths** for reasons that appear to be cache-line / LRU sensitivity rather than CPU clock.

This means a host-portable spike interpretation requires both an absolute target *and* a relative-to-baseline rule:

| Spike runs on … | Apply absolute target? | Apply relative rule? |
|---|---|---|
| 2026-04-29 Linux x86_64 reference (or within 1.5× of it on the same bench) | Yes | (subsumed by absolute) |
| Apple M1 / macOS 26.3 (or any host > 1.5× of reference on the *current* bench) | **No** | **`Vello result ≤ 1.2× host's current production stack measurement on the same bench`** |
| Other hosts not yet baselined | Add a `Cross-machine baseline` entry to `performance.md` first; then apply the relative rule | Required |

The relative rule (`≤ 1.2× current production`) tests the *adoption-relevant* question: does Vello regress this host's already-shipping performance? An absolute Linux µs target is meaningless on M1 because the production stack already exceeds it on M1; the spike's task there is to verify Vello does not make it *worse*.

**Cross-machine spike runs are explicitly permitted.** A halt-trigger fire on an absolute row is **not a halt** if (a) the spike host is documented as `> 1.5× of reference`, (b) the relative-rule version of the row passes. The §Spike Findings field 6 (Driver matrix coverage) must record which host the spike ran on so the Findings reader can apply the appropriate rule.

The **incremental warm frame** row pins the Salsa-cache-hit case that Kasane's render pipeline currently exploits (`salsa_views/` + `text/layout_cache.rs` + `text/raster_cache.rs`). Vello's `Scene` is whole-frame re-encoded, so the CPU side cannot benefit from Salsa beyond `query draw_commands(state) -> Vec<DrawCommand>`. If the incremental warm frame measurement is **worse than the warm frame measurement** (i.e. Salsa-hit gives no benefit), Vello adoption flips Kasane's pipeline from incremental to full-rebuild — a regression that the warm-frame-only matrix would not have caught.

The **hybrid CPU strip rasterisation share** row makes the §Hybrid vs compute strategic position decision a recorded measurement, not a post-hoc interpretation. The W5 instrumentation must time `cpu_encode + glifo_atlas` separately from `gpu_submit_latency` so this share is computable. Without this row, "should we move to compute later" becomes an unscoped follow-up.

The **LOC rows** make the §Context "11.5 K LOC + 8 WGSL" framing falsifiable. Realistic Mode A2 retire is ~3,900 LOC of `kasane-gui/src/gpu/` (raster_cache, gpu_atlas, glyph_rasterizer, glyph_emitter, vertex_builder, wgpu_cache, quad_pipeline, image_pipeline, text_effects, WGSL group); LayoutCache (629), styled_line (755), style_resolver (458), shaper (299), hit_test (357), scene_renderer (~2.6K) all *remain*. If retire < 1,200 LOC, the maintenance-cost portion of the adoption case collapses and ecosystem alignment must justify the move on its own.

### Decision Gates

| When | Gate | Action if failed |
|---|---|---|
| W2 Day 3 | Headless wgpu reads back deterministic pixels on CI | Fall back to local-only goldens (`KASANE_GOLDEN=local`); W2 continues |
| W3 Day 2 | `Path` variant doesn't force >50 changed match sites | Move `Path` to a `BackendCapabilities`-gated extension struct |
| W3 closing | `degradation_policy` visible-behaviour for all three values is specified (see §Decision item 3) | Halt W3 land; the field is dead-code semantics without the visible-behaviour table |
| Pre-W5 | Baseline frozen (see [`docs/roadmap.md` §2.2 ADR-032 baseline freeze](./roadmap.md)) — ADR-031 post-closure perf opportunity (3) sub-line shape cache is *not* reopened during the W5 measurement window | If a self-optimisation lands during W5, recompute baseline and restart the matrix; do not interpret W5 against a pre-self-opt baseline |
| W5 Day 2 | Frame ≤ 100 µs **and** Glifo accepts Kasane `font_id` keys | If 100 < frame ≤ 200 µs, reserve Day 4 for a `vello` (compute) retry against the same matrix before final halt — the hybrid-path failure does not entail compute-path failure (see §Non-Spike Decision Factors / Hybrid vs compute strategic position). Otherwise **halt spike**, write findings, re-evaluate in 6 months |
| W5 Day 4 | ≤ 2 matrix rows in red | Write `§Spike Findings — Stop`; exit timebox |
| W5 Day 5 | (regardless) | Finalise `§Spike Findings` — Accepted with adoption plan / Accepted as deferred / Rejected. **No production code change.** |

### Non-Spike Decision Factors

The Spike Measurement Matrix above tests *technical necessity*: can Vello clear the same performance and parity bars as the current stack? It does not test *strategic sufficiency*: should Kasane adopt Vello even when those bars are met. The nine factors below capture the strategic dimension. They are recorded here so the eventual adopt/reject decision can reference them by name regardless of W5 outcome, and so that the spike does not implicitly delegate strategic judgment to a performance number.

#### Plugin wire protocol impact

Vello introduces vector primitives (paths, brushes, strokes) that `kasane:plugin@2.0.0` WIT does not represent. A positive W5 implies `kasane:plugin@3.0.0` with `peniko::Path`-shaped types and a `DrawCommand::DrawPath` variant on the wire, plus recompilation of all bundled (~6) and example (~10) WASM plugins, plus a deprecation cycle for external plugins. The SDK migration path must be co-designed *with* the W5 result, not deferred to after-adoption: the wire-level redesign competes for attention with adoption itself, and undersizing it produces a tail of stabilisation PRs that erodes the adoption-decision rationale.

#### Backend semantic divergence risk

`DrawCommand` is presently a backend-agnostic contract: TUI and GUI both render `DrawBorder` as a "boxed region" with semantically equivalent (if visually different) output — ASCII frame vs pixel border, both communicating the same thing. Vello introduces high-fidelity primitives (true rounded corners, blur, gradient fills) that have no TUI analogue. The choice is binary: either (a) constrain the GUI primitive set to TUI-expressible semantics — limiting Vello's value proposition to "the same picture, antialiased" — or (b) formalise per-backend asymmetry through `BackendCapabilities` and accept that plugin authors must reason about it. This is a *product principle* decision (Kasane has held cross-backend uniformity since ADR-014); it cannot be deferred to post-spike implementation.

#### Salsa compatibility

`kasane-core/src/salsa_sync.rs` and `salsa_views/` invest in incremental computation. Vello's `Scene` is whole-frame re-encoded; it has no `PartialEq` so it cannot be a Salsa query result without bespoke equivalence (which would require freezing Scene's internal layout against future Vello version bumps). The realistic boundary therefore caps Salsa's reach at `query draw_commands(state) -> Vec<DrawCommand>`, with Scene encoding fully recomputed each frame. If the roadmap projects Salsa into the rendering pipeline below the DrawCommand boundary — for instance, "incremental scene patching" as a path to sub-µs partial redraws — that workstream is mutually exclusive with Vello adoption. The mutual exclusion needs to be ratified explicitly, not discovered later.

#### Color management opportunity

`peniko` carries first-class color spaces: sRGB, linear sRGB, display-p3, scRGB, Oklab. Current Kasane is sRGB-only with `colors.rs:srgb_color_to_linear` performing per-frame conversion at GPU upload. On display-p3 native displays — Apple Silicon Macs, the dominant developer hardware in 2026 — sRGB output incurs OS-managed gamut mapping with subtle perceptual-quality loss (saturated highlights desaturate; brand-color hex codes drift). Vello adoption makes display-p3 native rendering a configuration switch instead of a multi-week swap-chain refactor. The W5 spike does *not* measure this — its DSSIM target compares sRGB parity — but it is a non-trivial QoE gain on the dominant developer hardware and a non-zero contributor to the adoption case.

#### Self-optimisation alternative

The current wgpu stack has measurable headroom. Conservative aggregate against `parley_pipeline/frame_warm_24_lines = 56.7 µs` (post-`StyledLineScratch`):

- Persistent vertex buffer + ring allocator: −5 µs
- swash `font_metrics()` cache: −3 µs
- Brush palette intern: −2 µs
- Pipeline state cache (PSO): −2 µs
- Array-texture atlas consolidation (mask + color in one bind group): −3 µs
- Drop-shadow SDF replacement of Kawase Dual-Filter: −10 µs

Aggregate floor: −15 to −25 µs, projecting to ~35 µs warm. Self-optimisation requires no API stability dependency, no plugin SDK bump, and no ecosystem-alignment continuous cost. It should run *concurrently* with W2 (golden harness) and W3 (`GpuBackend` trait); whichever reaches its target first re-frames the W5 evaluation. If self-optimisation lands ~35 µs warm before W5 begins, the W5 target shifts from "match the bar" to "outperform a known low-risk path", which is a materially different decision.

**Concrete attack target (2026-05-01 measurement)**: per-frame Scene-encode allocations originally decomposed into 57 (view) + 29 (place) + 497 (scene walk + DrawCommand emit) = 583 total at 80×24, with the scene walk + emission phase accounting for 85 % of the budget. **First self-optimisation landed (2026-05-01)**: converting `ResolvedAtom.contents` from `String` to `CompactString` eliminated the per-atom heap allocation in `resolve_atoms` and reduced the per-frame total to 163 allocs at 80×24 (−72 %) / 271 at 200×60 (−80 %). Remaining scene walk + emit phase is now 77 allocs (47 %), no longer overwhelmingly dominant. The next-tier targets (annotation/inline_box vec sizing, `Atom.style: Arc<UnresolvedStyle>` clone elision via reference threading, transient Vec in `BufferLineAction` processing) require deeper profiling before attempt; the principle of "self-optimisation alternative is real and measurable" is now confirmed by this first −72 % step. See [performance.md §Scene Encoding Allocations](./performance.md#scene-encoding-allocations-adr-032-w5-input).

#### Linebender engagement operating cost

Adoption establishes a continuous upstream dependency on Vello / Glifo / Parley / peniko, where Linebender's primary consumer is Xilem (general-purpose UI), not cell-grid editors. Estimated continuous cost: 2–4 hours/week of issue-tracker monitoring, occasional PR contributions, and proactive coordination on breaking-change windows. This is a recurring maintenance line item that ADR-014's hand-rolled stack does not carry. Three observable post-adoption signals to track: (a) Glifo issue closure latency for cell-grid-specific reports, (b) Vello breaking-change cadence (semver-minor breaks), (c) responsiveness to feature requests outside Xilem's roadmap. A 6-month post-adoption review is warranted; until that review, treat upstream divergence (Linebender pivots, Glifo deprioritised) as the dominant compounding risk.

#### Hybrid vs compute strategic position

The Decision section selects `vello_hybrid` to neutralise ADR-014's compute-shader blocker. This is correct *for the spike*, but not necessarily *for adoption*. Hybrid trades Vello's principal architectural advantage (compute-driven sparse strip rasterisation across a six-stage pipeline: encode → PathTag scan → flatten → binning → coarse → fine) for hardware reach. Kasane's cell-grid + glyph workload exercises only the *coarse* and *fine* stages meaningfully — flatten and binning are largely idle for axis-aligned rectangles and atlas-blitted glyphs. If the hybrid CPU-side rasterisation penalty is < 20 % at the warm-frame target, hybrid is the *durable* choice and ADR-014's compute-shader blocker stays neutralised forever. If > 50 %, hybrid is a stepping-stone and a second migration to full `vello` (compute) follows within 12–18 months — at which point ADR-014's blocker recurs. **The W5 spike must record which regime applies** so post-adoption "should we move to compute" is not an unscoped follow-up arriving at a worse moment.

#### Parallel-paint future closure

`vello::Scene` is `!Send` (carries `Rc`-internal state) — adoption locks the paint stage to a single thread. Kasane's paint is single-threaded today, but multi-pane configurations (Phase 5 complete) make per-pane parallel paint a natural CPU-scaling axis once paint cost becomes the dominant frame budget. A workaround exists — build one `Scene` per pane and concatenate via `Scene::append` — but the append cost is linear in op count, and serial-append after parallel-build cannot extract GPU-side parallelism. Adoption therefore forecloses an axis the architecture currently has open. This is a *one-way door* in the decision sense: backing out would require either Linebender adding `Send` to `Scene` (low probability — internal Rc usage is intentional) or replacing Vello again. **The adoption decision must explicitly record whether the parallel-paint axis is being closed deliberately or by oversight.** A "deliberate" close is justifiable if the warm-frame target leaves enough headroom that single-threaded paint is not the bottleneck for the next 18+ months at expected pane counts (≤ 4 in current usage); an "oversight" close is recoverable only by re-evaluation under load.

#### Linebender alignment metric

ADR-014's hand-rolled stack carries no upstream dependency. ADR-031 introduced Parley + swash and accepted ~2–4 hr/wk of upstream coordination. Vello + Glifo adoption deepens this. The §Linebender engagement operating cost factor names three observable signals; this subsection makes one of them — *cell-grid issue closure latency* — the load-bearing alignment metric, because it is the only signal that distinguishes "Linebender fixes our problems" from "we file issues that linger".

**Definition.** For each issue Kasane (or a similar cell-grid consumer) files in `linebender/vello`, `linebender/glifo` (when published), or upstream Parley/peniko crates, record `closure_latency = closed_at - opened_at`. Distinguish:

- **CG issue**: cell-grid-specific (atlas eviction policy under monospace pressure, font_id key shape, COLR colour-emoji priority order, sub-pixel quantisation step, hybrid CPU strip cost regression on monospace workloads).
- **Xilem-aligned issue**: general 2D rendering bug that happens to also affect Kasane.

**Threshold.** A 6-month rolling median of `closure_latency(CG)` ≥ 2× `closure_latency(Xilem-aligned)` is grounds for re-evaluation under §Risks (Linebender pivot / Glifo deprioritised). The first measurement window opens at adoption + 3 months (allow upstream onboarding); subsequent windows are quarterly.

This metric is **adoption-conditional**: if ADR-032 closes "Rejected", Linebender alignment is not a recurring measurement. If "Accepted", the metric becomes part of the post-adoption review cadence specified in §Implications.

### Spike Findings

*To be filled in by W5. Do not commit downstream code changes (image-pipeline partial adoption, full migration) until this section is complete and ADR-032 is updated to "Accepted with adoption plan".*

The findings below are **required fields**; missing any field invalidates the spike result and forces a fresh 5-day timebox. Each field is recorded with its raw measurement, the matrix-row target, and a green/yellow/red verdict against the halt trigger. *No interpretation, no narrative-only entries* — interpretive prose belongs in the closing verdict paragraph after all fields are recorded.

#### Required field set

1. **Spike timebox window**: ISO-8601 start / end timestamps. If the spike was paused (Glifo crates.io block, GPU environment block), record paused interval explicitly.
2. **Vello / Glifo / wgpu version pin**: exact crates.io versions or git revs at spike runtime. Glifo `font_id` key shape compatibility verdict (accepts Kasane shape / requires adapter / cannot represent).
3. **Spike Measurement Matrix — every row**: raw value, unit, target, halt-trigger verdict. No row may be skipped; "not measured" requires a stated reason and counts as red against that row.
4. **Hybrid CPU strip vs GPU submit decomposition**: `cpu_encode`, `glifo_atlas`, `gpu_prepare`, `gpu_submit_latency`, `total_warm` (per the `kasane-vello-spike` instrumentation plan in this ADR). Compute and record `cpu_share = (cpu_encode + glifo_atlas) / total_warm`. Classify as **durable** (< 20 %) / **transitional** (20–50 %) / **stepping-stone** (≥ 50 %).
5. **Incremental warm frame measurement**: `frame_warm_one_line_changed` against the same fixture set as full warm. Record the Salsa-hit case explicitly; a measurement worse than the full warm frame is a regression flag.
6. **Driver matrix coverage**: list of (OS, GPU vendor, driver version, wgpu backend) tuples tested. CI-runner status (deterministic / per-tuple snapshot / local-only). DSSIM per tuple. **Host class for absolute-vs-relative target interpretation**: record whether the spike host is within 1.5× of the 2026-04-29 Linux x86_64 reference on `frame_warm_24_lines` (apply absolute targets) or > 1.5× off (apply relative `≤ 1.2× current production` rule per §Reference machine policy). Cite `docs/performance.md` for the host's pre-spike production baseline.
7. **Actual LOC retire vs predicted**: file-by-file table. Predicted (Mode A2): ~3,900 LOC across `text/raster_cache.rs`, `text/gpu_atlas.rs`, `text/glyph_rasterizer.rs`, `text/glyph_emitter.rs`, `text/wgpu_cache.rs`, `text/vertex_builder.rs`, `quad_pipeline.rs`, `image_pipeline.rs`, `text_effects.rs`, WGSL group. Record actual.
8. **Adapter LOC introduced**: count of new code in `kasane-vello-spike/` and any `kasane-gui` adapter modules. Mode A1 path adds ~400–600; Mode A2 ~150 churn. > 800 invalidates the LOC win.
9. **Plugin wire protocol delta**: which existing WIT types must change for `DrawCommand::DrawPath` and `BackendCapabilities::supports_paths` to land. Bundled and example WASM plugins requiring recompile (count). Plugin SDK semver bump required (yes/no, target version).
10. **§Non-Spike Decision Factors verdict per subsection**: nine subsections × { addressed-by-spike-data | unaddressed-strategic-judgment | not-applicable-given-W5-outcome }. Each "addressed-by-spike-data" entry cites the matrix row(s) supporting the verdict.
11. **Linebender response state**: written-inquiry status at spike time (responded / no-response / not-pursued). If responded, the response feeds §Linebender alignment metric establishment; if not pursued, record the reason (the response gating the spike was deliberately skipped per project owner direction; cite the directive).
12. **Closing verdict**: one of `Accepted with adoption plan` / `Accepted as deferred` / `Rejected`. The verdict paragraph is the *only* place for interpretive prose. It must reference each red-verdict row from (3) and explain why the verdict still stands, or which red row caused rejection.

#### Verdict-routing rule

The closing verdict is **mechanically determined** by the field set, not chosen by the author:

- Any halt-trigger red in (3) without an explicit Day-2 retry compensation → `Rejected`.
- All halt-triggers green or yellow, but (10) records ≥ 3 unaddressed strategic factors → `Accepted as deferred` (re-evaluate after the named factors resolve).
- All halt-triggers green or yellow, (10) records ≤ 2 unaddressed factors, (11) is non-blocking → `Accepted with adoption plan`. The adoption plan is [§Adoption Phase Plan](#adoption-phase-plan-conditional-on-positive-spike) (Z0 → Z1 → Z2 → Z3, with Z4 continuous) — already landed in this ADR for sequencing reference. On verdict close, Z0 begins; halt-and-revert exits and the §Implications dual-stack rule govern subsequent phase transitions.

This rule exists so a positive spike does not accidentally adopt under unaddressed strategic concerns, and a borderline spike does not escalate to "Rejected" when its reds are isolated to one row.

### Rejected Alternatives

| Alternative | Reason for rejection |
|---|---|
| Adopt Vello now (full replacement) | API still pre-1.0 (0.8.0); Glifo not yet on crates.io; no measured frame-time data on Kasane's workload. |
| Do nothing until Vello 1.0 | Passive monitoring loses the option value of the trait abstraction and golden tests, both of which pay off independently. Also delays the spike data needed for an informed 1.0-time decision. |
| Add Lyon for vector paths, keep current text stack | Solves only the path-rendering subset; does not address the broader Linebender ecosystem alignment. Adds a third dependency without converging the long-term stack. |
| Fork Glifo into kasane | Premature. Linebender is actively iterating; a fork commits us to maintenance of an upstream-divergent atlas implementation. |
| Partial adoption (images/blur only) without trait or spike | Bypasses the W5 measurement matrix; lacks data to justify the dual-pipeline integration cost. Reconsidered post-spike if W5 findings are positive on a subset. |
| **Forma (Google)** as an alternative 2D GPU renderer | Ostensibly Linebender-independent (hedges against §Risks Linebender pivot), with a simpler 3-stage pipeline closer to Kasane's cell-grid + glyph workload. Rejected: Forma is in maintenance mode as of 2026 Q1, has no glyph cache integration story comparable to Glifo, and adopting it would simply replicate "no ecosystem" with extra steps — solving the dependency question by introducing a less-active dependency. Re-evaluate if Forma sees renewed development *and* publishes a Glifo-equivalent atlas. |
| **Custom compute strip rasteriser** (kasane-internal, ~800–1,200 LOC) | Adds Vello's principal architectural advantage (compute-driven sparse strip rasterisation) to a `DrawPath`-only path while keeping the existing fragment pipeline for text/rect. Zero dependency growth; LOC budget known. Rejected: the maintenance burden of an in-house compute pipeline (WGSL authoring, cross-driver testing, compute-capability negotiation) is not less than the burden being escaped. The decision logic is "Linebender-funded compute is cheaper than self-funded compute" — true while Linebender stays committed, falsified if §Linebender alignment metric degrades. Re-evaluate as the *response to* a Linebender pivot, not as a primary alternative. |
| **Glifo-only adoption, Mode A1** (adapter-overlay; keep `WgpuBackend` text path, swap raster + atlas only via thin adapter) | Replaces `glyph_rasterizer.rs` + `atlas.rs` + `raster_cache.rs` (~1.4 K LOC) with a Glifo-driven equivalent and an adapter that re-exposes Kasane-compatible (`AtlasSlot`, bitmap, `bump_epoch`, `dropped` counter) semantics on top of Glifo's API. Rejected: the adapter LOC (~400–600 estimated) consumes most of the LOC win; the `bump_epoch` same-frame use protection (`raster_cache.rs:90-97`) and CPU-side data retention for device-loss recovery (`raster_cache.rs:79-89`) are Kasane-specific invariants Glifo does not surface; under shipping it forward, the adapter becomes a permanent maintenance liability with no upstream equivalent. Mode A1 is *cosmetic* adoption — small win, long-tail cost. |
| **Glifo-only adoption, Mode A2** (no Vello Scene; replace text path with Glifo + custom wgpu atlas binding) | Replaces ~2,400 LOC across `text/raster_cache.rs`, `text/gpu_atlas.rs`, `text/glyph_rasterizer.rs`, `text/glyph_emitter.rs`, `text/wgpu_cache.rs`, `text/vertex_builder.rs` with Glifo + an in-house WGSL shader that consumes Glifo's atlas. Skips Vello Scene entirely. Rejected for two reasons: (a) Linebender's primary Glifo consumer is the Vello Scene path; standalone consumers are off-roadmap and would become *secondary citizens* in the Linebender issue tracker — `closure_latency(CG)` for Mode A2-specific bugs is expected to exceed 2× the Xilem-aligned baseline within the first 6 months (the §Linebender alignment metric threshold for re-evaluation); (b) Mode A2 yields zero performance improvement (predicted ±3 µs warm) and trades one set of text-pipeline maintenance (current swash-driven) for another (Glifo-shaped, with a non-Linebender consumer's friction). The motivation collapses to "LOC reduction + cache-hierarchy flatten", neither of which clears the §Linebender engagement operating cost threshold. **Re-open trigger**: Linebender publishes a written commitment to Glifo-as-standalone-library (issue tracker prioritisation parity for non-Vello consumers), at which point the alignment metric concern is preemptively resolved and Mode A2 returns to evaluation. |

### Risks and Mitigations

| Risk | Mitigation |
|---|---|
| Vello 0.8 → 0.9 breaks mid-spike | `Cargo.lock` pinned for the spike branch; version bump deferred to a follow-up issue |
| Glifo not yet on crates.io | Git rev-pin in spike `Cargo.toml`; spike branch isolated from main `Cargo.lock` if path resolution fails |
| W3 collides with ADR-031 Phases 2-5 (`scene_graph.rs` churn) | `Path` variant is purely additive; W3 sequenced after the next ADR-031 phase tag |
| Driver-dependent rasterization breaks W2 on CI | Disable MSAA in test target; DSSIM-based comparison absorbs sub-pixel noise; per-(OS, driver) snapshot tuples permitted |
| Spike timebox exceeded | Hard halt at Day 5 regardless of completion; partial findings still feed §Spike Findings |
| User-visible regression (color emoji, BiDi, complex scripts) discovered post-adoption | Spike matrix gates emoji/variable-font parity; complex scripts (Arabic, Devanagari, CJK ligatures) are tested via golden fixtures in W2 before any adoption decision |
| Strategic divergence from Linebender (Parley adopted, Glifo skipped) | This ADR explicitly weighs convergence vs. divergence; a "Rejected" outcome on W5 is recorded as informed divergence, not avoidance |

### Implications

- **No production code changes** flow from this ADR alone. The current `WgpuBackend` (wrapping `SceneRenderer`) remains the sole production renderer.
- **Two artefacts ship regardless of outcome:** golden image regression tests (W2) and the `GpuBackend` trait abstraction (W3). Both close existing gaps in the kasane-gui codebase independent of any future Vello decision.
- **Plugin contribution surface gains two `BackendCapabilities` fields:** `supports_paths` (negotiation) and `degradation_policy` (rejection contract for capability-exceeding contributions). No new enum variant ships in this ADR; the actual `DrawPath` primitive is deferred to adoption work, when it can be added to the live boundary (`DrawCommand` in `kasane-core`) rather than to the dormant `GpuPrimitive`. This keeps the door open without introducing dead code.
- **§Non-Spike Decision Factors is a permanent decision frame, not a spike output.** It is recorded *before* W5 begins so the eventual adopt/reject judgment cites strategic considerations by name. The spike does not delegate strategic judgment to a performance number; it produces necessary-condition data that the strategic frame interprets.
- **ADR-014 §14-1 is *not* superseded by this ADR.** Supersession occurs only if ADR-032 is updated to "Accepted with adoption plan" after a positive spike. Until then, ADR-014's GUI-stack decision (winit + wgpu, with the text portion already updated by ADR-031) remains authoritative.
- **`docs/roadmap.md` §2.2 Backlog gains a tracked item** for Vello 1.0 / Glifo crates.io publication / spike result. These are the externalised triggers for re-opening this ADR.
- **Dual-stack operation rule (post-adoption only).** If ADR-032 closes "Accepted with adoption plan", `WgpuBackend` is **not deleted** until Vello publishes a 1.0 release. The `GpuBackend` trait abstraction (W3) is the load-bearing artefact that makes this rule enforceable: both backends compile, both are reachable via configuration, and the production runtime defaults to the Vello-backed implementation while preserving `WgpuBackend` as a back-out path. The rule expires on Vello 1.0 announcement; until then any "delete WgpuBackend" PR is rejected at review. This rule pays for the §Risks "Vello 0.x → 0.y break mid-migration" and "Linebender pivots / Glifo deprioritised" mitigations by keeping the dual-stack option alive at known compile-time cost.
- **§Spike Findings is a fields-required artefact, not free-form prose.** The 12 required fields (recorded in the §Spike Findings template) gate adoption: the verdict-routing rule mechanically determines `Accepted with adoption plan` / `Accepted as deferred` / `Rejected` from the field state. This prevents a positive-feeling spike from being adopted under unaddressed strategic concerns, and prevents an isolated red row from over-rejecting a borderline-positive spike.

### Adoption Phase Plan (Conditional on Positive Spike)

**Activation condition.** This phase plan applies *only* when ADR-032 closes "Accepted with adoption plan" via §Spike Findings field 12 (closing verdict). On `Accepted as deferred` or `Rejected`, the phase plan is dormant; on a future re-evaluation that flips the verdict to positive, the phase plan re-activates without rewrite.

**Phase identifier.** `Phase Z` — `Z0` … `Z4`. The `Z` prefix is deliberately separate from the alphabetical phase scheme used by ADR-031 (`Phase 0` … `Phase 12`) to make grep/cross-referencing unambiguous in CHANGELOG and commit messages.

**Sequencing rule.** Phases land in `Z0 → Z1 → Z2 → Z3` order. `Z4` runs continuously from `Z0` start. Each phase has a *halt-and-revert* exit: if its closing condition fails, the phase is reverted (single PR), the prior phase's state is restored, and ADR-032 §Spike Findings is reopened with the failing observation appended as a §Risks row. The `WgpuBackend` retirement (Z3) is the only one-way door; until Z3 lands the entire phase plan is reversible.

**Cross-references.** §Implications dual-stack rule governs `WgpuBackend` removal timing across all phases. §Linebender alignment metric measurement window opens at `Z0` start.

#### Phase Z0 — Adoption ABI break preparation

**Duration:** 2–3 weeks elapsed. **Land order:** before Z1.

**Deliverables:**

1. `DrawCommand::DrawPath` variant added to `kasane_core::render::scene::DrawCommand`. The variant carries a `kurbo::BezPath`-shaped path, a `peniko::Brush`, and a stroke option. Translation in `WgpuBackend` returns `BackendError::Unsupported("DrawPath")` (production gates capability via `BackendCapabilities::supports_paths = false`); `VelloBackend` translates to `Scene::fill` / `Scene::stroke`.
2. Plugin SDK WIT bump to `kasane:plugin@3.0.0`. New types: `path` (record of `commands: list<path-command>`), `brush` (variant: `solid | linear-gradient | radial-gradient` — gradient variants gated on `BackendCapabilities::supports_paths`), `stroke` (record of `width`, `caps`, `joins`, `dash-array`).
3. All bundled (~6) and example (~10) WASM plugins recompiled against `kasane:plugin@3.0.0`. The `kasane:plugin@2.0.0` ABI is **not** retained — same single-ABI-break strategy as ADR-031 §Phase 4 closure.
4. `BackendCapabilities::supports_paths` toggled to `true` on `VelloBackend` (was `cfg!(feature = "with-vello")` on the spike crate, now unconditional on the production Vello backend).
5. `BackendCapabilities::degradation_policy` per-frame check wired in `SceneRenderer` and the Vello backend so ADR-032 §Decision item 3 visible behaviour ships with the first capability-gated primitive (paths). Without this wiring the rejection path stays unreachable.
6. `kasane-vello-spike` crate retired; the Vello backend moves from `kasane-vello-spike/` into `kasane-gui/src/gpu/vello_backend/` and joins the workspace `members`.

**Closing condition (halt-and-revert if failed):**

- All bundled + example WASM plugins build green against `kasane:plugin@3.0.0`.
- `cargo test --workspace` green on both backends (TUI, WgpuBackend, VelloBackend).
- `BackendCapabilities::degradation_policy` rejection path emits the `BackendCapabilityRejected` diagnostic and the `tests/golden_render.rs` smoke fixture passes against both backends.

**LOC delta estimate:**

- New: `vello_backend/` ~600 LOC (Scene encoding for non-text DrawCommands per the translation contract paper-design).
- New: WIT 3.0.0 path/brush/stroke types ~150 LOC across `kasane-plugin-sdk/wit/`.
- Retired: 0 (additive phase).

**Back-out trigger:** any of (a) bundled plugin authors block on the `@3.0.0` migration > 1 week, (b) `degradation_policy` wiring exposes invariant violations in the existing scene_renderer code path, (c) `vello_backend/` move surfaces previously-hidden `kasane-gui` ↔ spike-crate coupling.

#### Phase Z1 — Text path migration (Mode A2)

**Duration:** 2–3 weeks elapsed. **Land order:** after Z0 close, before Z2.

**Deliverables:**

1. `text/raster_cache.rs` (~634 LOC), `text/gpu_atlas.rs` (~317 LOC), `text/glyph_rasterizer.rs` (~239 LOC), `text/glyph_emitter.rs` (~226 LOC), `text/wgpu_cache.rs` (~259 LOC), `text/vertex_builder.rs` (~251 LOC), `text/atlas.rs` (~210 LOC), and the per-pipeline WGSL group retired. Replaced by Glifo (`render_to_atlas` / `write_to_atlas`) in `vello_backend/text/`.
2. `text/text_renderer.rs` (~197 LOC) and `text/frame_builder.rs` (~521 LOC) rewritten to consume Glifo's atlas output and emit `Scene::draw_glyphs` calls. Net churn ~150 LOC; the rewritten files stay in `vello_backend/`.
3. `text/layout_cache.rs` (629 LOC), `text/styled_line.rs` (755 LOC), `text/style_resolver.rs` (458 LOC), `text/shaper.rs` (299 LOC), `text/hit_test.rs` (357 LOC) **retained** — these remain backend-agnostic per the §Translation Contract paper-design (Glifo provides glyph-cache, not Parley-Layout-cache).
4. `WgpuBackend` text path **retained, frozen** — no further changes accepted; the implementation is preserved as the back-out target per §Implications dual-stack rule.
5. W2 golden fixtures (`monochrome_grid`, `subpixel_quantisation_4step`, `curly_underline`, `color_emoji_priority`, `inline_box_text_flow`, `rtl_bidi_cursor`, `cjk_cluster_double_width`) pass against the Vello backend with DSSIM ≤ 0.05. The `WgpuBackend` snapshot remains the authoritative reference; Vello DSSIM is measured against it.

**Closing condition (halt-and-revert if failed):**

- All 6 buildable W2 fixtures DSSIM ≤ 0.05 against `WgpuBackend` snapshots.
- `parley_pipeline/frame_warm_24_lines` ≤ 70 µs at 80×24 against the Vello backend.
- `parley_pipeline/frame_warm_one_line_changed` ≤ 60 µs (the §Spike Measurement Matrix incremental-warm halt trigger).
- §Spike Findings field 7 (actual LOC retired) re-measured ≥ 2,400 LOC.

**LOC delta estimate:** −2,400 LOC retired, +200 LOC new (Vello-side text adapter). Net −2,200.

**Back-out trigger:** any closing-condition failure halts and reverts to the post-Z0 baseline. The `vello_backend/` text path is removed in the revert PR; Z0 work is preserved.

#### Phase Z2 — Quad / Image path migration

**Duration:** 1–2 weeks elapsed. **Land order:** after Z1 close, before Z3.

**Deliverables:**

1. `quad_pipeline.rs` (~250 LOC) and `image_pipeline.rs` (~250 LOC) retired in `WgpuBackend`. Replaced by `Scene::fill` (FillRect, DrawBorder interior, DrawShadow without blur, DrawPaddingRow, BeginOverlay) and `Scene::stroke` (DrawBorder outline, BorderTitle decorations) and `Scene::draw_image` (DrawImage) in `vello_backend/`.
2. `compositor/blur.rs` (~258 LOC) **decision recorded in §Spike Findings field 4 (Hybrid CPU strip share)**: if Vello hybrid blur API supports the `DrawShadow` workload at < 20% CPU share regression, retire `compositor/blur.rs`; if 20–50%, retain `compositor/blur.rs` as the fallback for `DrawShadow` only and route via `degradation_policy` per-primitive; if > 50%, treat blur as the trigger for full `vello`-compute migration evaluation (§Hybrid vs compute strategic position).
3. `texture_cache.rs` (~200 LOC) **retained** — image-cache retention is performance load-bearing per the §Translation Contract paper-design (Vello's image API is by-value, so without a cache every frame re-uploads).
4. WGSL shader group reduces from 8 to 0 in `WgpuBackend` text path retired in Z1; the remaining `quad.wgsl` / `image.wgsl` / `text_glow.wgsl` / `text_shadow.wgsl` / `compositor/blit.rs` shaders are evaluated for retirement per the per-primitive policy in #1 above.

**Closing condition (halt-and-revert if failed):**

- All 6 buildable W2 fixtures DSSIM ≤ 0.05 against the Z1 baseline (i.e. Z2 introduces no visual regression vs Z1's Vello-rendered output).
- `WgpuBackend`-side text path is **untouched** by Z2 (verify via `git diff` scope).
- `compositor/blur.rs` retirement decision recorded in CHANGELOG with the §Spike Findings field 4 measurement that justified it.

**LOC delta estimate:** −500 to −750 LOC retired (depending on blur retirement decision). Cumulative since Z0: −2,700 to −2,950 LOC.

**Back-out trigger:** Z2 revert restores `quad_pipeline.rs` / `image_pipeline.rs` / `compositor/blur.rs` to post-Z1 state. `vello_backend/` quad-image work is removed; Z1 text-path migration is preserved.

#### Phase Z3 — `WgpuBackend` retirement

**Duration:** 1 week elapsed. **Land order:** after Z2 close, **gated on §Implications dual-stack rule expiry**.

**Pre-condition (gating, not deliverable):**

- Vello has published a 1.0 stable release. The §Implications dual-stack rule expires on this announcement.
- The Vello backend has run as the production default for ≥ 3 months without halt-and-revert events on Z1 or Z2.
- `closure_latency(CG)` for Linebender-filed cell-grid issues is ≤ 2× `closure_latency(Xilem-aligned)` per §Linebender alignment metric. (If the metric is in red, defer Z3 by 6 months and reopen the metric measurement.)

**Deliverables:**

1. `WgpuBackend` and `kasane-gui/src/gpu/scene_renderer/` retired entirely. ~3,800 LOC removed (the residual after Z1 and Z2 retirements).
2. `GpuBackend` trait **retained** as the abstraction boundary for any future backend evaluation (a hypothetical Forma adoption, or a return to a hand-rolled compute pipeline). The trait now has `VelloBackend` as its sole production impl.
3. ADR-014 §14-1 formally superseded by ADR-032. The decisions.md table-of-status row for ADR-014 transitions to "Superseded by ADR-032".
4. ADR-032 status transitions from "Accepted with adoption plan" to "Accepted (post-Z3)". The phase plan's halt-and-revert exits are formally retired (Z3 is the one-way door).
5. §Spike Findings field 7 (actual LOC retired) re-measured for the final cumulative total. CHANGELOG records the cumulative delta (Z0 → Z3) for the retrospective in §Z4.

**Closing condition (no halt-and-revert — Z3 is one-way):**

- All `WgpuBackend`-bearing imports removed from the workspace (`grep -r WgpuBackend kasane-gui/ | wc -l == 0`).
- `cargo build --workspace` green without `--features with-vello` (since the feature flag is also retired in Z3).
- ADR-014 §14-1 supersession noted in `decisions.md` table-of-status.

**LOC delta estimate:** −3,800 LOC retired. **Cumulative since Z0: −6,500 to −6,750 LOC.** This is the §Spike Findings field 7 final measurement value the spike's "actual LOC retired ≥ 2,400" target was a *floor*, not a ceiling.

**Back-out trigger:** none — Z3 is one-way. Recovery from a Z3-induced incident requires re-implementing a backend from scratch (or reviving from git history). The pre-condition gates are deliberately conservative to make this acceptable.

#### Phase Z4 — Ecosystem participation (continuous)

**Duration:** continuous from Z0 start through the post-adoption lifetime of the Vello backend. **Land order:** parallel with Z0–Z3 and beyond.

**Deliverables (rolling):**

1. **Linebender alignment metric measurement.** Quarterly recording of `closure_latency(CG)` vs `closure_latency(Xilem-aligned)` per §Linebender alignment metric. First measurement window: Z0 + 3 months. Recorded in a new `docs/upstream-metrics.md` file (created at Z0 start).
2. **Upstream contribution backlog.** Issues / PRs filed against `linebender/vello`, `linebender/glifo`, `linebender/parley`, `linebender/peniko` for cell-grid-specific bugs and feature requests surfaced during Z1–Z3. Tracked in a roadmap entry under §2.2 Backlog → "ADR-032 Z4 upstream debts".
3. **Breaking-change response procedure.** Documented protocol for handling Vello / Glifo semver-minor breaks: pin the previous version in `Cargo.lock`, file a tracking issue, integrate the break in a dedicated follow-up PR. Pinned in `docs/development.md` (or equivalent) at Z0 + 1 month.
4. **6-month post-Z3 retrospective.** ADR-032 reopens for a retrospective entry that records: actual cumulative LOC delta vs estimate, actual `cpu_share` measurement at the 6-month boundary (re-classifies Vello hybrid as durable / transitional / stepping-stone in §Hybrid vs compute strategic position), Linebender alignment metric trajectory, any §Risks rows that activated.

**Closing condition:** Z4 has no closing condition — it is the post-adoption operational mode. The 6-month retrospective is a recurring artefact, not a phase exit.

**Re-evaluation trigger (Z4 → §Hybrid vs compute strategic position re-eval):** if the 6-month or 12-month retrospective records `cpu_share ≥ 50%` (stepping-stone classification) or Linebender alignment metric in red, ADR-032 reopens for a §Hybrid vs compute or §Linebender pivot decision. This is *not* a back-out from Z3; it is a forward decision about whether to migrate from `vello_hybrid` to `vello` (compute), or from Linebender's stack to a hedge.

#### Phase plan summary table

| Phase | Duration | LOC delta | Halt? | Pre-cond |
|---|---|---|---|---|
| Z0 | 2–3 w | +750 / 0 retired | yes | §Spike Findings positive |
| Z1 | 2–3 w | −2,400 / +200 | yes | Z0 close |
| Z2 | 1–2 w | −500 to −750 | yes | Z1 close |
| Z3 | 1 w | −3,800 | **no** (one-way) | Z2 close + Vello 1.0 + 3-month soak + alignment metric green |
| Z4 | continuous | n/a | n/a | Z0 start |

Cumulative: **−6,500 to −6,750 LOC** retired by Z3 close. §Spike Findings field 7's "≥ 2,400" target is a *floor*; the realistic outcome is approximately 2.7× that.

#### Phase plan vs §Implications dual-stack rule

The dual-stack rule (§Implications) governs `WgpuBackend` *removal*. The phase plan governs *what work happens before that removal*. The two intersect at Z3:

- Z0–Z2 land **before** the dual-stack rule expires; both backends remain reachable.
- Z3's pre-condition includes "Vello 1.0 announcement" — the same trigger that expires the dual-stack rule.
- Z3's deliverable removes `WgpuBackend` — the action the dual-stack rule prohibited.

This intersection is intentional: Z3 is the moment the dual-stack rule is satisfied (rule expires) and consumed (removal happens). No other phase can satisfy or consume the rule.
