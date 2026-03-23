# WASM Plugin Constraints

This document catalogs the constraints of the WASM plugin system compared to native plugins. Each constraint is labeled:

- **\[By Design\]** — intentional, security or architectural boundary
- **\[Not Yet Implemented\]** — planned for future phases
- **\[Improvement\]** — current API works but has ergonomic friction

For a development guide, see [plugin-development.md](./plugin-development.md). For API reference, see [plugin-api.md](./plugin-api.md).

## What WASM Plugins Can Do

WASM plugins cover the primary UI extension surface:

| Capability | Description |
|---|---|
| Slot contributions | Inject elements at named slots (`BUFFER_LEFT`, `STATUS_RIGHT`, etc.) |
| Line annotations | Per-line gutter elements and background colors |
| Overlays | Floating elements with smart positioning and collision avoidance |
| Element transforms | Modify or replace existing UI elements (status bar, menu, info, buffer) |
| Menu item transforms | Per-item atom replacement in completion menus |
| Cursor style override | Change cursor shape (block, bar, underline, hidden) |
| Input handling | Observe and consume key/mouse events (first-wins dispatch) |
| Process spawning | Launch external processes, receive stdout/stderr/exit events |
| Timers | Schedule periodic or one-shot callbacks |
| Inter-plugin messaging | Send `Vec<u8>` payloads to other plugins by ID |
| Session management | Spawn, switch, close Kakoune sessions |
| Configuration | Read and write config values at runtime |
| Static surfaces | Declare plugin-owned surfaces with slots at init time |
| WASI capabilities | Request filesystem, environment, clock, process access |

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
| PaintHook (cell grid) | Full | None | No low-level rendering |
| Pane lifecycle | Full | None | No create/close/focus hooks |
| Pane rendering | Full | None | Cannot own custom panes |
| Pane commands | Full | None | No split/close/focus commands |
| Workspace commands | Full | None | No layout manipulation |
| Workspace notifications | Full | None | No `on_workspace_changed` |
| Theme token registration | Full | None | Cannot define custom faces |
| State access | Direct `&AppState` | ~40 getter functions | See [Host State Access](#host-state-access) |
| Cache invalidation | Automatic (`PartialEq`) | Manual `state_hash()` | See [Developer Experience](#developer-experience) |
| Fuel / timeout | N/A | None | No runaway protection |

## Extension Point Gaps

### PaintHook \[Not Yet Implemented\]

Native plugins can return `PaintHook` implementations that directly mutate the `CellGrid` after painting. This enables effects like custom cursor rendering, background gradients, or post-processing.

WASM plugins have no access to the cell grid. Roadmap §3.5 plans to redesign `PaintHook` into a high-level render hook that does not depend on direct `CellGrid` manipulation, enabling WASM parity.

> Workaround: Use a native `PluginBackend` for low-level rendering effects.

### Pane Lifecycle and Rendering \[Not Yet Implemented\]

Native plugins can receive `on_pane_created`, `on_pane_closed`, and `on_focus_changed` notifications, render custom pane content via `render_pane`, and handle pane-specific input via `handle_pane_key`. They can also declare `PanePermissions` (split, focus, spawn, float, cross-pane, tabs).

WASM plugins have no pane-related APIs. This depends on Phase 5 (Surface / Workspace) and roadmap §3.5 (Pane / Workspace parity model).

> Workaround: Use surfaces for dedicated panels. For IDE-like multi-pane layouts, use a native plugin.

### Workspace Commands and Notifications \[Not Yet Implemented\]

Native plugins can issue `PaneCommand` and `WorkspaceCommand` to manipulate layout, and receive `on_workspace_changed` notifications.

WASM has no equivalent commands or notifications. Depends on the parity model in roadmap §3.5.

### Dynamic Surfaces \[Not Yet Implemented\]

WASM surfaces are declared statically at init time via `surfaces()`. Native surfaces can be created, destroyed, and reparented at runtime. WASM surfaces cannot change their placement or slot declarations after initialization.

### Theme Token Registration \[Not Yet Implemented\]

Native plugins can issue `RegisterThemeTokens` commands to define custom face names. WASM has no equivalent command.

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
| `Workspace(WorkspaceCommand)` | Available | Not available |
| `RegisterThemeTokens(...)` | Available | Not available |

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

### Interactive Element ID Encoding \[Improvement\]

Interactive elements use a single `u32` identifier for hit-testing. Plugins encoding multiple pieces of information (element index, channel, direction) must manually pack and unpack this value:

```rust
fn encode_picker_id(color_idx: usize, channel: usize, is_down: bool) -> u32 {
    2000 + (color_idx * 6 + channel + if is_down { 3 } else { 0 }) as u32
}
```

There is no namespace isolation between plugins — ID collisions are the plugin author's responsibility.

> Workaround: Use a base offset (e.g., 2000) to avoid collisions with other plugins.

### SendKeys Character Escaping \[Resolved\]

The SDK provides `kasane_plugin_sdk::keys::push_literal()` and `kasane_plugin_sdk::keys::command()` for key escaping. Plugins no longer need to implement their own escaping logic.

### Process Job ID Management \[Improvement\]

Plugins spawning multiple processes must manually assign and track job IDs, handle stale results across process generations, and buffer output:

```rust
const JOB_FD: u64 = 1;
const JOB_FIND_FALLBACK: u64 = 2;
const JOB_FZF_BASE: u64 = 100;
```

A higher-level job abstraction in the SDK could simplify this pattern.

### Multi-Language Support \[Not Yet Implemented\]

The WIT interface is language-neutral by design — any language compiling to the WASM Component Model can produce Kasane plugins. However, all existing examples and the SDK are Rust-only. No other language has been validated, and no language-specific tooling or documentation exists.

## Rationale & Evolution

### Why These Constraints Exist

**Sandbox isolation** (By Design constraints): The WASM sandbox is a core safety guarantee. Plugins cannot crash or corrupt the editor. Direct `AppState` access is withheld to prevent coupling to internal data structures and to maintain the host's ability to evolve its state representation. Synchronous execution, no threading, and no network access follow from the WASI capability model and the security-first design principle.

**Phased implementation** (Not Yet Implemented constraints): The WASM plugin system was built bottom-up, starting with UI decoration (Phase W), then I/O (Phase P), then surfaces (Phase 5). Pane lifecycle, workspace management, and PaintHook parity require foundational work in the native API before WASM exposure. Roadmap §3.5 tracks the native escape hatch redesign that will enable WASM parity for these features.

**API ergonomics** (Improvement constraints): The current API works but carries friction inherited from the Component Model boundary (serialized state, opaque handles, manual ID encoding). These are addressable through SDK improvements without WIT changes in most cases.

### Evolution Path

| Constraint | Resolution path | Roadmap reference |
|---|---|---|
| PaintHook | Redesign into high-level render hook | §3.5 |
| Pane / Workspace | Define parity model, then expose via WIT | §3.5 |
| Missing host state | Add getters as needed per use case | — |
| Theme tokens | Add `RegisterThemeTokens` command to WIT | — |
| Dynamic surfaces | Extend WIT surface API with lifecycle commands | — |
| Fuel / timeout | Configure wasmtime fuel or epoch interruption | — |
| State hash | `state!` macro (generation counter pattern) | §3.4 — resolved |
| SendKeys escaping | `keys::push_literal()` / `keys::command()` | §3.4 — resolved |
| ID encoding | SDK helper or structured metadata on elements | §3.4 |
| Job management | SDK job abstraction | §3.4 |
| Multi-language | Validate and document non-Rust toolchains | §3.4 |

## Related Documents

- [plugin-development.md](./plugin-development.md) — development guide and quick start
- [plugin-api.md](./plugin-api.md) — API reference and extension point details
- [roadmap.md](./roadmap.md) — implementation phases and next deliverables
