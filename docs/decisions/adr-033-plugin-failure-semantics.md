# ADR-033: Plugin Failure Semantics

**Status**: Proposed (2026-05-01)

### Context

Plugin handlers can fail for reasons outside the framework's control: a native plugin panics on an unexpected app-state shape, a WASM plugin traps after consuming its epoch budget (PR #74), or a process-task callback returns an error from a logic bug. Today the host has no defined contract for what happens next. Empirically the outcomes are:

- **Native panic**: propagates out of `PluginBridge` dispatch and unwinds through `PluginRuntime`. The host process either aborts (panic = abort profile) or unwinds to the event loop, where the panic is unhandled. Either way, one plugin's bug crashes the entire editor.
- **WASM trap**: `wasmtime` returns a `Trap` from the host shim. PR #74 widened the runtime's epoch budget and surfaces a trap in tests as a panic, but production code paths simply propagate the trap as a generic error — the failing plugin keeps being invoked next frame and traps again.
- **Handler-returned error / wrong shape**: not currently representable. Handlers are infallible by signature (`fn(&State, ...) -> (State, Effects)`); a logic bug typically manifests as a panic.

The asymmetry between native panics (fatal) and WASM traps (persistently failing but non-fatal) is itself a problem: the same conceptual failure produces different host behaviour based on the plugin's runtime, which leaks an implementation detail into the user-visible failure mode.

This ADR defines a single contract for plugin failure across both runtimes.

### Decision

Adopt a three-axis failure contract — **isolate**, **recover**, **observe** — applied uniformly to native and WASM plugins.

#### 1. Isolation: per-handler failure containment

Every plugin handler invocation runs inside a failure boundary that catches the runtime-specific failure mode:

- **Native handlers**: wrapped in `std::panic::catch_unwind`. Plugin authors who use `RefUnwindSafe`-incompatible types in their state can opt out via `AssertUnwindSafe`, but the default behaviour is to catch.
- **WASM handlers**: traps are already trapped at the `wasmtime` boundary in `kasane-wasm`. The trap is converted into the same internal `PluginFailure` value as a native panic.

A failure within one handler does **not**:
- Roll back state changes from earlier handlers in the same frame.
- Cancel the in-flight frame. Already-collected contributions and effects are still rendered/applied.
- Affect other plugins' handlers in the same frame.

A failure does **not propagate** out of `LoadedPlugin` dispatch. The framework consumes it and produces the recovery action below.

#### 2. Recovery: bounded retry with fail-closed

Each `LoadedPlugin` carries a `consecutive_failures: u8` counter. On any handler failure:

1. The counter increments.
2. If the counter is `< THRESHOLD` (default `3`), the plugin remains active. Future frames re-invoke its handlers normally; a successful handler call resets the counter to 0.
3. If the counter reaches `THRESHOLD`, the plugin transitions to `disabled = true`. All subsequent frames skip every handler this plugin would have served until the user explicitly re-enables it.

The disable transition is **not** a graceful shutdown — `on_shutdown` is not called, because the plugin's panic-free contract is already broken and shutdown handlers may compound the failure. This trades cleanliness for predictability.

`THRESHOLD = 3` is a default, not a hard constant. Configurable via `kasane.kdl` per-plugin, motivated by the cost asymmetry: a plugin that panics once per minute degrades the user experience more than one that panics on first frame and gets disabled immediately.

Manual re-enable is a host-side affordance (a command, e.g. `:kasane-plugin-reenable <id>`); spec'd here as the recovery path but implemented separately.

#### 3. Observability: PluginDiagnostic at Critical severity

Each failure synthesises a `PluginDiagnostic` with:

- `severity: Critical` (currently the highest level; ADR-030 §observability lists `Info | Warning | Error | Critical`).
- `kind: PluginDiagnosticKind::Failure { handler, runtime, message }` (new variant).
- `target`: the plugin ID and, when available, the handler entry point name.

These diagnostics are surfaced through the existing `diagnostics_overlay` plugin (Ctrl-Shift-D in default keymap) and through the structured logging target `kasane::plugin_failure`. The disable transition emits an additional `Critical` diagnostic announcing that the plugin has been quarantined.

### Cross-runtime parity

| Failure mode             | Native handler                 | WASM handler                    |
|--------------------------|--------------------------------|---------------------------------|
| Logic panic / divide-by-zero | `catch_unwind` returns `Err`   | `wasmtime::Trap`                |
| Allocation failure       | `catch_unwind` returns `Err`   | `Trap::OutOfBounds` or similar  |
| Epoch budget exceeded    | n/a                            | `Trap::Interrupt` (PR #74)      |
| State `state_hash` panic | `catch_unwind` returns `Err`   | n/a (host-side)                 |

All four converge on the same `PluginFailure` internal type and feed the same recovery counter. The diagnostic distinguishes the runtime in `PluginDiagnosticKind::Failure { runtime, .. }` so the user-facing message reads `"plugin <id>: native panic in handle_key"` vs. `"plugin <id>: wasm trap (epoch budget) in handle_key"`.

### What this is *not*

- **Not a transactional rollback.** Effects and state mutations from earlier handlers in the same frame remain applied. Implementing rollback would require effect-stage atomicity that ADR-026 (ElementPatch) and ADR-029 (pub/sub) explicitly do not provide.
- **Not a sandbox escape mitigation.** WASM plugins already have wasmtime's sandbox (memory isolation, capability denials per ADR-013). This ADR is about *liveness*, not *integrity*.
- **Not graceful degradation per handler.** A plugin's `decorate_gutter` panicking does not silently produce an empty gutter and continue serving `decorate_background` in the same frame. The plugin's *whole* set of handlers for that frame is treated as failed; the next frame retries everything.

### Risks

| Risk                                                             | Mitigation                                                                 |
|------------------------------------------------------------------|----------------------------------------------------------------------------|
| `catch_unwind` not safe for plugin states with interior mutability | `Plugin::State` already requires `Clone + PartialEq + Debug + Send + Hash` — interior mutability is not idiomatic. Explicit `RefUnwindSafe` documentation in plugin-development.md. |
| Threshold of 3 too aggressive (transient failures get disabled)  | Configurable per-plugin; transient failures (e.g. file-watcher restart) often have a natural retry pattern in the plugin's own state machine |
| Threshold of 3 too lenient (broken plugin runs 3× before quarantine) | Acceptable: the user sees diagnostics on the first failure, can disable manually before the third |
| Disable is sticky across editor restarts                          | No — disable lives in `LoadedPlugin` runtime state, not config. Restart re-enables. The user must take explicit action only if they want *permanent* disable, via the existing `plugins.disabled` config |
| Failure inside a transparency-required handler violates ADR-030 invariants | The ADR-030 §10.2a recovery witness already covers this — Hide directives surrender to recovery handlers; same applies when the handler that *issued* the directive fails |

### Implications

- **`LoadedPlugin` gains two fields**: `consecutive_failures: u8` and `disabled: bool`. (`PluginBridge` will be renamed to `LoadedPlugin` per the planned bridge restructuring; this ADR is agnostic to that rename.)
- **`PluginDiagnosticKind` gains a `Failure { handler: &'static str, runtime: PluginRuntimeKind, message: String }` variant**. `PluginRuntimeKind` is a new enum (`Native | Wasm`).
- **Dispatch sites in `bridge.rs` (or `loaded_plugin/dispatch.rs` post-restructuring) are wrapped in `catch_unwind`**. The existing `wasmtime` trap-handling in `kasane-wasm` is updated to feed the same recovery counter rather than logging-and-continuing.
- **`docs/plugin-development.md` documents the contract**: handlers should not panic; if they do, the plugin is quarantined after `THRESHOLD` failures.
- **Implementation lands as a single PR** (after the planned `PluginBridge` → `LoadedPlugin` rename in Phase δ, so the dispatch path is already structured for the wrap). Pre-rename, the wrap can also live in `PluginBridge` directly — the ADR is implementation-order-neutral.
- **No plugin API change.** `Plugin` trait methods are unchanged. The contract is enforced at the host boundary, not the plugin's signature.
