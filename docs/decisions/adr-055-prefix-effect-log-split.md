# ADR-055: Prefix / Effect-Log Split for Non-Deterministic Execution Identity

**Status:** Proposed (2026-05-22). Derived from
[ddd-cst-vision.md §4.4.7](../ddd-cst-vision.md). Foundational
theoretical commitment underpinning the content-addressing track
(vision Phase δ). Depends on [ADR-053](./adr-053-algebraic-effect-macros-plugin-sdk.md)
to close the non-determinism boundary.

### Context

Content addressing presupposes determinism: identical inputs produce
identical hashes. The DDD-CST direction (vision §2) commits to
content-addressed plugin distribution (Phase δ) and an auditable
time-store (Phase ζ). But Kasane already contains, and will
increasingly contain, explicitly non-deterministic components:

- **LLM responses** (vision Phase η). The same prompt may produce
  different outputs across calls.
- **External I/O.** File contents, network replies, system clock
  readings are environment-dependent.
- **Effect handlers** (ADR-053). The handler installed on a given epoch
  determines the response to a yield; different handlers produce
  different responses.

Naïvely content-addressing such artefacts is incoherent — there is no
fixed "content" to hash. Two stances exist:

**Stance 1 (rejected): hash the response.** Treat each LLM / I/O
response as content and hash it. This trivially satisfies "identity ≡
content" but loses **reproducibility**: re-running the same prompt
rarely produces the same hash, defeating the point of content
addressing.

**Stance 2 (adopted): separate the deterministic skeleton from the
non-deterministic leaves.** A computation decomposes into:

1. A **deterministic prefix** — pure code + capability handles + input
   references — that *names* what would be computed.
2. An **effect-edge log** — the recorded sequence of effect yields and
   their observed responses.

The computation's identity is the **pair** `(prefix_hash, effect_log_hash)`.

Without this split, the entire identity track of DDD-CST is
non-starter. Vision §4.7.3 D3 calls this the load-bearing edge:
"effects must be the *sole* non-determinism boundary — otherwise content
addressing's identity ≡ content property fails." This is why I3
(effects, ADR-053) must precede or co-evolve with I4 (identity).

### Decision

Adopt Stance 2 as a foundational identity discipline. Concretely:

1. **Deterministic prefix.** A plugin's *code*, *manifest-declared
   capabilities*, and *input dependencies* form a closure that is
   content-addressed. The prefix hash:

   ```
   prefix_hash := H(
       plugin_code_hash,
       sorted(capability_declarations),
       sorted(salsa_input_dependencies),
       sdk_version,
   )
   ```

   Two plugins with bit-identical code but different manifests have
   *different* prefix hashes (vision §4.7.3 D4 — identity reflects
   authority).

2. **Effect log.** Each effect yield records an edge:

   ```
   EffectEdge {
       yield_site_hash: H(prefix_hash, yield_program_point),
       request_hash: H(effect_payload),
       response_hash: H(observed_response),
       epoch: VersionId,
       prev_edge: Option<EffectLogHash>,  // hash chain
   }
   ```

   The log is a Merkle log (hash-chained). `effect_log_hash` is the
   hash of the most recent edge plus its `prev_edge` recursively.

3. **Joined identity.**

   ```
   execution_id := (prefix_hash, effect_log_hash)
   ```

   - **Replay** = re-run the prefix against the recorded effect log;
     outputs are deterministic given recorded responses.
   - **Rerun** = re-run the prefix with a fresh effect log; outputs
     may differ. This is acknowledged in the identity (new
     `effect_log_hash`), not hidden.

4. **Boundary discipline (load-bearing invariant).** **Nothing in the
   deterministic prefix may read non-deterministic state directly.**
   All non-determinism — clock, RNG, network, filesystem, plugin pub/sub,
   LLM, user input — is *announced* via effects (ADR-053). Reading
   non-deterministic state without yielding is a bug that silently
   corrupts content addressing.

   This invariant is checked by:
   - The SDK macro for compile-time WIT-import audit (no `wasi-clocks`
     or `wasi-random` import permitted in prefix code paths).
   - A property test that runs the same prefix twice with identical
     effect logs and asserts identical outputs.

### Scope

**In scope (this ADR — theoretical commitment).**

- Definition of `prefix_hash`, `EffectEdge`, `effect_log_hash`,
  `execution_id` as types in `kasane-plugin-cas` (new crate, empty
  until Phase δ implementation lands).
- The boundary-discipline invariant and its check mechanism
  (compile-time SDK audit + runtime property test harness).
- A specification document, this ADR, that subsequent phase ADRs cite.

**Out of scope (deferred to subsequent ADRs).**

- The actual content-addressed plugin store (Phase δ implementation).
- Effect-log persistence to disk (Phase ζ time-store).
- Effect-log truncation / checkpointing policy.
- Replay UI / debugger.
- Cross-machine sync of execution identities (Phase ζ / Phase θ).
- Cryptographic signing of execution IDs.

### Rationale

1. **Stance 1 is incoherent.** Hashing the LLM response makes every
   re-execution a new identity, which is the same as saying "no
   identity". Content addressing without reproducibility is a misuse
   of cryptographic hashing.

2. **The split has independent precedent.** Nix separates inputs
   (deterministic) from build environment (controlled); the prefix
   here is the analogue. Unison separates AST (hashable) from
   evaluation (run-time). The novelty is the *editor* application,
   not the technique.

3. **The boundary discipline is the only way the invariant holds.**
   ADR-053 closes the imperative escape hatches; this ADR specifies
   what closure is *for*. Without ADR-053, there is no way to
   guarantee that prefix code is pure. With it, the guarantee is
   mechanical: no effect yield → no non-determinism.

4. **The "replay vs rerun" distinction is user-facing.** A bug report
   citing `execution_id = (prefix_hash, effect_log_hash)` is
   reproducible; one citing just `prefix_hash` is reproducible *up to
   recorded LLM responses*. Surfacing the distinction in the identity
   shape avoids the false confidence of "this code is reproducible"
   when in fact only its skeleton is.

5. **Capability declarations are part of the prefix (vision §4.7.3 D4).**
   Two plugins with identical code but different capability manifests
   have different authority and therefore different behaviour on the
   same inputs. The prefix hash encodes this; ADR-052 commits to
   emitting capability declarations hash-stably.

### Alternatives considered

- **Stance 1 (hash the response).** Rejected for incoherence — see
  §4.4.7.
- **Hash only the prefix; treat the effect log as opaque metadata.**
  Loses the auditability claim: bug reports cannot reproduce the
  observed I/O. Loses the replay capability. Rejected.
- **Two-level identity with no Merkle chain on the effect log.** The
  effect log becomes a flat list; tampering becomes undetectable.
  Rejected: the chain costs O(1) per edge and gives integrity.
- **Defer the entire identity question until Phase δ.** Tempting,
  but vision §4.7.3 D3 argues the boundary discipline must be
  *established before* content addressing arrives, not retrofitted.
  Specifically, ADR-053 (effect closure) is justified in part by this
  ADR's needs; rejecting this ADR weakens ADR-053's rationale.

### Consequences

- **Positive.**
  - Content addressing of plugin code is coherent in the presence of
    LLMs and external I/O.
  - Replay / rerun distinction is encoded in identity shape; auditors
    can verify which is which.
  - The boundary discipline gives a sharp invariant; violations
    become bugs with a concrete failure mode (silent identity drift).
  - Capability declarations and identity are *correlated by
    construction* — POLA reflected in identity.

- **Negative.**
  - Long-running sessions accumulate effect logs. Truncation policy
    is out of scope here but unavoidable downstream.
  - Plugin authors must understand that any value they want to be
    content-addressed must come from a yielded effect, not from
    ambient state. Education cost.
  - Bug reports become two-piece identifiers, slightly more friction
    than a single hash.

### Exit criterion

This ADR is foundational and has no implementation exit on its own.
It is *satisfied* when:

- `kasane-plugin-cas` exports the type definitions and the SDK macro
  enforces the boundary discipline at compile time.
- A property test harness can run the same prefix against an effect
  log and assert byte-identical outputs across at least 100 trials.
- A bundled plugin's prefix hash is stable across at least two
  consecutive `cargo build` runs on the same machine.
- The first phase that *uses* execution identities (likely Phase δ
  store) cites this ADR.

### Abandon criterion

- The boundary discipline cannot be mechanically enforced — i.e.
  prefix code routinely reads non-deterministic state without a
  detectable trace. Specifically: if Rust's standard library reads
  the clock, RNG, or environment in ways the SDK macro cannot audit
  (e.g. via FFI or `unsafe` blocks).
- Effect log size grows so quickly that even an aggressive
  truncation policy makes the joined identity meaningless in practice.
- The "replay" property fails on workloads we actually care about —
  i.e. recorded LLM responses cannot deterministically drive the
  prefix because the LLM's intermediate state escaped the effect
  edge (e.g. via global model cache).

If abandoned, the next-best fallback is **hash only the prefix; log
the responses informally**. Content addressing becomes "what code did
this *intend* to run", not "what did it actually do". Audit and
replay are sacrificed. Phase δ can still ship; Phase ζ becomes much
weaker.

### Open questions

- **OQ-1 (vision Q10).** Effect log growth curve under sustained LLM
  use. A 4-hour AI-pair-programming session may produce ≫ 10⁴ edges.
  Truncation policy candidates: epoch eviction, signed checkpoints,
  per-plugin compaction. This ADR leaves the policy open;
  measurement during Phase η informs the choice.
- **OQ-2.** Plugin internal state and prefix purity. A plugin's
  mutable `&mut self` is *between* yields. Is the post-yield state
  part of the prefix (then every yield is a continuation snapshot,
  expensive) or part of the effect log (then identity is per-yield,
  not per-call)? This ADR provisionally places it in the effect log
  via the `response_hash` field; the SDK macro elaborates.
- **OQ-3.** What about Salsa cache state across executions? The cache
  is derived from inputs; it is not part of either prefix or effect
  log. Replays start from cold caches and must produce identical
  outputs. This ADR commits to that property and notes it for the
  Phase δ implementation.
- **OQ-4 (vision Q19).** Time leak. The effect log itself can be a
  time leak — old edges retained beyond need. ADR-053's frame-
  boundary discipline limits *application* state; this ADR's log
  needs its own retention discipline.
- **OQ-5.** Multi-plugin interaction. When plugin A yields an effect
  whose handler reads plugin B's state, the edge response embeds
  B's state hash. Cross-plugin entanglement in identity is intended
  (D4 extended) but raises questions about isolation. Phase η
  re-examines.
