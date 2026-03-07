# Profiling Guide

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

## CI Benchmark Tracking

Benchmark results are automatically tracked on PRs via the `.github/workflows/bench.yml` workflow. A 15% regression threshold triggers a PR comment (but does not fail CI). Historical results are stored on the `gh-pages` branch.
