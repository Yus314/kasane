# ADR-013: WASM Plugin Runtime — Component Model Adoption

**Status:** Decided

**Context:**
While evaluating runtime loading approaches for external plugins in Phase 5b, it was necessary to quantitatively assess the performance feasibility of WASM sandboxing. The current compile-time binding approach (`kasane::run()` + `#[kasane::plugin]`) is type-safe but requires rebuilding to add plugins. WASM would enable install-and-activate without rebuilds, expanding the plugin ecosystem.

**Benchmark environment:** `kasane-wasm-bench` crate (wasmtime 43, criterion)

**Evaluation method:** A 4-stage gate approach with pass criteria predefined for each gate, evaluated incrementally.

### 13-1: Benchmark Results

A 4-stage gate approach was used. All gates passed (Gate 3 conditionally — ratio criterion fails for lightweight functions, but absolute values fit within frame budget):

- **Gate 1 (Raw WASM overhead):** ~25 ns/call boundary crossing. Pass.
- **Gate 2 (Data crossing):** 59 ns–4.50 μs depending on payload. Pass.
- **Gate 3 (Component Model overhead):** ~500 ns fixed overhead from canonical ABI. 4.1x–23.7x ratio vs raw module, but absolute values < 7 μs. Conditional pass.
- **Gate 4 (Realistic simulation):** ~1.8 μs/plugin (linear), 5 plugins = 8.91 μs (18.2% of frame budget). Pass.

For detailed benchmark tables, see [performance.md — WASM Plugin Benchmarks](../performance.md#wasm-plugin-benchmarks).

### 13-2: Frame Budget Analysis

5 plugins consume 18.2% of the ~49 μs frame budget; 10 plugins consume 36.7%. L1 cache (DirtyFlags) completely skips WASM calls on frames with no state change (cache hit: 0.26 ns).

For detailed budget breakdown, see [performance.md — WASM Plugin Benchmarks](../performance.md#realistic-plugin-simulation).

### 13-3: Decision

**Adopt Component Model (wasmtime) as the foundation for the plugin runtime.**

**Rationale:**

1. **Sufficient absolute performance**: 18% of budget with 5 plugins, 37% even with 10 plugins. Ample headroom remains for the host-side pipeline.
2. **DX superiority**: Type-safe interface definitions via WIT, automatic serialization (canonical ABI), no manual memory management. Overwhelmingly superior compared to the maintenance cost of raw module's binary protocol.
3. **Language independence**: Plugins can be written in any language supporting the wasm32-wasip2 target, including Rust, C/C++, Go, JavaScript (wasm-bindgen), etc.
4. **Sandbox safety**: WASM's linear memory model prevents plugins from corrupting host memory.
5. **Acceptable startup cost**: Compilation 10 ms + 10 instances 280 μs ≈ 10 ms. Imperceptible to users.
6. **Synergy with caching**: The existing DirtyFlags + PluginSlotCache (L1/L3) mechanism completely avoids WASM calls on frames with no state change.

**Trade-offs:**

- Component Model adds 13-21x overhead for lightweight functions. However, the absolute value is ~550 ns, only 1.1% of the frame budget (~49 μs).
- Raw module approach has 10-20x lower overhead, but manual memory management, binary protocol, and lack of type safety significantly degrade DX.
- Native plugins (current approach) still offer the best performance, but the rebuild requirement limits ecosystem scalability.

**Future direction:**

- Phase W1: WIT interface design (define kasane's Plugin trait equivalent in WIT)
- Native plugins maintained as escape hatches for Decorator/Replacement and other WIT-unexposed APIs
- Host function pattern (guest→host calls for state retrieval) established as the primary data flow
- Leverage Component Model compile result caching (`Engine::precompile_component`) to speed up subsequent startups
