# DDD-CST Vision — Long-Horizon Roadmap

**Distributed Differential Dataflow with Capability-Secured Time**

## Status

**EXPLORATORY DRAFT.** Not a committed roadmap. This document records a
long-horizon research direction explored in May 2026 as a thought experiment
about the theoretical limit of Kasane's external-communication architecture.

This document is **input to future ADRs, not a binding plan**. Individual
phases may be promoted to operational roadmap items via ADR; until that
happens, treat every claim here as speculative.

Companion documents:
- [vision.md](./vision.md) — the short-horizon, user-facing product vision
- [roadmap.md](./roadmap.md) — the operational, near-term roadmap
- [decisions.md](./decisions.md) — adopted ADRs

If this document conflicts with any of the above, the above wins.

## Reading Guide

- §1–§3 motivate and state the core principle. Read first.
- §4 unpacks the theoretical foundations behind each invariant. Read if
  any of "differential dataflow", "object capability", "algebraic effects",
  "content addressing", "CRDT", or "substructural type" is unfamiliar.
  §4.1.8 (pull/push reconciliation), §4.2.7 (attenuation
  decidability), §4.4.7 (content addressing under non-determinism),
  and §4.7.3 (foundation dependencies) carry the load-bearing
  theoretical claims; readers focused on the architecture's defensibility
  should read these in particular.
- §5–§6 set scope boundaries. Read after §3 or §4.
- §7 is the phase table. Skim once, then jump to phases of interest.
- §8–§16 are the per-phase details. Read on demand.
- §17–§19 cover cross-cutting concerns.
- §21 lists open questions; §20 defines exit conditions. §21.5
  acknowledges the FRP-inherited pathologies that DDD-CST must answer.
- §22 surveys related systems.
- Appendices provide glossary and worked examples.

If you have 10 minutes: §1, §3, §7, §20.
If you have 30 minutes: add §8, §9, §10 (the first three phases).
If you have time for theory: §4 in full — at minimum §4.1.8, §4.2.7,
§4.4.7, §4.7.
If you are evaluating whether to invest: §17–§21 are the load-bearing
sections.

---

## 1. Why This Document Exists

Kasane's external-communication question — "how do plugins talk to git
daemons, LSP servers, file systems, networks?" — has been answered three times
already:

1. **Existing primitives**: `spawn-process`, pub/sub, `ProcessTaskSpec`. These
   cover one-shot subprocess + intra-plugin messaging.
2. **perken's proposal** (AntoineBalaine/kasane `upstream_port`, 2026-05):
   add a host-mediated daemon registry with `query-daemon` WIT primitive
   for shared long-lived services.
3. **Salsa-first proposal** (this conversation, see §8 Phase α): treat
   external data as Salsa inputs, unifying memoisation, dependency tracking,
   and cross-plugin sharing.

Each of these is a partial answer. They cover increasingly larger fractions
of the problem space but leave a residue:

- (1) cannot share state across plugins
- (2) cannot integrate cleanly with the existing Salsa/TEA layer
- (3) does not handle event push, distribution, or AI integration

The deepest reframing — that "external communication" is not a separate
problem at all, but the visible face of *incremental computation over
capability-secured time-versioned values from many concurrent sources* —
admits a single substrate that subsumes all three. That substrate is what
this document calls **DDD-CST**.

DDD-CST is unlikely to be implemented in its full form by any single
contributor. The reason to write it down is:

- Future ADRs benefit from having the asymptote drawn
- Individual phases (especially α through δ) are valuable in isolation
- It is easier to evaluate a near-term proposal against a far-term north star
  than to evaluate it against an open horizon

## 2. The Core Principle

> **External communication is not a separate problem. It is one face of a
> more fundamental problem: how does an editor compute incremental,
> time-versioned, capability-secured dataflow over evolving values from many
> sources?**

If that more fundamental problem is solved, "external communication" is no
longer a category in the architecture — it dissolves into "dataflow nodes
backed by external producers" with the same machinery as buffer content,
cursor position, user preferences, and plugin-derived state.

## 3. The Four Invariants

Any phase of DDD-CST must preserve these. Phases that violate one of them
are not DDD-CST phases — they are something else.

| # | Invariant | What it forbids |
|---|---|---|
| **I1** | Dataflow as compute model | string-keyed lookup as a primary API, hand-written cache invalidation, per-plugin re-implementation of derived computations |
| **I2** | Capability as authority model | ambient authority, manifest-grep-based gating, string-typed permission tokens |
| **I3** | Algebraic effects as side-effect model | scattered side effects in plugin code, untracked I/O, untyped command emission |
| **I4** | Content-addressed time as persistence model | mutable global state, version-numbered (rather than hash-identified) artifacts, single-writer assumption |

These invariants are violated by every existing editor and every existing
Kasane component. That is expected — they describe the *target*, not the
present.

The invariants are abbreviations for substantial bodies of theory. §4
unpacks them. Readers already fluent in differential dataflow, object
capabilities, algebraic effects, content-addressing, CRDTs, and
substructural types may skip to §5. Others should read §4 before §7's
phase descriptions, which assume this background.

## 4. Conceptual Foundations

Each invariant in §3 names a body of theory developed over decades by
distinct research communities. This section explains them at the depth
needed to evaluate the phase proposals in §7. The goal is operational
understanding, not exhaustive coverage — citations point readers to the
canonical literature.

### 4.1 Differential Dataflow — Foundation for I1

#### 4.1.1 Origins

The lineage runs roughly:
- **Datalog incrementalisation** (1980s–1990s): bottom-up evaluation of
  recursive queries; first hints of delta-based reasoning.
- **Stonebraker's stream-database work** (Aurora, Borealis, early 2000s):
  continuous queries over streams of tuples.
- **Naiad** (Murray, McSherry, Isaacs, Isard, Barham, Abadi, MSR 2013):
  introduced **timely dataflow** — dataflow operators that carry
  *timestamps* drawn from a lattice, enabling unified treatment of
  iteration, incremental update, and out-of-order arrival.
- **Differential Dataflow** (McSherry et al.): built on timely dataflow,
  added the **differential** part — collections evolve via delta
  multisets, operators propagate deltas with overhead proportional to
  change size rather than full re-evaluation.
- **Materialize** (commercial, 2019+): productionised differential
  dataflow as a streaming database.

The Rust ecosystem hosts both libraries (`timely-dataflow`,
`differential-dataflow`) maintained by McSherry and contributors.

#### 4.1.2 Core Idea

A computation is a **dataflow graph**. Nodes are *operators*. Edges carry
**collections** — multisets of typed tuples. Each tuple is tagged with a
**timestamp** from a partially-ordered set (a lattice).

When an input collection changes, the change is expressed as a delta: a
multiset of (tuple, time, multiplicity) triples. Positive multiplicity
adds occurrences; negative removes. Operators receive deltas, update
internal state, and emit output deltas.

Crucially, the *cost* of processing a change is roughly proportional to
the size of the change, not the size of the underlying collections. This
is the "incremental" promise made precise.

#### 4.1.3 Operator Vocabulary

DD's operator set is small and composes broadly:

| Category | Operators | Behaviour under delta |
|---|---|---|
| Element-wise | `map`, `filter`, `flat_map` | Pass deltas through transformed |
| Reducing | `count`, `sum`, `distinct`, `group_by` | Maintain per-key aggregate, emit aggregate delta |
| Combining | `join`, `antijoin`, `semijoin`, `concat` | Maintain index per side, emit delta of join |
| Recursive | `iterate` | Compute least-fixed-point until delta is empty |
| Time | `delay`, `consolidate` | Manipulate timestamps |

`iterate` is what gives DD its name "differential": it propagates
*differences* between fixpoint iterations rather than recomputing the
entire fixpoint each round. This is what makes recursive query
evaluation tractable.

#### 4.1.4 Time as a Lattice

DD's most subtle move: timestamps are not totally ordered. They are drawn
from a **lattice** — a partial order with well-defined least-upper-bound
(join) and greatest-lower-bound (meet) operations.

This lets the same operators handle:
- **Linear time** (single coordinate): timestamps are natural numbers.
- **Iterative time** (loop counter): timestamps include the iteration
  count.
- **Distributed time** (vector clock): timestamps include per-actor
  counters.
- **Hybrid time**: combinations of the above.

The operator implementation is the same in every case; only the
timestamp type changes.

#### 4.1.5 Why It Matters Here

An editor frontend computes many derived values from many inputs:
- Line layout from buffer text + window width
- Syntax tokens from buffer text + grammar
- Display directives from layout + plugin transforms
- Fold ranges from indent levels + fold requests
- Diff marks from buffer + git daemon response

Each derived value has a fan-in of inputs and a fan-out of consumers.
Updating one input invalidates a tree of dependent values; recomputing
everything is wasteful. DD provides operators that propagate exactly the
minimum delta.

Salsa solves a subset of the same problem (demand-driven memoisation
under a single linear time), but lacks DD's iterate operator and
partial-order time. Phase α adopts the DD operator vocabulary in a
Salsa-implementable subset; Phase ζ extends to partial-order time.

#### 4.1.6 Comparison Table: Salsa vs Differential Dataflow

| Feature | Salsa | Differential Dataflow |
|---|---|---|
| Compute style | Demand-driven (pull) | Eager propagation (push) |
| Time | Single linear epoch | Partial-order lattice |
| Recursion | Manual via input cycle | Built-in `iterate` |
| Distribution | Single-machine | Cluster-scale |
| Maturity in Rust | Production (rust-analyzer, Kasane) | Production (Materialize) |
| Library size | ~10K LoC | ~50K LoC |
| Memory model | Per-query memoisation | Per-operator arrangement (sorted, shared) |
| Best fit | Compiler-style dependency graphs | Streaming aggregations, joins, recursion |

**Operational summary**: Salsa is DD restricted to (a) single-coordinate
time, (b) no `iterate`, (c) demand-driven evaluation. For most current
Kasane uses, those restrictions are fine. Phase ζ lifts them when needed.

#### 4.1.7 Limits

- Per-tuple overhead is microseconds-scale; hot paths with millions of
  tuples per frame are unsuitable.
- Stateful operators retain history; memory growth must be managed.
- Confluent monotone semantics is the natural fit; non-monotone updates
  (retractions) are supported but require careful operator selection.
- Literature is database-centric; UI-oriented applications of DD are
  underdocumented.

#### 4.1.8 The Pull/Push Reconciliation Problem

Salsa is *demand-driven*: a query computes only when asked. Differential
Dataflow is *data-driven*: an input change propagates eagerly through
operators. Editor I/O is *event-driven*: Kakoune emits protocol frames,
LSP servers push diagnostics, file watchers fire on changes, network
sockets receive bytes. The frontend has no syntactic moment of "asking"
for these events; they arrive.

This is the **load-bearing reconciliation problem** of Phase α. It is
not a detail; if it has no clean answer, the Salsa-input model fails on
the editor's actual input shape.

The mismatch in concrete terms:

| Aspect | Salsa (pull) | Editor I/O (push) |
|---|---|---|
| Initiator | Consumer ("give me the line layout") | Producer ("here is a protocol frame") |
| Reaction | Computed when read | Must be absorbed when arrived |
| Back-pressure | Implicit (consumer pacing) | Explicit (host buffering, drop policy) |
| Ordering | Total within a revision | Causal across sources, concurrent within |

**The reconciliation pattern** that makes this tractable is a
*push-to-set, pull-to-derive* split:

1. **Push half** — host-owned: every external source has a dedicated
   `salsa::Input` slot. The transport adapter (Kakoune connection, LSP
   socket, file watcher) translates push events into `set_*` calls on
   the appropriate input, **synchronously within a host frame**. This
   is the only mutation surface; plugins cannot push.

2. **Pull half** — plugin-owned: plugins read via Salsa queries. They
   see the input value as of the current revision. They never block on
   external arrival; if the data has not arrived, they read whatever
   default or previous-revision value is in the slot.

3. **Frame boundary** — the host's event loop: drains all pending push
   events into Salsa inputs *before* invoking any plugin pull. This
   establishes a stable revision per frame, mirroring how React batches
   state updates before re-rendering.

4. **Back-pressure** — host-mediated: if push events arrive faster than
   frames can drain them, the host applies a per-source policy
   (coalesce by replacing the slot, queue with a cap, or drop oldest).
   These policies are themselves capabilities — Phase β.

5. **Causality** — preserved by frame-internal ordering until Phase ζ
   introduces vector-clocked inputs; concurrent pushes from independent
   sources are interleaved arbitrarily within a frame but
   deterministically across frames.

#### 4.1.9 Why The Reconciliation Works

The split is sound because *every* push-side source has a defining
property: **arrival rate at the editor is bounded by human
perception**. Kakoune emits at most a few hundred frames/sec; LSP
replies are network-latency-bound; file watchers fire at filesystem
event rates. None approach the pull rate Salsa would need to sustain.
This is what makes "drain all pushes before any pull" a stable
discipline rather than a starvation risk.

Three FRP failure modes are addressed by this split:

- **Glitch problem** (transient inconsistent state when multiple inputs
  update non-atomically): avoided by draining all pushes *before* any
  pull within a frame. Plugins never observe a mid-update state.
- **Time leak** (event history accumulating in memory): bounded by the
  Salsa slot model — each input holds only its current value; history
  is delegated to ADR-035's explicit history dimension.
- **Causality loop** (a derived value feeding back into its own input):
  forbidden by construction — only host-owned transport adapters may
  call `set_*`; plugin output flows through `Effects`, not back into
  Salsa inputs.

The cost of this discipline is a single-frame latency between event
arrival and plugin visibility. For editor workloads, this is
imperceptible.

#### 4.1.10 What DDD-CST Adopts

- **Operator vocabulary** as the plugin compute model (map / join /
  iterate / reduce as first-class primitives)
- **Delta propagation** as the invalidation mechanism
- **Lattice-typed time** for Phase ζ onward
- **Push-to-set, pull-to-derive split** as the bridge between editor
  events and Salsa queries

#### 4.1.11 What DDD-CST Does Not Adopt (Yet)

- Cluster execution (deferred to Phase θ)
- Multi-version concurrency control (a DD strength that editors don't
  need)
- Full timely dataflow's progress tracking (overkill for single-machine)
- Sub-frame Salsa observation of in-flight pushes (forbidden — would
  reintroduce the glitch problem)

### 4.2 Capability Theory — Foundation for I2

#### 4.2.1 Origins

- **Dennis & Van Horn** (1966): "Programming Semantics for Multiprogrammed
  Computations" — the founding capability paper, introduces the term and
  the core idea.
- **Hewitt's Actor model** (1973): every actor is a capability;
  communication is by passing actor references.
- **KeyKOS** (1980s) and **EROS** (1990s): capability-secured operating
  systems demonstrating practical OS-scale capabilities.
- **E language** (Tribble, Miller, late 1990s+): object capabilities in
  a programming language with promise pipelining.
- **Mark Miller's dissertation** (Johns Hopkins, 2006): "Robust
  Composition: Towards a Unified Approach to Access Control and
  Concurrency Control" — the canonical modern reference.
- **Spritely Goblins** (Lemmer-Webber, 2020+): capability-secured actors
  in Guile Scheme; ongoing decentralised-systems work.
- **Cap'n Proto / Cap'n Web** (Varda, Cloudflare): capability-secured
  RPC at production scale.

#### 4.2.2 Three Pillars of Capability Security

**Pillar 1: Unforgeable reference.** A capability is an object reference
(handle, token, fat pointer). The *only* way to obtain one is to be
given one. There is no "look up by name" facility — names are
designations, not authorities.

**Pillar 2: No ambient authority.** A program with no capabilities can
do nothing. It cannot read files, send network requests, or invoke
external services. Every authority is explicit, possessed by some
specific actor.

**Pillar 3: Authority is propagated by reified grants.** When actor A
gives actor B a capability, that grant is a first-class operation, not
implicit context. Because grants are explicit, *revocation* is possible
— A can later invalidate B's grant.

These three combine into a property called **POLA** (Principle of Least
Authority): each actor naturally tends to hold the minimum capabilities
required for its task, because broader capabilities require explicit
grant.

#### 4.2.3 The Confused Deputy

The textbook example of capability-design failure:

A compiler service has authority to write to a billing log (so it can
charge for compilations). It accepts an output path argument from
clients. A malicious client passes the billing-log path as the output;
the compiler dutifully overwrites the log with its compiled artefact.
The compiler is the "confused deputy" — its broad authority got
attributed to the client's narrow intent.

Capability fix: the compiler holds a *capability* to write to the
billing log (and only to the billing log). The output path the client
provides is interpreted in the client's authority. The compiler can't
confuse the two because they're different capability handles with
different types.

This pattern recurs constantly in real systems. Capabilities make it
representable; without them, every API call risks confused-deputy bugs.

#### 4.2.4 Canonical Patterns

**Revoker Pattern**:
```
let (cap, revoker) = make_revokable(target_cap);
give(cap, untrusted);
// ...
revoker.revoke();  // cap.invoke() now returns Err(Revoked)
```
The revoker is itself a capability — a capability to "make this other
capability dead". Composable: any cap can be wrapped.

**Membrane Pattern** (Miller):
```
let membrane = Membrane::new();
let wrapped = membrane.wrap(original_cap);
give(wrapped, untrusted);
// All capabilities that crossed the membrane are tracked
membrane.revoke_all();  // wipes them all at once
```
The membrane intercepts every capability that crosses a trust boundary.
Useful for sandbox teardown: revoke once to clean up an entire
plugin's authority.

**Sealer / Unsealer** (Morris's brand pattern):
```
let (seal, unseal) = make_brand("git-admin");
let sealed_box = seal(git_admin_cap);
// pass sealed_box through untrusted code; it's opaque
let recovered = unseal(sealed_box)?;  // only unsealer holder succeeds
```
Used for *rights amplification* — a sealed value can be passed widely,
but only the unsealer's holder can extract authority from it. Also
implements nominal types in untyped settings.

**Sturdy References** (Tahoe-LAFS, Spritely Goblins):
```
let sturdy = sturdyfy(cap, host_secret);
// serialise sturdy to disk
// ... process restart ...
let cap = resolve(sturdy, host_secret)?;
```
Capabilities normally exist only at runtime. A sturdy ref is a
serialised form that survives across restarts, provided the host
retains its secret. Required for "remember plugin authorisations".

**Promise Pipelining** (E language, Cap'n Proto):
```
let r1 = remote_obj.method1();    // returns promise
let r2 = r1.method2();            // chains before r1 resolves
let r3 = other_obj.combine(r1, r2);
// All three calls are sent in one network round-trip
```
Without pipelining: 3 round-trips. With pipelining: 1. Critical for
cross-network capability operations; less critical locally but still
useful.

#### 4.2.5 Capability vs ACL

Why prefer capabilities over Access Control Lists (the dominant
alternative in commercial systems)?

| Property | ACL | Capability |
|---|---|---|
| Check at | access time | possession time |
| Identity | required | not required |
| Authority | ambient (caller's identity) | explicit (handle held) |
| Confused deputy | inherent risk | prevented by construction |
| Composition | difficult (whose identity?) | natural (compose handles) |
| Auditability | identity log | capability provenance |
| Revocation | per-identity flush | per-grant revoker |

ACLs work well at the OS-user boundary (a stable identity exists, the
OS enforces it). They break down inside an application where "identity"
is a fiction the application invents.

Capabilities work well wherever the *handle* can be made unforgeable.
Inside an application this is enforced by the type system; across
networks by cryptography.

#### 4.2.6 Limits of Capability Theory

- **User-facing UI is hard.** "Grant cap X to plugin Y" is a question
  users struggle with. Existing systems (Android permissions, macOS
  Privacy & Security, iOS) use heuristics or simple dialogs; none
  perfectly.
- **Denial-of-service is orthogonal.** A capability holder can spam
  legitimate requests, exhausting resources. Rate-limiting, quotas, and
  priority must be layered on.
- **Distributed delegation chains are fragile.** Long chains of grants
  across actors are hard to reason about; the literature is thinner
  here.
- **Capabilities don't address all security properties.** Information
  flow, side channels, and timing attacks need additional mechanisms.

#### 4.2.7 Attenuation and the Decidability Constraint

Attenuation is the operation of producing a weaker capability from a
stronger one. It is the *primary mechanism* by which POLA is achieved
in practice: a plugin receives a powerful root capability and narrows
it for each delegation.

For attenuation to be a sound algebra it must satisfy:

| Law | Statement |
|---|---|
| Identity | `a.attenuate(⊤) = a` |
| Monotonicity | `a.attenuate(p) ≤ a` |
| Idempotence | `a.attenuate(p).attenuate(p) = a.attenuate(p)` |
| Conjunction | `a.attenuate(p).attenuate(q) = a.attenuate(p ∧ q)` |
| Confluence | `a.attenuate(p).attenuate(q) = a.attenuate(q).attenuate(p)` when `p, q` are independent |

**The decidability problem**: if predicates `p, q` are arbitrary
Wasm-executable functions, every law above becomes undecidable. The
host cannot verify Conjunction or Confluence without running both
predicates on every request — which means attenuation reduces to
"sequential filter chain", losing the algebraic structure that
justifies its use.

This is a known fault line in capability theory. The literature offers
two stances: restrict predicates to a decidable fragment, or accept
opacity.

**DDD-CST's choice: decidable fragment.** Phase β specifies the
*Attenuation Predicate Language* (APL) as a closed grammar whose
equivalence and subsumption are decidable in polynomial time:

```
predicate ::= path_prefix( P )       -- P : string literal
            | range( field, lo, hi ) -- lo, hi : compile-time const
            | enum_subset( S )       -- S : compile-time const set
            | timestamp_before( T )  -- T : compile-time const
            | predicate ∧ predicate
            | predicate ∨ predicate
            | ¬ predicate
            | ⊤ | ⊥
```

Properties:

- **Closed under conjunction**: `p ∧ q` is in the language if `p, q` are.
- **Decidable subsumption**: `p ≤ q` (i.e. `p → q`) reducible to SAT on
  a finite Boolean algebra of atoms; tractable because predicates over
  the same field collapse to interval / set algebra.
- **No host-side execution**: predicates are *data*, not code. The host
  evaluates them against request attributes by case analysis.
- **No Wasm trap per check**: predicate evaluation is host-native; cost
  is `O(predicate_depth)`, not `O(Wasm_call)`.

The grammar deliberately excludes:

- Arbitrary computation (Turing-complete predicates)
- Predicates over plugin-private state (would break host-side checking)
- Predicates referencing time-varying values (would break idempotence)

A plugin needing a check outside APL must yield the request as an
effect (Phase γ) and let the handler decide — exiting the attenuation
algebra into the explicit effect-handler control plane. This is the
*correct* escape valve: it makes the line between "static authority
bound" and "dynamic policy check" visible in the type system.

#### 4.2.8 What DDD-CST Adopts

- **Unforgeable handles** via WIT `resource` (Wasm Component Model)
- **POLA** as a design discipline for every plugin manifest
- **Revoker and membrane** patterns for plugin teardown
- **Sturdy refs** for persistent plugin authorisations
- **Promise pipelining** for distributed dataflow (Phase θ onward)
- **APL** (Attenuation Predicate Language) as a decidable predicate
  fragment for attenuation

#### 4.2.9 What DDD-CST Does Not Adopt (Yet)

- Full distributed capability transport (Phase θ scope)
- Cryptographic capability signing (could come with Phase δ's
  content-addressing)
- Verified capability propagation via type-system enforcement (requires
  dependent types or substantial Rust-macro work)
- Turing-complete predicates in attenuation (forbidden by §4.2.7
  decidability constraint — escape to effect handlers instead)

### 4.3 Algebraic Effects — Foundation for I3

#### 4.3.1 Origins

- **Plotkin & Power** (2001+): algebraic theory of computational effects
  — effects modelled as operations of an algebraic signature.
- **Plotkin & Pretnar** (2009): "Handlers of Algebraic Effects" — added
  effect handlers as a generalisation of exception handling.
- **Eff language** (Bauer, Pretnar, 2012): first practical language with
  algebraic effects.
- **Frank** (Lindley, McBride, Hillerström, 2017): elegant calculus with
  user-defined effects.
- **Koka** (Leijen, Microsoft Research, 2014+): effect-typed
  mainstream-style language with row-polymorphic effects.
- **OCaml 5** (Sivaramakrishnan et al., 2022): multicore OCaml
  implemented via effect handlers.
- **React Suspense** (Facebook, 2018+): JS adaptation of
  effect-handler-style suspension.

#### 4.3.2 The Idea

A function performs side effects by **announcing** them, not by
executing them. `perform Effect::ReadFile(path)` says "I want to read a
file"; the enclosing **handler** decides what actually happens. The
function can be resumed with the result, or not resumed at all.

This is similar to:
- **Exceptions**, but with the option to *resume* the function
  after handling.
- **Coroutines**, but with typed effect tracking.
- **Monads** (Haskell IO), but compositional in a way monads aren't
  (no monad-transformer stack required).
- **Free monads**, with which they are formally equivalent.

#### 4.3.3 Two Conceptual Halves

**Effect declaration.** A function declares (in its type) which effects
it may perform:

```
fn read_config<E: Reads<ConfigFile> + Parses<Json>>()
    -> Result<Config, ConfigError>
{
    let raw = perform Effect::ReadFile("/etc/config")?;
    let parsed = perform Effect::Parse::<Json>(raw)?;
    Ok(parsed)
}
```

The function doesn't *do* anything I/O-related. It announces intent. The
type signature lists which announcements are possible.

**Handler installation.** A surrounding scope provides interpretations:

```
handle {
    read_config()
} with {
    Effect::ReadFile(path) => resume(fs.read(path)),
    Effect::Parse(raw)     => resume(json::from_str(raw)),
}
```

The handler decides: actually perform (`resume(value)`), abort
(`fail()`), substitute (`resume(mock_value)`), or capture the
continuation for later (`save(resume)`).

#### 4.3.4 Deep vs Shallow Handlers

**Deep handlers** handle all yields throughout the wrapped function's
execution. After a yield is handled, control continues *inside the
same handler*. This is the default in most production designs.

**Shallow handlers** handle exactly one yield. After resumption, the
handler is gone; subsequent yields fall through to outer handlers.
Shallow is more expressive (enables some idioms deep can't), but harder
to reason about. Frank and Eff support both; Koka uses deep by default.

#### 4.3.5 Continuation Properties

Effect handlers receive a **continuation** representing "the rest of the
computation". The continuation is the function from `(result of this
yield) -> (eventual final result)`. Handlers can:

- **Resume once** (`resume(v)`): single-shot continuation. The normal case.
- **Resume zero times** (`fail()`): abort. Like an exception.
- **Resume many times**: multi-shot continuation. Forks execution.
  Enables search, non-determinism, AI tree search, transactional rollback.

Most production languages restrict to **single-shot** for simpler
reasoning and easier implementation. For Kasane: single-shot is
sufficient. We don't fork plugin execution.

#### 4.3.6 Why This Matters for Plugins

Plugins are *naturally* effect-typed. Their behaviour is "I might want
to write the buffer, send a Kakoune command, query an LSP, etc." But
without explicit effect tracking, these are hidden inside imperative
function calls.

With algebraic effects:

- **Testing**: the host installs a mock handler. The plugin runs
  deterministically without any real I/O. Test asserts on the sequence
  of yielded effects.
- **Security**: the host refuses forbidden effects. A plugin lacking
  the capability for `Effect::WriteFile` cannot perform it — the
  handler rejects, the plugin sees the failure as a normal Result.
- **Batching**: the host collects all effects yielded in a frame,
  reorders or merges them, and dispatches optimally.
- **Auditing**: every effect is logged. The audit trail is a sequence
  of `(plugin_id, effect, time, granted)` tuples in the time-store.
- **Compile-time tracking**: the SDK macro checks at compile time that
  a plugin only yields effects its manifest authorises.

This collection of benefits is the case for Phase γ.

#### 4.3.7 Comparison with Monads

| Aspect | Monads | Algebraic Effects |
|---|---|---|
| Composition | Stack of monad transformers (clumsy) | Flat row of effects (natural) |
| Effect set | Fixed at type | Polymorphic (effect rows) |
| Mainstream uptake | Haskell, Scala (cats-effect), F# | Koka, Eff, OCaml 5, React Suspense |
| Performance | Often poor without specialisation | Often poor without compiler help |
| Handler abstraction | Nested do-blocks per layer | Single `handle` block |
| Plugin author friction | High (transformer stacks) | High (effect rows) |
| Theoretical equivalence | Free monads ≅ algebraic effects | (yes, formally) |

Both are viable. Effects have cleaner composition; monads have more
library support. For Kasane on Wasm:
- True effects need Wasm stack-switching (proposal stage).
- Monadic encoding works today but is verbose.
- The chosen path (Phase γ) is **CPS-encoded macros** that present an
  effect-yield surface to the plugin author and compile to existing
  imperative dispatch under the hood.

#### 4.3.8 What DDD-CST Adopts

- **Effects as data** (already present in Kasane's `Effects` type)
- **Handler-based testing** (Phase γ deliverable)
- **Static effect tracking** via manifest + type checks (Phase γ + β
  interaction)
- **Compile-time effect-authority correspondence** (an effect requiring
  capability X cannot be yielded by a plugin lacking X's manifest
  declaration)

#### 4.3.9 What DDD-CST Does Not Adopt (Yet)

- True continuation-capturing semantics (needs Wasm stack-switching;
  approximated by CPS until available)
- Multi-shot continuations (no use case)
- Effect polymorphism in the type system (would need a new language)

### 4.4 Content-Addressed Code and Data — Foundation for I4

#### 4.4.1 Origins

- **Merkle trees** (Ralph Merkle, 1979): cryptographic structure for
  content identification via hash trees.
- **Bitkeeper, Monotone, Git** (2005): content-addressed snapshots of
  source code; Git's UI exposed the model widely.
- **Nix** (Dolstra, 2003+): content-addressed builds at the package
  manager level — every build artefact's path includes the hash of its
  inputs.
- **IPFS** (Benet, 2014): content-addressed networking — content
  retrieved by hash, not URL.
- **Unison** (Chiusano, Bjarnason, 2015+): content-addressed *code* —
  every function identified by hash of its normalised AST.

#### 4.4.2 The Core Move

Identify artefacts by *what they are* (cryptographic hash of content),
not *where they are* (path, URL, name+version). The consequences:

- **Identity ≡ content**: two artefacts with the same hash are
  bit-identical. Provenance doesn't change identity.
- **Duplicate detection is free**: same hash → same content → store
  once.
- **Reproducibility is verifiable**: rebuild produces the same hash, or
  you have a non-reproducible build.
- **Names are a separate layer**: human-readable labels point to
  hashes; renames are metadata operations, not content operations.

#### 4.4.3 Content-Addressed Code: Unison's Innovation

In most languages, code references other code *by name*. `foo()` looks
up `foo` in some namespace. Names can change; binding can change.
"Updating a dependency" can break downstream code by renaming or
removing names.

In Unison:
1. Every function is normalised (alpha-renamed: parameter names erased).
2. The normalised AST is hashed.
3. References between functions are by hash.
4. Names are metadata stored separately, mapping from human-readable
   labels to hashes.

Consequences:

- **Updating a library doesn't break dependents.** Dependents reference
  old hashes; the old code remains in the store. To "upgrade", a
  dependent must consciously switch its references.
- **Renaming a function is a metadata operation.** Code is unchanged;
  only the label moves.
- **Identical functions in different libraries deduplicate
  automatically.** Same hash → same function → stored once.
- **Conflicts during merge resolve by hash equality, not text diff.**
  If two branches both add a function with the same hash, no conflict.
  If they add functions with different hashes (even if same name),
  conflict on the name only.

#### 4.4.4 Content-Addressed Builds: Nix's Innovation

Builds depend on inputs (sources) *and* environment (compiler version,
libraries, env vars). Most build systems treat the environment as
implicit, making "reproducible build" impossible to verify.

Nix specifies the full environment as part of the build *derivation*.
The output is stored at `/nix/store/<hash>-<name>/` where `<hash>` is
derived from the full input specification. Identical inputs → identical
hash → bit-identical output, regardless of who, where, when.

Consequences:
- **Two machines build the same plugin → guaranteed identical output.**
- **Rollback is free**: point to an old hash; the old build is still
  there.
- **Garbage collection is precise**: any hash not reachable from a
  user-pinned root can be dropped.

#### 4.4.5 Structural Sharing via Merkle DAGs

A content-addressed store doesn't store each version of an artefact
separately. It stores each *unique sub-artefact* once. Two artefacts
sharing 90% of their structure share 90% of storage.

For Kasane plugins: editing one line of a plugin produces a new hash for
the top-level artefact, but most internal AST nodes are unchanged.
Their hashes match the previous version; storage is shared. The new
plugin version may add only kilobytes despite being a "new" plugin.

#### 4.4.6 Implications for Editor Plugins

- **Distribution**: ship hashes, not version numbers. "Install
  `sha256:a3f2...`" is unambiguous; "Install version 1.2.3" is not.
- **Updates**: switch a name pointer. The old hash remains, so rollback
  is one pointer flip.
- **Reproducibility**: a bug report citing a plugin hash identifies
  exactly the code that ran.
- **Audit**: which hashes ran in this session? (See Phase ζ time-store.)
- **Trust**: hashes can be cryptographically signed; signature
  verifies provenance separately from content.
- **Cross-machine consistency**: my Kasane and your Kasane running
  hash `a3f2...` are running the same plugin, byte for byte.

#### 4.4.7 Content Addressing Under Non-Determinism

Content addressing presupposes determinism: same input → same hash. But
DDD-CST contains explicitly non-deterministic components:

- **LLM responses** (Phase η): the same prompt may produce different
  outputs across calls.
- **External I/O** (Phase ε): file contents, network replies, and
  clock readings are environment-dependent.
- **Effect handlers** (Phase γ): a `Effect::ReadFile` yields control to
  a host-installed handler whose behaviour is policy-dependent.

Naively content-addressing such artefacts is incoherent — there is no
fixed "content" to hash. Two stances exist; DDD-CST adopts the second.

**Stance 1 (rejected): hash the response.** Treat each LLM/IO response
as content and hash it. This trivially satisfies "identity ≡ content"
but loses *reproducibility*: re-running the same prompt rarely
produces the same hash, defeating the whole point of content
addressing.

**Stance 2 (adopted): separate the deterministic skeleton from the
non-deterministic leaves.** A computation is decomposed into:

1. **Deterministic prefix** — the *closure* of pure code, capability
   handles, and input references that *names* what would be computed.
   This is content-addressed: same prefix → same hash.
2. **Effect-edge log** — the sequence of effect yields and their
   observed responses. Each edge is `(yield_site_hash, response_hash,
   epoch)`. The log itself is hash-chained (Merkle log).
3. **Joined identity** — a computation's identity is the *pair*
   `(prefix_hash, effect_log_hash)`. Re-running with the same prefix
   but different responses produces a *new* identity, correctly.

Operational consequences:

- A plugin's *code* is content-addressed (Phase δ proper). A plugin's
  *execution* is identified by (code_hash, effect_log_hash).
- LLM calls are recorded as effect-log edges; the time-store (Phase ζ)
  is precisely this Merkle log per session.
- "Replay" means: re-run the prefix with a recorded effect log;
  outputs are deterministic given recorded responses.
- "Rerun" means: re-run the prefix with a fresh effect log; outputs
  may differ — this is acknowledged, not hidden.
- Auditing an AI agent's behaviour reduces to inspecting its
  effect-log; the prefix-hash certifies that no code substitution
  occurred between recording and audit.

**The boundary discipline**: nothing in the deterministic prefix may
read non-deterministic state directly. All non-determinism is
*announced* via effects (Phase γ). This is exactly the reason §4.3
adopts algebraic effects — it is the bridge that makes content
addressing coherent in a system that interacts with the world.

This is also why Phase δ (content store) does not depend on Phase η
(AI agents) being designed first: Phase δ addresses *code*, which is
determinable; Phase η-side non-determinism is sequestered into the
effect log layer.

#### 4.4.8 Limits

- **Hashing is one-way.** You can't "find similar plugins" by hash;
  similarity needs a separate index.
- **Naming layer is essential** (humans don't remember hashes). The
  naming layer is what most content-addressed systems get *wrong* —
  Git's branches and tags are mutable; Unison's namespace tooling is
  young.
- **Mutable state must be explicit.** The store is immutable. Mutable
  pointers (e.g. "current version of plugin X") are a separate, smaller
  mutable layer.
- **Trust establishment**: who decides which hashes are "legitimate"?
  Signing helps; community curation matters more.
- **The effect log can grow without bound.** Sessions with many LLM
  calls produce large logs; truncation policy is an operational
  concern (oldest-epoch eviction, signed checkpoints, etc.).

#### 4.4.9 What DDD-CST Adopts (Phase δ and beyond)

- **Plugin store at content-addressed paths** (`$XDG_DATA_HOME/kasane/store/<hash>/`)
- **Naming layer separate from content layer** (kdl config maps names → hashes)
- **Hash-based plugin manifest references**
- **Rollback via pointer redirection**
- **Prefix/effect-log split** for non-deterministic execution (joint
  identity `(prefix_hash, effect_log_hash)`)
- **Hash-chained effect log** as the persistent record of
  non-deterministic responses (Phase ζ time-store)

#### 4.4.10 What DDD-CST Does Not Adopt (Yet)

- Distributed content-addressed sync (Phase θ scope)
- Content-addressed *data*, not just code (Phase ζ extends to time-store)
- Cryptographic signing layer (separate ADR if needed)
- Content-addressing the LLM response *itself* as if it were code
  (rejected — see §4.4.7 Stance 1)

### 4.5 CRDTs and Partial-Order Time — Deepening of I4

#### 4.5.1 Origins

- **Operational Transformation** (Ellis, Gibbs, 1989+): the
  collaborative-editing precursor; Google Docs uses descendants of OT.
- **Bayou** (Xerox PARC, 1995): eventual consistency for mobile
  databases.
- **Shapiro, Preguiça, Baquero, Zawirski** (2011): "A comprehensive
  study of Convergent and Commutative Replicated Data Types" — the
  taxonomy paper that gave CRDTs their name.
- **Yjs** (Nicolaescu, 2015+): production-grade JS CRDT for editors.
- **Automerge** (Kleppmann, 2017+): JSON-shaped CRDT with broad
  applicability.
- **Diamond Types** (Beyer, 2022+): performance-optimised text CRDT.

#### 4.5.2 The Problem CRDTs Solve

Multiple replicas hold the same logical data. Each can update
independently (no coordination). How do you guarantee that replicas
eventually converge to the same state regardless of update order?

Classical answer: don't allow independent updates. Use locks, two-phase
commit, consensus protocols. Unsuitable for editors where users (or AI
agents) edit concurrently and offline.

CRDT answer: design data types whose operations *commute* (order
doesn't matter) and whose merges are *associative* (grouping doesn't
matter) and *idempotent* (re-applying doesn't change state). Then any
order of operations, any grouping of merges, produces the same final
state.

#### 4.5.3 Two Families

**State-based CRDTs (CvRDT)**: each replica holds a state in a
join-semilattice. Replicas exchange states; merge is the join (least
upper bound). Example: G-Counter. State is a vector of per-actor counts;
join is element-wise max. Each actor increments only its own slot, so
the merge correctly sums all increments.

**Operation-based CRDTs (CmRDT)**: each replica holds an operation log.
Replicas exchange operations. Each operation is applied if its
prerequisites have arrived (causal delivery). Example: collaborative
text where ops are "insert character with ID X to the right of ID Y".
Concurrent inserts get unique IDs and a deterministic ordering rule.

Modern editor CRDTs are predominantly op-based, sometimes with
state-based optimisations (delta-state CRDTs).

#### 4.5.4 Text CRDTs in Detail

Text is hard because *position references shift* as edits occur. If
actor A inserts "X" at position 5 and concurrently actor B deletes
characters 3–4, A's intended position 5 is now a different position.

Approaches:

- **RGA (Replicated Growable Array)**: each character has a unique ID
  (e.g. `(actor_id, seq_number)`). Position is expressed as "right of
  ID Y". Inserts under the same parent are ordered by ID. Deletes mark
  tombstones rather than removing.
- **YATA (Yjs Algorithm)**: refinement of RGA with intent-preservation
  rules — concurrent inserts at the same position interleave in a
  way that better matches user expectation.
- **Diamond Types**: high-performance op-based CRDT, ~6× faster than
  Yjs for text in benchmarks (2024).
- **Logoot, LSEQ**: dense-position CRDTs with variable-length position
  identifiers. Avoid tombstones but suffer from identifier growth.

For Kasane Phase ζ: **Yjs (via `yrs` Rust crate) is the most mature
choice today**. Diamond Types worth re-evaluating in 2027.

#### 4.5.5 Vector Clocks: The Time Substrate

CRDTs need to track *causality*. Lamport's happened-before relation
(1978):
- Events in the same actor: ordered by local sequence.
- Cross-actor: A sends a message m, B receives m → A's send
  happens-before B's receive.
- Otherwise: events are **concurrent** (neither precedes the other).

Vector clocks track this efficiently:
- Each actor maintains a vector of counters, one per actor.
- Local event: bump own counter.
- Send: attach full vector to message.
- Receive: element-wise max(local, received), then bump own counter.

Comparison:
- V₁ < V₂ iff V₁[i] ≤ V₂[i] for all i, and V₁ ≠ V₂. (happens-before)
- Neither V₁ < V₂ nor V₂ < V₁: concurrent.

This yields a **partial order** — not all events are comparable. The
partial order *is the causal structure*.

#### 4.5.6 Why Partial Order Matters in Editors

A single-user, single-machine editor has a linear order: every event
has an earlier and later by wall clock.

A multi-source editor has *naturally concurrent* events:
- User typing while LSP delivers diagnostics
- AI agent editing while user is also editing
- Git daemon updating diff state asynchronously
- File watcher delivering filesystem changes

Forcing these into linear order means *choosing* an order, losing the
information that they were concurrent. This loss matters when:
- Resolving conflicts ("which edit was intentional? which was
  reactive?")
- Building undo UX ("undo my edit but keep the agent's")
- Synchronising across machines ("my laptop's state vs my desktop's")

Partial-order time captures the actual causal structure. CRDTs operate
correctly on it. Time-travel UI can navigate it (showing branches,
not just a line).

#### 4.5.7 Implications for Editor Design

- **"Undo" becomes subtle**: undo *whose* edit, *to what point*? The
  user needs a UI that distinguishes their edits, AI edits, plugin
  edits.
- **"Current state" is a view**: there is no single "now"; there is
  a *frontier* in the partial order, and the editor renders some
  consistent slice through it.
- **Inter-device sync is CRDT merge**: not a database replication.
  Devices exchange operations, merge, both converge.
- **Application logic must be CRDT-aware**: it cannot assume linear
  history. This is the largest design impact.

#### 4.5.8 Limits

- **Memory overhead**: vector clocks grow with actor count. Acceptable
  for tens of actors; problematic for thousands. Interval-tree clocks
  (Almeida 2008) and bloom clocks address this with tradeoffs.
- **User mental model**: most users think of time as linear. The UI
  must hide partial order or expose it carefully.
- **Some operations don't commute naturally**. "Rename file to A" and
  "rename file to B" concurrent: which wins? CRDTs typically use
  last-writer-wins with timestamp tiebreaker, accepting information
  loss.
- **Application invariants are harder**. "No two users can have
  cursor on the same line" is hard to enforce with CRDTs (which avoid
  coordination by design).

#### 4.5.9 What DDD-CST Adopts (Phase ζ)

- **Yjs sequence CRDT** for buffer text
- **Vector clocks** for `VersionId` (replacing current linear `u64`)
- **CRDT-aware Salsa inputs**: each external source has its own
  actor ID; merging is automatic.

#### 4.5.10 What DDD-CST Does Not Adopt (Yet)

- Full CRDT-typed plugin state (would require redefining state
  semantics for *every* plugin; deferred until concrete demand)
- Causal broadcast across machines (Phase θ scope)
- Strong consistency for invariant-bearing operations (would need a
  separate coordination layer)

### 4.6 Substructural Type Systems — The Connecting Thread

#### 4.6.1 Origins

- **Linear logic** (Girard, 1987): resources used exactly once.
- **Linear types** in functional programming (Wadler, 1990).
- **Rust's ownership and borrowing** (Klabnik, Matsakis, 2010+):
  affine types in mainstream production.
- **Linear Haskell** (Bernardy et al., 2017): linear types retrofitted
  onto Haskell.
- **Pony language** (Drossopoulou, McNeil, Steed, Clebsch, 2015):
  reference capabilities with linear/ephemeral distinction.

#### 4.6.2 The Classification

| System | Allowed uses of a variable |
|---|---|
| Structural (unrestricted) | Any number of times, any order |
| **Affine** | At most once |
| **Linear** | Exactly once |
| Relevant | At least once |
| Ordered | Linear + in a specific order |
| Unique | Linear + exclusive access (no aliases) |

Rust's default is **affine**: a value can be used once and then is
"moved" (consumed). The borrow checker layers borrow analysis on top
for shared-but-scoped access.

#### 4.6.3 Why This Matters Here

Capabilities, effects, and time-versioned references all benefit from
substructural typing:

- **Capabilities**: a handle should be *uniquely held* unless
  explicitly forked. Affine typing enforces this — passing a handle to
  a function *consumes* it from the caller. Cloning requires explicit
  `.fork()` returning two handles.
- **Effects**: a continuation should be invokable *exactly once*
  (single-shot) or *at most once* (escapable). Linear/affine typing
  enforces this — the handler must `resume` or `fail`, but not both,
  and not twice.
- **Time-versioned refs**: a snapshot reference is valid only within
  its time scope. Linear typing with scope-bound lifetimes enforces
  this — the snapshot cannot escape its enclosing transaction.
- **Sessions / protocols**: a session-typed channel proceeds through
  states; the type system enforces protocol adherence (e.g., "must
  send `initialize` before any query"). Phase β + Phase ε together
  could use this for the daemon protocol.

#### 4.6.4 Rust as a Pragmatic Substructural Language

Rust isn't purely linear, but its ownership + borrow + Drop system
provides most of the benefits:

- **`Drop` types approximate linear types**: must be consumed (or
  Drop runs at scope end).
- **`&` and `&mut` approximate borrows**: scoped, non-consuming uses.
- **Phantom-typed lifetimes approximate ordered/scoped typing**: e.g.
  `&'tx Snapshot<'tx>` cannot escape `'tx`.

For DDD-CST, Rust + Wasm Component Model `resource` is *good enough*.
True linear types (Linear Haskell, Idris 2) would be cleaner, but the
gap is manageable.

#### 4.6.5 What DDD-CST Adopts

- **Resource handles are affine**: consuming returns Nothing; cloning
  requires explicit `.fork()`.
- **Capability `attenuate` is non-consuming**: produces a new handle
  without invalidating the parent.
- **Capability `delegate` is consuming**: hands the handle to another
  party.
- **Effect handlers see continuations as affine**: must resume or fail,
  exactly once.

#### 4.6.6 What DDD-CST Does Not Adopt (Yet)

- Full linear typing throughout the SDK (would need a non-Rust
  language or substantial macro work).
- Session types for protocol-typed services (a Phase ε refinement,
  not initial scope).
- Verified linearity (proof-carrying-code level, out of scope).

### 4.7 How the Foundations Compose

The six concepts above are not independent. Their composition is the
substantive claim of DDD-CST.

#### 4.7.1 Role Summary

| Foundation | Role | Provides |
|---|---|---|
| Differential Dataflow | Compute model | Incremental, declarative, delta-propagating computation over typed collections |
| Capability theory | Access model | Unforgeable handles determining which dataflow nodes a plugin can read or write |
| Algebraic effects | Side-effect model | Plugins yield effects naming a capability and an action; handlers dispatch |
| Content addressing | Identity model | Every dataflow node, plugin, and effect handler has a content hash |
| CRDTs | Merge model | When concurrent updates arrive from multiple sources, they merge deterministically |
| Substructural types | Static guarantees | Capabilities can't be leaked, effects can't be smuggled past handlers, snapshots can't outlive their scope |

#### 4.7.2 Dependency Structure

The naive reading of §3's four invariants treats them as orthogonal
axes. They are not. Each foundation *presupposes* or *enables* others,
and the substrate's coherence rests on these dependencies being
acyclic and stable.

The dependency graph among foundations:

```
                ┌─────────────────────┐
                │ Substructural Types │  (enables static guarantees)
                └────────┬────────────┘
                         │
       ┌─────────────────┼─────────────────┐
       ▼                 ▼                 ▼
  ┌─────────┐      ┌──────────┐     ┌──────────────┐
  │   DD    │◀────▶│ Effects  │◀───▶│ Capabilities │
  │  (I1)   │ dual │  (I3)    │ dual│   (I2)       │
  └────┬────┘      └────┬─────┘     └──────┬───────┘
       │                │                  │
       │ determinism    │ purity boundary  │ authority bound
       │ requirement    │ for hashing      │ on dataflow read
       ▼                ▼                  ▼
  ┌─────────────────────────────────────────┐
  │  Content Addressing  (I4 ‒ identity)    │
  └─────────────────┬───────────────────────┘
                    │ requires causal model
                    ▼
            ┌──────────────────────┐
            │  CRDT / Partial-     │
            │  Order Time (I4 ‒    │
            │  merge)              │
            └──────────────────────┘
```

Read top-to-bottom: each layer depends on the ones above it.

#### 4.7.3 The Non-Obvious Dependencies

Five edges in the graph above are surprising and load-bearing. They
are spelled out here so reviewers can attack them directly.

**(D1) DD ↔ Effects: pull/push are dual.** §4.1.8 frames the
reconciliation as "host pushes into Salsa slots, plugin pulls via
Salsa queries". The push half *is* an effect — `Effect::SetInput(id,
value)` — issued by the transport adapter as a host-internal effect.
The pull half *is* a capability invocation — `cap.read()` returning
the current Salsa value. So the editor's I/O loop is structurally a
two-handed effect/capability pair. Removing either side breaks the
discipline.

**(D2) Capabilities ↔ Effects are duals (Reader ≅ Free).** As noted in
§4.3.7, capability passing and effect handling are isomorphic
encodings of the same access pattern: `f: cap<S> → A` ≅
`f: A in Eff<S>`. The reason DDD-CST keeps both is *not* belt-and-
braces redundancy; it is **separation of static and dynamic policy
surface**:

- Capabilities encode authority that is *bound at plugin load*:
  declared in the manifest, checked once, then held. This is the
  static surface — what a plugin is *permitted* to attempt.
- Effects encode *dynamic policy*: each yield reaches a handler that
  may approve, transform, batch, or reject. This is the runtime
  surface — what a plugin actually *does* with its authority on a
  given epoch.

The reduction "capability is sugar for effect" loses the static
surface; "effect is sugar for capability" loses the dynamic surface.
Both are needed. The cost is design discipline: every
authority-relevant facility must be classified as *static-bound*
(capability) or *dynamic-mediated* (effect), not both.

**(D3) Effects → Content addressing (purity boundary).** §4.4.7
adopts the prefix/effect-log split: the deterministic prefix is
content-addressed, the effect log captures non-determinism. This
*requires* effects to be the *sole* non-determinism boundary —
otherwise content addressing's identity ≡ content property fails.
This is why I3 cannot be retrofitted after I4; it must precede or
co-evolve.

**(D4) Capabilities → Content addressing (skeletal closure).** A
plugin's prefix is hashed including its *capability bindings* (which
caps it requested in its manifest). Two plugins with bit-identical
code but different capability requests are *different* artefacts —
their authority is part of their identity. This is the
content-addressing analogue of POLA: identity reflects authority.

**(D5) Content addressing → CRDT (causal coherence).** A CRDT's
merge operator requires that concurrent operations be uniquely
identifiable across replicas. Content hashes provide this identity
*for free* — operation IDs become hashes of the operation content
plus the operator's vector clock. Without content addressing, CRDT
implementations must invent per-replica ID schemes (Yjs's
`(client_id, clock)` pairs); with it, IDs are derived. This is why
Phase ζ depends on Phase δ in the roadmap and is not an arbitrary
ordering.

**(D6) Substructural types → all.** Linear/affine typing is the
mechanism that makes every other invariant *statically enforceable*
rather than runtime-checked: capabilities can't leak because handles
are affine; effects can't be smuggled because continuations are
linear; snapshots can't outlive scope because lifetimes are
substructural. This is why §4.6 calls it "the connecting thread" —
it is the formal substrate that lets the rest be type-checked rather
than tested.

#### 4.7.4 What Composition Buys

A plugin in DDD-CST is, formally:

> **A content-addressed unit of code that holds a fixed set of
> capability handles, declares a dataflow graph over their state, and
> announces its side effects as structured yields whose semantics are
> determined by the host's handler stack.**

Every clause carries weight:

- *Content-addressed*: identity is the hash, not the name. (I4)
- *Fixed set of capability handles*: authority is bounded at load time.
  (I2)
- *Declares a dataflow graph*: computation is incremental and
  declarative. (I1)
- *Announces its side effects as structured yields*: I/O is reified,
  inspectable, mockable. (I3)
- *Whose semantics are determined by the host's handler stack*: the
  host retains policy authority over plugin behaviour.

Removing any clause loses a *specific* guarantee tied to its
dependency edge:

| Removed | Edge lost | What breaks |
|---|---|---|
| Content addressing | D3, D4, D5 | Identity ambiguity; replay non-reproducible; CRDT IDs must be invented |
| Capability handles | D2 (static), D4 | Ambient authority returns; confused-deputy risk; identity loses authority dimension |
| Dataflow graph | D1, D3 | Hand-written invalidation per plugin; push/pull split has no consumer side |
| Structured yields | D1, D2 (dynamic), D3 | I/O becomes ambient; effect log can't be sequestered; content addressing leaks |
| Handler stack | D2 (dynamic) | Plugins exercise authority unmediated; no dynamic policy surface |

This is what justifies the "single substrate" framing. The composition
is not a feature list; it is a small dependency graph in which each
edge has a specific cost when cut.

### 4.8 Why the Synthesis Is Plausible Now

Each foundation has matured at a different pace:
- Differential dataflow: production-ready (Materialize).
- Capability theory: production OS-level (KeyKOS descendants, IBM
  i-Series) and starting in user-space (Cap'n Proto, Goblins).
- Algebraic effects: production in OCaml 5; library-level in Rust.
- Content addressing: production at scale (Git, Nix, IPFS, Unison).
- CRDTs: production in editors (Yjs, Automerge, Figma).
- Substructural types: production in Rust.

What's *novel* in DDD-CST is the *integration*: bringing all six into
one editor substrate, with the design discipline that the invariants
demand. No existing system has done this combination. The components
are mainstream; the synthesis is research-grade.

## 5. What This Document Is NOT

To prevent scope creep and pre-empt objections:

- **NOT a commitment.** No phase has been agreed to. No timeline has been
  promised. A single maintainer cannot complete this.
- **NOT a replacement for the daemon-registry direction.** Phase ε
  explicitly retrofits perken's design as a transport layer beneath the
  Salsa-first substrate. Both directions converge.
- **NOT a license to rewrite the codebase.** Each phase is designed to land
  as an incremental, opt-in change that coexists with existing patterns.
- **NOT an editor-agnostic project (until Phase ι).** Through Phase η,
  Kakoune remains the canonical buffer server.
- **NOT an AI-only project.** Phase η covers AI integration but is not the
  centre of gravity; it is one consumer of the Phase ζ infrastructure.
- **NOT formally verified (and may never be).** Verification is desirable
  but not on the critical path. §21 lists open questions including
  which proof obligations might be worth pursuing if effort allows.

## 6. Why Now?

The reasons this direction is plausible *now* and was not 10 years ago:

- **Salsa-style incremental computation** is mature enough for production
  (Kasane already uses it, RustC builds on it, Materialize ships it
  commercially).
- **Component Model resources** in WIT (stable in Wasmtime 22+) provide
  capability handles with automatic lifecycle management.
- **CRDT libraries** (Yjs, Automerge) are battle-tested for editor use.
- **Content-addressed code** is industrially deployed (Unison, Nix flakes,
  Nix store).
- **Algebraic effects** have escaped academia (OCaml 5 multicore, Koka 2.x,
  React Suspense).
- **Capability theory** is no longer fringe (CapTP, Cap'n Web, Spritely
  Goblins, Pony language).

The synthesis is novel; the components are mainstream. This is what makes
the direction tractable now and not 10 years ago.

## 7. Phase Overview

Phases are labelled with Greek letters in execution order. Each phase has:
- **Goal**: the user-visible or developer-visible outcome
- **Deliverable**: the concrete artefacts that ship
- **Invariant focus**: which of I1–I4 the phase primarily advances
- **Estimated horizon**: order-of-magnitude effort
- **Exit criterion**: what makes the phase complete
- **Abandon criterion**: what would justify stopping work on this phase

| Phase | Goal | Invariant | Horizon | Status | ADR |
|---|---|---|---|---|---|
| α | External data as Salsa inputs | I1 | 1–2 quarters | Proposed | [ADR-051](./decisions/adr-051-external-data-as-salsa-inputs.md) |
| β | Capability resources via WIT | I2 | 1–2 quarters | Proposed | [ADR-052](./decisions/adr-052-capability-resources-via-wit.md) |
| γ | Algebraic effect macros for plugin SDK | I3 | 2–3 quarters | Proposed | [ADR-053](./decisions/adr-053-algebraic-effect-macros-plugin-sdk.md) |
| δ | Content-addressed plugin store | I4 | 2–4 quarters | Proposed | — (not yet extracted) |
| ε | Daemon registry retrofit as transport layer | I1+I2 | 1–2 quarters | Proposed | [ADR-054](./decisions/adr-054-daemon-registry-as-transport-layer.md) |
| ζ | Partial-order time and CRDT merge | I4 | 4–6 quarters | Speculative | — |
| η | AI agent capability gating | I2+I3 | 2–4 quarters | Speculative | — |
| θ | Distributed dataflow nodes | I1+I4 | 6–12 quarters | Visionary | — |
| ι | Editor-agnostic substrate | All | 12+ quarters | Visionary | — |

Two **theoretical-foundation ADRs** sit beneath the phase table and are
cited by multiple phases: [ADR-055](./decisions/adr-055-prefix-effect-log-split.md)
(prefix/effect-log split — §4.4.7, depends on Phase γ, gates Phase δ)
and [ADR-056](./decisions/adr-056-attenuation-predicate-language.md)
(APL — §4.2.7, the static attenuation surface for Phase β).

"Quarter" here means a 3-month period of *focused* work by a competent
contributor. Calendar time is longer.

## 8. Phase α: External Data as Salsa Inputs

### Goal
Every datum produced by an external source — git diff, LSP diagnostics,
file system event, network response — enters Kasane as a Salsa input rather
than as a one-off message routed through `apply_protocol` or pub/sub.

### Why this is invariant I1's first step
Salsa already tracks dependencies and memoises computation for buffer
content, line layout, and rendering output. Extending its input set to
include external sources unifies the two halves of "what data does this
plugin see" under a single dependency graph.

### Deliverables
1. New module `kasane-core/src/salsa_inputs/external.rs` containing:
   - `ExternalInputId<T>: Copy + Eq + Hash` — typed handle for an external
     data source
   - `ExternalInputRegistry` — host-side map from `ExternalInputId` to
     `salsa::Input` slot
   - `commit_external<T>(id, value, version)` — host API for source
     producers to update an input
2. WIT additions in `kasane-wit/wit/plugin.wit`:
   - `register-external-input: func(name: string, schema: external-schema) -> external-handle`
   - `read-external-input: func(handle: external-handle) -> option<list<u8>>`
   - These are *internal* primitives for transport adapters, not direct
     plugin API.
3. Adapter for at least one source: the `kak -ui json` protocol stream
   itself becomes an `ExternalInputRegistry` entry rather than a direct
   `apply_protocol` callsite. This proves the abstraction works for the
   most-bandwidth source first.

### What this looks like to plugin authors
Almost nothing changes in Phase α. The plugin-facing API stays as today
(`host_state::get_lines_text`, etc.). The change is *under the hood*: those
host functions now read from Salsa-backed inputs rather than direct fields.

This is intentional: Phase α is an internal refactor, not a user-facing
feature. Its value is *enabling* phases β–η, not delivering a new capability.

### Exit criterion
- All existing `apply_protocol` paths route through `ExternalInputRegistry`.
- Plugin observation of external data goes through Salsa queries with full
  dependency tracking.
- Performance is within 110% of the pre-phase baseline (measured against
  `delta-24`).

### Abandon criterion
- Salsa's per-query overhead exceeds 1µs on the hot path and cannot be
  reduced.
- The dependency-tracking discipline produces "leaky" invalidations that
  force whole-tree recomputation on every protocol message.

### Dependencies
- ADR-035 §2 (Time as Salsa input dimension) — already landed.
- `AppState.observed.lines` Salsa migration — **already landed**
  (`kasane-core/src/salsa_sync.rs:149`); buffer-lines viability on
  the hot path has been demonstrated.
- No new external crates.

### Status of validation
The original "concrete first PR" — migrating `AppState.observed.lines`
to a Salsa input — is already implemented. This validated the most
basic question (does Salsa-input on hot path work for the
highest-bandwidth source). The interesting questions for Phase α
proper are now the *next* layer.

### Concrete first PR (revised)
Build the `ExternalInputRegistry` skeleton and migrate **one
non-buffer external source** to it — the strongest candidate is the
LSP-diagnostics stream once an LSP transport exists, or as a stepping
stone, the file-watcher notifications used by syntax reload. The goal
is to exercise the *registry abstraction* (typed handles, dynamic
registration, host-mediated `commit_external`) on a source whose
event shape differs materially from buffer lines (sparse, bursty,
multi-source rather than dense and single-source).

This validates the push/pull reconciliation pattern of §4.1.8 against
a workload that the existing buffer-lines path does not exercise.

### Then: Plugin-facing API
Once the registry handles a second source cleanly, expose the typed
`ExternalInputId<T>` to plugins (initially read-only, via Salsa
queries) and ensure dependency tracking propagates through composed
queries that mix buffer and non-buffer inputs. This is where the
push/pull-split discipline is most likely to leak if it leaks at all.

## 9. Phase β: Capability Resources via WIT

### Goal
Replace string-keyed authority (e.g. `query_daemon("git", ...)`) with
capability handles represented as WIT resources. A plugin that wants to use
a service must hold a resource handle for it. Authority is checked at
handle acquisition, not at every call.

### Why this is invariant I2's first step
Resource handles are unforgeable in WIT. They cannot be constructed by the
plugin; they can only be received from the host or delegated from another
plugin. This eliminates an entire class of bugs (typo'd service names,
forgotten authority checks, escalation via string concatenation).

### Deliverables
1. WIT redesign:
   ```wit
   resource service {
       query: func(req: list<u8>) -> future<result<list<u8>, service-error>>;
       events: func(topic: string) -> stream<list<u8>>;
   }
   open-service: func(spec: service-spec) -> result<service, open-error>;
   ```
2. Manifest schema extension: `capabilities` block declares which services a
   plugin can open, with optional attenuation predicates.
3. Host-side `CapabilityBroker` enforcing manifest-declared bounds at
   `open-service` time.
4. Migration of perken's `query-daemon` (post-Phase ε) to the resource form.

### What this looks like to plugin authors
```rust
let git = ctx.open_service::<GitService>("git")?;
let diff = git.query(GitRequest::Diff { path }).await?;
```
The `git` handle is dropped automatically when out of scope. Reusing it
across frames requires storing it in plugin state, which makes its lifetime
explicit.

### Exit criterion
- All host-mediated external access goes through capability resources.
- A plugin that omits a capability declaration in its manifest fails to
  load (compile-time error in the SDK, runtime error at the host).
- Attenuation (e.g. `git.attenuate(repo_path: workspace)`) works
  end-to-end.

### Abandon criterion
- Wasmtime's resource implementation has irrecoverable perf overhead
  (>10µs per resource method call) that cannot be amortised.
- Plugin authors find the explicit handle threading unbearable in
  ergonomic tests.

### Dependencies
- Wasmtime 22+ resource support — stable as of 2026-Q1.
- Phase α not strictly required, but they reinforce each other.

### Concrete first PR
Introduce a single new resource — `BufferView` — and migrate one host
function (`get_lines_text`) to be a method on it. This validates the
resource ergonomics with the lowest-stakes possible API surface.

## 10. Phase γ: Algebraic Effect Macros for the Plugin SDK

### Goal
Plugin authors write side-effecting code as effect yields rather than
imperative calls. The SDK macro compiles this to existing
`Effects` / `Command` enums today, and to true continuation-passing
algebraic effects once Wasm stack-switching stabilises.

### Why this is invariant I3's first step
The existing `Effects` type is *halfway* there: it lets plugins declare
desired side effects as data. Phase γ pushes this further by:
1. Making *all* side effects go through `Effects` (no escape hatches).
2. Adding type-level tracking: `MyPlugin: HasEffects<ReadFile | OpenBuffer>`.
3. Providing handler-based test infrastructure: `plugin.run_with_handler(mock)`
   replaces real I/O with deterministic mocks.

### Deliverables
1. New trait hierarchy in `kasane-plugin-sdk`:
   ```rust
   trait Effectful {
       type Effects: EffectSet;
       fn step(&mut self, effects: &mut Effects::Yielder) -> EffectfulResult;
   }
   ```
2. `define_plugin!` macro extension to produce `Effectful` impls from
   declarative effect blocks:
   ```rust
   effects on KeyPress(Ctrl + 'd') {
       let file = yield Effect::PickFile;
       yield Effect::OpenBuffer(file);
   }
   ```
3. Test harness in `kasane-plugin-sdk-test`:
   ```rust
   let result = my_plugin
       .with_effect_handler(MockHandler::default())
       .step()?;
   assert_eq!(result.yielded_effects, vec![Effect::PickFile, ...]);
   ```

### What this looks like to plugin authors
Today:
```rust
fn handle_key(&mut self, k: Key, app: &AppView) -> Effects {
    if k == Key::Ctrl('d') {
        Effects::new().with_command(Command::OpenBuffer(some_path()))
    } else {
        Effects::default()
    }
}
```
Phase γ:
```rust
effects on KeyPress(Ctrl + 'd') {
    let path = yield Effect::PickFile(self.workspace());
    yield Effect::OpenBuffer(path);
}
```
The macro generates the imperative form. Plugin authors think in terms of
yielded effects.

### Exit criterion
- 80% of existing bundled plugins migrate to the effect form.
- `kasane-plugin-sdk-test` can test any plugin without spawning real
  subprocesses or touching the file system.
- The `define_plugin!` macro's output passes clippy `-D warnings`.

### Abandon criterion
- The macro becomes so complex that compile-error messages become
  incomprehensible.
- Effect handler dispatch adds >5µs per yield on the hot path.

### Dependencies
- Phase β preferred (effects often consume capabilities). Not strictly
  required.

### Concrete first PR
Define `Effect` taxonomy as a single enum mirroring existing `Command`
variants. Plumb through `define_plugin!` without changing the public API.
This validates the encoding before changing any plugin code.

## 11. Phase δ: Content-Addressed Plugin Store

### Goal
Every plugin version is identified by the content hash of its source +
manifest + dependencies. Installation is "register this hash"; updates are
"register a new hash and switch the active pointer".

### Why this is invariant I4's first step
Reproducible builds, deterministic plugin behaviour across machines, and
trivial rollback all depend on identifying plugins by content rather than
by name+version. This is Nix's approach generalised to plugin code.

### Deliverables
1. New crate `kasane-plugin-cas` providing:
   - `PluginHash` — content-derived identifier
   - `PluginStore` — on-disk store at `$XDG_DATA_HOME/kasane/store/`
   - `ManifestRef` — sturdy reference to a stored plugin
2. CLI: `kasane plugin install <path>` produces a hash; `kasane plugin
   activate <hash>` switches the active version; `kasane plugin gc` drops
   unreferenced hashes.
3. Per-user config records active hashes:
   ```kdl
   plugins {
       git-diff hash="sha256:a3f2..."
       lsp-rust hash="sha256:4d7e..."
   }
   ```

### What this looks like to users
```
$ kasane plugin install ~/code/my-plugin
Installed sha256:a3f2... (built from commit b9c1...)
$ kasane plugin activate sha256:a3f2...
Activated my-plugin (sha256:a3f2...)
$ kasane plugin rollback
Reverted to sha256:8e21... (my-plugin v0.3.2)
```

### Exit criterion
- Two machines installing the same plugin source produce the same hash.
- Plugin uninstall + reinstall produces an idempotent store state.
- The Kasane SDK macros emit hash-stable WIT bindings (no nondeterministic
  timestamps in generated code).

### Abandon criterion
- Build-system reproducibility is unachievable without re-engineering cargo
  (which is out of scope).
- The user-facing complexity of "managing hashes" outweighs the
  reproducibility benefit.

### Dependencies
- Nix-style reproducibility tooling (Cargo `--locked` + sealed
  environments) — partially exists.

### Concrete first PR
Add `--print-hash` flag to the existing `kasane plugin build` command. This
exposes the hash before any infrastructure consumes it, letting us validate
hash stability on real plugins before designing the store.

## 12. Phase ε: Daemon Registry Retrofit as Transport Layer

### Goal
perken's daemon-registry POC (from AntoineBalaine/kasane `upstream_port`)
is landed in upstream as a *transport layer beneath* the capability
resources of Phase β and the Salsa inputs of Phase α.

### Why this is invariant I1+I2
Daemons remain useful (long-lived shared services need a transport). But
they are no longer the plugin-facing API. Plugins see `service` resources;
those resources are *backed by* daemon connections, but plugins don't know
that.

### Deliverables
1. perken's `kasane-wasm/src/daemon_registry.rs` lands in upstream, renamed
   to `kasane-wasm/src/transports/daemon.rs`.
2. `DaemonRegistry` implements `ServiceTransport`:
   ```rust
   trait ServiceTransport {
       fn open(&self, spec: ServiceSpec) -> Result<Service, OpenError>;
   }
   ```
3. Other transports become possible: `ProcessTransport` (one-shot
   subprocess), `HttpTransport` (network), `InProcessTransport` (native
   plugin callsite). The capability resource doesn't care which is in use.
4. perken's `kasane-git-daemon` and `kasane-lpr-daemon` ship as
   reference implementations of the daemon-backed transport.

### What this looks like to plugin authors
```rust
// Plugin code doesn't care whether "git" is a daemon, subprocess, or in-process call.
let git: Service<GitProtocol> = ctx.open_service("git")?;
let diff = git.query(GitRequest::Diff { path }).await?;
```
The manifest declares which transport is required:
```kdl
plugin "git-diff" {
    capabilities {
        git transport="daemon"
    }
}
```

### Exit criterion
- perken's POC compiles against the current upstream WIT.
- Existing perken-fork users can switch to upstream Kasane without
  re-installing their daemons.
- A simple `HttpTransport` exists alongside `DaemonTransport`, proving the
  transport abstraction generalises.

### Abandon criterion
- Capability resources from Phase β cannot be retrofitted onto perken's
  socket protocol without unacceptable churn.

### Dependencies
- Phase β strongly preferred (so daemons appear as resources).
- Phase α preferred (so daemon responses can populate Salsa inputs).

### Concrete first PR
Open a PR to AntoineBalaine/kasane proposing the retrofit. perken is the
right reviewer because they wrote the POC and understand its constraints.
This is also the *human-coordination* moment, not just a technical one.

## 13. Phase ζ: Partial-Order Time and Text CRDT Merge

### Scope
This phase is deliberately narrowed in scope versus the original
ambition. **Text CRDT for buffer content + vector-clock-typed time
for inputs** are in scope. **Plugin-state CRDT** is *not*: per
§4.5.10, generic CRDT for arbitrary plugin state is an open research
problem (CRDT composition is not closed under dependent invariants),
and forcing it would require every plugin author to design and prove
their own CRDT semantics. Plugins that need multi-replica state
remain free to adopt CRDT-shaped state types, but the substrate does
not impose this.

### Goal
The *buffer text* and *external-input slots* stop being implicitly
single-writer. Buffer text becomes a sequence CRDT; external-input
slots become per-source vector-clocked values that merge by
last-writer-wins-per-source. Plugin internal state remains
linear-time and single-writer.

### Why this is invariant I4's deeper step
Linear time forces every input source into a single sequence. For
buffer text and external sources, this is the limiting assumption
for multi-device sync, concurrent AI text edits, and future
multi-user collaboration. For per-plugin state, the cost of CRDT
discipline exceeds the benefit, so it stays linear.

### Deliverables
1. `VersionId` gains a `VectorClock` variant; existing total-order
   sites are audited and converted to handle incomparable versions
   explicitly (or assert linear domain).
2. Buffer text storage migrates to a sequence CRDT (Yjs RGA or
   equivalent — see §4.5.4).
3. External-input slots gain per-source actor IDs; concurrent pushes
   from different sources merge by union; concurrent pushes from the
   same source coalesce by last-write per §4.1.8 back-pressure policy.
4. `Snapshot::merge(other) -> Snapshot` for buffer-text and
   external-input subgraphs only.
5. Conflict UI for buffer text: a built-in plugin surfaces text-CRDT
   merge conflicts as overlays.

### Explicitly out of scope
- Generic plugin-state CRDT (per §4.5.10).
- CRDT-aware Salsa derived queries (derived values remain functions
  of CRDT inputs; the CRDT property is at the input layer).
- Strong-consistency invariants across replicas (no coordination
  layer is added; invariants that need coordination fall outside
  Phase ζ).
- CRDT for selections, cursors, fold state, or any other interactive
  per-replica state — these stay single-writer per replica with
  whatever ad-hoc reconciliation the application chooses.

### What this looks like to users
Day-to-day editing is unchanged. The new capability is:
- "Continue this session on my other machine" — buffer text and
  external-input history sync via the Phase δ content-addressed
  store; per-replica plugin state (cursor, selection, fold state)
  does *not* sync and is reset on the target machine.
- "Resume this buffer from a checkpoint" — time-travel UI as a
  built-in affordance.
- "Two AI agents edit the same buffer concurrently" — handled like
  two human collaborators *for the text*. Per-agent plugin state
  (which lines each agent is "considering") is not unified; agents
  see each other's text edits but not each other's intermediate
  reasoning state.

### Exit criterion
- Multi-device sync of buffer text and external inputs works between
  two machines with shared content store.
- All existing single-machine functionality continues to work; in the
  trivial case (single replica, no concurrent writers), the runtime
  overhead vs the prior linear-time path is within 10% on hot-path
  benchmarks.
- All `VersionId` consumers either handle incomparable versions
  explicitly or carry a static assertion that they operate only on a
  linear sub-domain. There are no silent total-order assumptions left.

### Abandon criterion
- The complexity of partial-order time is intractable for plugin
  authors despite the narrowed scope.
- CRDT overhead on the hot path cannot be brought within the §17
  performance budget.
- The audit of `VersionId` consumers reveals so many sites that
  silently assume totality that the migration cost exceeds the
  multi-device benefit.
- The only realistic users (multi-device single-user) are too small
  a market to justify the complexity.

### Dependencies
- Phase α (Salsa inputs become per-source-tagged so that vector
  clocks have well-defined actor IDs).
- Phase δ (content store for inter-machine sync; effect-log split
  per §4.4.7 provides the persistence substrate).
- Yjs or Automerge as Rust crate dependency.

### Concrete first PR
Audit all `VersionId` callsites in `kasane-core` for total-order
assumptions. Classify each as (a) safe under linear sub-domain
assertion, (b) requires explicit incomparable-case handling, or (c)
requires redesign. This audit is the gating evidence for whether the
scope above is achievable; the count of (c) sites determines whether
Phase ζ remains feasible.

## 14. Phase η: AI Agent Capability Gating

### Goal
LLM-driven agents (completion, refactor, code review) are first-class
participants in the dataflow, with their authority bounded by explicit
capabilities and their actions fully auditable via the time-store.

### Why this is invariant I2+I3
AI agents are *the* test case for capability bounding: they are powerful,
their behaviour is non-deterministic, and they cannot be trusted with
ambient authority. They are also *the* test case for algebraic effects:
their outputs are sequences of yielded actions that the host can approve,
reject, or batch.

### Deliverables
1. `AiAgent` capability resource with attenuation:
   ```rust
   let bounded = full_agent
       .attenuate(WriteAccess::lines(cursor..cursor+10))
       .attenuate(ReadAccess::buffer_only())
       .attenuate(NetworkAccess::denied());
   ```
2. Per-agent audit log in the time-store: every agent invocation produces
   an epoch with input, prompt, response, and yielded effects.
3. Plugin "AI-pair-programming" reference implementation: shows
   conversational AI editing with full capability gating.

### What this looks like to users
A user grants an agent capability via a UI prompt:
> *Allow `claude-code` to edit lines 100–120 of `src/main.rs`? (yes/once/no)*

The grant produces a time-bounded, scope-bounded capability that the agent
can use until revoked. Every action the agent takes is logged and
replayable.

### Exit criterion
- An AI agent can complete code without ever being granted broad write
  authority.
- A user can review the full sequence of agent actions in a debugger UI.
- A user can revoke an agent's capability mid-session.

### Abandon criterion
- LLM latency dominates so heavily that the surrounding capability
  infrastructure is irrelevant to the user experience.
- The capability-bounding model is too restrictive for useful agent
  behaviour and forces users to grant blanket permissions.

### Dependencies
- Phase β (capability resources).
- Phase γ (effects taxonomy).
- Phase ζ optional but useful (time-travel debugging of agent actions).

### Concrete first PR
Add an `AiAgent` opaque resource with no methods. Define its capability
shape in the manifest. Have a stub plugin that requests it. The plumbing
exists; the agent itself can come later.

## 15. Phase θ: Distributed Dataflow Nodes

### Goal
Dataflow nodes can live on different processes, different machines, or
different networks. A plugin computing a derived value need not know
whether its inputs are local, remote, or composite.

### Why this is invariant I1+I4
Differential Dataflow was designed for distributed execution from day one
(Naiad ran on clusters). The same operators that work on a single machine
work across machines, with the only difference being transport latency.

### Deliverables
1. Network-transparent `Collection<T>` whose `T` may be sourced from a
   remote node.
2. Capability-secured RPC for cross-node dataflow operator invocation
   (Cap'n Web or similar).
3. At least one realistic distributed scenario: e.g. "shared LSP server
   for a team", where one machine hosts the LSP and N teammates' editors
   share its analysis.

### Exit criterion
- The same plugin code runs unchanged whether its data is local or remote.
- Latency is acceptable for the targeted use case (e.g. <100ms for shared
  LSP completion in same-LAN scenario).

### Abandon criterion
- The single-user use case never produced demand for distribution.
- Network failures expose so many edge cases that the abstraction leaks.

### Dependencies
- Phases α, β, ζ.

### Concrete first PR
Identify one realistic distributed scenario with a real user. Build it
end-to-end as a vertical slice before abstracting.

## 16. Phase ι: Editor-Agnostic Substrate

### Goal
Kasane stops being "a Kakoune frontend" and becomes "a substrate that any
text editor (or non-text editor) can adopt as its dataflow runtime".
Kakoune becomes one of N supported buffer servers.

### Why this is the asymptote
With Phases α–η in place, the Kakoune-specific code is concentrated in one
adapter module (`KakouneAdapter` as a Salsa-input source). Swapping it for
a Neovim adapter, a Helix adapter, or a self-hosted buffer server is a
local change.

### Deliverables
1. `BufferServer` capability resource with a stable interface.
2. Adapters: Kakoune (existing), Neovim, Helix, self-hosted.
3. Renamed packages reflect editor-agnosticism (this is the most
   conservative deliverable — naming follows reality, not the other way
   around).

### Exit criterion
- One non-Kakoune adapter ships and is usable for real editing.
- The "Kakoune frontend" framing in vision.md is updated to reflect
  multi-editor support.

### Abandon criterion
- No non-Kakoune editor user emerges in the lifetime of the project.
- The Kakoune-specific assumptions are too deeply baked to extract.

### Dependencies
- All previous phases.

### Concrete first PR
Audit `kasane-core` for Kakoune-specific assumptions. The result of that
audit is the gap list to close before this phase is even possible.

## 17. Cross-Cutting Concerns

### 17.1 Performance Budget

Each phase must hold these bounds (measured against `delta-24` baseline):
- CPU per frame at 80×24: ≤ 75µs (current ~57µs, 30% headroom)
- TUI redraw: ≤ 100µs full, ≤ 50µs incremental
- Plugin tick: ≤ 200µs for any single plugin
- Memory: ≤ 50MB resident for typical session

Phases that breach these without explicit ADR-024 reframing must be
revised.

### 17.2 Testing Strategy

| Phase | Required test surface |
|---|---|
| α | Salsa dependency-correctness property tests; existing snapshot tests must pass |
| β | Capability-handle leak tests; attenuation property tests |
| γ | Effect-handler determinism tests; mock-handler replay tests |
| δ | Hash-stability tests across rebuilds |
| ε | Daemon crash + recovery integration tests |
| ζ | CRDT confluence property tests (any merge order → same result) |
| η | AI-agent capability containment tests (agent cannot escape grants) |
| θ | Network-partition behaviour tests |
| ι | Multi-adapter golden tests (same input across adapters → equivalent output) |

### 17.3 Plugin Author UX

DDD-CST is *invisible to most plugin authors*. The SDK macros hide the
machinery. The promise is:
- A plugin author writing "fold by indentation" need not know about Salsa,
  CRDTs, or vector clocks.
- A plugin author writing "shared git-diff service" need not know about
  capability handles — but *can* use them for fine-grained authority.
- A plugin author writing "real-time multiplayer cursor" *will* encounter
  CRDT semantics, but only because their problem inherently requires them.

The complexity budget for the typical plugin author must not exceed
"learning React hooks". This is the bar for adoption.

### 17.4 Migration Paths

Each phase must support an opt-in migration:
- Old API and new API coexist for ≥1 release cycle.
- Migration tooling (`kasane plugin upgrade`) handles mechanical
  refactors.
- Deprecation warnings precede removal by ≥2 release cycles.

### 17.5 Governance

Long-horizon projects fail without governance. Concrete needs:
- Each phase requires an ADR before implementation begins.
- Each ADR cites this document.
- Each ADR records an explicit "if this phase is abandoned, the
  next-best fallback is X."

## 18. The Bootstrap Subset

If full DDD-CST is too ambitious, what's the *minimum viable substrate*
that preserves the invariants?

**MVP-DDD-CST = Phases α + β + γ + ε.**

This subset:
- Implements I1, I2, I3 (skips I4's content-addressing).
- Stays single-machine, single-user, linear-time.
- Subsumes perken's daemon-registry without growing past Kasane's current
  conceptual surface.
- Can be implemented by a small team in 1–2 years.

MVP-DDD-CST is what this roadmap most realistically targets in the
foreseeable future. Phases δ–ι are listed for completeness and as
direction-setting, but a contributor should not start work on them before
α–γ + ε are landed and stable.

## 19. Decision Points

Concrete moments where the direction can be confirmed or pivoted:

1. **Salsa-input on the hot path** — **resolved**. The buffer-lines
   migration to `db.set_buffer_lines(...)` has already landed
   (`kasane-core/src/salsa_sync.rs:149`); performance on the highest-
   bandwidth source is acceptable. Pivot to "Salsa for plugin-side
   queries only, raw mutation for hot path" is no longer needed for
   this source. The next layer of the question — whether the
   abstraction scales to *sparse, multi-source* external inputs — is
   captured in Decision Point 1' below.

   1'. **After the LSP-diagnostics / second-source migration**: does
   the `ExternalInputRegistry` abstraction (typed handles, dynamic
   registration, push/pull split per §4.1.8) hold without
   per-source bespoke wiring? If no, pivot to "registry for buffer
   lines only; other sources keep their current paths."

2. **After Phase β PR #1**: do WIT resources offer real ergonomic
   benefit over typed handles in user-land Rust? If no, scale back to
   thin wrapper over current API.

   2'. **After APL (Attenuation Predicate Language) prototype**: is
   the decidable predicate fragment of §4.2.7 expressive enough for
   the first 5 real attenuation use cases? If predicates must
   constantly "escape to effect handler", the static surface is too
   narrow and Phase β's authority story is mostly runtime, not
   compile time. Re-evaluate fragment grammar.

3. **After Phase γ PR #1**: does the `effects` macro generate code that
   plugin authors can debug? If no, retain effects as runtime-only
   construct without macro sugar.

4. **After Phase ε lands in upstream**: is perken still a co-maintainer
   willing to drive further phases? If no, slow the cadence.

5. **Before Phase ζ commit**:
   (a) is there a real multi-device user? If no, defer indefinitely.
   (b) does the `VersionId`-callsite audit (§13 concrete first PR)
   yield a tractable migration list, or are there hidden total-order
   assumptions everywhere? The audit count gates the phase.

6. **Before Phase η commit**: is the AI-agent use case validated by
   real demand from real users, and does §4.4.7's prefix/effect-log
   split adequately sequester LLM non-determinism in practice? If
   either is no, defer indefinitely.

## 20. Exit Conditions

Conditions under which this entire document should be deprecated:

1. **Better substrate emerges externally.** If a project like Unison
   Cloud, Spritely Goblins, or a future Materialize-for-editors offers the
   DDD-CST capabilities natively, Kasane should consider adopting that
   substrate rather than building its own.
2. **Differential Dataflow proves the wrong abstraction.** If by the end
   of Phase α it becomes clear that the editor's compute patterns don't
   benefit from DD's incrementalisation, the entire substrate-rebuild
   premise collapses.
3. **Kasane's user base never grows past the single-developer threshold.**
   Without external contributors, no roadmap longer than 1 year is
   credible. DDD-CST is fundamentally a multi-contributor project.
4. **Kakoune ceases active development.** If the upstream editor dies, the
   user base for any Kakoune frontend dies with it.
5. **The maintainer's circumstances change.** Personal capacity to invest
   in a decade-long project must be honestly reassessed annually.

When any of these conditions triggers, this document should be marked
SUPERSEDED with a pointer to whatever direction supplants it.

## 21. Open Questions

Theoretical or empirical questions whose answers affect the roadmap but
are not yet known.

### 21.1 Substrate and computation model

1. **Is differential dataflow the right substrate, or is incremental
   spreadsheet semantics (Salsa, Adapton) sufficient?** §4.1.6
   positions Salsa as DD-restricted-to-linear-time-no-iterate. Phase α
   answers empirically which restrictions actually bite.
2. **Does the push/pull-reconciliation split (§4.1.8) hold under
   bursty, multi-source workloads?** The single-frame-drain discipline
   relies on push rates staying below pull rates. Sources that burst
   (LSP startup, large diff arrival, file-watcher mass-rename events)
   stress this assumption.
3. **Do composed Salsa + effect-handler + DD iterate cascades have a
   provable termination contract?** Three independent fixpoint
   semantics compose; the editor needs cascade depth bounds or fuel
   limits to avoid Emacs-style hook-recursion failures. No such bound
   is specified yet.

### 21.2 Capability and effect surface

4. **Can WIT resources scale to thousands of live handles without
   per-handle overhead?** Phase β answers this.
5. **Is APL (§4.2.7) expressive enough for the first 5 real attenuation
   use cases?** If predicates routinely escape to effect handlers, the
   static surface is narrower than the design suggests.
6. **What is the right effect taxonomy for editor plugins?** Frank-like
   row polymorphism? Closed enum? Open trait family? Phase γ explores.
7. **Will Wasm stack-switching arrive in time?** Effects are
   implementable without it, but with significant performance cost.
   The Phase γ "5 µs/yield" abandon threshold is close to the
   current Wasmtime Component-Model call overhead — the headroom is
   small.
8. **Capability/effect duality (§4.7.3 D2): is the static/dynamic
   split sustainable in practice?** Some authority-relevant facilities
   straddle the line (e.g. timed grants). If the split becomes
   case-by-case, the substrate has two redundant surfaces rather than
   complementary ones.

### 21.3 Identity, time, and merge

9. **Is content-addressed plugin distribution a real demand or just
   theoretically elegant?** Phase δ tests it on real users.
10. **Does the prefix/effect-log split (§4.4.7) survive workloads
    with very long effect logs?** Sessions with frequent LLM calls
    produce large logs; effective truncation/checkpointing policy
    is unknown.
11. **Are there CRDTs for *plugin state* (not just text)?** §4.5.10
    defers this; Phase ζ ships text + external-input CRDT only.
    Whether per-plugin CRDT discipline ever becomes tractable is open.
12. **Are there enough multi-device single-users to justify Phase ζ?**
    No data exists yet.
13. **How many `VersionId` callsites silently assume total order?**
    The Phase ζ audit (§13 concrete first PR) is the gating
    measurement.

### 21.4 AI agents, privacy, and editor-agnosticism

14. **What is the trust model for AI agents in an editor?** Phase η is
    the first to confront this practically.
15. **Are attenuation predicates predictive of LLM behaviour?** A
    capability says what the agent *may* attempt; it does not constrain
    what the LLM *proposes*. The gap between authority bound and
    proposal bound is unaddressed.
16. **What is the right unit of editor-agnosticism?** Buffer model?
    Cursor model? Mode system? Phase ι answers.
17. **How does Phase η interact with end-to-end encryption / privacy?**
    AI agents that need full context conflict with users who want
    privacy.

### 21.5 Inheriting FRP's known pathologies

The Core Principle (§2) is structurally the FRP thesis: derived values
as a graph over time-indexed inputs. FRP literature has documented
three pathologies that any such substrate must address explicitly.

18. **Glitch problem.** When multiple inputs update non-atomically, a
    naive substrate exposes transient inconsistent intermediate
    states to dependent computations. §4.1.8 claims to address this
    via the frame-boundary push-drain discipline. Has this held under
    workloads where a frame contains pushes that *depend on each
    other* (e.g. a file rename + a buffer reload triggered by the
    same OS event)?
19. **Time leak.** Classical FRP retains past event values longer
    than needed, causing unbounded memory growth. Salsa's slot model
    holds only the current value, and history is sequestered into
    ADR-035's history dimension. Does this discipline hold under
    sustained editing sessions, and what is the memory growth curve
    of the effect log (§4.4.7) and time-store (Phase ζ)?
20. **Causality loop.** A derived value that feeds back into its own
    input is a fixed-point definition whose convergence is
    undecidable in general. §4.1.9 forbids this *by construction*
    (only host adapters may `set_*`; plugin output goes via
    `Effects`). Is this construction-level constraint sufficient, or
    will pragmatic plugin patterns repeatedly want to violate it (and
    if so, what is the supported alternative)?

### 21.6 Type-theoretic and language-design open questions

21. **Are macro-encoded effect rows debuggable enough?** Phase γ's
    macro DSL hides the underlying state-machine encoding. When
    compile errors or runtime mismatches surface through the macro
    layer, are they comprehensible without exposing the unsugared
    form?
22. **Is Rust + Wasm Component Model `resource` truly "good enough"
    (§4.6.4)?** True linear typing would prevent capability leak
    statically; affine typing prevents only the most direct cases.
    The gap matters under composition.

## 22. Related Work and Inspirations

| System | What we borrow | What we leave |
|---|---|---|
| **Salsa** | Incremental computation with dependency tracking | Single-machine, linear-time |
| **Differential Dataflow / Materialize** | Operators, fixpoint, partial-order time | Cluster-scale infrastructure |
| **Unison** | Content-addressed code, hash-stable distribution | Custom language requirement |
| **Spritely Goblins / E-language** | Capability theory, sealing, sturdy refs | Distributed-by-default complexity |
| **Cap'n Web / Cap'n Proto** | Promise pipelining, capability transport | Schema-first IDL approach |
| **Yjs / Automerge** | Sequence CRDTs, conflict-free merge | Browser-centric implementations |
| **Frank / Koka / OCaml 5 multicore** | Algebraic effect formalism | Pure-language requirement |
| **Adapton / Bonsai** | Demand-driven incremental computation | OCaml-only ecosystem |
| **React + React Suspense** | Effect-yielding UI components | DOM-centric model |
| **Classical FRP (Elliott, Hudak)** | The thesis that derived UI values are graphs over time-indexed inputs (§2 Core Principle) | Continuous-time semantics; denotational purity that resisted production scaling |
| **Yampa / Arrowized FRP** | Causality enforced by the type of the dataflow combinator (no future-reading) | Arrow-combinator surface — too austere for plugin authors |
| **Sodium / ReactiveX** | Push-based event-graph implementations and the documented glitch/time-leak/causality-loop pathologies | Single-process imperative event-graph as the user-facing model |
| **Self-Adjusting Computation (Acar)** | Dependency-tracked incremental recomputation; direct ancestor of Adapton and Salsa | OCaml-centric ecosystem |
| **Emacs** | Plugins as first-class citizens, everything-is-customisable | Untyped, insecure, imperative |
| **Smalltalk image** | Live-coding, time-travel debugging | No isolation |
| **Plan 9 / 9P** | Uniform interface (everything-is-a-file) | Untyped bytes-in-bytes-out |
| **Nix** | Reproducible builds, content-addressed store | Domain-specific language |
| **Materialize** | DD in production | Stream-database focus |

The synthesis "DD + capabilities + effects + content-addressing + CRDTs
in an editor substrate" appears to be novel; the components are mature.

**On the FRP lineage.** DDD-CST's Core Principle (§2) is structurally
the FRP thesis. The FRP literature spent three decades on the same
abstraction and produced no production-scale editor substrate;
classical FRP failed primarily on three pathologies — glitches, time
leaks, and causality loops — which are catalogued in Sodium's
documentation and Conal Elliott's later push-pull FRP work. §4.1.9
adopts the *push-to-set, pull-to-derive* discipline precisely to
address these failure modes, and §21.5 lists them as ongoing open
questions rather than solved problems. Acknowledging this lineage
is intellectual responsibility: DDD-CST is the FRP attempt that hopes
to succeed by sequestering effects (§4.3), bounding authority (§4.2),
and capping inputs at a frame boundary — not by reinventing the
abstraction.

## 23. References

- McSherry, F. et al. "Differential Dataflow." CIDR 2013.
- Miller, M. S. "Robust Composition: Towards a Unified Approach to Access
  Control and Concurrency Control." PhD dissertation, JHU, 2006.
- Plotkin, G., Pretnar, M. "Handlers of Algebraic Effects." ESOP 2009.
- Hickey, R. "Simple Made Easy." Strange Loop 2011.
- Chiusano, P., Bjarnason, R. "Functional Programming in Scala." Manning
  2014. (Influence on effect tracking discussion.)
- Elliott, C., Hudak, P. "Functional Reactive Animation." ICFP 1997.
  (Classical FRP foundation.)
- Elliott, C. "Push-Pull Functional Reactive Programming." Haskell
  Symposium 2009. (Push/pull reconciliation precedent for §4.1.8.)
- Nilsson, H., Courtney, A., Peterson, J. "Functional Reactive
  Programming, Continued." Haskell Workshop 2002. (Yampa /
  arrowized FRP; causality through types.)
- Cooper, G. H., Krishnamurthi, S. "Embedding Dynamic Dataflow in a
  Call-by-Value Language." ESOP 2006. (Glitch-free FRP evaluation.)
- Acar, U. A. "Self-Adjusting Computation." PhD dissertation, CMU
  2005. (Ancestor of Adapton and Salsa.)
- Shapiro, M., Preguiça, N., Baquero, C., Zawirski, M. "A
  Comprehensive Study of Convergent and Commutative Replicated Data
  Types." INRIA RR-7506, 2011.
- Salsa documentation: https://salsa-rs.github.io/salsa/
- Unison documentation: https://www.unison-lang.org/docs/
- Spritely Goblins: https://spritely.institute/goblins/
- Yjs: https://docs.yjs.dev/

## Appendix A: Glossary

- **DDD-CST**: Distributed Differential Dataflow with Capability-Secured
  Time. The substrate this document describes.
- **Capability**: An unforgeable reference granting specific authority over
  a resource. From the object-capability literature.
- **Attenuation**: Producing a weaker capability from a stronger one. Always
  monotone (cannot increase authority).
- **Sealing**: Wrapping a capability in an opaque box that only a specific
  holder can unseal. Enables rights amplification patterns.
- **Sturdy reference**: A serialisable form of a capability that can be
  reconstituted across process restarts with host cooperation.
- **Epoch**: One unit of dataflow propagation. May be linearly ordered
  (Phases α–ε) or partially ordered (Phase ζ+).
- **Algebraic effect**: A side-effect description that a function `yield`s
  rather than performs. The runtime handler decides what actually happens.
- **Content addressing**: Identifying artefacts by hash of their content
  rather than by name+version.
- **CRDT**: Conflict-free Replicated Data Type. Data type whose
  concurrent updates merge deterministically.
- **Resource (WIT)**: A WebAssembly Component Model construct providing
  unforgeable handles to host-managed values.

## Appendix B: Worked Example — Inline Diff Plugin in Phase ε

A plugin that displays inline git diff in the active buffer, written
against the Phase-ε API (Phases α + β + ε landed; γ and beyond not
required):

```rust
use kasane_plugin_sdk::*;

define_plugin! {
    name: "inline-diff",
    version: "1.0",

    capabilities {
        buffer: cap<BufferView, ReadOnly>,
        git: cap<GitService, RepoLocal>,
        display: cap<DisplaySink, AnnotationsOnly>,
    }

    salsa_queries {
        // Buffer-derived
        let current_text = buffer.text_at(buffer.version);

        // External, via Salsa-input-backed git transport
        let head_text = git.show(buffer.repo, "HEAD", buffer.path);
        let diff = diff_of(&head_text, &current_text);

        // Derived
        let marks = diff.flat_map(|hunk| hunk.expand_to_marks());
    }

    display() {
        marks.iter().map(|m| {
            DisplayDirective::InsertAfter {
                after: m.line,
                content: m.text.clone(),
                face: m.face,
            }
        }).collect()
    }
}
```

What's invisible in this code:
- `buffer.text_at(version)` is a Salsa query — memoised, dependency-tracked
- `git.show(...)` returns a `future` — host pipelines via the daemon
  transport, plugin sees the resolved value
- `diff` is a Salsa-derived value — recomputed only when inputs change
- `marks` is also Salsa-derived
- `display()` is called only when `marks` changes
- The whole plugin runs in a Wasm sandbox with no `spawn-process`, no `kak
  -p`, no manual cache management

This is the *expressive endpoint* of DDD-CST: a non-trivial plugin
expressed as pure declarations over typed inputs and outputs.

---

*This document is a snapshot of long-horizon thinking captured 2026-05.
Update or supersede as the project evolves.*
