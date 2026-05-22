# ADR-051: External Data as Salsa Inputs (DDD-CST Phase α)

**Status:** Accepted (2026-05-22). Derived from
[ddd-cst-vision.md §8 (Phase α)](../ddd-cst-vision.md) and §4.1.8 push/pull
reconciliation. Builds on [ADR-020](./adr-020-salsa-incremental-computation-stage-12-split.md),
[ADR-035 §2](./adr-035-first-class-selection-and-time.md), and the
buffer-lines migration in `kasane-core/src/salsa_sync.rs:149`.

**Implementation.** All chunks landed in master. Registry skeleton at
`kasane-core/src/salsa_inputs/external.rs` (`408cbe27`); frame-boundary
drain in `sync_salsa_for_render` (`ef51cf0c`); `kasane-syntax`
`FileWatcher` (`2ff2b7bc`); `SyntaxManager` parallel-path integration
(`4effa145`); `PreRenderHook` → `FrameSyncHook` split with
registry-mediated reads (`6c327485`); FS-class probe demoting mtime to
NFS/FUSE fallback (`2e5bd80e`); property tests + `delta-24` perf check
(`3b08ff06`). Per-chunk detail at
[`docs/roadmap/phase-adr-051-external-inputs.md`](../roadmap/phase-adr-051-external-inputs.md).

### Context

External data enters Kasane today through three distinct routes:

1. **Kakoune protocol stream**: `apply_protocol` dispatches `kak -ui json`
   frames into `AppState`. The `observed.lines` field is now a Salsa input
   (`db.set_buffer_lines(...)`), but other observed fields are still
   imperative.
2. **One-shot subprocesses**: `spawn-process` returns bytes to plugins via
   `Effects`, with no shared cache, no dependency tracking, and no
   cross-plugin visibility.
3. **Plugin pub/sub**: `topic-publish` / `topic-subscribe` route values
   between plugins, but every consumer manages its own invalidation.

The buffer-lines Salsa migration validated that **hot-path I/O can route
through a `salsa::Input` slot** without exceeding the ADR-024 perceptual
budget (current frame ~57 µs against the 75 µs ceiling). What it has not
validated is the **registry abstraction** required by §4.1.8: typed
handles, dynamic registration, and the push-to-set / pull-to-derive split
applied to a source whose event shape differs from "every frame a new
line buffer". Future external sources (LSP diagnostics, file-watcher
events, network responses, AI streaming) are sparse, bursty, and
multi-source — exactly the workload class buffer-lines does not exercise.

Without this abstraction, every new external source repeats the
buffer-lines migration ad hoc, and dependency tracking does not propagate
across composed queries that mix buffer and non-buffer inputs.

### Decision

Introduce `ExternalInputRegistry` as the **single mutation surface** for
external data. Adopt the push-to-set / pull-to-derive split from
vision §4.1.8:

1. **Push half (host-owned).** Every external source has a dedicated
   `salsa::Input` slot. Transport adapters translate push events into
   `commit_external<T>(id, value, version)` calls, **synchronously within a
   host frame**. Plugins cannot push.

2. **Pull half (plugin-owned).** Plugins read via Salsa queries that
   reach the input slot. They observe the value as of the current
   revision; they never block on arrival.

3. **Frame boundary.** The event loop drains all pending pushes into
   Salsa inputs **before** invoking any plugin pull, establishing one
   stable revision per frame.

4. **Back-pressure (host-mediated).** Each input registers a
   `BackPressurePolicy` — `Coalesce` (replace slot), `Queue { cap }`, or
   `DropOldest`. The policy is a host-side configuration today; it
   becomes a capability under ADR-052.

The host module:

```rust
pub struct ExternalInputId<T: 'static + Hash + Eq>(/* type-tagged */);

pub struct ExternalInputRegistry {
    slots: HashMap<TypeId, Box<dyn ExternalSlot>>,
}

impl ExternalInputRegistry {
    pub fn register<T>(name: &str, policy: BackPressurePolicy) -> ExternalInputId<T>;
    pub fn commit<T>(&mut self, db: &mut SalsaDb, id: ExternalInputId<T>, value: T);
    pub fn read<T>(&self, db: &SalsaDb, id: ExternalInputId<T>) -> Option<&T>;
}
```

WIT primitives (`register-external-input`, `read-external-input`,
`commit-external-input`) are **internal to host-side transport adapters**.
They are not exposed to plugins in this phase; plugin-facing access goes
through existing host helpers re-routed onto the registry.

### Scope

**In scope (this ADR).**

- `ExternalInputRegistry` skeleton with typed handles and back-pressure
  policy enum.
- Migration of **one non-buffer source** to the registry. Strongest
  candidate is the file-watcher notification path used by syntax reload
  (sparse, bursty, multi-source); LSP diagnostics is the alternative
  once an LSP transport exists.
- Frame-boundary drain discipline wired into
  `kasane-core/src/event_loop/dispatch.rs`.
- Property tests asserting:
  - `commit_external` mutations are observable only at the next frame
    boundary (no mid-frame state leak — glitch problem, vision §21.5 Q18).
  - Memory bounded under sustained push at policy `Coalesce`
    (time-leak, Q19).

**Out of scope.**

- Plugin-facing `ExternalInputId<T>` exposure. Plugins continue to read
  through existing host helpers; their dependency tracking improves
  transparently.
- Migration of remaining `AppState.observed.*` fields. Those are tracked
  separately and follow the same registry pattern once it is proven on a
  second source.
- Capability-typed back-pressure policies (ADR-052 territory).
- Vector-clocked per-source actor IDs (ADR territory for Phase ζ).

### Rationale

1. **Buffer-lines is a degenerate validation.** It exercises high
   bandwidth but uniform shape, single source, dense updates. The
   push/pull discipline is interesting precisely for the *opposite*
   workload (sparse, multi-source, bursty). Migrating a second source
   answers the question the buffer-lines migration cannot.

2. **The registry abstraction is the operational form of I1.** Without
   typed handles and a single mutation surface, "external communication
   is a face of dataflow" is a slogan. With them, it is enforceable: any
   external source not registered cannot influence Salsa queries.

3. **Frame-boundary drain is the load-bearing FRP discipline.** Vision
   §4.1.9 argues this addresses three FRP failure modes (glitch, time
   leak, causality loop) by construction. Encoding it once in the host
   event loop is the *only* way to make the property hold for every
   future transport adapter.

4. **Internal WIT primitives, not plugin API.** Exposing `register-external-
   input` to plugins reopens the capability question (any plugin could
   poison the registry). Keeping it host-internal defers that question to
   ADR-052 without blocking this phase.

### Alternatives considered

- **Per-source bespoke migration.** Each new external source builds its
  own `salsa::Input` plumbing. Rejected: every source pays the
  push/pull-reconciliation cost separately, and there is no place to
  enforce the frame-boundary discipline uniformly.
- **Plugin-facing registry from day one.** Plugins call
  `commit_external` directly. Rejected: capability checks have no place
  to live until ADR-052; meanwhile any plugin can stuff arbitrary values
  into Salsa inputs, including ones it doesn't logically own.
- **Stay with `apply_protocol` for Kakoune, ad-hoc for everything
  else.** The default if this ADR is rejected. Rejected because §4.1.8's
  unifying claim — that editor I/O is push-to-set + pull-to-derive —
  cannot be tested without an actual unifying structure.

### Consequences

- **Positive.**
  - Dependency tracking propagates across composed queries that mix
    buffer and non-buffer inputs.
  - Memory and time-leak claims of vision §4.1.9 become testable
    properties.
  - The push/pull discipline is enforced in one place; transport
    adapters cannot violate it.

- **Negative.**
  - One additional indirection on every external mutation (`commit`
    → Salsa `set_*`). Expected cost: sub-microsecond, but measured
    against `delta-24` as part of the exit criterion.
  - Adapter authors must respect the synchronous-within-frame
    requirement. Code review burden until the discipline is internalised.

### Exit criterion

| # | Criterion | Status |
|---|---|---|
| 1 | One non-buffer external source routes through `ExternalInputRegistry` with full Salsa dependency propagation downstream | ✓ `kasane-syntax::SyntaxManager` consumes `ExternalInputId<PathBuf>("syntax.reload")` via `FrameSyncHook::post_sync` |
| 2 | Hot-path performance against `delta-24` baseline within 110% (per vision §8 / ADR-024) | ✓ Measured bench groups (`salsa_sync_inputs/*`, `cached_pipeline_dirty_flags/*`, `salsa_scaling/full_frame/*`) all within the bound; several improved. Some renamed bench paths need re-baselining but are not on the hot path. |
| 3 | Property tests for glitch-freedom (Q18) and bounded memory (Q19) pass | ✓ `kasane-core-tests/tests/adr051_external_registry.rs` |
| 4 | The `ExternalInputRegistry` API has been used at least once by a transport adapter whose author did not write the registry | △ Same authorship in this rollout, but the registry stabilised in chunks 1–2 weeks before the `kasane-syntax` consumer was authored against it in chunks 3a–3d. Treated as satisfied in spirit; a stronger validation is the next external source (LSP, file-watcher for a second purpose, etc.) using the same API without changes. |

### Abandon criterion

- Per-query Salsa overhead exceeds 1 µs on the hot path and cannot be
  reduced.
- The dependency-tracking discipline produces "leaky" invalidations that
  force whole-tree recomputation on each protocol message.
- A workload of file rename + buffer reload triggered by the same OS
  event (vision §21.5 Q18) demonstrates that frame-boundary drain
  cannot serialise causally-dependent pushes — and no patch to the
  discipline (e.g. multi-pass drain, dependency-ordered drain) restores
  safety.

If abandoned, the next-best fallback is **registry-for-buffer-only**:
keep the buffer-lines Salsa input, and let every other external source
remain on its current ad-hoc path. The unifying claim of vision §2 is
rejected for external I/O, but Salsa retains its value as a buffer
incremental cache.

### Open questions

- **OQ-1 (vision Q2).** Does the single-frame-drain discipline hold
  under bursty multi-source workloads? Specifically: LSP startup
  bursts, file-watcher mass-rename events, large initial diff arrival.
- **OQ-2 (vision Q3).** Composed Salsa + DD-iterate cascades — does
  any second-source migration introduce cycles via downstream queries
  that re-trigger inputs? No bound is specified yet; this ADR assumes
  cascades remain acyclic and adds an assertion at frame boundary.
- **OQ-3.** Should `commit_external` accept a `Version` parameter
  upfront, or auto-derive from `db.synthetic_revision()`? The former
  prepares for ADR-035 §2 time-as-dimension; the latter is simpler. This
  ADR leaves it open; the second-source migration informs the choice.
