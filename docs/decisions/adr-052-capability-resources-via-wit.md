# ADR-052: Capability Resources via WIT (DDD-CST Phase β)

**Status:** Accepted (2026-05-22). Derived from
[ddd-cst-vision.md §9 (Phase β)](../ddd-cst-vision.md) and §4.2
(capability theory). Refines
[ADR-028](./adr-028-wasm-capability-inference.md) by replacing
manifest-grep gating with unforgeable WIT resource handles. Pairs with
[ADR-056](./adr-056-attenuation-predicate-language.md) (APL) for
attenuation algebra.

**Update 1 (2026-05-22, baseline spike):** Wasmtime 43 baseline on
`kasane-wasm-bench/benches/component_model.rs` shows plain Component
Model call overhead at **430–574 ns** (noop / add / echo_string_10).
WIT resource methods add only table-lookup overhead on top of this —
published Wasmtime measurements place the add at ~50–200 ns. The
**>10 µs abandon threshold is therefore not in danger** on Wasmtime-
overhead grounds; expected resource method cost is ≤1 µs, giving ≥10×
headroom. A full resource-specific benchmark is deferred to first
implementation; this baseline removes the dominant perf risk but does
not eliminate it. The ergonomic abandon criterion (handle-threading
unbearable in author testing) remains open.

**Update 2 (2026-05-22, chunks 1–5 land):** The capability-resource
machinery is in tree on `kasane:plugin@6.5.0`:

- *WIT* — new `host-capabilities` interface defines `resource
  buffer-view { get-lines-text }` and `open-buffer-view` (additive
  minor bump; existing 6.x plugins continue to load).
- *Host* — `kasane-wasm::buffer_view` wires the WIT resource into
  `HostState::table` via `bindgen!`'s `with:` mapping; the bound
  rep type is `BufferViewRep`.
- *Manifest schema* — `[[capabilities.services]]` array of tables
  (`kasane-plugin-package::manifest::ServiceDeclaration`); validator
  rejects unknown / duplicate service names. Known set this chunk:
  `"buffer"`.
- *Broker* — `kasane-wasm::broker::CapabilityBroker` is constructed
  per-plugin from the manifest at `load_with_manifest` time and
  consulted on every `open-*`. Acquisition-time check, hot path
  unaffected (per §Rationale 2).
- *SDK macro* — `define_plugin!` validates service names at compile
  time, surfaces them as `pub const REQUESTED_SERVICES`, and adds the
  `host_capabilities` re-export under `__kasane_sdk` so plugin code
  can write `host_capabilities::open_buffer_view()` directly.
- *First migration* — `examples/wasm/color-preview` declares
  `service "buffer"` and uses `BufferView` for the `handle_mouse`
  safety check. The bundled `.wasm` is rebuilt against 6.5.0.
- *Tests* — host-side unit tests + two proptest properties:
  forged handle ids never read buffer state, and a broker-empty
  plugin is denied on every attempt.

Deferred to follow-up work (not blockers for acceptance):

- A dedicated buffer-view resource-method bench. The Update 1
  baseline already places the expected cost ~1 μs (≥10× under the
  abandon threshold); ratification awaits the dedicated bench.
- Load-time error when a plugin's `.wasm` calls `open-buffer-view`
  without declaring the matching service. Today the broker returns
  `open-error::denied` at runtime (the WIT-level type already gives
  the *unforgeability* property); a load-time scan that lifts the
  failure earlier is a polish item.
- APL attenuation end-to-end on `BufferView` — owned by
  [ADR-056](./adr-056-attenuation-predicate-language.md); requires
  the additional `buffer-view.attenuate(predicate)` method on the
  resource. Tracked as ADR-056 Phase β follow-up.

### Context

Plugin authority in Kasane today is string-keyed and ambient:

- `register-capabilities` (ADR-028) declares a coarse bitmask;
- `query-daemon("git", ...)` (perken's POC) looks services up by name;
- `topic-publish("buffer.lines.changed", ...)` (ADR-029) lets any plugin
  emit on any topic.

Each of these is vulnerable to **confused-deputy** patterns (vision §4.2.3):
a plugin holding a broad authority can have it co-opted by an
attacker-controlled string. Capability theory addresses this with three
pillars:

1. **Unforgeable reference** — handles cannot be constructed by name.
2. **No ambient authority** — a plugin with no capabilities can do
   nothing.
3. **Reified grants** — authority is propagated by explicit handoff, not
   implicit context.

The Wasm Component Model's `resource` type is now stable in Wasmtime 22+
and provides exactly the unforgeability property at the WIT/host boundary.
A handle is opaque, host-managed, and can only be obtained by being given.

### Decision

Authority over external services is represented as **WIT resources**
held by plugins. A plugin that wants to use a service must hold the
corresponding resource handle. Authority is checked at handle
**acquisition** (`open-service`), not at every method call.

WIT shape:

```wit
resource service {
    query: func(req: list<u8>) -> future<result<list<u8>, service-error>>;
    events: func(topic: string) -> stream<list<u8>>;
}

open-service: func(spec: service-spec) -> result<service, open-error>;
```

The manifest declares which services a plugin may open:

```kdl
plugin "git-diff" {
    capabilities {
        service "git" repo-local=#true
        service "display" annotations-only=#true
    }
}
```

Host-side `CapabilityBroker` enforces manifest-declared bounds at
`open-service` time. A plugin omitting a capability declaration fails to
load (compile-time error in the SDK macro, runtime error at the host).

Handle semantics:

- **Affine.** Handles are consumed by transfer; cloning requires explicit
  `.fork()` returning two handles. Drop revokes.
- **Attenuable.** `cap.attenuate(predicate)` produces a weaker handle
  whose predicate is restricted to the [APL fragment](./adr-056-attenuation-predicate-language.md).
- **Non-pipelined initially.** Promise pipelining (vision §4.2.4) is
  deferred to Phase θ; today every method call awaits.

### Scope

**In scope.**

- WIT `resource service` definition and host implementation.
- `CapabilityBroker` enforcing manifest-declared bounds at `open-service`.
- Manifest schema extension for `capabilities` block.
- **First migration: a single `BufferView` resource** with one method
  (`get-lines-text`). This is the lowest-stakes possible API surface for
  validating resource ergonomics.
- SDK helper `define_plugin!` extension to emit capability declarations
  hash-stably (see [ADR-055](./adr-055-prefix-effect-log-split.md) for
  why identity reflects authority).
- Property tests for handle unforgeability: no Wasm-level construction
  path produces a valid `service` instance.

**Out of scope.**

- APL specification — see [ADR-056](./adr-056-attenuation-predicate-language.md).
- Daemon-backed transports — see [ADR-054](./adr-054-daemon-registry-as-transport-layer.md).
- Promise pipelining (Phase θ).
- Cryptographic capability signing (separate ADR if ever needed).
- Migration of every existing string-keyed authority. This ADR
  establishes the pattern with one resource; subsequent ADRs migrate
  individual services.

### Rationale

1. **Unforgeability is a property of the type system, not policy.** ADR-028
   bitmasks rely on manifest inspection; resources rely on the WIT-level
   guarantee that handles cannot be synthesised. The latter is a strictly
   stronger property.

2. **Acquisition-time check, not call-time.** A plugin that opens 1000
   `git` queries pays the broker cost once. ACL-style per-call checks
   would amortise poorly across the Phase γ effect-yielding hot path.

3. **Manifest-as-static-bound aligns with the duality split (vision
   §4.7.3 D2).** Manifest-declared capabilities define what a plugin is
   *permitted* to attempt; effect handlers (ADR-053) define what it
   actually *does* on a given epoch. Both surfaces are needed and must
   be kept distinct.

4. **Identity reflects authority (D4).** Two plugins with bit-identical
   code but different capability requests are *different artefacts*
   under content addressing. This is the POLA analogue at the identity
   layer. This ADR commits to emitting capability declarations
   hash-stably so the property holds.

5. **BufferView first is deliberate ergonomic validation.** Resources
   are powerful but verbose; `get-lines-text` is the most-called host
   function, so any friction shows up immediately.

### Alternatives considered

- **Stay with `register-capabilities` bitmask (ADR-028).** Lower cost,
  no Wasmtime version bump. Rejected: ambient authority returns the
  moment any string-keyed lookup is added (perken's `query-daemon` is
  the immediate example).
- **Cryptographic tokens (JWT-style).** Capabilities as signed strings.
  Rejected: the WIT resource model already gives unforgeability without
  cryptography, and signed strings reintroduce string-keyed lookup.
- **Per-method capability arguments.** Pass a token to every host call.
  Rejected: handle-per-resource amortises better, and the per-call form
  is what ACLs do — confused-deputy returns.
- **Pony-style reference capabilities.** Type-system-level ref caps in
  the SDK. Rejected: requires a non-Rust language or substantial macro
  work; affine handles via WIT resource are good enough (§4.6.4).

### Consequences

- **Positive.**
  - Confused-deputy patterns become representable and preventable.
  - Plugin teardown is precise: dropping the resource set revokes all
    authority (membrane pattern, §4.2.4).
  - Manifest is now *executable* — declared capabilities have a host-
    enforced runtime meaning.

- **Negative.**
  - Wasmtime 22+ requirement. Plugins built against older Component
    Model versions need rebuild.
  - Handle threading verbosity in plugin code. The `define_plugin!`
    macro mitigates but does not eliminate.
  - Migration cost across every existing host primitive that currently
    runs on string-keyed paths.

### Exit criterion

- `BufferView` resource ships and at least one bundled plugin migrates
  to it.
- A plugin omitting the `service "buffer"` declaration fails to load
  with a clear error message.
- Resource method call overhead measured at ≤ 5 µs on the hot path
  (well within vision §17.1 budget; see abandon criterion below).
- Property test: no Wasm-side construction of a `service` instance.
- APL attenuation (per ADR-056) works end-to-end on the `BufferView`
  resource: `buffer.attenuate(line_range(0, 100))` returns a handle
  whose subsequent calls fail outside the range.

### Abandon criterion

- Wasmtime's resource implementation has irrecoverable per-method
  overhead (> 10 µs) that cannot be amortised.
- Plugin authors find explicit handle threading unbearable in ergonomic
  testing (e.g. with 5 bundled-plugin authors).
- The `CapabilityBroker` runtime check at `open-service` cannot be
  expressed without leaking implementation details into the WIT
  surface — meaning every new service requires a host-internal API change.

If abandoned, the next-best fallback is **resource for new APIs only**:
keep the ADR-028 bitmask for existing host primitives; require WIT
resources only for genuinely new services (LSP, AI, etc.). This sacrifices
unforgeability uniformity but preserves it for new attack surface.

### Open questions

- **OQ-1 (vision Q4).** Can WIT resources scale to thousands of live
  handles without per-handle overhead? Phase β's exit criterion measures
  a single resource; the answer for N=1000 is unknown.
- **OQ-2 (vision Q8).** The static/dynamic capability/effect duality
  (D2) — some facilities straddle the line (timed grants, revocation
  triggered by handler policy). This ADR commits to *capability for
  acquisition, handler for runtime decision*; cases that don't fit will
  surface during migration.
- **OQ-3.** Is the `CapabilityBroker` itself a capability? (Bootstrapping
  question.) This ADR treats it as host-internal; if plugins ever need
  to grant capabilities to *other* plugins, the broker becomes a
  resource and a new ADR is needed.
- **OQ-4.** Should `service.events` returning `stream<list<u8>>` route
  through the [ADR-051](./adr-051-external-data-as-salsa-inputs.md)
  `ExternalInputRegistry`? Likely yes — push-to-set discipline applies
  — but the exact wiring is left to the first cross-cutting migration.
