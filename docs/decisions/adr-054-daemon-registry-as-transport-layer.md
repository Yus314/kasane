# ADR-054: Daemon Registry as Transport Layer (DDD-CST Phase ε)

**Status:** Proposed (2026-05-22). Derived from
[ddd-cst-vision.md §12 (Phase ε)](../ddd-cst-vision.md). Retrofits
perken's daemon-registry POC (AntoineBalaine/kasane `upstream_port`,
2026-05) underneath [ADR-052](./adr-052-capability-resources-via-wit.md)
capability resources and [ADR-051](./adr-051-external-data-as-salsa-inputs.md)
Salsa inputs. This is the **integration ADR** for perken's upstream
contribution.

### Context

Two independent threads of external-service work exist:

1. **perken's upstream_port** (AntoineBalaine/kasane, 2026-05) introduces
   a `DaemonRegistry` host module and a `query-daemon` WIT primitive.
   Long-lived shared services (a `git` daemon, an `lpr` daemon) run as
   separate processes; plugins talk to them through a string name.
2. **DDD-CST Phases α + β** (ADRs 051 + 052) introduce Salsa-input-backed
   external data and unforgeable capability resources for services.

These threads converge: the daemon-registry is a *transport* — one
implementation of "long-lived shared external service". Plugins don't
need to know it's a daemon; they need a capability resource that may
*happen* to be backed by a daemon connection.

Without this convergence:

- perken's `query-daemon` is plugin-facing → string-keyed, ambient
  authority returns.
- Capability resources from ADR-052 have no concrete transports →
  cannot ship without bespoke wiring per service.
- The host gains two competing extension surfaces for "external
  service": one named-string (daemon), one capability-handle (resource).

### Decision

`DaemonRegistry` becomes one of N `ServiceTransport` implementations,
sitting **beneath** the ADR-052 capability resource and **feeding**
the ADR-051 input registry where appropriate.

```rust
pub trait ServiceTransport: Send + Sync {
    fn open(&self, spec: ServiceSpec) -> Result<ServiceHandle, OpenError>;
    fn name(&self) -> &'static str;
}

pub struct DaemonTransport { /* perken's daemon_registry.rs, renamed */ }
pub struct ProcessTransport { /* one-shot subprocess */ }
pub struct HttpTransport { /* network */ }
pub struct InProcessTransport { /* native plugin callsite */ }
```

Manifest declares the required transport per capability:

```kdl
plugin "git-diff" {
    capabilities {
        service "git" transport="daemon"
    }
}
```

The `CapabilityBroker` (ADR-052) selects the transport based on the
manifest and the host's registered transports. Plugins receive a
`service` resource (ADR-052); the transport behind it is opaque.

`query-daemon` WIT primitive is **retired** from plugin-facing surface.
Existing perken-fork plugins migrate to capability resources via a
deprecation cycle (≥1 release per §17.4).

### Scope

**In scope.**

- perken's `kasane-wasm/src/daemon_registry.rs` lands in upstream,
  renamed to `kasane-wasm/src/transports/daemon.rs`.
- `ServiceTransport` trait and three additional implementations
  (`ProcessTransport`, `HttpTransport`, `InProcessTransport`).
- `ServiceSpec` schema that carries the manifest-declared transport
  selector.
- One reference daemon (`kasane-git-daemon`) ships with the upstream
  release.
- Migration shim: existing `query-daemon` callsites lower to
  `open_service` with `transport="daemon"`.
- Daemon crash + auto-restart integration test.

**Out of scope.**

- Distributed (cross-machine) daemons. That is Phase θ.
- Plugin-launched daemons. All daemons spawn from the host or pre-exist
  on the system; plugins cannot fork processes.
- A daemon discovery protocol (DNS-SD, Bonjour, etc.). Daemons are
  configured statically per host installation.
- Capability resources for non-service authority (file access, network,
  etc.) — those are separate ADRs.

### Rationale

1. **Transport ≠ authority.** Plugins should care about *what service
   they may use* (capability), not *how the service is reached*
   (transport). Conflating the two — as `query-daemon` does — leaks
   implementation into the plugin API.

2. **Multiple transports validate the abstraction.** A `ServiceTransport`
   trait with only one implementation (daemon) is not an abstraction —
   it is a renamed struct. Shipping `HttpTransport` alongside is the
   minimum that proves the trait generalises.

3. **perken's coordination matters.** This is also a *human* ADR. The
   retrofit must be acceptable to perken as the POC author; the
   concrete first PR is opened against AntoineBalaine/kasane, not
   upstream. Co-maintainership viability is decision point §19.4.

4. **Salsa-input feeding is opt-in per transport.** Some transports
   produce streaming events (LSP `publishDiagnostics`); those route
   through ADR-051 `ExternalInputRegistry`. Others are request-response
   (git query); those return values synchronously. The transport
   declares which mode it provides; the broker wires accordingly.

5. **`query-daemon` retirement is non-negotiable for the long horizon
   but staged.** Keeping two parallel surfaces (daemon-string + resource)
   forever bifurcates the plugin ecosystem. A deprecation cycle keeps
   perken-fork users alive through the transition.

### Alternatives considered

- **Keep `query-daemon` plugin-facing; resources are sugar over it.**
  Rejected: confused-deputy patterns return through the string-keyed
  surface.
- **One transport per ADR.** Ship daemon-only now; add HTTP later.
  Rejected: the trait without two implementations is unfalsifiable —
  we cannot tell whether it abstracts correctly.
- **Reject perken's POC and design from scratch.** Rejected: perken
  has a working implementation, real users, and contributor
  bandwidth. Greenfield design would discard those.
- **Daemons as plugins.** Run the git daemon as a Kasane plugin in
  Wasm. Rejected: defeats the "long-lived shared service" property;
  daemons exist precisely because plugin sandboxes are not the right
  shape for long-running native services.

### Consequences

- **Positive.**
  - perken's contribution lands without sacrificing capability discipline.
  - The transport abstraction is *testable* (multiple implementations
    exist).
  - Daemon crashes and HTTP timeouts share recovery code paths.
  - Plugin authors write transport-agnostic code; transport switches
    are manifest edits.

- **Negative.**
  - Indirection cost: every capability call goes broker → transport →
    daemon/HTTP/etc. Per-call overhead must stay within vision §17.1
    budget.
  - Migration burden on perken-fork users.
  - Daemon protocol versioning becomes a host concern (the host
    mediates; clients see only the resource surface).

### Exit criterion

- perken's POC compiles against upstream WIT (ADR-052 in place).
- Existing perken-fork users can switch to upstream Kasane without
  re-installing their daemons (the on-disk daemon binaries are
  unchanged; only the plugin-side wire format changes).
- `HttpTransport` exists and is used by at least one reference plugin.
- Daemon crash + restart integration test passes — the resource handle
  survives a daemon crash by failing in-flight requests and reconnecting
  transparently on the next call.
- `query-daemon` callsites in upstream are zero.

### Abandon criterion

- Capability resources from ADR-052 cannot be retrofitted onto perken's
  socket protocol without unacceptable churn (estimate: > 2 weeks of
  protocol redesign).
- perken withdraws from co-maintenance and no other contributor can
  drive the integration.
- The `ServiceTransport` trait turns out to require per-transport
  trait methods so different that the trait collapses to "yes I'm a
  transport" with no shared surface.

If abandoned, the next-best fallback is **perken's POC ships as-is**:
`query-daemon` stays plugin-facing, capability resources of ADR-052
exist in parallel for non-daemon services. The bifurcation is accepted;
the unifying claim of vision §12 is rejected.

### Open questions

- **OQ-1.** Should the `ProcessTransport` (one-shot subprocess) preserve
  the existing `spawn-process` ergonomics for backwards compatibility,
  or expose only the new resource surface? This ADR commits to the
  latter (deprecate `spawn-process` over ≥2 release cycles).
- **OQ-2.** Daemon authentication: how does the host ensure the daemon
  it talks to is the one it spawned (and not a rogue process binding
  to the socket)? perken's POC uses filesystem-permission-based trust;
  audit before exposing user-installed daemons.
- **OQ-3.** Transport selection at runtime vs manifest-only. Today the
  manifest pins `transport="daemon"`. Should the host be allowed to
  substitute (e.g. fall back to `process` if daemon is unavailable)?
  This ADR says no — substitution would break the manifest's static
  authority claim — but the question recurs.
- **OQ-4 (vision §19.4).** Is perken still a co-maintainer through this
  retrofit? Decision point gates further phase commitment; tracked in
  vision §19.
