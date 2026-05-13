# ADR-008: JSON Parser — simd-json

**Status:** Decided

**Context:**
`draw` messages deliver JSON with rows × atoms per frame, so parser performance directly impacts rendering latency (NF-001: under 16ms).

**Decision:** Adopt simd-json.

**Rationale:**
- High-speed parsing leveraging SIMD instructions (SSE4.2/AVX2/NEON)
- serde-compatible API (same `Deserialize` derive as `serde_json`) for type-safe deserialization
- `draw` messages can be large JSON containing tens to hundreds of atoms, making parser performance differences more apparent
- Fallback to `serde_json` is easy if needed (API compatible)
