# Performance Overview

This document explains how to read Kasane's performance work and what performance properties the project optimizes for.
For benchmark tables, bottleneck analysis, and optimization status, see [performance-benchmarks.md](./performance-benchmarks.md). For measurement workflows, see [profiling.md](./profiling.md).

## Scope

This document defines performance principles and reading guidance.
It does not attempt to be the canonical source for benchmark tables, per-phase timings, or implementation status logs.

## What Matters

Kasane optimizes for the following properties.

1. Terminal and backend I/O dominate end-to-end latency more than `view()` or `layout()`.
2. Reducing unnecessary invalidation matters more than making already-cheap pure computation marginally faster.
3. Allocation behavior on hot paths matters because jitter is more harmful than raw throughput regressions.
4. Plugin overhead must stay bounded and predictable in both native and WASM paths.
5. TUI and GUI backends share semantics, but have different dominant costs and different fast paths.

## Performance Principles

1. Terminal I/O is usually the dominant cost on the TUI path.
2. Measure before optimizing; performance work must be grounded in benchmarks and replayable workloads.
3. Avoid allocations on hot paths such as paint, diff, and plugin dispatch where possible.
4. Bound plugin costs and keep scaling behavior understandable.
5. Cache only where invalidation can be expressed clearly enough to preserve correctness.
6. Treat caching as part of the rendering policy, not as an invisible implementation detail.
7. Prefer exactness-preserving optimizations first; when exactness is intentionally weakened, document it in [semantics.md](./semantics.md).

## Reading Guide

Use the following documents depending on the question.

| Question | Document |
|---|---|
| What are the current benchmark numbers? | [performance-benchmarks.md](./performance-benchmarks.md) |
| How do I run profiling or collect traces? | [profiling.md](./profiling.md) |
| What correctness constraints do caches and `stable()` obey? | [semantics.md](./semantics.md) |
| Why was a performance architecture decision made? | [decisions.md](./decisions.md) |

## Updating Performance Docs

When performance-related behavior changes, update the documents by role.

1. Update [performance-benchmarks.md](./performance-benchmarks.md) for new numbers, bottlenecks, and optimization status.
2. Update [performance.md](./performance.md) only if the project-level principles or reading guidance changed.
3. Update [semantics.md](./semantics.md) if correctness conditions, invalidation policy, or observability assumptions changed.
4. Update [profiling.md](./profiling.md) if the measurement workflow changed.

## See also

- [performance-benchmarks.md](./performance-benchmarks.md) — benchmark tables and optimization status
- [profiling.md](./profiling.md) — measurement workflows
- [semantics.md](./semantics.md) — correctness conditions for caches and invalidation
- [decisions.md](./decisions.md) — historical rationale for performance decisions
