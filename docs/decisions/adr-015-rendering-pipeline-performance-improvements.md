# ADR-015: Rendering Pipeline Performance Improvements

**Decision:** Incrementally resolve 4 structural inefficiencies in the rendering pipeline.

### Background

The CPU pipeline was ~49 μs (80×24) within the frame budget, but the following inefficiencies were wasting scaling potential and resources:

1. **Per-frame allocation**: `grid.diff()` allocates a `Vec<CellDiff>` every frame (~196 KB on full redraw, 71% of per-frame heap allocation)
2. **Inefficient escape sequence generation**: `TuiBackend::draw()` emits `MoveTo` for every cell and resets+reapplies all SGR attributes on each Face change
3. **Narrow line_dirty optimization coverage**: Only exact match of `dirty == DirtyFlags::BUFFER`. Ineffective for `BUFFER|STATUS` (the most common batch)
4. **Container fill overhead**: `paint_container` executes per-cell `put_char(" ")` with wide character cleanup checks

### Implementation

All 4 stages implemented: (P4) container fill → `clear_region()`, (P1) zero-allocation diff via `diff_into()` / `iter_diffs()`, (P3) line_dirty coverage extension via `selective_clear()`, (P2) `draw_grid()` with cursor auto-advance + incremental SGR diff.

Key results: TUI backend 2.4–3x faster, diff allocation eliminated (196 KB → 0), common editing pattern 57% CPU reduction.

### Implementation Status

#### Stage P4: Container Fill Bulk Optimization — Implemented

Replaced per-cell `put_char(" ")` loop in `paint_container` with `clear_region()`. Eliminates per-cell bounds checking, wide-char cleanup branches, and CompactString construction. ~0.5–2 μs savings per container paint.

#### Stage P1: Zero-Allocation Diff Path — Implemented

| Method | Description | Allocation |
|---|---|---|
| `diff_into(&mut buf)` | Reuses caller-provided `Vec<CellDiff>` | 0 (warm buffer) |
| `iter_diffs()` | Zero-copy iterator yielding `(u16, u16, &Cell)` | 0 |
| `is_first_frame()` | Returns `self.previous.is_empty()` | N/A |

#### Stage P3: Line-Dirty Coverage Expansion — Implemented

Extended line-dirty optimization from `dirty == DirtyFlags::BUFFER` (exact match) to `dirty.contains(DirtyFlags::BUFFER)`. The common case of `BUFFER|STATUS` (Draw + DrawStatus in same batch) now benefits from per-line dirty tracking via `selective_clear()`.

| Scenario | Before | After | Savings |
|---|---|---|---|
| BUFFER\|STATUS, 1 line changed | ~57 μs (full pipeline) | ~17 μs | −70% |

#### Stage P2: Direct-Grid Backend Draw + Incremental SGR — Implemented

`draw_grid()` on `RenderBackend` trait iterates `grid.iter_diffs()` directly, with two optimizations:
1. **Cursor auto-advance**: Skip `MoveTo` for consecutive cells on the same row (terminal auto-advances after Print)
2. **Incremental SGR**: `emit_sgr_diff()` compares faces field-by-field, emitting only changed attributes/colors

| Benchmark | Legacy `draw()` | Optimized `draw_grid()` | Speedup |
|---|---|---|---|
| Full redraw 80×24 | 138 μs | 44.5 μs | 3.1x |
| Full redraw 200×60 | 782 μs | 228 μs | 3.4x |

#### Overall ADR-015 Impact

- **TUI backend I/O**: 3.1–3.4x faster escape sequence generation
- **Per-frame allocation**: 196 KB → 0 (diff phase)
- **Common editing pattern** (BUFFER|STATUS, 1 line): ~70% CPU pipeline reduction
- **Container paint**: ~0.5–2 μs savings per container

### Resolved Bottlenecks

#### Buffer Line Cloning — Resolved

Element tree uses owned types, which would require cloning all buffer lines every frame.

**Resolution: BufferRef pattern (implemented)**. `Element::BufferRef { line_range }` eliminates clone cost.

#### Container Fill Loop — Resolved

`paint.rs` previously performed O(w*h) `put_char(" ")` calls for container background fill.

**Resolution (P4):** Replaced with `clear_region()` bulk operation, eliminating per-cell overhead.

#### diff() Allocation Dominance — Resolved

diff() previously allocated 196 KB per frame (71% of total) due to CellDiff owning cloned Cell data.

**Resolution (P1+P2):** `diff_into()` reuses a caller-provided buffer. `iter_diffs()` provides zero-copy iteration. The TUI event loop now uses `draw_grid()` directly, eliminating all CellDiff allocation.

#### grid.diff() Exceeds Target — Accepted

diff() at 12.2 us (incremental) exceeds the original 10 us target. Cell comparison involves CompactString (24B) + Face (16B) + u8 per cell. The `dirty_rows` optimization helps but the per-cell comparison cost is inherently higher than estimated.

### 240Hz Analysis

The CPU pipeline uses <2% of the 4.17 ms budget at 80×24, making 240fps achievable for the GPU backend with animation path separation:

```
Content frame:    parse → apply → view → place → paint → diff → draw  (~57 μs + I/O)
Animation frame:  ──────────── skip ────────────── → scroll offset → GPU draw
```

TUI is not meaningful at 240fps (terminal emulators refresh at 60-120Hz). GUI (wgpu) has 4-8x CPU headroom. Per-frame diff allocation has been eliminated by `iter_diffs()` zero-copy path. Salsa integration further improves large-screen headroom by 30-36%.

For current benchmark data, see [performance.md](./performance.md).
