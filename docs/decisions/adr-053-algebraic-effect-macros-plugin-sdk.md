# ADR-053: Algebraic Effect Macros for the Plugin SDK (DDD-CST Phase γ)

**Status:** Accepted (2026-05-22, chunks 1–5 landed). Derived from
[ddd-cst-vision.md §10 (Phase γ)](../ddd-cst-vision.md) and §4.3
(algebraic effects). Refines
[ADR-044](./adr-044-handler-effect-tier-hierarchy.md) (handler/effect
tier hierarchy) and the existing `Effects` type. Pairs with
[ADR-052](./adr-052-capability-resources-via-wit.md) for capability
↔ effect duality.

**Update 1 (2026-05-22, baseline spike):** Wasmtime 43 baseline on
`kasane-wasm-bench/benches/component_model.rs` shows host-call round-
trip at **~115 ns per host call** (`on_state_changed` aggregate
779 ns − guest call 430 ns ÷ 3 host calls) and a single-host-call
guest function at **~400 ns** (`contribute_lines_24`). A CPS-encoded
effect yield maps to one host call plus macro-generated state-machine
bookkeeping; estimated per-yield cost is **≤1 µs** worst-case.
The **>5 µs/yield abandon threshold has ≥5× headroom**. Full
effect-yield benchmark deferred to implementation; macro-debuggability
abandon criterion (Q21) remains the primary open risk.

**Update 2 (2026-05-22, chunks 1–5 landed):**
- Chunk 1: `Effect` enum + `Effectful` / `EffectSet` / `Sealed` / `Yielder`
  traits in `kasane-plugin-sdk::effects`. Starter taxonomy: `Redraw`,
  `EvalCommand`, `SetClipboard`, `PasteClipboard`.
- Chunk 2: `define_plugin!` macro accepts `effects on <Trigger>(<params>)
  { yield Effect::X(...); ... }`. CPS-lowers to existing tier-1
  `KakouneSideEffects` return path via a macro-generated `__KasaneYielder`
  bridge. Supported triggers: `StateChanged`, `Init`, `SessionReady`.
  Conflicting legacy + algebraic blocks for the same trigger are rejected.
- Chunk 3: Compile-time capability projection. The macro scans yield
  sites, demands literal `Effect::<Variant>(...)` constructors (indirect
  yields rejected), unions the per-variant capability sets, and emits a
  `__KasaneEffectSet` marker type with `REQUIRED_CAPABILITIES` populated.
- Chunk 4: `MockHandler` in `kasane-plugin-sdk::effects` (not `-test` —
  lives next to `Effect` / `Yielder` to avoid a circular dep). Closure-
  based predicates: `.respond(pred, reply).reject(pred, error)`.
  Implements `Yielder` so it drops into any test that drives plugin
  `step()`.
- Chunk 5: Migrated cursor-line + color-preview (both bundled plugins
  → 100%, satisfying the ≥80% exit criterion). Per-yield benchmark at
  `kasane-wasm-bench/benches/effect_yield.rs` measures **24–33 ns per
  emit** against `MockHandler` (`emit_unmatched`: ~24 ns, `emit_respond`:
  ~33 ns with `Reply` clone, `emit_reject`: ~28 ns). Adding the ~115 ns
  wasmtime host call gives an end-to-end per-yield cost of **~150 ns —
  ≈33× below the 5 µs threshold**.

**Deferred to a follow-up update:** the manifest-side cross-check
(yielded effect ⊆ manifest-declared capabilities). The chunk-1 capability
namespace ("clipboard") is not yet a recognised key in the existing
`[capabilities]` schema (`wasi`, `services`); aligning the two requires
a capability-namespace design decision that benefits from being made
when a concrete service migration is in flight. Until that lands, the
`REQUIRED_CAPABILITIES` projection is consumed by the chunk-4 mock
handler and is informational at runtime — a partial realisation of the
"security at the type level" goal in §4.3.6.

### Context

Kasane already has a typed `Effects` enum that plugins return from
handler methods. This is **algebraic effects, halfway there**:

- Effects are reified as data (good).
- Effects can be inspected, mocked, batched at the host boundary (good).
- But plugin authors still write **imperative side-effect calls** (bad —
  `ctx.spawn_process(...)`, `ctx.publish_topic(...)`, etc.). These are
  invisible to the host's effect machinery and bypass the auditability
  and testability properties.
- There is no compile-time enforcement that a plugin's *yielded* effects
  match its *manifest-declared* authority. Mismatches surface at
  runtime as host rejections.

The vision §4.3.6 case for completing the algebraic-effect picture rests
on four benefits:

1. **Testing without I/O.** Mock-handler dispatch replaces real I/O for
   deterministic plugin tests.
2. **Security at the type level.** A plugin lacking a manifest-declared
   capability cannot construct the effect that requires it.
3. **Batching.** Host collects all yields per frame, reorders, merges,
   dispatches optimally.
4. **Auditing.** Every effect is logged; the audit trail is the input
   to [ADR-055](./adr-055-prefix-effect-log-split.md)'s effect log.

The cost is a macro layer that compiles a declarative `effects` block
to existing imperative dispatch.

### Decision

**All plugin side effects route through `Effects`.** No imperative
escape hatches remain in the plugin-facing SDK. The `define_plugin!`
macro is extended with declarative effect blocks; the macro generates
the existing imperative form.

New trait surface in `kasane-plugin-sdk`:

```rust
pub trait Effectful {
    type Effects: EffectSet;
    fn step(
        &mut self,
        yielder: &mut <Self::Effects as EffectSet>::Yielder,
    ) -> EffectfulResult;
}

pub trait EffectSet: Sealed {
    type Yielder;
    /// Manifest projection — the set of WIT-level capabilities required
    /// to yield any effect in this set. Computed at compile time.
    const REQUIRED_CAPABILITIES: &'static [CapabilityName];
}
```

Effect declaration in the macro:

```rust
effects on KeyPress(Ctrl + 'd') {
    let path = yield Effect::PickFile(self.workspace());
    yield Effect::OpenBuffer(path);
}
```

The macro:

1. Lowers `yield Effect::X(...)` to `yielder.emit(Effect::X(...))?`.
2. Computes `REQUIRED_CAPABILITIES` from the union of effects mentioned
   in `yield` sites.
3. Emits a compile-time check that `REQUIRED_CAPABILITIES` is a subset
   of the manifest's declared capabilities.
4. Generates the existing `Effects`-returning handler form unchanged for
   wire compatibility with the current runtime.

Test harness in `kasane-plugin-sdk-test`:

```rust
let mut handler = MockHandler::new()
    .respond(Effect::PickFile(_), Path::new("/tmp/foo.rs"))
    .reject(Effect::Network(_));

let result = my_plugin
    .with_effect_handler(handler)
    .step()?;

assert_eq!(
    result.yielded_effects,
    vec![Effect::PickFile(..), Effect::OpenBuffer(..)],
);
```

Continuation semantics: **single-shot only**. Plugin execution is not
forked. After a yield, the handler returns a value and the plugin
resumes exactly once (or the host rejects and the plugin sees `Err`).

### Scope

**In scope.**

- `Effectful` trait and `EffectSet` projection.
- `define_plugin!` macro extension for `effects on <Trigger> { ... }`
  blocks (CPS-encoded under the hood — no Wasm stack-switching
  required).
- Effect taxonomy expansion: a single `Effect` enum mirroring existing
  `Command` variants, plus new variants for external services
  (ADR-052 resources).
- Mock-handler harness in `kasane-plugin-sdk-test`.
- Compile-time capability/effect correspondence check.
- Migration of one bundled plugin as validation (suggested:
  `cursor-line` — minimal effects, easy diff).

**Out of scope.**

- True continuation-capturing semantics. Awaits Wasm stack-switching;
  approximated by CPS macro-generated state machines until then.
- Multi-shot continuations. No editor use case (no search,
  non-determinism, or rollback in plugin path).
- Effect rows / row polymorphism in the type system. Closed enum is
  sufficient and matches Rust's strengths; Frank-style polymorphism
  would require a new language.
- Effect-log persistence — that is [ADR-055](./adr-055-prefix-effect-log-split.md).

### Rationale

1. **Halfway is worse than either extreme.** Today plugins can reach
   side effects via two paths (yield + imperative). The host's effect
   machinery cannot see imperative calls, breaking auditability and
   testability claims. A single path is the only design that delivers
   the §4.3.6 benefits.

2. **Capability/effect duality (D2) becomes mechanically enforced.**
   The macro reads the manifest, intersects with effect-set, and emits
   a `const_assert`. A plugin cannot ship if it yields effects outside
   its manifest. This is the duality from §4.7.3 D2 made *static*.

3. **Single-shot is sufficient.** The "fork and search" idiom that
   multi-shot enables (AI tree search, transactional rollback) has no
   in-plugin use case. AI agents (Phase η) live *outside* plugin code
   and reach Kasane via service resources.

4. **CPS today, real continuations later.** The macro produces a state
   machine, the same way `async fn` lowers. When Wasm stack-switching
   stabilises, the macro's lowering changes; plugin code is unchanged.
   This decouples plugin authoring from runtime evolution.

5. **Mock handler enables test isolation.** Today bundled-plugin tests
   either spawn real subprocesses (slow, flaky) or assert on `Effects`
   shape (brittle). Mock-handler dispatch lets tests assert on the
   effect *sequence* including handler-returned values, without I/O.

### Alternatives considered

- **Stay halfway.** Keep `Effects` as today, accept imperative side
  effects. Rejected: §4.3.6 benefits don't materialise without
  closure of the escape hatches.
- **Effect monad via lifetimes (no macro).** Plugin authors write
  `effect_ctx.do(...)` chains. Rejected: ergonomically worse than the
  macro DSL; verbose enough that authors will reach back for imperative
  shortcuts.
- **Run plugins on a separate effect-driven runtime.** Replace the
  current `Effects`-returning step model entirely. Rejected: enormous
  migration cost and unnecessary — the macro lowers to existing
  dispatch.
- **Defer until Wasm stack-switching stabilises.** Wait for true
  continuations. Rejected: the runtime cost of CPS is acceptable
  (vision §10 budget ≤ 5 µs/yield); waiting blocks ADRs that depend on
  effects being closed (notably 055).

### Consequences

- **Positive.**
  - Capability/effect correspondence is compile-time enforced.
  - Bundled-plugin tests no longer spawn subprocesses.
  - Audit log (ADR-055) has a single coherent stream to capture.
  - The Phase γ deliverable enables Phase δ (content addressing) by
    closing the non-determinism boundary (D3, vision §4.4.7).

- **Negative.**
  - Macro complexity. Compile errors must surface meaningfully through
    the macro layer — vision Q21 flags this as open.
  - CPS-encoded state machines bloat compiled Wasm size (estimate:
    +5–15% per plugin until stack-switching arrives).
  - Effect taxonomy churn: every new external service (ADR-052) adds
    an `Effect` variant. Backwards-compatibility discipline required.

### Exit criterion

- 80% of existing bundled plugins migrate to the effect block form.
- `kasane-plugin-sdk-test` exercises every bundled plugin without
  touching the file system or spawning subprocesses.
- `define_plugin!` macro output passes `cargo clippy -- -D warnings`.
- A plugin that yields an effect outside its manifest fails to compile
  with a comprehensible error (the §17.3 "≤ React hooks" complexity bar).
- Per-yield overhead measured at ≤ 5 µs (vision §10 abandon threshold).

### Abandon criterion

- Macro errors are incomprehensible without exposing the unsugared
  CPS form. (Q21.)
- Effect-handler dispatch adds > 5 µs per yield on the hot path with no
  feasible optimisation.
- The compile-time capability/effect check produces false positives
  often enough that authors disable it.

If abandoned, the next-best fallback is **effects as runtime-only
construct**: keep the imperative escape hatches, but route all of them
through `Effects` at the FFI boundary. The host gains audit and batch
visibility; static enforcement is lost.

### Open questions

- **OQ-1 (vision Q6).** What is the right effect taxonomy? Closed enum
  (this ADR's choice) vs open trait family vs row polymorphism. This
  ADR commits to closed enum; revisit if the variant count exceeds 100
  or if extension by plugins becomes a demand.
- **OQ-2 (vision Q7).** Wasm stack-switching timing. The CPS encoding
  is the bridge; if stack-switching ships within 2 years, the bridge is
  short. If it slips to 5+, CPS becomes the permanent encoding.
- **OQ-3 (vision Q21).** Macro debuggability. Concrete test: write a
  plugin that yields a misspelled effect; is the error pointing at the
  yield site, or at the macro internals?
- **OQ-4.** Interaction with [ADR-051](./adr-051-external-data-as-salsa-inputs.md)
  push-to-set boundary. Effects yielded mid-step may need to wait until
  the next frame for the resulting input update to be visible. This
  ADR assumes the step model already accommodates this (handlers
  resume on the next frame for input-mutating effects); confirm during
  the first migration.
