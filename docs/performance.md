# Performance Analysis

This document analyzes the performance characteristics of kasane's declarative UI architecture.
It covers the rendering pipeline, measured benchmarks, bottleneck analysis, and optimization strategy.

All measurements taken with `cargo bench` (criterion, release profile) unless noted otherwise.

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
grid.diff()             -- O(w*h) dirty cell detection
  |
  v
backend.draw(&diffs)    -- O(changed_cells) -- terminal I/O (dominant cost)
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
| `backend.draw()` | O(changed_cells) | **100-3,000 us** | Escape sequence generation + I/O |
| `backend.flush()` | O(1) | **50-500 us** | stdout write |

Note: paint-only cost (26.3 us) is derived from the `paint/80x24` benchmark (30.7 us, which includes `grid.clear()`) minus `grid_clear/80x24` (4.4 us). swap cost (~5.3 us) is the residual from the `full_frame` benchmark.

**Dominant cost: terminal I/O (`backend.draw()` + `backend.flush()`).**
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

| Benchmark | What It Measures | Measured | Notes |
|---|---|---|---|
| `backend_draw/full_redraw/80x24` | 80x24 all cells | 163 us | Escape sequence generation |
| `backend_draw/full_redraw/200x60` | 200x60 all cells | 1.01 ms | Large screen |
| `backend_draw/incremental_1line` | 1 line changed | 2.32 us | Most common pattern |
| `backend_draw/full_redraw_realistic/80x24` | Realistic data at 80x24 | 150 us | Diverse Face values |

The realistic benchmark is slightly faster than uniform because the uniform benchmark alternates Face on every cell, causing more style transition escape sequences.

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

**Key finding**: diff() accounts for 71% of per-frame bytes (196 KB) due to CellDiff vector allocation. JSON parsing allocates heavily (1,847 allocs for 100 lines) but is amortized by simd_json's speed.

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

### Severity: Medium

#### Container Fill Loop

`paint.rs` performs O(w*h) `put_char(" ")` calls for container background fill. Each call triggers:
- Bounds checking
- Wide-char boundary cleanup (6-8 conditional branches)
- CompactString construction

A `fill_row()` bulk operation would avoid per-cell overhead.

#### grid.diff() Exceeds Target

diff() at 12.2 us (incremental) exceeds the original 10 us target. Cell comparison involves CompactString (24B) + Face (16B) + u8 per cell. The `dirty_rows` optimization helps but the per-cell comparison cost is inherently higher than estimated.

#### diff() Allocation Dominance

diff() allocates 196 KB per frame (71% of total) due to CellDiff owning cloned Cell data. A reference-based or streaming diff API could eliminate this.

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

## Compiler-Driven Optimization (ADR-010) Evaluation

[ADR-010](./decisions.md) defines a three-stage compilation model. Evaluation against real benchmark data:

### Stage 1: Input Memoization

| Metric | Value |
|---|---|
| Current view() cost | 0.24 us (0 plugins) / 2.35 us (10 plugins) |
| Expected savings | 0.2-2.3 us |
| Assessment | At 0.24 us, savings are negligible. Becomes worthwhile with real plugins (>2 us). |

**Recommendation**: Implement when first real plugins exist (Phase 4a validation).

### Stage 2: Static Layout Cache

| Metric | Value |
|---|---|
| Current place() cost | 0.37 us |
| Target threshold | 5 us (14x headroom) |
| Cache memory cost | ~1.5 KB (50B/node * 30 nodes) |
| Cache invalidation | u16 comparison * 2 per frame |

**Recommendation**: Defer until place() exceeds 5 us.

### Stage 3: Fine-Grained Update Code Generation

| Metric | Value |
|---|---|
| Current paint() cost | 26.3 us (80x24) / 83.6 us (200x60) |
| Terminal I/O cost | 163-1,012 us (backend.draw) |
| paint as % of I/O | 2-25% |
| Potential savings | ~23 us at 80x24 |

**Recommendation**: Defer. Simpler optimizations (fast-path ASCII fill, dirty region skipping) offer better ROI than code generation.

### Overall Verdict

ADR-010 is architecturally sound but premature. The CPU pipeline (49 us) is dominated by clear()+paint() (30.7 us) and diff() (12.2 us), which are better addressed by:

1. **DirtyFlags per-component skip** -- avoid full repaint on status-only changes
2. **Container fill fast-path** -- `fill_row()` instead of per-cell `put_char()`
3. **Streaming diff** -- eliminate 196 KB allocation per frame

These simpler optimizations should be pursued first. ADR-010 stages become relevant when plugin count grows or when terminal I/O is no longer the bottleneck (e.g., GPU backend).

## WASM Plugin Benchmarks

Measured with `cargo bench -p kasane-wasm-bench` (wasmtime 42, Component Model, criterion).
See [ADR-013](./decisions.md#adr-013-wasm-プラグインランタイム--component-model-採用) for the full decision record.

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

| Scenario | Measured | Budget (40 μs) | Notes |
|---|---|---|---|
| 1 plugin full frame | **1.80 μs** | 4.5% | |
| 3 plugins full frame | **5.45 μs** | 13.6% | |
| 5 plugins full frame | **8.91 μs** | 22.3% | |
| 10 plugins full frame | **18.0 μs** | 45.0% | |
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

## Performance Principles

1. **Terminal I/O is dominant**: CPU pipeline (49 us) is 1-20% of terminal I/O (163-1,012 us). Improving diff accuracy (minimizing changed cells) matters more than optimizing grid operations.
2. **Avoid allocations on hot paths**: Minimize heap allocations in paint, layout, and diff. BufferRef pattern avoids large data clones. Target: <50 allocations per frame.
3. **Measure before optimizing**: `cargo bench --bench rendering_pipeline` for measurement, CI detects >15% regressions. All optimization decisions are data-driven.
4. **Bound plugin costs**: Native plugins: 10 add ~4 us (view 2.35 us + dispatch 1.70 us). WASM plugins: 10 add ~18 us. Monitor linear scaling; investigate if total exceeds ~20 us (native) or ~25 us (WASM).
5. **Cache only when needed**: place() has 14x headroom to threshold. VirtualList and layout caching are deferred until problems are observed.
6. **WASM + caching synergy**: DirtyFlags-based caching eliminates WASM calls on unchanged frames (0.26 ns cache hit). Design WASM plugin APIs to maximize cache hit rate.
