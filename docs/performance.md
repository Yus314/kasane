# Performance

This document is the authoritative reference for Kasane's performance characteristics: principles, benchmark data, bottleneck analysis, and optimization status.
For measurement and profiling workflows, see [profiling.md](./profiling.md).

**Last verified**: 2026-03-17

## Principles

Ordered by tier. Within each tier, original numbering is preserved for cross-references.

**Tier 1 — Inviolable** (violations are bugs):

8. Prefer exactness-preserving optimizations first; when exactness is intentionally weakened, document it in [semantics.md](./semantics.md).
7. Cache only where invalidation can be expressed clearly enough to preserve correctness. Treat caching as part of the rendering policy, not as an invisible implementation detail.

**Tier 2 — Priority** (trade-offs require ADR):

2. Reducing unnecessary invalidation matters more than making already-cheap pure computation marginally faster.
6. Measure before optimizing; performance work must be grounded in benchmarks and replayable workloads.

**Tier 3 — Guideline** (inform design, not hard constraints):

1. Terminal and backend I/O dominate end-to-end latency more than `view()` or `layout()`.
3. Allocation behavior on hot paths matters because jitter is more harmful than raw throughput regressions.
4. Plugin overhead must stay bounded and predictable in both native and WASM paths.
5. TUI and GUI backends share semantics, but have different dominant costs and different fast paths.

## Service-Level Objectives

| Metric | SLO | Current | Headroom |
|---|---|---|---|
| Full frame CPU (80×24) | p99 < 200 μs | 81.7 μs | 2.4× |
| Per-plugin WASM overhead | < 3 μs/plugin | ~1.8 μs | 1.7× |
| Per-frame allocation (warm) | < 50 allocs | 31* | 1.6× |
| state.apply(Draw) 80 lines | < 200 μs | ~132 μs | 1.5× |

*Allocation count predates Salsa integration; re-measurement pending.

## Degradation Model

```
frame_cost(w, h, n) ≈ base_cpu(w,h) + salsa_sync(dirty) + plugin_overhead(n) + backend_io(changed)
```

- `base_cpu(80,24)` ≈ 57 μs, sub-linear with area
- `salsa_sync` ≈ 0.2–7 μs (depends on DirtyFlags)
- `plugin_overhead` ≈ 1.8 μs × n (WASM CM)
- `backend_io` ≈ 44–228 μs (TUI, terminal I/O dominated)

## Frame Execution Flow

```
Event received
  |
  v
Event batch processing (try_recv drains all pending)
  |
  v
state.apply()           -- O(lines * atoms) for Draw; includes detect_cursors, compute_lines_dirty
  |
  v
sync_salsa_inputs()     -- Salsa incremental sync (slot contributions, annotations, overlays)
  |
  v
view(&state, &registry) -- Element tree construction (with ViewCache memoization)
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
| `view()` | O(nodes) | 5.0 us (0 plugins) / 10.4 us (10 plugins) | Element tree + plugin contributions + Salsa sync |
| `place()` | O(nodes) | 1.2 us | Flexbox + grid + overlay layout |
| `grid.clear()` | O(w*h) | 4.4 us | Reset cells to default face |
| `paint()` | O(w*h) | 28.5 us | Atom-to-cell conversion, unicode-width |
| `grid.diff()` (incremental) | O(w*h) = 1,920 cells | 12.2 us | Cell equality comparison |
| `grid.diff()` (full redraw) | O(w*h) | 39.2 us | First frame only; builds all CellDiffs |
| `grid.swap()` | O(w*h) | ~6.0 us | Buffer swap + previous clear |
| **CPU pipeline total** | | **57.3 us** | `full_frame` benchmark |
| `backend.draw_grid()` | O(changed_cells) | **44-228 us** | Zero-copy diff + incremental SGR + I/O |
| `backend.flush()` | O(1) | **50-500 us** | stdout write |

Note: paint-only cost (28.5 us) is derived from the `paint/80x24` benchmark (32.9 us, which includes `grid.clear()`) minus `grid_clear/80x24` (4.4 us). swap cost (~6.0 us) is the residual from the `full_frame` benchmark.

**Dominant cost: terminal I/O (`backend.draw_grid()` + `backend.flush()`).**
The CPU pipeline totals ~57 us, which is **0.36%** of a 16 ms frame budget.

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
| Salsa incremental | sync_salsa_inputs | Incremental recomputation of plugin contributions and overlays |
| PaintPatch | render pipeline | CursorPatch (1 μs), StatusBarPatch (6 μs) bypass full pipeline |
| detect_cursors two-strategy | state.apply | Attribute heuristic (fast path) + face-matching fallback |

## Declarative Pipeline Overhead

Additional cost of the declarative pipeline (`view() -> layout() -> paint()`) compared to the former imperative pipeline (`render_frame()` writing directly to CellGrid).

### Per-Phase Breakdown (full_frame ~ 57 us)

```
view()  construct:  5.0  us =====            (8.7%)
place() layout:     1.2  us =                (2.1%)
clear() 80x24:      4.4  us ====             (7.7%)
paint() 80x24:     28.5  us =====================  (49.7%)  <-- dominant
diff()  incr:      12.2  us ==========       (21.3%)
swap():             6.0  us =====            (10.5%)
-------------------------------
total              ~57   us
```

The view() cost has increased from the original 0.24 μs to 5.0 μs due to Salsa incremental sync, plugin registry preparation, session/workspace metadata, and a richer element tree. Despite the increase, view() is still only 8.7% of the full frame, and the Salsa integration provides significant warm-cache benefits (see [Salsa Pipeline Benchmarks](#salsa-pipeline-benchmarks)).

### Measured Per-Phase Values

#### 1. Element Tree Construction: view()

| Condition | Measured | Notes |
|---|---|---|
| 0 plugins | **5.0 us** | Core UI + Salsa sync + plugin registry |
| 10 plugins | **10.4 us** | Each plugin contributes to slots |

Includes Salsa input synchronization (~2-3 μs for 23 lines) and plugin registry preparation. Plugin scaling adds ~540 ns/plugin.

#### 2. Layout Calculation: place()

| Condition | Measured |
|---|---|
| Standard 80x24 (0 plugins) | **1.2 us** |

Handles flexbox, grid, overlay, and scrollable layout. The increase from the original 0.37 μs reflects the richer element tree (Grid, Scrollable, Stack, overlay positioning).

#### 3. Rendering: clear() + paint()

The `paint/*` benchmarks measure `grid.clear()` + `paint()` combined. Paint-only costs are derived by subtracting the standalone `grid_clear` measurement.

| Condition | Benchmark (clear+paint) | Paint-only | Cells | Per-cell (paint-only) |
|---|---|---|---|---|
| 80x24 | **32.9 us** | 28.5 us | 1,920 | 14.9 ns |
| 80x24 (realistic) | **36.8 us** | 32.5 us | 1,920 | 16.9 ns |
| 200x60 | **117.3 us** | 89.8 us | 12,000 | 7.5 ns |

Scales near-linearly with area. Per-cell cost drops at larger sizes due to improved cache efficiency. The "realistic" benchmark (diverse Face values and varied line lengths) adds ~14% overhead.

### Net Declarative Overhead

| Phase | Initial Estimate | Measured |
|---|---|---|
| Element construction (view) | ~1-4 us | 5.0-10.4 us |
| Layout calculation (place) | ~1 us | 1.2 us |
| Recursive traversal (paint overhead) | ~0.3 us | Included in paint |
| **Total** | **~4-7 us** | **~7 us** (0 plugins) / **~12 us** (10 plugins) |

**7-12 us out of the full pipeline (57 us) = 12-21%. Relative to terminal I/O (100-700 us) = 1-12%.**
**No practical impact on end-to-end latency.**

## Micro Benchmarks (10)

| Benchmark | What It Measures | Target | Measured | Verdict |
|---|---|---|---|---|
| `element_construct/plugins_0` | view() tree construction (0 plugins) | < 10 us | 5.0 us | OK (2x headroom) |
| `element_construct/plugins_10` | view() tree construction (10 plugins) | < 15 us | 10.4 us | OK |
| `flex_layout` | place() layout calculation | < 5 us | 1.2 us | OK (4x headroom) |
| `paint/80x24` | clear() + paint() combined | -- | 32.9 us | paint-only: 28.5 us |
| `paint/200x60` | clear() + paint() combined (large) | -- | 117.3 us | paint-only: 89.8 us |
| `paint/80x24_realistic` | clear() + paint() combined (varied Face/lengths) | -- | 36.8 us | paint-only: 32.5 us |
| `grid_clear/80x24` | clear() standalone | -- | 4.4 us | |
| `grid_clear/200x60` | clear() standalone (large) | -- | 27.6 us | |
| `grid_diff/full_redraw` | diff() first frame | -- | 39.2 us | First frame only |
| `grid_diff/incremental` | diff() no changes | -- | 12.2 us | Steady-state path |

**Note**: `element_construct` target raised from 10 μs to 15 μs (10 plugins) to account for Salsa sync and plugin registry preparation, which are now part of the measured path. The `grid_diff/full_redraw` increase (24.2 → 39.2 μs) reflects Cell structure growth; this only affects the first frame.

## Integration Benchmarks (8)

| Benchmark | What It Measures | Target | Measured | Verdict |
|---|---|---|---|---|
| `full_frame` | view -> layout -> paint -> diff -> swap | < 16 ms | 57.3 us | OK (279x headroom) |
| `draw_message` | state.apply(Draw) + full frame | < 5 ms | 61.1 us | OK (82x headroom) |
| `menu_show/items/10` | Menu display + full frame | < 5 ms | 70.1 us | OK |
| `menu_show/items/50` | Menu 50 items + full frame | < 5 ms | 73.0 us | OK |
| `menu_show/items/100` | Menu 100 items + full frame | < 5 ms | 71.3 us | OK |
| `incremental_edit/lines/1` | 1 line edit -> view + paint + diff | -- | 52.2 us | |
| `incremental_edit/lines/5` | 5 line edit -> view + paint + diff | -- | 54.3 us | |
| `message_sequence` | draw_status + draw -> full frame | -- | 61.2 us | |

`menu_show` is independent of item count because `menu_max_height=10` caps the visible rows.

## Extended Benchmarks

### JSON-RPC Parsing

| Benchmark | What It Measures | Measured | Notes |
|---|---|---|---|
| `parse_request/draw_lines/10` | JSON-RPC parse (10-line draw) | 58.8 us | |
| `parse_request/draw_lines/100` | JSON-RPC parse (100-line draw) | 517 us | |
| `parse_request/draw_lines/500` | JSON-RPC parse (500-line draw) | 2.55 ms | |
| `parse_request/draw_status` | JSON-RPC parse (draw_status) | 2.85 us | Small message, high frequency |
| `parse_request/menu_show_50` | JSON-RPC parse (menu_show 50 items) | 50.8 us | |

### State Application

| Benchmark | What It Measures | Measured | Notes |
|---|---|---|---|
| `state_apply/draw_lines/23` | state.apply(Draw) standalone | 32.7 us | Includes detect_cursors + compute_lines_dirty |
| `state_apply/draw_lines/100` | state.apply(Draw) standalone | 132 us | O(lines × atoms) cursor scanning |
| `state_apply/draw_lines/500` | state.apply(Draw) standalone | 709 us | |
| `state_apply/draw_status` | state.apply(DrawStatus) | 1.20 us | |
| `state_apply/menu_show_50` | state.apply(MenuShow) | 4.99 us | |

The `state_apply/draw_lines` cost is dominated by `detect_cursors()`, which scans all atoms for cursor attributes (FINAL_FG+REVERSE) and computes display widths via `UnicodeWidthStr`. Kakoune sends only viewport lines (24-80), so the worst case in practice is ~132 μs (100 lines).

### Scaling Characteristics

| Benchmark | What It Measures | Measured | Notes |
|---|---|---|---|
| `scaling/full_frame/80x24` | Full frame at 80x24 | 55.8 us | Baseline |
| `scaling/full_frame/200x60` | Full frame at 200x60 | 232 us | 4.2x (area ratio: 6.25x) |
| `scaling/full_frame/300x80` | Full frame at 300x80 | 410 us | 7.3x (area ratio: 12.5x) |
| `scaling/parse_apply_draw/500` | Parse + apply (500 lines) | 3.21 ms | |
| `scaling/parse_apply_draw/1000` | Parse + apply (1000 lines) | 6.43 ms | Near-linear |
| `scaling/diff_incremental/80x24` | diff() no-change 80x24 | 12.2 us | |
| `scaling/diff_incremental/200x60` | diff() no-change 200x60 | 74.6 us | 6.1x (area ratio: 6.25x) |
| `scaling/diff_incremental/300x80` | diff() no-change 300x80 | 152.9 us | 12.5x (area ratio: 12.5x) |

Full frame scales sub-linearly with area (cache effects). diff() scales linearly.

## TUI Backend Benchmarks

Measured with `kasane-tui/benches/backend.rs` using MockBackend (no real terminal I/O).

### draw_grid (ADR-015 optimized path)

| Benchmark | What It Measures | Measured | Notes |
|---|---|---|---|
| `draw_grid/full_redraw/80x24` | 80x24 all cells | 44.5 us | Cursor auto-advance + incremental SGR |
| `draw_grid/full_redraw/200x60` | 200x60 all cells | 228 us | Large screen |
| `draw_grid/incremental_1line` | 1 line changed | 13.9 us | Diff + SGR for changed row |
| `draw_grid/full_redraw_realistic/80x24` | Realistic data at 80x24 | 43.5 us | Diverse Face values |

### draw (legacy CellDiff path)

| Benchmark | What It Measures | Measured | Notes |
|---|---|---|---|
| `backend_draw/full_redraw/80x24` | 80x24 all cells | 138 us | Escape sequence generation |
| `backend_draw/full_redraw/200x60` | 200x60 all cells | 782 us | Large screen |
| `backend_draw/incremental_1line` | 1 line changed | 1.94 us | Most common pattern |
| `backend_draw/full_redraw_realistic/80x24` | Realistic data at 80x24 | 131 us | Diverse Face values |

**ADR-015 improvement:** `draw_grid()` is **3.1x faster** than the legacy `draw()` path at 80×24 (44.5 μs vs 138 μs) by eliminating per-cell `MoveTo` commands via cursor auto-advance and reducing SGR escape volume via incremental diff (`emit_sgr_diff`).

## E2E Pipeline Benchmarks

JSON bytes -> parse -> apply -> render -> diff -> backend.draw -> escape bytes.

| Benchmark | What It Measures | Measured | Notes |
|---|---|---|---|
| `e2e_pipeline/json_to_escape_80x24` | Full pipeline (uniform data) | 169 us | |
| `e2e_pipeline/json_to_escape_realistic` | Full pipeline (realistic data) | 125 us | |

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

Note: These allocation numbers predate Salsa integration and should be re-measured with `--features bench-alloc` for up-to-date figures.

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
| **Full frame** | 55.1 us | 56.7 us | 81.7 us | 90.0 us | 115.3 us |
| view() | 5.1 us | 5.1 us | 6.2 us | 10.8 us | 12.4 us |
| place() | 1.2 us | 1.2 us | 1.5 us | 5.2 us | 6.5 us |
| clear()+paint() | 31.5 us | 31.7 us | 37.1 us | 42.6 us | 49.1 us |
| diff() | 12.5 us | 12.6 us | 17.3 us | 20.2 us | 39.5 us |

Tail latency is well-controlled. The p99.9 full frame (90.0 us) is only 1.63x the p50 (55.1 us). The max (115.3 us) stays well under 1 ms.

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
| `normal_editing_50msg` | 50 | 4.80 ms | 96 us |
| `fast_scroll_100msg` | 100 | 17.4 ms | 174 us |
| `menu_completion_20msg` | 20 | 1.78 ms | 89 us |
| `mixed_session_200msg` | 200 | 20.4 ms | 102 us |

Fast scroll is heavier per-message due to full buffer redraws.

## GPU CPU-Side Benchmarks

Measured with `kasane-gui/benches/cpu_rendering.rs`.

| Benchmark | What It Measures | Measured | Notes |
|---|---|---|---|
| `gpu/bg_instances_80x24` | Background rectangle instance generation | 3.74 us | |
| `gpu/row_hash_24rows` | Row content hashing (24 rows) | 51.1 us | Change detection for GPU |
| `gpu/row_spans_80cols` | Row span computation (80 cols) | 458 ns | Face run-length encoding |
| `gpu/color_resolve_1920cells` | Color resolution for 1,920 cells | 1.95 us | Theme color -> RGBA |

## Salsa Pipeline Benchmarks

Measured with `cargo bench --bench rendering_pipeline -- salsa`.

### Salsa Input Synchronization

Overhead of syncing AppState changes into Salsa tracked inputs.

| Benchmark | Measured | Notes |
|---|---|---|
| `salsa_sync_inputs/buffer_content/23_lines` | 2.27 us | Typical viewport |
| `salsa_sync_inputs/buffer_content/59_lines` | 5.26 us | Large viewport |
| `salsa_sync_inputs/buffer_content/79_lines` | 6.94 us | |
| `salsa_sync_inputs/buffer_content/realistic_23` | 1.59 us | Realistic face distribution |
| `salsa_sync_inputs/buffer_cursor_only` | 238 ns | Cursor move, no buffer change |
| `salsa_sync_inputs/status` | 213 ns | Status bar update only |
| `salsa_sync_inputs/menu/100_items` | 3.79 us | |
| `salsa_sync_inputs/all_flags/80x24` | 3.08 us | All flags dirty at 80x24 |
| `salsa_sync_inputs/all_flags/300x80` | 7.61 us | All flags dirty at 300x80 |

Cursor-only updates (238 ns) and status updates (213 ns) are extremely cheap, enabling efficient incremental recomputation.

### Salsa vs Legacy Pipeline

| Scenario | Salsa | Legacy | Difference |
|---|---|---|---|
| Full cold (80x24) | 45.7 us | 44.3 us | Salsa +3% |
| Menu select warm | **46.6 us** | 52.5 us | **Salsa −11%** |
| Incremental edit | 48.9 us | 46.4 us | Salsa +5% |

Salsa's value emerges in **warm-cache scenarios**: menu selection (the most interactive operation) is 11% faster because Salsa skips unchanged computations.

### Salsa Scaling Advantage

| Screen size | Salsa | Legacy | Improvement |
|---|---|---|---|
| 80×24 | 54.1 us | 55.8 us | −3% |
| 200×60 | **163.6 us** | 232.1 us | **−30%** |
| 300×80 | **261.9 us** | 410.0 us | **−36%** |

At larger screen sizes, Salsa's incremental computation provides **30–36% speedup** over the legacy pipeline. This is because Salsa avoids redundant recomputation of unchanged element subtrees and layout regions.

### Salsa Patched Path

| Benchmark | Measured | Notes |
|---|---|---|
| `salsa_patched/status_update` | 1.45 us | Status-only PaintPatch via Salsa |
| `salsa_scene/cold` | 27.3 us | Scene cache cold start |
| `salsa_scene/warm` | 5.17 us | Scene cache warm (cached DrawCommands) |

## Cache and Patch Benchmarks

Measured with `cargo bench --bench rendering_pipeline -- cache\|section\|patch`.

### View and Scene Caching

| Benchmark | Measured | Notes |
|---|---|---|
| `view_cache/menu_select_cold` | 11.3 us | Cold cache: full view rebuild |
| `view_cache/menu_select_warm` | 6.5 us | Warm: base subtree cached, menu rebuilt |
| `scene_cache_cold` | 22.8 us | Cold: full scene construction |
| `scene_cache_warm` | 7.0 us | Warm: cached DrawCommands |
| `scene_cache_menu_select` | 18.2 us | Menu section rebuild only |

### Section and Patch Paint

| Benchmark | Measured | vs full_frame | Notes |
|---|---|---|---|
| `section_paint_status_only` | 37.3 us | 65% | STATUS-only sectioned repaint |
| `section_paint_menu_select` | 51.2 us | 89% | MENU_SELECTION sectioned repaint |
| `patch_status_update` | **6.17 us** | **11%** | StatusBarPatch: ~80 cells |
| `patch_menu_select` | **6.80 us** | **12%** | MenuSelectionPatch: ~10 cells |
| `patch_cursor_move` | **1.01 us** | **1.8%** | CursorPatch: 2 cells |

### Line-Dirty Optimization

| Benchmark | Measured | Notes |
|---|---|---|
| `line_dirty_single_edit` | 14.1 us | 1 line changed: selective_clear + paint |
| `line_dirty_all_changed` | 11.8 us | All lines dirty: full repaint |
| `line_dirty_buffer_status/1_line_changed` | 17.2 us | BUFFER\|STATUS combo, 1 line |

## Bottleneck Analysis

Ranked by severity with current measurements.

### Severity: High (Resolved)

#### Buffer Line Cloning

Element tree uses owned types, which would require cloning all buffer lines every frame.

**Resolution: BufferRef pattern (implemented)**. `Element::BufferRef { line_range }` eliminates clone cost. The view() cost of 5.0 μs (0 plugins) includes Salsa sync and plugin registry preparation, confirming that buffer data is not cloned.

### Severity: Medium

#### `detect_cursors()` Cost in state.apply(Draw)

`detect_cursors()` scans all draw atoms for cursor attributes (FINAL_FG+REVERSE) and computes per-atom display widths via `UnicodeWidthStr::width()`. This is O(lines × atoms) per Draw message.

| Lines | state_apply(Draw) | Notes |
|---|---|---|
| 23 | 32.7 us | Typical viewport |
| 100 | 132 us | Large viewport |
| 500 | 709 us | Exceeds 200 μs budget |

Kakoune sends only viewport lines (24-80), so the worst practical case is ~132 μs (100 lines), within the latency budget. However, future optimization opportunities exist:
- Early exit when attribute heuristic succeeds (already implemented)
- Lazy detection scoped to cursor_pos neighborhood
- Display width caching

### Severity: Medium (Resolved by ADR-015)

#### Container Fill Loop — Resolved

`paint.rs` previously performed O(w*h) `put_char(" ")` calls for container background fill. Each call triggered bounds checking, wide-char boundary cleanup (6-8 conditional branches), and CompactString construction.

**Resolution (ADR-015 P4):** Replaced with `clear_region()` bulk operation, eliminating per-cell overhead.

#### diff() Allocation Dominance — Resolved

diff() previously allocated 196 KB per frame (71% of total) due to CellDiff owning cloned Cell data.

**Resolution (ADR-015 P1+P2):** `diff_into()` reuses a caller-provided buffer (zero allocation on warm path). `iter_diffs()` provides zero-copy iteration yielding `&Cell` references. The TUI event loop now uses `draw_grid()` which calls `iter_diffs()` directly, eliminating all CellDiff allocation.

#### grid.diff() Exceeds Target

diff() at 12.2 us (incremental) exceeds the original 10 us target. Cell comparison involves CompactString (24B) + Face (16B) + u8 per cell. The `dirty_rows` optimization helps but the per-cell comparison cost is inherently higher than estimated.

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
| view() cost | 5.0 us (0 plugins) / 10.4 us (10 plugins) |
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
| Cold/Warm ratio | 22.8 μs cold → 7.0 μs warm (3.3x speedup) |

### Stage 4: Compiled Paint Patches — Implemented

| Metric | Value |
|---|---|
| StatusBarPatch | STATUS-only dirty → repaint ~80 cells: **6.17 μs** (vs 57 μs full) |
| MenuSelectionPatch | MENU_SELECTION-only dirty → swap face on ~10 cells: **6.80 μs** |
| CursorPatch | Cursor moved, no dirty flags → swap face on 2 cells: **1.01 μs** |
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

### Stage P3: Line-Dirty Coverage Expansion — Implemented

Extended line-dirty optimization from `dirty == DirtyFlags::BUFFER` (exact match) to `dirty.contains(DirtyFlags::BUFFER)`. The common case of `BUFFER|STATUS` (Draw + DrawStatus in same batch) now benefits from per-line dirty tracking via `selective_clear()`.

| Scenario | Before | After | Savings |
|---|---|---|---|
| BUFFER\|STATUS, 1 line changed | ~57 μs (full pipeline) | ~17 μs | −70% |

### Stage P2: Direct-Grid Backend Draw + Incremental SGR — Implemented

`draw_grid()` on `RenderBackend` trait iterates `grid.iter_diffs()` directly, with two optimizations:
1. **Cursor auto-advance**: Skip `MoveTo` for consecutive cells on the same row (terminal auto-advances after Print)
2. **Incremental SGR**: `emit_sgr_diff()` compares faces field-by-field, emitting only changed attributes/colors

| Benchmark | Legacy `draw()` | Optimized `draw_grid()` | Speedup |
|---|---|---|---|
| Full redraw 80×24 | 138 μs | 44.5 μs | 3.1x |
| Full redraw 200×60 | 782 μs | 228 μs | 3.4x |

### Overall ADR-015 Impact

- **TUI backend I/O**: 3.1–3.4x faster escape sequence generation
- **Per-frame allocation**: 196 KB → 0 (diff phase)
- **Common editing pattern** (BUFFER|STATUS, 1 line): ~70% CPU pipeline reduction
- **Container paint**: ~0.5–2 μs savings per container

## WASM Plugin Benchmarks

Measured with `cargo bench -p kasane-wasm-bench` (wasmtime 42, Component Model, criterion).
See [ADR-013](./decisions.md#adr-013-wasm-plugin-runtime--component-model-adoption) for the full decision record.

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

| Scenario | Measured | Budget (~57 μs) | Notes |
|---|---|---|---|
| 1 plugin full frame | **1.80 μs** | 3.2% | |
| 3 plugins full frame | **5.45 μs** | 9.6% | |
| 5 plugins full frame | **8.91 μs** | 15.6% | |
| 10 plugins full frame | **18.0 μs** | 31.6% | |
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
| CPU pipeline (view→diff→swap) | 57 μs | 232 μs | 410 μs | 4,170 μs |
| Salsa pipeline | 54 μs | 164 μs | 262 μs | 4,170 μs |
| WASM plugins (10) | 18 μs | 18 μs | 18 μs | |
| **CPU total (legacy)** | **~75 μs** | **~250 μs** | **~428 μs** | **4,170 μs** |
| **CPU total (Salsa)** | **~72 μs** | **~182 μs** | **~280 μs** | **4,170 μs** |
| **Budget usage (Salsa)** | **1.7%** | **4.4%** | **6.7%** | |

The CPU pipeline is well within 240fps budget even at large display sizes. Salsa further improves headroom at larger sizes (30-36% faster than legacy).

### Backend Considerations

**TUI**: Not meaningful at 240fps. Terminal emulators refresh at 60-120Hz. `backend.draw()` cost (44-782 μs) is dominated by terminal I/O, not Kasane.

**GUI (wgpu)**: Achievable. The CPU pipeline leaves ~4,100 μs for GPU rendering and presentation at 80×24. With PaintPatch and DirtyFlags, animation frames (smooth scroll, cursor blink) bypass most of the CPU pipeline:

```
Content frame:    parse → apply → view → place → paint → diff → draw  (~57 μs + I/O)
Animation frame:  ──────────── skip ────────────── → scroll offset → GPU draw
```

### Limiting Factors

| Factor | Impact at 240fps | Mitigation |
|---|---|---|
| JSON-RPC parse (500 lines) | 2.55 ms = 61% of budget | Kakoune sends only viewport lines (24-80) |
| ~~diff() allocation (196 KB/frame)~~ | ~~GC pressure → jitter~~ | Resolved: `draw_grid()` + `iter_diffs()` = zero allocation |
| Event source rate | Kakoune is reactive, not 240Hz | Only animation frames need 240fps |

### Conclusion

240fps is achievable for the GPU backend with animation path separation (zero-cost redraws when no new Kakoune data arrives). The CPU pipeline has 4-8x headroom. Per-frame diff allocation (formerly 196 KB) has been eliminated by ADR-015's `iter_diffs()` zero-copy path. Salsa integration further improves large-screen headroom by 30-36%.

## `#[kasane::component]` Compiler-Driven Optimization

The `#[kasane::component]` macro follows Svelte's "let the compiler do the work" philosophy, progressively generating optimized code from declarative `view()` functions ([ADR-010](./decisions.md#adr-010-compiler-driven-optimization--svelte-like-two-layer-rendering)):

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

- [profiling.md](./profiling.md) — how to reproduce measurements
- [semantics.md](./semantics.md) — invalidation and correctness semantics
- [decisions.md](./decisions.md) — ADR history behind optimization work
