# WASM Plugin Constraints

This document catalogs the constraints of the WASM plugin system compared to native plugins. Each constraint is labeled:

- **\[By Design\]** — intentional, security or architectural boundary
- **\[Not Yet Implemented\]** — planned for future phases
- **\[Improvement\]** — current API works but has ergonomic friction

For a development guide, see [plugin-development.md](./plugin-development.md). For API reference, see [plugin-api.md](./plugin-api.md).

## What WASM Plugins Can Do

WASM plugins cover the primary UI extension surface: slot contributions, line annotations, overlays, element transforms, input handling, process spawning, timers, session management, and more. For the full extension point catalog, see [plugin-api.md §1](./plugin-api.md#1-extension-points).

## Quick Reference

| Feature | Native (`PluginBackend`) | WASM | Gap |
|---|---|---|---|
| Slot contributions | Full | Full | — |
| Line annotations | Full | Full | — |
| Overlays | Full | Full | — |
| Element transforms | Full | Full | — |
| Menu item transforms | Full | Full | — |
| Cursor style override | Full | Full | — |
| Input handling (key/mouse) | Full | Full | — |
| Process spawning | Full | Full | — |
| Timers | Full | Full | — |
| Inter-plugin messaging | `Box<dyn Any>` | `Vec<u8>` | Serialization required |
| Static surfaces | Full | Full | — |
| Dynamic surfaces | Full | Static only | Cannot add/remove/move post-init |
| Pane lifecycle | Full | None | No create/close/focus hooks |
| Pane rendering | Full | None | Cannot own custom panes |
| Pane commands | Full | None | No split/close/focus commands |
| Workspace commands | Full | Full | — |
| Workspace notifications | Full | Full | — |
| Theme token registration | Full | Full | — |
| State access | Direct `&AppState` | ~40 getter functions | See [Host State Access](#host-state-access) |
| Capability declaration | Auto-inferred from `HandlerRegistry` | `register-capabilities()` WIT export | SDK macro auto-generates bitmask |
| Cache invalidation | Automatic (`PartialEq`) | Manual `state_hash()` | See [Developer Experience](#developer-experience) |
| Fuel / timeout | N/A | None | No runaway protection |

## Extension Point Gaps

### Pane Lifecycle and Rendering \[Not Yet Implemented\]

Native plugins can receive `on_pane_created`, `on_pane_closed`, and `on_focus_changed` notifications, render custom pane content via `render_pane`, and handle pane-specific input via `handle_pane_key`. They can also declare `PanePermissions` (split, focus, spawn, float, cross-pane, tabs).

WASM plugins have no pane-related APIs. This depends on Phase 5 (Surface / Workspace) and roadmap §3.5 (Pane / Workspace parity model).

> Workaround: Use surfaces for dedicated panels. For IDE-like multi-pane layouts, use a native plugin.

### Workspace Commands and Notifications \[Resolved — WIT v0.22.0\]

WASM plugins can issue `WorkspaceCommand` (focus direction, resize, resize direction) and receive `on-workspace-changed` notifications with a `workspace-snapshot`.

### Dynamic Surfaces \[Not Yet Implemented\]

WASM surfaces are declared statically at init time via `surfaces()`. Native surfaces can be created, destroyed, and reparented at runtime. WASM surfaces cannot change their placement or slot declarations after initialization.

### Theme Token Registration \[Resolved — WIT v0.22.0\]

WASM plugins can issue `register-theme-tokens(list<theme-token-default>)` to define custom face names with fallback defaults.

## Host State Access

### Guarded Access Model \[By Design\]

WASM plugins cannot access `AppState` directly. All state is read through ~40 typed getter functions in the `host-state` WIT interface. This is an intentional sandbox boundary: the host controls what state is visible to each plugin, preventing unintended coupling to internal data structures.

> Workaround: None needed — this is the intended API surface. Request new getters via feature requests if specific state is missing.

### Missing State Queries \[Not Yet Implemented\]

The following `AppState` fields have no corresponding `host-state` getter:

| State | Native access | WASM access |
|---|---|---|
| Selection ranges and mode | `state.selections` | Not available |
| Search state and regex | `state.search` | Not available |
| Input mode | `state.input_mode` | Not available |
| Named registers | `state.registers` | Not available |
| Macro recording/playback | `state.macro_state` | Not available |
| Completion candidates | `state.completions` | Not available |
| Pane tree structure | `state.panes` | Not available |
| Workspace layout | `state.workspace` | Not available |
| Full face registry | `state.faces` | Not available |

These gaps block certain plugin categories. For example, a "selection highlight" plugin cannot be written in WASM because selection state is not exposed.

### Bulk Buffer Access \[Resolved — WIT v0.18.0\]

~~Buffer content is accessed one line at a time via `get_line_text(line_index)`.~~

Since WIT v0.18.0, bulk retrieval is available via `get-lines-text(start, end)` and `get-lines-atoms(start, end)`. These return all lines in the given range in a single host call, eliminating per-line round-trip overhead.

## Command API

### No Structured Kakoune Command API \[Improvement\]

WASM plugins interact with Kakoune exclusively through `SendKeys(Vec<String>)` — a list of keystroke strings. There is no structured command API for operations like "insert text at cursor" or "execute command".

The SDK provides helpers for the most common patterns:

```rust
use kasane_plugin_sdk::keys;

// Escape special characters in text and push as individual keys
keys::push_literal(&mut keys_vec, "edit foo.rs");

// Build a full <esc>:cmd<ret> sequence
let keys = keys::command("edit foo.rs");
```

There is no feedback mechanism — plugins cannot know whether a command succeeded or failed.

### Missing Command Variants \[Not Yet Implemented\]

| Command | Native | WASM |
|---|---|---|
| `Pane(PaneCommand)` | Available | Not available |

## Runtime Constraints

### Synchronous Execution \[By Design\]

All WASM plugin calls are synchronous and block the host thread. The plugin cannot use async/await or spawn background tasks within the WASM runtime. Long-running work should be delegated to external processes via `SpawnProcess`.

### No Threading \[By Design\]

WASI does not include threading support. Wasmtime does not enable shared memory or atomics. Each plugin instance is single-threaded.

### No Network Access \[By Design\]

WASI socket support is not enabled. Plugins requiring network access must delegate to an external process.

> Workaround: Spawn a helper process via `SpawnProcess` and communicate over stdin/stdout.

### No Fuel Metering or Timeout \[Improvement\]

The WASM runtime does not enforce fuel limits or call timeouts. A plugin with an infinite loop will block the editor indefinitely. Adding fuel metering or per-call timeouts would protect against this without affecting well-behaved plugins.

### Element Handle Scope \[By Design\]

Element handles (`u32`) returned by `element-builder` functions are valid only within the current plugin call. They cannot be stored and reused across calls. The element arena is cleared before each invocation.

### Mutex Poisoning on Trap \[Improvement\]

If a WASM trap propagates as a Rust panic (unlikely but possible in edge cases), the `Mutex<WasmPluginRuntime>` is poisoned and the entire application panics on the next call. Adding `catch_unwind` or using a non-poisoning mutex would improve resilience.

## Developer Experience

### Manual State Hash \[Improvement\]

Native `Plugin` trait plugins get automatic cache invalidation via `PartialEq` comparison. WASM plugins must implement `state_hash() -> u64` manually, typically using generation counters.

The `kasane_plugin_sdk::state!` macro reduces state management boilerplate by generating the struct, `Default` impl, `bump_generation()`, and `thread_local! STATE`:

```rust
kasane_plugin_sdk::state! {
    struct PluginState {
        active_line: i32 = 0,
        color_lines: HashMap<usize, ColorLine> = HashMap::new(),
    }
}

fn state_hash() -> u64 {
    STATE.with(|s| s.borrow().generation)
}
```

The `generation` counter pattern is sufficient for most plugins. Complex plugins may still need custom hash combining.

### Interactive Element ID Encoding \[Resolved\]

The SDK provides the `interactive_id!` macro for declarative bit-packed ID encoding with automatic stride calculation and namespace isolation via `PluginTag`. Manual bit-packing is no longer necessary.

### SendKeys Character Escaping \[Resolved\]

The SDK provides `kasane_plugin_sdk::keys::push_literal()` and `kasane_plugin_sdk::keys::command()` for key escaping. Plugins no longer need to implement their own escaping logic.

### Process Job ID Management \[Resolved\]

The SDK provides `kasane_plugin_sdk::job::JobTracker` for automatic generation tracking (discards stale results) and `kasane_plugin_sdk::process::ProcessHandle` with `.with_fallback()` for process pipeline helpers. Manual ID allocation is no longer necessary for common patterns.

Native plugins can use the higher-level `ProcessTaskSpec` / `ProcessTaskResult` model via `HandlerRegistry::on_process_task()`, which provides framework-managed job IDs, stdout buffering, and fallback chains. This model is not yet exposed to WASM plugins.

### Multi-Language Support \[Not Yet Implemented\]

The WIT interface is language-neutral by design — any language compiling to the WASM Component Model can produce Kasane plugins. However, all existing examples and the SDK are Rust-only. No other language has been validated, and no language-specific tooling or documentation exists.

## Rationale & Evolution

### Why These Constraints Exist

**Sandbox isolation** (By Design constraints): The WASM sandbox is a core safety guarantee. Plugins cannot crash or corrupt the editor. Direct `AppState` access is withheld to prevent coupling to internal data structures and to maintain the host's ability to evolve its state representation. Synchronous execution, no threading, and no network access follow from the WASI capability model and the security-first design principle.

**Phased implementation** (Not Yet Implemented constraints): The WASM plugin system was built bottom-up, starting with UI decoration (Phase W), then I/O (Phase P), then surfaces (Phase 5). Pane lifecycle and workspace management require foundational work in the native API before WASM exposure.

**API ergonomics** (Improvement constraints): The current API works but carries friction inherited from the Component Model boundary (serialized state, opaque handles, manual ID encoding). These are addressable through SDK improvements without WIT changes in most cases.

### Evolution Path

| Constraint | Resolution path | Roadmap reference |
|---|---|---|
| Pane lifecycle/rendering | Define parity model, then expose via WIT | — |
| Missing host state | Add getters as needed per use case | — |
| Theme tokens | `register-theme-tokens` in WIT | — resolved |
| Workspace commands | `workspace-command` + `on-workspace-changed` in WIT | — resolved |
| Dynamic surfaces | Extend WIT surface API with lifecycle commands | — |
| Fuel / timeout | Configure wasmtime fuel or epoch interruption | — |
| State hash | `state!` macro (generation counter pattern) | §3.4 — resolved |
| SendKeys escaping | `keys::push_literal()` / `keys::command()` | §3.4 — resolved |
| ID encoding | `interactive_id!` macro with namespace isolation | §3.4 — resolved |
| Job management | `JobTracker` + `ProcessHandle` in SDK | §3.4 — resolved |
| Multi-language | Validate and document non-Rust toolchains | §3.4 |

## Related Documents

- [plugin-development.md](./plugin-development.md) — development guide and quick start
- [plugin-api.md](./plugin-api.md) — API reference and extension point details
- [roadmap.md](./roadmap.md) — implementation phases and next deliverables
