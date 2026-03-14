# Performance Benchmarks and Optimization Status

This document collects benchmark data, bottleneck analysis, and optimization status for Kasane.
For performance principles and reading guidance, see [performance.md](./performance.md). For profiling workflows, see [profiling.md](./profiling.md).

## Frame Execution Flow

```
Event received
  |
  v
Event batch processing (try_recv drains all pending)
  |
  v
state.apply()           -- O(message size)
  |
  v
view(&state, &registry) -- Element tree construction
  |
  v
place(&element, area)   -- Layout calculation (flexbox + grid + overlay)
  |
  v
grid.clear() + paint()  -- CellGrid rendering
  |
  v
backend.draw_grid(&grid) -- O(changed_cells) -- diff + escape gen + I/O (dominant cost)
backend.flush()
  |
  v
grid.swap()             -- O(w*h) buffer swap
```

## Per-Frame Cost (80x24 Terminal)

Measured with `cargo bench --bench rendering_pipeline`.

| Phase | Complexity | Measured | Notes |
|---|---|---|---|
| `view()` | O(nodes) | 0.24 us (0 plugins) / 2.35 us (10 plugins) | Element tree construction |
| `place()` | O(nodes) | 0.37 us | Flexbox layout calculation |
| `grid.clear()` | O(w*h) | 4.4 us | Reset cells to default face |
| `paint()` | O(w*h) | 26.3 us | Atom-to-cell conversion, unicode-width |
| `grid.diff()` (incremental) | O(w*h) = 1,920 cells | 12.2 us | Cell equality comparison |
| `grid.diff()` (full redraw) | O(w*h) | 24.2 us | First frame only; builds all CellDiffs |
| `grid.swap()` | O(w*h) | ~5.3 us | Buffer swap + previous clear |
| **CPU pipeline total** | | **48.8 us** | `full_frame` benchmark |
| `backend.draw_grid()` | O(changed_cells) | **58-335 us** | Zero-copy diff + incremental SGR + I/O |
| `backend.flush()` | O(1) | **50-500 us** | stdout write |

Note: paint-only cost (26.3 us) is derived from the `paint/80x24` benchmark (30.7 us, which includes `grid.clear()`) minus `grid_clear/80x24` (4.4 us). swap cost (~5.3 us) is the residual from the `full_frame` benchmark.

**Dominant cost: terminal I/O (`backend.draw_grid()` + `backend.flush()`).**
The CPU pipeline totals ~49 us, which is **0.3%** of a 16 ms frame budget.

## Existing Optimizations

| Optimization | Target | Effect |
|---|---|---|
| CompactString | Cell.grapheme | Avoids heap allocation for short strings (<=24B inline) |
| bitflags Attributes | Face.attributes | `Vec<Attribute>` -> u16. Copy-type eliminates allocation |
| Double buffering | CellGrid | `std::mem::swap()` pointer exchange. O(1) |
| Differential rendering | CellGrid.diff() | Only changed cells sent to terminal. Minimizes I/O |
| Event batching | main.rs | `try_recv()` drains all pending events, then renders once |
| SIMD JSON | protocol.rs | High-speed JSON parsing via simd_json |
| BufferRef | Element tree | Buffer lines referenced, not cloned. Zero-copy in view() |
| dirty_rows | CellGrid | Row-level dirty tracking skips unchanged rows in diff() |

## Declarative Pipeline Overhead

Additional cost of the declarative pipeline (`view() -> layout() -> paint()`) compared to the former imperative pipeline (`render_frame()` writing directly to CellGrid).

### Per-Phase Breakdown (full_frame ~ 49 us)

```
view()  construct:  0.24 us =              (0.5%)
place() layout:     0.37 us =              (0.8%)
clear() 80x24:      4.4  us ====           (9.0%)
paint() 80x24:     26.3  us =====================  (53.9%)  <-- dominant
diff()  incr:      12.2  us ==========     (25.0%)
swap():             5.3  us =====          (10.8%)
-------------------------------
total              ~49   us
```

### Measured Per-Phase Values

#### 1. Element Tree Construction: view()

| Condition | Measured | Notes |
|---|---|---|
| 0 plugins | **0.24 us** | Core UI (~20-30 nodes) |
| 10 plugins | **2.35 us** | Each plugin contributes to StatusRight |

Core UI construction costs 240 ns, well below the initial estimate (~1 us). Plugin scaling is near-linear.

#### 2. Layout Calculation: place()

| Condition | Measured |
|---|---|
| Standard 80x24 (0 plugins) | **0.37 us** |

About 1/3 of the initial estimate (~1 us). The flexbox measure/place two-pass is extremely lightweight.

#### 3. Rendering: clear() + paint()

The `paint/*` benchmarks measure `grid.clear()` + `paint()` combined. Paint-only costs are derived by subtracting the standalone `grid_clear` measurement.

| Condition | Benchmark (clear+paint) | Paint-only | Cells | Per-cell (paint-only) |
|---|---|---|---|---|
| 80x24 | **30.7 us** | 26.3 us | 1,920 | 13.7 ns |
| 80x24 (realistic) | **35.2 us** | 30.8 us | 1,920 | 16.0 ns |
| 200x60 | **110.8 us** | 83.6 us | 12,000 | 7.0 ns |

Scales near-linearly with area. Per-cell cost drops at larger sizes due to improved cache efficiency. The "realistic" benchmark (diverse Face values and varied line lengths) adds ~17% overhead.

#### 4. Plugin Dispatch

| Plugins | collect_slot (8 slots) + apply_decorator | Measured |
|---|---|---|
| 1 | 8 vtable calls + sort + fold | **0.22 us** |
| 5 | 40 calls + sort + fold | **1.07 us** |
| 10 | 80 calls + sort + fold | **1.70 us** |

Near-linear scaling (~170 ns/plugin).

#### 5. Decorator Chain

| Plugins | Measured | Notes |
|---|---|---|
| 1 | **26 ns** | sort + 1 fold |
| 5 | **70 ns** | sort + 5 folds |
| 10 | **118 ns** | sort + 10 folds |

Well below the initial estimate (~500 ns for 5 plugins). Real plugin decorators will add slightly more, but should remain sub-microsecond.

### Net Declarative Overhead

| Phase | Initial Estimate | Measured |
|---|---|---|
| Element construction (view) | ~1-4 us | 0.24-2.35 us |
| Layout calculation (place) | ~1 us | 0.37 us |
| Recursive traversal (paint overhead) | ~0.3 us | Included in paint |
| Plugin dispatch | ~1 us | 1.70 us (10 plugins) |
| Decorator chain | ~0.5 us | 0.12 us (10 plugins) |
| **Total** | **~4-7 us** | **~3 us** (0 plugins) / **~5 us** (10 plugins) |

**3-5 us out of the full pipeline (49 us) = 6-10%. Relative to terminal I/O (200-3,600 us) = 0.1-2.5%.**
**No practical impact.**

## Micro Benchmarks (14)

| Benchmark | What It Measures | Target | Measured | Verdict |
|---|---|---|---|---|
| `element_construct/plugins_0` | view() tree construction (0 plugins) | < 10 us | 0.24 us | OK (42x headroom) |
| `element_construct/plugins_10` | view() tree construction (10 plugins) | < 10 us | 2.35 us | OK (4x headroom) |
| `flex_layout` | place() layout calculation | < 5 us | 0.37 us | OK (14x headroom) |
| `paint/80x24` | clear() + paint() combined | -- | 30.7 us | paint-only: 26.3 us |
| `paint/200x60` | clear() + paint() combined (large) | -- | 110.8 us | paint-only: 83.6 us |
| `paint/80x24_realistic` | clear() + paint() combined (varied Face/lengths) | -- | 35.2 us | paint-only: 30.8 us |
| `grid_clear/80x24` | clear() standalone | -- | 4.4 us | |
| `grid_clear/200x60` | clear() standalone (large) | -- | 27.2 us | |
| `grid_diff/full_redraw` | diff() first frame | < 10 us | 24.2 us | Exceeds (note 1) |
| `grid_diff/incremental` | diff() no changes | < 10 us | 12.2 us | Exceeds (note 1) |
| `decorator_chain/plugins/1` | apply_decorator (1 stage) | < 1 us | 26 ns | OK |
| `decorator_chain/plugins/5` | apply_decorator (5 stages) | < 1 us | 70 ns | OK |
| `decorator_chain/plugins/10` | apply_decorator (10 stages) | < 1 us | 118 ns | OK (8x headroom) |
| `plugin_dispatch/plugins/1` | All 8 slots + decorator (1 plugin) | < 5 us | 0.22 us | OK |
| `plugin_dispatch/plugins/5` | All 8 slots + decorator (5 plugins) | < 5 us | 1.07 us | OK |
| `plugin_dispatch/plugins/10` | All 8 slots + decorator (10 plugins) | < 5 us | 1.70 us | OK (3x headroom) |

**Note 1**: `grid_diff` exceeds the initial 10 us target. Cell comparison cost (CompactString 24B + Face 16B + u8) is higher than estimated. However, full_redraw only occurs on the first frame; incremental is the steady-state path. At 12.2 us, diff is 25% of the CPU pipeline but negligible vs. the 16 ms frame budget.

## Integration Benchmarks (8)

| Benchmark | What It Measures | Target | Measured | Verdict |
|---|---|---|---|---|
| `full_frame` | view -> layout -> paint -> diff -> swap | < 16 ms | 48.8 us | OK (328x headroom) |
| `draw_message` | state.apply(Draw) + full frame | < 5 ms | 50.2 us | OK (100x headroom) |
| `menu_show/items/10` | Menu display + full frame | < 5 ms | 59.9 us | OK |
| `menu_show/items/50` | Menu 50 items + full frame | < 5 ms | 59.9 us | OK |
| `menu_show/items/100` | Menu 100 items + full frame | < 5 ms | 59.9 us | OK |
| `incremental_edit/lines/1` | 1 line edit -> view + paint + diff | -- | 44.0 us | |
| `incremental_edit/lines/5` | 5 line edit -> view + paint + diff | -- | 46.0 us | |
| `message_sequence` | draw_status + set_cursor + draw -> full frame | -- | 50.1 us | |

`menu_show` is independent of item count because `menu_max_height=10` caps the visible rows.

## Extended Benchmarks (20)

### JSON-RPC Parsing

| Benchmark | What It Measures | Measured | Notes |
|---|---|---|---|
| `parse_request/draw_lines/10` | JSON-RPC parse (10-line draw) | 61.8 us | |
| `parse_request/draw_lines/100` | JSON-RPC parse (100-line draw) | 540 us | |
| `parse_request/draw_lines/500` | JSON-RPC parse (500-line draw) | 2.68 ms | |
| `parse_request/draw_status` | JSON-RPC parse (draw_status) | 2.85 us | Small message, high frequency |
| `parse_request/set_cursor` | JSON-RPC parse (set_cursor) | 849 ns | Minimal message |
| `parse_request/menu_show_50` | JSON-RPC parse (menu_show 50 items) | 55.9 us | |

### State Application

| Benchmark | What It Measures | Measured | Notes |
|---|---|---|---|
| `state_apply/draw_lines/23` | state.apply(Draw) standalone | 2.44 us | |
| `state_apply/draw_lines/100` | state.apply(Draw) standalone | 5.34 us | |
| `state_apply/draw_lines/500` | state.apply(Draw) standalone | 17.7 us | |
| `state_apply/draw_status` | state.apply(DrawStatus) | 947 ns | |
| `state_apply/set_cursor` | state.apply(SetCursor) | 724 ns | |
| `state_apply/menu_show_50` | state.apply(MenuShow) | 4.37 us | |

### Scaling Characteristics

| Benchmark | What It Measures | Measured | Notes |
|---|---|---|---|
| `scaling/full_frame/80x24` | Full frame at 80x24 | 56.9 us | Baseline |
| `scaling/full_frame/200x60` | Full frame at 200x60 | 223 us | 3.9x (area ratio: 6.25x) |
| `scaling/full_frame/300x80` | Full frame at 300x80 | 399 us | 7.0x (area ratio: 12.5x) |
| `scaling/parse_apply_draw/500` | Parse + apply (500 lines) | 2.76 ms | |
| `scaling/parse_apply_draw/1000` | Parse + apply (1000 lines) | 5.35 ms | Near-linear |
| `scaling/diff_incremental/80x24` | diff() no-change 80x24 | 12.3 us | |
| `scaling/diff_incremental/200x60` | diff() no-change 200x60 | 75.2 us | 6.1x (area ratio: 6.25x) |
| `scaling/diff_incremental/300x80` | diff() no-change 300x80 | 150 us | 12.2x (area ratio: 12.5x) |

Full frame scales sub-linearly with area (cache effects). diff() scales linearly.

## TUI Backend Benchmarks

Measured with `kasane-tui/benches/backend.rs` using MockBackend (no real terminal I/O).

### draw_grid (ADR-015 optimized path)

| Benchmark | What It Measures | Measured | Notes |
|---|---|---|---|
| `draw_grid/full_redraw/80x24` | 80x24 all cells | 58 us | Cursor auto-advance + incremental SGR |
| `draw_grid/full_redraw/200x60` | 200x60 all cells | 335 us | Large screen |

### draw (legacy CellDiff path)

| Benchmark | What It Measures | Measured | Notes |
|---|---|---|---|
| `backend_draw/full_redraw/80x24` | 80x24 all cells | 163 us | Escape sequence generation |
| `backend_draw/full_redraw/200x60` | 200x60 all cells | 1.01 ms | Large screen |
| `backend_draw/incremental_1line` | 1 line changed | 2.32 us | Most common pattern |
| `backend_draw/full_redraw_realistic/80x24` | Realistic data at 80x24 | 150 us | Diverse Face values |

**ADR-015 improvement:** `draw_grid()` is **2.4x faster** than the legacy `draw()` path at 80×24 (58 μs vs 163 μs) by eliminating per-cell `MoveTo` commands via cursor auto-advance and reducing SGR escape volume via incremental diff (`emit_sgr_diff`).

## E2E Pipeline Benchmarks

JSON bytes -> parse -> apply -> render -> diff -> backend.draw -> escape bytes.

| Benchmark | What It Measures | Measured | Notes |
|---|---|---|---|
| `e2e_pipeline/json_to_escape_80x24` | Full pipeline (uniform data) | 193 us | |
| `e2e_pipeline/json_to_escape_realistic` | Full pipeline (realistic data) | 164 us | |

## Allocation Breakdown (Per-Phase)

Measured with `--features bench-alloc` (debug profile, single frame at 80x24).

| Phase | Alloc Count | Bytes | Notes |
|---|---|---|---|
| view | 6 | 1,176 | Element tree construction |
| place | 11 | 336 | Layout result vectors |
| clear+paint | 3 | 1,196 | Atom-to-cell conversion |
| diff | 10 | 196,416 | CellDiff vector + Cell clones |
| swap | 1 | 76,800 | Previous buffer allocation |
| **full_frame total** | **31** | **275,924** | |
| parse_request (100 lines) | 1,847 | 275,172 | JSON parsing dominates |

**Key finding**: diff() accounted for 71% of per-frame bytes (196 KB) due to CellDiff vector allocation. **This has been eliminated by ADR-015**: the TUI event loop now uses `draw_grid()` with `iter_diffs()` (zero-copy, zero allocation). JSON parsing allocates heavily (1,847 allocs for 100 lines) but is amortized by simd_json's speed.

## Allocation Hotspots (Code Analysis)

| Location | What | Frequency |
|---|---|---|
| `paint.rs` `atoms.to_vec()` | Atom slice duplication | lines * frames |
| `paint.rs` `ch.to_string()` | Grapheme -> String conversion | cells * frames |
| `grid.rs` `cell.clone()` in diff() | Cell cloning into CellDiff | changed_cells * frames |
| `view/mod.rs` `atom.contents.clone()` | String clone in status bar construction | status_atoms * frames |

The paint/view allocations are relatively small (3-6 allocs). The diff() allocations dominate because CellDiff owns its Cell data.

## Latency Distribution

Measured over 10,000 iterations (release profile).

| Phase | p50 | p90 | p99 | p99.9 | max |
|---|---|---|---|---|---|
| **Full frame** | 50.3 us | 57.4 us | 77.7 us | 84.9 us | 97.5 us |
| view() | 0.3 us | 0.3 us | 0.5 us | 0.5 us | 10.4 us |
| place() | 0.4 us | 0.4 us | 0.6 us | 0.7 us | 7.4 us |
| clear()+paint() | 31.8 us | 34.7 us | 49.9 us | 55.2 us | 58.1 us |
| diff() | 13.0 us | 19.6 us | 22.3 us | 26.8 us | 30.2 us |

Tail latency is well-controlled. The p99.9 full frame (84.9 us) is only 1.7x the p50 (50.3 us). The max (97.5 us) stays under 100 us.

### Latency Budget Tests

All pass (release profile):

| Test | Budget | Status |
|---|---|---|
| `state_apply_under_200us` | < 200 us | Pass |
| `parse_request_under_500us` | < 500 us | Pass |
| `full_frame_under_2ms` | < 2 ms | Pass |

## Replay Benchmarks

End-to-end scenario replay (parse + apply + render for each message).

| Scenario | Messages | Measured | Per-message |
|---|---|---|---|
| `normal_editing_50msg` | 50 | 5.47 ms | 109 us |
| `fast_scroll_100msg` | 100 | 22.4 ms | 224 us |
| `menu_completion_20msg` | 20 | 2.35 ms | 117 us |
| `mixed_session_200msg` | 200 | 25.0 ms | 125 us |

Fast scroll is heavier per-message due to full buffer redraws.

## GPU CPU-Side Benchmarks

Measured with `kasane-gui/benches/cpu_rendering.rs`.

| Benchmark | What It Measures | Measured | Notes |
|---|---|---|---|
| `gpu/bg_instances_80x24` | Background rectangle instance generation | 6.70 us | |
| `gpu/row_hash_24rows` | Row content hashing (24 rows) | 57.1 us | Change detection for GPU |
| `gpu/row_spans_80cols` | Row span computation (80 cols) | 702 ns | Face run-length encoding |
| `gpu/color_resolve_1920cells` | Color resolution for 1,920 cells | 8.32 us | Theme color -> RGBA |

## Bottleneck Analysis

Ranked by severity with current measurements.

### Severity: High (Resolved)

#### Buffer Line Cloning

Element tree uses owned types, which would require cloning all buffer lines every frame.

**Resolution: BufferRef pattern (implemented)**. `Element::BufferRef { line_range }` eliminates clone cost. view() at 0.24 us (0 plugins) confirms BufferRef's effectiveness.

### Severity: Medium (Resolved by ADR-015)

#### Container Fill Loop — Resolved

`paint.rs` previously performed O(w*h) `put_char(" ")` calls for container background fill. Each call triggered bounds checking, wide-char boundary cleanup (6-8 conditional branches), and CompactString construction.

**Resolution (ADR-015 P4):** Replaced with `clear_region()` bulk operation, eliminating per-cell overhead.

#### diff() Allocation Dominance — Resolved

diff() previously allocated 196 KB per frame (71% of total) due to CellDiff owning cloned Cell data.

**Resolution (ADR-015 P1+P2):** `diff_into()` reuses a caller-provided buffer (zero allocation on warm path). `iter_diffs()` provides zero-copy iteration yielding `&Cell` references. The TUI event loop now uses `draw_grid()` which calls `iter_diffs()` directly, eliminating all CellDiff allocation.

#### grid.diff() Exceeds Target

diff() at 12.2 us (incremental) exceeds the original 10 us target. Cell comparison involves CompactString (24B) + Face (16B) + u8 per cell. The `dirty_rows` optimization helps but the per-cell comparison cost is inherently higher than estimated.

#### BufferLine Decorator Multiplicative Cost

Multiple `DecorateTarget::BufferLine` decorators create N * M function calls (N decorators * M lines):

```
3 decorators (line numbers, git marks, breakpoints) x 50 lines
  = 150 decorator calls = 150 node additions ~ 5-10 us
```

**Mitigation**: Recommend `DecorateTarget::Buffer` (column-based) over per-line decoration.

### Severity: Low

#### word_wrap Vec Allocation

`render_wrapped_line()` allocates 2 Vecs per call. Only triggered during info popup display (infrequent).

#### Large Element Trees Without Virtualization

1,000+ node trees could reach ~50 us for construction + layout. Acceptable now, but 10,000+ nodes would compete with terminal I/O.

**Future mitigation**: `Element::VirtualList` for visible-range-only rendering.

## Compiler-Driven Optimization (ADR-010) — Implementation Status

[ADR-010](./decisions.md) defines a multi-stage compilation model. All four stages have been implemented.

### Stage 1: DirtyFlags-Based View Memoization — Implemented

| Metric | Value |
|---|---|
| view() cost | 0.24 us (0 plugins) / 2.35 us (10 plugins) |
| Implementation | ViewCache, ComponentCache\<T\>, DirtyFlags u16, MENU→MENU_STRUCTURE+MENU_SELECTION split |
| Result | view() sections skipped entirely when corresponding DirtyFlags are clear |

### Stage 2: Verified Dependency Tracking — Implemented

| Metric | Value |
|---|---|
| Implementation | `#[kasane::component(deps(FLAG, ...))]` proc macro, AST-based field access analysis, FIELD_FLAG_MAP |
| Compile-time check | Accesses to state fields not covered by declared deps cause compile error |
| Escape hatch | `allow(field, ...)` for intentional dependency gaps |

### Stage 3: SceneCache (DrawCommand-Level Caching) — Implemented

| Metric | Value |
|---|---|
| Implementation | Per-section DrawCommand caching (base, menu, info) |
| Invalidation | Mirrors ViewCache: BUFFER\|STATUS\|OPTIONS→base, MENU→menu, INFO→info |
| GPU benefit | Cursor-only frames reuse cached scene (0 us pipeline work) |

### Stage 4: Compiled Paint Patches — Implemented

| Metric | Value |
|---|---|
| StatusBarPatch | STATUS-only dirty → repaint ~80 cells (vs 1,920 full) |
| MenuSelectionPatch | MENU_SELECTION-only dirty → swap face on ~10 cells |
| CursorPatch | Cursor moved, no dirty flags → swap face on 2 cells |
| LayoutCache | base_layout, status_row, root_area cached with per-section invalidation |

### Overall Result

All four stages are operational. The pipeline automatically selects the minimal repaint path:

1. **PaintPatch** (2-80 cells) → **sectioned repaint** (~1 section) → **full pipeline** (fallback)

All identified optimization opportunities have been addressed by ADR-015:
- **Container fill** → `clear_region()` (P4)
- **Zero-alloc diff** → `diff_into()` / `iter_diffs()` (P1)
- **Line-dirty expansion** → `selective_clear()` for BUFFER|STATUS (P3)
- **Backend SGR optimization** → `draw_grid()` with cursor auto-advance + incremental SGR diff (P2)

## Rendering Pipeline Optimization (ADR-015) — Implementation Status

[ADR-015](./decisions.md) addresses four structural inefficiencies in the rendering pipeline. All four stages have been implemented.

### Stage P4: Container Fill Bulk Optimization — Implemented

Replaced per-cell `put_char(" ")` loop in `paint_container` with `clear_region()`. Eliminates per-cell bounds checking, wide-char cleanup branches, and CompactString construction. ~0.5–2 μs savings per container paint.

### Stage P1: Zero-Allocation Diff Path — Implemented

| Method | Description | Allocation |
|---|---|---|
| `diff_into(&mut buf)` | Reuses caller-provided `Vec<CellDiff>` | 0 (warm buffer) |
| `iter_diffs()` | Zero-copy iterator yielding `(u16, u16, &Cell)` | 0 |
| `is_first_frame()` | Returns `self.previous.is_empty()` | N/A |

`diff_into()` incremental: 12.3 μs vs `diff()` 13.3 μs (−7%, plus zero allocation).

### Stage P3: Line-Dirty Coverage Expansion — Implemented

Extended line-dirty optimization from `dirty == DirtyFlags::BUFFER` (exact match) to `dirty.contains(DirtyFlags::BUFFER)`. The common case of `BUFFER|STATUS` (Draw + DrawStatus in same batch) now benefits from per-line dirty tracking via `selective_clear()`.

| Scenario | Before | After | Savings |
|---|---|---|---|
| BUFFER\|STATUS, 1 line changed | ~49 μs (full pipeline) | ~21 μs | −57% |

### Stage P2: Direct-Grid Backend Draw + Incremental SGR — Implemented

`draw_grid()` on `RenderBackend` trait iterates `grid.iter_diffs()` directly, with two optimizations:
1. **Cursor auto-advance**: Skip `MoveTo` for consecutive cells on the same row (terminal auto-advances after Print)
2. **Incremental SGR**: `emit_sgr_diff()` compares faces field-by-field, emitting only changed attributes/colors

| Benchmark | Legacy `draw()` | Optimized `draw_grid()` | Speedup |
|---|---|---|---|
| Full redraw 80×24 | 163 μs | 58 μs | 2.8x |
| Full redraw 200×60 | 1,010 μs | 335 μs | 3.0x |

### Overall ADR-015 Impact

- **TUI backend I/O**: 2.4–3x faster escape sequence generation
- **Per-frame allocation**: 196 KB → 0 (diff phase)
- **Common editing pattern** (BUFFER|STATUS, 1 line): ~57% CPU pipeline reduction
- **Container paint**: ~0.5–2 μs savings per container

## WASM Plugin Benchmarks

Measured with `cargo bench -p kasane-wasm-bench` (wasmtime 42, Component Model, criterion).
See [ADR-013](./decisions.md#adr-013-wasm-プラグインランタイム-component-model-採用) for the full decision record.

### Raw WASM Overhead

| Benchmark | Measured | Notes |
|---|---|---|
| Empty call (noop) | **26.5 ns** | WASM boundary crossing cost |
| Integer call (add) | **23.5 ns** | |
| Host import (1x) | **29.2 ns** | Guest → host function call |
| Host import (10x) | **77.5 ns** | ~5 ns per additional host call |
| Native noop | 1.2 ns | Baseline comparison |

### Component Model Call Overhead

| Function | Raw Module | Component Model | Ratio | Notes |
|---|---|---|---|---|
| noop | 26.5 ns | **552 ns** | 20.8x | ~500 ns canonical ABI overhead |
| add | 23.5 ns | **556 ns** | 23.7x | |
| echo_string 100B | 59 ns | **758 ns** | 12.9x | |
| build_gutter 24 | 1.50 μs | **6.12 μs** | 4.1x | Overhead amortizes over payload |
| on_state_changed | 42 ns | **787 ns** | 18.7x | 3 host calls inside |
| contribute_lines 24 | 75 ns | **1.04 μs** | 13.9x | |
| full_cycle | 115 ns | **1.84 μs** | 16.0x | state_changed + contribute_lines |

### Realistic Plugin Simulation

| Scenario | Measured | Budget (~49 μs) | Notes |
|---|---|---|---|
| 1 plugin full frame | **1.80 μs** | 3.7% | |
| 3 plugins full frame | **5.45 μs** | 11.1% | |
| 5 plugins full frame | **8.91 μs** | 18.2% | |
| 10 plugins full frame | **18.0 μs** | 36.7% | |
| Cache hit (no state change) | **0.26 ns** | ~0% | DirtyFlags skip |

Scaling is linear at **~1.8 μs per plugin**.

### WASM vs Native Comparison

| Operation | Native | WASM (CM) | Ratio | Notes |
|---|---|---|---|---|
| cursor_line full cycle | 9.5 ns | 2.01 μs | 212x | Absolute cost (2 μs) is negligible |
| gutter_24 | 1.63 μs | 6.18 μs | 3.8x | Ratio drops with real computation |

### Instantiation Cost (Startup Only)

| Operation | Measured | Notes |
|---|---|---|
| Component compilation | **9.97 ms** | One-time at startup |
| 1 instance | **29.3 μs** | Per-Store |
| 5 instances | **131 μs** | |
| 10 instances | **280 μs** | |

Total startup for 10 plugins ≈ 10 ms. Cacheable via `Engine::precompile_component`.

## High Refresh Rate Analysis (240Hz)

Frame budget at 240fps: **4.17 ms (4,170 μs)**.

### CPU Pipeline vs 240fps Budget

| | 80×24 | 200×60 | 300×80 | Budget |
|---|---|---|---|---|
| CPU pipeline (view→diff→swap) | 49 μs | 223 μs | 399 μs | 4,170 μs |
| WASM plugins (10) | 18 μs | 18 μs | 18 μs | |
| **CPU total** | **~67 μs** | **~241 μs** | **~417 μs** | **4,170 μs** |
| **Budget usage** | **1.6%** | **5.8%** | **10.0%** | |

The CPU pipeline is well within 240fps budget even at large display sizes.

### Backend Considerations

**TUI**: Not meaningful at 240fps. Terminal emulators refresh at 60-120Hz. `backend.draw()` cost (100-3,000 μs) is dominated by terminal I/O, not Kasane.

**GUI (wgpu)**: Achievable. The CPU pipeline leaves ~3,750 μs for GPU rendering and presentation at 80×24. With PaintPatch and DirtyFlags, animation frames (smooth scroll, cursor blink) bypass most of the CPU pipeline:

```
Content frame:    parse → apply → view → place → paint → diff → draw  (~49 μs + I/O)
Animation frame:  ──────────── skip ────────────── → scroll offset → GPU draw
```

### Limiting Factors

| Factor | Impact at 240fps | Mitigation |
|---|---|---|
| JSON-RPC parse (500 lines) | 2.68 ms = 64% of budget | Kakoune sends only viewport lines (24-80) |
| ~~diff() allocation (196 KB/frame)~~ | ~~GC pressure → jitter~~ | Resolved: `draw_grid()` + `iter_diffs()` = zero allocation |
| Event source rate | Kakoune is reactive, not 240Hz | Only animation frames need 240fps |

### Conclusion

240fps is achievable for the GPU backend with animation path separation (zero-cost redraws when no new Kakoune data arrives). The CPU pipeline has 4-8x headroom. Per-frame diff allocation (formerly 196 KB) has been eliminated by ADR-015's `iter_diffs()` zero-copy path.

## `#[kasane::component]` Compiler-Driven Optimization

The `#[kasane::component]` macro follows Svelte's "let the compiler do the work" philosophy, progressively generating optimized code from declarative `view()` functions ([ADR-010](./decisions.md#adr-010-コンパイラ駆動最適化-svelte-的二層レンダリング)):

**Stage 1: Input Memoization**

Retains previous input parameter values and skips Element construction when all inputs are identical:

```rust
#[kasane::component]
fn file_tree(entries: &[Entry], selected: usize) -> Element { ... }
// → If entries and selected are unchanged, returns cached Element
```

**Stage 2: Static Layout Cache**

The proc macro detects structurally static parts and calculates layout only once.

**Stage 3: Fine-Grained Update Code Generation**

The proc macro statically analyzes each Element's input parameter dependencies at the AST level and generates code that directly updates only the changed cells in CellGrid.

**Two-Layer Rendering Model:**

```
              +---------------------+
              |  Declarative API    |  ← Plugin authors work here
              |  (Element, view())  |
              +------+--------------+
                     |
         +-----------+----------+
         v                      v
  Compiled path          Interpreter path
  (proc macro gen)       (generic Element tree)
         |                      |
  Static structure →     Element → layout()
    direct CellGrid        → paint() → CellGrid
    update
```

- **Compiled path**: Parts that `#[kasane::component]` can statically analyze. Updates CellGrid directly, bypassing the Element tree.
- **Interpreter path**: Parts where plugins dynamically provide Elements via `Plugin::contribute()`. Uses the full pipeline.
- **Fallback**: Code without `#[kasane::component]` runs through the interpreter path. Optimization is opt-in; correctness is always guaranteed by the interpreter path.

## See also

- [performance.md](./performance.md) — performance principles and reading guidance
- [profiling.md](./profiling.md) — how to reproduce measurements
- [semantics.md](./semantics.md) — invalidation and correctness semantics
- [decisions.md](./decisions.md) — ADR history behind optimization work
