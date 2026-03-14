# Profiling Guide

This document is the procedural guide for measuring and profiling Kasane.
For performance principles see [performance.md](./performance.md), and for benchmark results see [performance-benchmarks.md](./performance-benchmarks.md).

## Criterion Benchmarks

### Running all benchmarks

```sh
cargo bench --bench rendering_pipeline
```

### Viewing HTML reports

After running benchmarks, open the HTML report:

```sh
# Each benchmark group produces its own report
ls target/criterion/*/report/index.html
```

### Comparing against a baseline

```sh
# Save current performance as baseline
cargo bench --bench rendering_pipeline -- --save-baseline base

# After making changes, compare against baseline
cargo bench --bench rendering_pipeline -- --baseline base
```

### Running a specific benchmark

```sh
cargo bench --bench rendering_pipeline -- "full_frame"
cargo bench --bench rendering_pipeline -- "paint/80x24"
```

### Running extended benchmarks only

```sh
cargo bench --bench rendering_pipeline -- "parse_request|state_apply|scaling"
```

### Running TUI backend benchmarks

```sh
cargo bench -p kasane-tui --bench backend
```

### Longer measurement time (better statistical precision)

```sh
cargo bench --bench rendering_pipeline -- --measurement-time 5
```

### Allocation measurement (feature-gated)

```sh
cargo bench --bench rendering_pipeline --features bench-alloc -- "alloc"
```

The `bench-alloc` feature installs a counting global allocator that tracks allocation count and bytes during benchmark iterations. It is not included in CI runs.

## iai-callgrind (Deterministic Regression Detection)

iai-callgrind uses Valgrind's callgrind to count instructions, making results fully deterministic and immune to CI runner noise.

### Running

```sh
cargo bench --bench iai_pipeline
```

Requires `valgrind` installed and `iai-callgrind-runner` in PATH:

```sh
sudo apt install valgrind          # Ubuntu/Debian
cargo install iai-callgrind-runner --version 0.14.2
```

### Reading output

Output shows instruction counts, L1/L2 cache stats, and estimated cycles per benchmark function. Instruction counts should be identical (±0.01%) across runs.

### Benchmarked functions

| Benchmark | What it measures |
|-----------|-----------------|
| `iai_full_frame` | view → place → paint → diff → swap (80×24) |
| `iai_parse_draw_100` | 100-line draw JSON parse |
| `iai_state_apply_draw` | state.apply() for 23-line draw |
| `iai_paint_80x24` | paint only (80×24) |
| `iai_grid_diff_full` | Full redraw diff |
| `iai_grid_diff_incremental` | Identical content diff (empty) |

### Baseline management

iai-callgrind automatically compares against the previous run. To save a baseline:

```sh
cargo bench --bench iai_pipeline -- --save-baseline=my_baseline
cargo bench --bench iai_pipeline -- --baseline=my_baseline
```

## Replay Benchmarks

Replay benchmarks simulate realistic multi-message editing sessions, measuring throughput for sustained operation sequences.

### Running

```sh
cargo bench --bench replay
```

### Scenarios

| Scenario | Messages | Description |
|----------|----------|-------------|
| `normal_editing` | ~50 | draw_status + draw loop (typing) |
| `fast_scroll` | 100 | Continuous full-screen redraws (page scroll) |
| `menu_completion` | 20 | menu_show → status updates → menu_show (completion) |
| `mixed_session` | 200 | Combination of all above |

### Adding new scenarios

1. Add a `generate_*()` function in `kasane-core/benches/replay.rs` that returns `Vec<Vec<u8>>` (JSON-RPC message bytes)
2. Use existing fixture helpers from `fixtures.rs`: `draw_json()`, `draw_status_json()`, `menu_show_json()`
3. Add a `group.bench_function()` call in `bench_replay()`

## Allocation Budget Tracking

Allocation counts are deterministic and ideal for CI regression detection.

### Running locally

```sh
cargo run -p kasane-core --bin alloc_budget --features bench-alloc
```

Output is JSON with per-phase allocation counts and bytes:

```json
{"full_frame":{"count":31,"bytes":275924},"view":{"count":6,"bytes":1176},...}
```

### CI integration

The CI workflow runs `alloc_budget` on every push/PR. The output is captured for tracking. If allocation counts increase significantly, investigate the responsible phase.

### Adjusting budgets

The alloc_budget binary reports actual values. When optimizing, compare before/after output to verify allocation reductions.

## Latency Distribution (p99/p999)

Measures tail latency using HdrHistogram for 10,000 iterations. This is a **local-only** tool (wall-clock based, not suitable for CI).

### Running

```sh
cargo bench --bench latency
```

### Output format

```
Full frame latency distribution (10000 iterations):
  p50:       38.2 us
  p90:       42.1 us
  p99:       51.3 us
  p99.9:     89.7 us
  max:      124.0 us
```

Per-phase breakdowns (view, place, paint, diff) are also reported.

### Interpreting results

- **p50**: Typical case. Should be well within 16ms frame budget.
- **p99**: Worst 1%. Directly affects perceived smoothness.
- **p99.9/max**: Outliers from GC, scheduling, or cache effects.

## Latency Budget Tests

Safety net tests that catch catastrophic regressions (10x slowdowns) in CI. Uses generous budgets designed for shared CI runners.

### Running

```sh
cargo test -p kasane-core --test latency_budget -- --ignored
```

### Budgets

| Test | Budget | What it checks |
|------|--------|----------------|
| `full_frame_under_2ms` | < 2ms | Full pipeline (80×24) |
| `parse_request_under_500us` | < 500μs | 100-line draw JSON parse |
| `state_apply_under_200us` | < 200μs | state.apply() (23-line draw) |

Tests run 100 iterations and check the **median**. Budgets are intentionally generous (5-10x typical values) to avoid CI flakiness.

## GUI CPU Benchmarks

Benchmarks the CPU-side work of the GPU rendering pipeline. **No GPU or display server required.**

### Running

```sh
cargo bench -p kasane-gui --bench cpu_rendering
```

### Benchmarked functions

| Benchmark | What it measures |
|-----------|-----------------|
| `gpu/bg_instances_80x24` | Background instance data construction |
| `gpu/row_hash_24rows` | Row content hash (dirty tracking) |
| `gpu/row_spans_80cols` | Text span building for one row |
| `gpu/color_resolve_1920cells` | ColorResolver for all 80×24 cells |

These functions were extracted from `CellRenderer::render()` as free functions in `kasane_gui::gpu::cell_renderer`.

## Runtime Tracing with `perf-tracing`

The `perf-tracing` feature gates tracing spans on hot-path functions. When disabled (default), the `perf_span!` macro expands to nothing (zero cost).

### Enable tracing

```sh
cargo run -p kasane --features kasane-core/perf-tracing
```

Spans are emitted via the `tracing` crate. Use a subscriber like `tracing-subscriber` with `RUST_LOG=info` to capture them.

### Instrumented functions

| Function | Span name |
|----------|-----------|
| `view()` | `view` |
| `place()` | `layout_place` |
| `paint()` | `paint` |
| `grid.diff()` | `grid_diff` |
| `grid.swap()` | `grid_swap` |

## Flamegraph Profiling

### Install cargo-flamegraph

```sh
cargo install flamegraph
```

### Generate a flamegraph from benchmarks

```sh
# Use the profiling profile (release + debug info)
cargo flamegraph --bench rendering_pipeline --profile profiling -- --bench "full_frame"
```

### Using perf directly (Linux)

```sh
# Build with debug info
cargo bench --bench rendering_pipeline --profile profiling --no-run

# Find the benchmark binary
BENCH_BIN=$(cargo bench --bench rendering_pipeline --profile profiling --no-run 2>&1 \
  | grep -oP 'target/profiling/deps/rendering_pipeline-[a-f0-9]+')

# Record with perf
perf record -g --call-graph dwarf "$BENCH_BIN" --bench "full_frame"

# Generate flamegraph
perf script | stackcollapse-perf.pl | flamegraph.pl > flamegraph.svg
```

### Install perf tools (if needed)

```sh
# Arch Linux
sudo pacman -S perf

# Ubuntu/Debian
sudo apt install linux-tools-common linux-tools-$(uname -r)

# flamegraph.pl (Brendan Gregg's scripts)
git clone https://github.com/brendangregg/FlameGraph
export PATH=$PATH:$(pwd)/FlameGraph
```

## CI Benchmark Workflow

The `.github/workflows/bench.yml` workflow runs on every push to master and on PRs.

### What runs in CI

| Step | Tool | Purpose |
|------|------|---------|
| Core criterion benchmarks | criterion | Wall-clock regression tracking (15% threshold) |
| TUI criterion benchmarks | criterion | Backend escape sequence perf |
| Replay benchmarks | criterion | Multi-message session throughput |
| GUI CPU benchmarks | criterion | GPU CPU-side perf |
| iai-callgrind benchmarks | iai-callgrind | Deterministic instruction-count regression |
| Allocation budget | alloc_budget | Per-phase allocation count tracking |
| Latency budget tests | #[test] #[ignore] | Catastrophic regression safety net |

### Thresholds

- **Criterion**: 15% regression triggers a PR comment (does not fail CI)
- **iai-callgrind**: Reports instruction count changes (informational)
- **Allocation budget**: Reports current counts (informational)
- **Latency budget**: Hard failure on >5-10x regression

### Investigating regressions

1. **Criterion alert**: Check the PR comment for which benchmark regressed. Run locally with `--baseline` to confirm.
2. **iai-callgrind change**: Run locally with valgrind to see instruction count delta. Use `--save-baseline` / `--baseline` for comparison.
3. **Allocation increase**: Run `alloc_budget` locally, compare JSON output before/after your change.
4. **Latency budget failure**: Run `cargo test -p kasane-core --test latency_budget -- --ignored` locally. If it passes locally, the CI runner may be overloaded — re-run the CI job.

Historical benchmark results are stored on the `gh-pages` branch at `dev/bench/`.

## See also

- [performance.md](./performance.md) — performance principles
- [performance-benchmarks.md](./performance-benchmarks.md) — current benchmark results
- [semantics.md](./semantics.md) — correctness constraints for measured optimizations
- [index.md](./index.md) — docs entry point
