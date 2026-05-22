# ADR-056: Attenuation Predicate Language (APL)

**Status:** Proposed (2026-05-22). Derived from
[ddd-cst-vision.md §4.2.7](../ddd-cst-vision.md). Foundational
theoretical commitment underpinning attenuation in
[ADR-052](./adr-052-capability-resources-via-wit.md). Defines the
*static* surface of capability attenuation; the dynamic surface lives in
[ADR-053](./adr-053-algebraic-effect-macros-plugin-sdk.md) effect
handlers.

### Context

Attenuation is the operation of producing a weaker capability from a
stronger one. It is the *primary mechanism* by which POLA (Principle of
Least Authority) is achieved in practice: a plugin receives a root
capability and narrows it for each downstream delegation.

For attenuation to be a sound algebra it must satisfy the following
laws (vision §4.2.7):

| Law | Statement |
|---|---|
| Identity | `a.attenuate(⊤) = a` |
| Monotonicity | `a.attenuate(p) ≤ a` |
| Idempotence | `a.attenuate(p).attenuate(p) = a.attenuate(p)` |
| Conjunction | `a.attenuate(p).attenuate(q) = a.attenuate(p ∧ q)` |
| Confluence | `a.attenuate(p).attenuate(q) = a.attenuate(q).attenuate(p)` when `p, q` are independent |

**The decidability problem.** If predicates `p, q` are arbitrary
Wasm-executable functions, every law above becomes undecidable. The
host cannot verify Conjunction or Confluence without running both
predicates on every request — which means attenuation reduces to a
*sequential filter chain*, losing the algebraic structure that
justifies its use.

This is a known fault line in capability theory. Without resolving it,
ADR-052's `cap.attenuate(predicate)` cannot make any algebraic claim;
it becomes a runtime filter with no static guarantees.

### Decision

Define APL — the **Attenuation Predicate Language** — as a closed
grammar whose equivalence, subsumption, and conjunction are decidable
in polynomial time. APL predicates are *data*, not code; the host
evaluates them by case analysis without invoking Wasm.

**Grammar:**

```
predicate ::= path_prefix( P )           -- P : string literal
            | range( field, lo, hi )     -- lo, hi : compile-time const
            | enum_subset( field, S )    -- S : compile-time const set
            | timestamp_before( T )      -- T : compile-time const
            | timestamp_after( T )       -- T : compile-time const
            | predicate ∧ predicate
            | predicate ∨ predicate
            | ¬ predicate
            | ⊤ | ⊥
```

**Atom types** (extensible by ADR, not by plugin):

- `path_prefix(P)` — request path starts with literal `P`.
- `range(field, lo, hi)` — numeric field within `[lo, hi]`.
- `enum_subset(field, S)` — enum-typed field in the const set `S`.
- `timestamp_before(T)` / `timestamp_after(T)` — temporal bound.

**Properties guaranteed:**

- **Closed under conjunction and negation.** `p ∧ q` and `¬p` are in
  APL whenever `p, q` are.
- **Decidable subsumption.** `p ≤ q` (i.e. `p → q`) reducible to SAT on
  a finite Boolean algebra of atoms; tractable because predicates over
  the same field collapse to interval / set algebra.
- **No host-side Wasm execution.** Predicate evaluation is host-native
  case analysis; cost is `O(predicate_depth)`, not `O(wasm_call)`.
- **No time-varying atoms.** Atoms reference compile-time constants
  only. Predicates that depend on *runtime* state (e.g. "buffer
  language is Rust") must escape to effect handlers.

**Escape valve.** A plugin needing a check outside APL **yields a
request as an effect** (ADR-053) and lets the handler decide. This
exits the attenuation algebra into the explicit effect-handler control
plane. The escape is *visible* in the type system:

```rust
// In APL — static, decidable
let read_only = git.attenuate(path_prefix("/repos/myproj/"));

// Outside APL — handler decides, dynamic surface
let conditional = git.guard_by_effect(Effect::CheckRepoState);
```

`attenuate` returns a typed `Cap<S, AplBound>` whose subsumption is
checkable; `guard_by_effect` returns `Cap<S, HandlerBound>` whose
authority depends on runtime handler decisions. The two surfaces are
distinct types — the line between static and dynamic policy is in the
type signature.

### Scope

**In scope.**

- APL grammar specification (above).
- Host implementation of APL subsumption check (`p ≤ q`) and
  conjunction normalisation (`p ∧ q → CNF`).
- WIT type for APL predicates: `predicate` as a recursive `variant` /
  `record` shape, *not* a `func`.
- Property tests for the five algebraic laws on a random APL sample.
- SDK helper: `apl!(path_prefix("/foo") ∧ range(line, 0, 100))` macro
  producing a checked predicate value.
- Integration with ADR-052: `cap.attenuate(apl_predicate)` returns a
  typed handle whose subsumption is verifiable.

**Out of scope.**

- Turing-complete predicates. Forbidden by construction.
- Predicates over plugin-private state. Plugins' internal state is not
  host-inspectable; including it would re-require Wasm execution.
- Predicates referencing time-varying *values* (e.g. "buffer language is
  Rust right now"). Yield to an effect handler instead.
- User-defined APL atoms. The grammar is host-controlled; extending it
  requires an ADR. This prevents plugin-driven grammar bloat.
- Cryptographic predicate signing.

### Rationale

1. **Algebraic claims require decidability.** Vision §4.2.7 walks
   through this: without decidable subsumption, the laws are not just
   unprovable in general — they are *operationally* useless because the
   host cannot exploit them for normalisation. APL recovers the
   algebra.

2. **Predicates as data, not code.** This is the deepest decision.
   Wasm-executable predicates require host-side Wasm calls per check;
   APL predicates are a recursive AST the host walks in native code.
   The performance gap is ~100× per check based on Wasmtime call
   overhead estimates.

3. **The escape valve is *correct*, not a workaround.** Some policy
   genuinely depends on runtime state (e.g. "may edit only when the
   buffer is unmodified"). Pretending APL covers it would silently
   break Conjunction (the value changes between successive checks).
   Yielding to a handler makes the dynamic dependency visible.

4. **The grammar is conservative on purpose.** Five atom types cover
   the first 5 attenuation use cases identified in the perken
   conversations (path scope, line range, repo allowlist, time-bound
   grant, command subset). Vision §19.2' is the decision point: if
   the first real use cases routinely escape to handlers, the grammar
   is widened by ADR, not by plugin authors.

5. **CNF normalisation enables broker fast-paths.** With APL,
   `attenuate(p).attenuate(q)` collapses to `attenuate(p ∧ q)` in the
   host; the broker can perform a single composite check rather than
   walking a chain. With Wasm predicates, no such normalisation
   exists.

### Alternatives considered

- **Arbitrary Wasm predicates.** Rejected per §4.2.7 — kills the
  algebra and increases per-check cost.
- **Datalog-like predicate language.** More expressive but
  subsumption is still decidable. Rejected for now as overkill;
  reconsider if APL proves too narrow.
- **No attenuation; just refusal at the broker.** Plugins must
  request the exact subset they want; the broker grants or denies
  whole capabilities. Rejected: forces capability proliferation
  ("git-read-readme", "git-read-src", ...) and loses POLA's "narrow
  what you have" idiom.
- **Predicates as Rust types only (compile-time only).** No runtime
  representation. Rejected: cross-plugin delegation requires a
  serialisable form; Rust-only predicates can't be passed between
  Wasm modules.

### Consequences

- **Positive.**
  - Attenuation is an algebra, not a filter chain. Conjunction
    normalisation halves host-side check cost on chained attenuations.
  - Static policy surface (APL) and dynamic policy surface (effect
    handlers) are distinct *types*; mixing them is a type error.
  - Predicate subsumption can be used at audit time: "did plugin X
    ever attempt a request outside its declared APL bound?" is
    answerable from the prefix hash (ADR-055) without replay.

- **Negative.**
  - Grammar is restrictive. Real attenuation patterns may need
    repeated escape to handlers; if the escape rate is high, the
    static surface is mostly fictional.
  - Adding atom types requires ADR. This is intentional but
    increases coordination cost.
  - APL ⇄ handler escape adds a categorical decision per attenuation
    site: plugin authors must pick the right form.

### Exit criterion

- APL grammar implemented; subsumption check passes property tests
  for the five laws on randomly-generated predicates (≥ 10⁴ samples).
- The `apl!` macro produces predicates type-checked at compile time.
- ADR-052's `BufferView.attenuate(apl!(range(line, 0, 100)))` works
  end-to-end with broker-side check.
- The first 5 real attenuation use cases (per vision §19.2') express
  in APL without escape. **This is the gating measurement.**

### Abandon criterion

- The first 5 real use cases require escape to handlers. The static
  surface is mostly fictional; vision §19.2' triggers
  re-evaluation. Options:
  - Widen the grammar (datalog-like) — new ADR.
  - Drop the static surface and revert to runtime-only attenuation —
    abandon this ADR.
- APL subsumption proves intractable in practice (e.g. CNF blow-up
  on real workloads, despite polynomial worst case).
- Plugin authors find the APL/handler split too subtle and
  consistently choose the wrong form, leading to either
  under-attenuated or over-blocked plugins.

If abandoned, the next-best fallback is **runtime-only attenuation**:
`cap.attenuate(closure)` runs the closure on every request; no
algebraic claims are made. ADR-052's attenuation still functions but
loses its static guarantees.

### Open questions

- **OQ-1 (vision Q5).** Is the APL grammar expressive enough for the
  first 5 real attenuation use cases? This is the load-bearing
  empirical question; tracked in vision §19 decision point 2'.
- **OQ-2.** Atom-type extension cadence. If new use cases require new
  atoms every few months, the "host-controlled grammar" rule creates
  ADR-fatigue. Acceptable cadence: ≤ 1 new atom per release.
- **OQ-3.** APL serialisation format on the WIT wire. Compact binary
  vs human-readable text vs WIT-native variant tree. This ADR
  commits to the WIT-native variant tree (host-natively parsable, no
  separate parser); reconsider if Wasm Component Model adds a more
  compact encoding.
- **OQ-4.** Interaction with [ADR-055](./adr-055-prefix-effect-log-split.md).
  APL predicates are part of the *prefix* (deterministic); handler
  escapes are *effect-log* edges. The prefix hash must include the
  exact APL tree (not just its semantic class) — two predicates with
  identical semantics but different syntactic form should hash
  identically iff their CNF normal form is identical. This ADR
  commits to hashing post-CNF-normalisation form.
- **OQ-5 (vision Q8).** Static/dynamic duality at the boundary —
  some authority decisions straddle (timed grants where the timestamp
  comparison is APL, the "is this still valid" check is dynamic).
  This ADR's `attenuate` vs `guard_by_effect` split handles the
  common case but not the hybrid. Concrete hybrids surface during
  ADR-052 migration.
