# ADR-024: Perception-Oriented Performance Policy

**Status:** Current

### Context

- vision.md declares "the most perceptive user on the best hardware should be unable to perceive any difference from native Kakoune"
- performance.md operationalizes performance as SLOs and benchmarks, but the values lack perceptual derivation
- Without a stopping condition, optimization becomes self-justifying
- Principle 3 (jitter) was T3 despite being the most perceptually salient artifact
- The document doesn't position Kasane within the full input-to-photon chain

### Decision

Adopt a three-layer performance policy:

**Layer 1 — Perceptual Compass** (strategic direction):
- Goal: Kasane's overhead vs native Kakoune imperceptible to most perceptive user on best current hardware (240 Hz, experienced typist)
- Order-of-magnitude guide, not precise threshold (perception is probabilistic and context-dependent)
- Imperceptibility = stopping condition for optimization

**Layer 2 — Engineering Guardrails** (tactical defense):
- Quantitative SLOs prevent sub-threshold regression accumulation (ratchets, not perceptual thresholds)
- Plugin budgets (< 3 μs) ensure ecosystem scalability (separate from perception)
- CI 115% alert threshold operationalizes the ratchet

**Layer 3 — Optimization Accountability** (justification requirement):
- Below-threshold optimization must state justification:
  (a) Headroom for planned features (multi-pane, plugin growth, larger terminals)
  (b) Structural improvement side effects (e.g., Salsa's primary value is maintainability)
  (c) Regression budget preservation
- Unjustified optimization is over-engineering

### Input-to-Photon Model

Keypress-to-pixel chain for TUI path:

```
keypress → terminal emulator → Kakoune → JSON-RPC → [Kasane] → terminal emulator render → display scanout
```

- Kasane controls only the bracketed segment
- Kasane's steady-state overhead (~59 μs CPU + ~49 μs backend) ≈ 0.1 ms — roughly 2-3% of the 240 Hz scanout period (4.17 ms)
- Even worst practical case (large viewport ~413 μs + backend I/O) stays under 1 ms
- The comparison baseline is native Kakoune, not zero latency — Kasane must not add perceptible overhead on top

### Challenges and Mitigations

| Challenge | Mitigation |
|---|---|
| Perception is probabilistic, not a sharp threshold | Layer 1 provides order-of-magnitude guidance; Layer 2 provides precise ratchets |
| Sub-threshold regressions accumulate invisibly | SLOs as ratchets + CI 115% threshold catch drift |
| Non-perceptual costs (power, resource contention) | Acknowledged as secondary considerations; do not override the perceptual compass |
| "Best hardware" is a moving target | Scope to current + next generation (240-480 Hz); revisit when display technology shifts |
| Composition problem (each component claims imperceptibility, sum is perceptible) | Kasane's budget defined as share of total chain (≤10-25%), not in isolation |

### Implications

- performance.md Principles restructured: Principle 3 (jitter) promoted T3→T1; Principles 9, 10 added at T2
- SLO values unchanged — they coincidentally align with the perceptual derivation
- Historical ADRs (010, 013, 015, 020) not retroactively reframed; policy applies prospectively
- Origin: vision.md line 68. This ADR develops it; performance.md operationalizes it.
