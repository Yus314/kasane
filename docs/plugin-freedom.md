# Plugin Author Freedom: Comprehensive Analysis

This document systematically maps what plugin authors can and cannot do
in Kasane's plugin system. The goal is to distinguish **intentional design
constraints** from **gaps that may be relaxed in the future**.

For the API reference see [plugin-api.md](./plugin-api.md).
For the development guide see [plugin-development.md](./plugin-development.md).
For composition semantics see [semantics.md](./semantics.md).

---

## 1. Execution Model

### 1.1 Two Plugin Forms

| | Native Plugin (`Plugin` trait) | WASM Plugin (WIT interface) |
|---|---|---|
| **Runtime** | In-process, direct Rust types | wasmtime sandbox, WIT types |
| **State ownership** | Framework holds `Box<dyn PluginState>` | Guest-side thread-local (SDK macro managed) |
| **Function semantics** | Pure `(&self, &State) → (State, Effects)` | Equivalent via WIT boundary |
| **OS access** | Unrestricted (`std::fs`, `std::net`, FFI) | Gated by WASI capability model |
| **Distribution** | `.so` / compiled into binary | `.wasm` component (portable) |

**Core trade-off**: Native plugins have full Rust ecosystem access. WASM plugins
trade that for sandbox safety. WASM is the recommended distribution form; capability
parity is actively pursued.

### 1.2 Pure Function Constraint

All plugin methods are designed as **side-effect-free pure functions**:

- **View methods** (`contribute_to`, `transform`, `annotate_line_with_ctx`, etc.):
  Read-only — return values, cannot issue commands.
- **Lifecycle methods** (`on_state_changed_effects`, `on_io_event_effects`, etc.):
  Return `(new_state, Effects)` — new state plus effect requests.
- **Input handlers** (`handle_key`, `handle_mouse`):
  Return `Option<(new_state, Vec<Command>)>` — consume or pass through.

**Rationale**: Salsa memoization compatibility, deterministic rendering, testability.
Plugins cannot mutate `AppState` directly.

---

## 2. Read Side: AppView

`AppView` (`kasane-core/src/plugin/app_view.rs`) defines everything a plugin can read.

### 2.1 Accessible Information

| Tier | Information | Example methods |
|------|-------------|-----------------|
| 0 | Cursor position, buffer content, terminal size | `cursor_pos()`, `lines()`, `cols()`, `rows()` |
| 0 | Editor mode, selections, multi-cursor | `editor_mode()`, `selections()`, `secondary_cursors()` |
| 1 | Status bar | `status_line()`, `status_prompt()`, `status_style()` |
| 2 | Menu | `menu()`, `has_menu()` |
| 3 | Info popups | `infos()`, `has_info()` |
| 4 | UI options, plugin config | `ui_options()`, `plugin_config()` |
| 5 | Session | `session_descriptors()`, `active_session_key()` |
| 9 | Theme | `theme_face()`, `is_dark_background()` |
| Derived | Layout | `available_height()`, `visible_line_range()`, `is_prompt_mode()` |

### 2.2 Inaccessible Information

- **Other plugins' internal state** — Fully isolated. Plugin A cannot read Plugin B's state.
- **Rendering pipeline intermediates** — Element tree, layout results, CellGrid.
- **Registered plugin list** — Which plugins exist and their priorities are opaque.
- **Composed contribution set** — Other plugins' slot contributions and transform chain composition.
- **DisplayMap** — Display↔buffer line mapping is only partially accessible via `AnnotateContext`.

---

## 3. Write Side: Extension Points

### 3.1 Extension Point Catalog

| Extension Point | Composition Type | Commutative | What the plugin author controls |
|---|---|---|---|
| **Contribute** | `ContributionSet` (monoid) | Yes | Element + priority + size hint per slot |
| **Transform** | `TransformChain` (monoid) | No | Wrap, replace, or pass through elements/overlays |
| **Annotate** | Priority-based merge | Yes | Gutter elements + background layers + inline decoration + virtual text |
| **Overlay** | `OverlaySet` (monoid) | Yes | Floating panels with anchor, z-index, collision context |
| **DisplayDirective** | `DirectiveSet` (monoid) | Yes | Fold, hide, insert virtual text lines |
| **MenuTransform** | `MenuTransformChain` (monoid) | No | Menu item appearance modification |
| **CursorStyle** | `FirstWins<T>` | No | Cursor appearance override (first plugin wins) |
| **CellDecoration** | Vec concatenation | Yes | Per-cell face override with blend mode |
| **ScrollPolicy** | First-wins dispatch | No | Scroll behavior override |
| **PaintHook** | Vec concatenation | Yes | Direct CellGrid writes (**native only**) |

### 3.2 Extension Point Details

**Contribute**: 8 well-known slots (`BUFFER_LEFT`, `BUFFER_RIGHT`, `ABOVE_BUFFER`,
`BELOW_BUFFER`, `ABOVE_STATUS`, `STATUS_LEFT`, `STATUS_RIGHT`, `OVERLAY`) plus
custom slots via `SlotId::new("custom.name")`. Priority controls ordering within a
slot. `ContribSizeHint` enables layout negotiation (`Auto`, `Fixed(n)`, `Flex(weight)`).

**Transform**: Operates on `TransformTarget` (Buffer, StatusBar, Menu, Info, and
their refinements like `MenuPrompt`, `MenuInline`, `BufferLine(n)`). The subject is
`TransformSubject` — either an `Element` or an `Overlay` (element + anchor). Plugins
can use `map_element()`, `map_anchor()`, and `map_overlay()` to modify the subject.
`TransformScope` declares intent: `Wrapper`, `Replacement`, `Prepend`, `Append`,
`Attribute`, `Structural`, or `Identity`. `TransformDescriptor` enables debug-time
collision detection.

**Annotate**: Per buffer line:
- Left/right gutter elements (e.g., line numbers, git markers)
- Background layers (`Face` + `z_order` + `BlendMode`)
- Inline decoration (`InlineDecoration`)
- Virtual text items (`Vec<VirtualTextItem>` with per-item priority)

**Overlay**: `OverlayContribution` provides floating UI with `OverlayAnchor` positioning
and `z_index`. `OverlayContext` supplies collision avoidance hints: `screen_cols`,
`screen_rows`, `menu_rect`, `existing_overlays`, `focused_surface_id`.

**DisplayDirective**: Buffer-line-level display transformations:
- `Fold { range, summary }` — collapse lines into a summary
- `Hide { range }` — hide lines entirely
- `InsertAfter { after, content }` — insert virtual text line after a buffer line
- `InsertBefore { before, content }` — insert virtual text line before a buffer line

**PaintHook** (native only): Direct `CellGrid` writes in the final rendering stage.
WASM plugins are limited to the Element tree.

---

## 4. Effects and Side Effects

### 4.1 Command Enum

Plugins request side effects by returning `Vec<Command>` from lifecycle methods
and input handlers:

| Command | Description | Constraint |
|---------|-------------|------------|
| `SendToKakoune(request)` | Send JSON-RPC request to Kakoune | — |
| `RequestRedraw(flags)` | Trigger redraw with dirty flags | — |
| `ScheduleTimer { delay, target, payload }` | Set timer (received via `update_effects`) | — |
| `PluginMessage { target, payload }` | Send message to another plugin | Async, unidirectional, no delivery guarantee |
| `SetConfig { key, value }` | Publish config value (readable via `plugin_config()`) | — |
| `Paste` | Execute paste | — |
| `Quit` | Terminate application | — |
| `EditBuffer { edits }` | Structured buffer edits | Via Kakoune key simulation |
| `InjectInput(event)` | Inject synthetic input event | — |
| `SpawnProcess { .. }` | Spawn external process | `allows_process_spawn()` must be `true` |
| `WriteToProcess { job_id, data }` | Write to process stdin | Process must exist |
| `CloseProcessStdin { job_id }` | Close process stdin | Process must exist |
| `KillProcess { job_id }` | Terminate process | Process must exist |
| `ResizePty { job_id, rows, cols }` | Resize PTY | Process must exist |
| `SpawnPaneClient { pane_key, placement }` | Spawn pane client | `WORKSPACE` authority |
| `ClosePaneClient { pane_key }` | Close pane client | `WORKSPACE` authority |
| `Workspace(cmd)` | Workspace operations (focus, resize, split) | `WORKSPACE` authority |
| `RegisterSurface { surface, placement }` | Register a new Surface | `DYNAMIC_SURFACE` authority |
| `UnregisterSurface { surface_id }` | Remove a Surface | `DYNAMIC_SURFACE` authority |
| `RegisterThemeTokens(tokens)` | Register theme token defaults | — |
| `Session(cmd)` | Session management commands | — |

### 4.2 Lifecycle Phase Constraints

| Phase | Available Effects |
|-------|-------------------|
| **Bootstrap** (`on_init_effects`) | `BootstrapEffects` — `DirtyFlags` only. No commands. |
| **SessionReady** (`on_active_session_ready_effects`) | `SessionReadyEffects` — Limited command set (`SessionReadyCommand`). |
| **Runtime** (all other lifecycle methods) | `RuntimeEffects` — Full `Command` set. |
| **View phase** (contribute, transform, annotate, etc.) | **No effects** — Pure value return only. |

---

## 5. Inter-Plugin Cooperation

### 5.1 Available Mechanisms

1. **PluginMessage**: Async message passing via `Command::PluginMessage { target, payload }`.
   Receiver processes in `update_effects()` by downcasting `Box<dyn Any>`. **No type
   safety, no delivery guarantee, no RPC.**

2. **ConfigEntry**: Publish values via `Command::SetConfig { key, value }`. Other plugins
   read via `AppView::plugin_config()`. **Indirect, delayed (next frame).**

3. **Transform chain**: Indirectly modify other plugins' contributions through the
   Transform extension point. **The transformer does not know whose contribution it is
   transforming.**

### 5.2 Impossible Cooperation Patterns

- **Conditional contribution**: "If Plugin B contributed to slot X, adjust my contribution"
  — impossible. Each plugin generates contributions independently.
- **Relative positioning**: "Place my overlay next to Plugin C's overlay" — impossible.
  Overlays use absolute positioning or anchors.
- **Resource budget negotiation**: "Negotiate gutter width budget across plugins"
  — impossible. Framework resolves via layout.
- **Synchronous RPC**: Plugin A → B → A request/response — impossible. Messages are async.
- **Atomic transactions**: Coordinated state updates across multiple plugins — impossible.
  Each plugin's state transition is independent.

---

## 6. UI Ownership

### 6.1 Surface System

Plugins can own **independent UI regions** by implementing the `Surface` trait:

- `view()` — render an Element tree
- `handle_event()` — process input events
- `on_state_changed()` — react to app state changes
- `declared_slots()` — publish slots for other plugins to contribute to
- `initial_placement()` — declare workspace placement
- `size_hint()` — layout negotiation

Surfaces are first-class UI citizens with the same status as the built-in Kakoune buffer
surface. A plugin can own multiple surfaces, each with independent state, event handling,
and slot definitions.

### 6.2 Workspace Control

Plugins with `PluginAuthorities::WORKSPACE` can:
- Split panes, move focus, resize
- Spawn new pane clients
- Query workspace layout via `WorkspaceQuery`

---

## 7. Capability Gating (WASM)

### 7.1 WASI Capabilities

| Capability | Effect |
|------------|--------|
| `Filesystem` | Plugin data directory (read-write) + cwd (read-only) |
| `Environment` | Read environment variables |
| `MonotonicClock` | Always granted (WASI default); declared for auditability |
| `Process` | External process spawning (checked at command dispatch) |

### 7.2 Kasane Authorities

| Authority | Effect |
|-----------|--------|
| `DynamicSurface` | Runtime Surface registration/unregistration |
| `PtyProcess` | PTY-attached process spawning (interactive shells) |
| `WorkspaceManagement` | Workspace layout commands |

**Design principle**: Capabilities are declared by the plugin and resolved against
user configuration (`deny_capabilities`, `deny_authorities` in `config.toml`).
Least-privilege by default.

---

## 8. Composition Algebra

### 8.1 Monoid Foundation

The composition framework (`kasane-core/src/plugin/compose.rs`) formalizes plugin
output combination via the `Composable` trait:

```rust
trait Composable: Sized {
    fn empty() -> Self;                    // identity element
    fn compose(self, other: Self) -> Self; // binary operation
}
```

Each plugin's output is generated **context-free** and composed as a monoid
homomorphism `Free(A) → M`.

### 8.2 Inexpressible Patterns

The context-free nature of monoid composition makes these patterns algebraically
inexpressible:

1. **Context-dependent composition**: `plugin_i : (State, ∏_{j≠i} Contribution_j) → Contribution_i`
2. **Lattice-based negotiation**: `meet(demands) ≤ budget` resource constraints
3. **Fixed-point semantics**: Computing convergent solutions for mutually-referencing contributions

### 8.3 Theoretical Alternatives

- **Two-pass composition**: Propose phase + adjustment phase. Cost: 2× plugin calls per frame.
- **Lattice extension**: Define `meet`/`join` operations. Limited applicability.
- **Free monoid + deferred interpretation**: Injectable resolution strategies. Tension with Salsa memoization.

---

## 9. Known Gaps and Constraints

### Intentional Constraints (Design Decisions)

| Constraint | Rationale |
|------------|-----------|
| No direct `AppState` mutation | Pure function semantics, deterministic rendering |
| No commands during view phase | Eliminate rendering side effects |
| No access to other plugins' state | Plugin isolation, testability |
| No WASM network I/O | Sandbox security |
| No WASM PaintHook | CellGrid direct manipulation is incompatible with sandboxing |

### Gaps That May Be Relaxed

| Gap | Impact | Status |
|-----|--------|--------|
| No inter-plugin RPC | Complex plugin cooperation patterns (request/response, negotiation) are impossible. | Open — would require new protocol design. |
| No composition result visibility | Adaptive UI (adjusting contributions based on what other plugins contribute) is impossible. | Open — fundamental to the monoid model. |

---

## 10. Freedom Overview

```
┌─────────────────────────────────────────────────────────┐
│                    Fully Free                           │
│  · Arbitrary Element tree construction                  │
│  · Contribution to any slot (well-known or custom)      │
│  · Own Surfaces with independent state and event loop   │
│  · Theme token declaration with fallback                │
│  · External process spawning and I/O                    │
│  · Timer-based async processing                         │
│  · Buffer editing (via Kakoune key simulation)          │
│  · Overlay anchor modification via Transform            │
│  · Synthetic input injection                            │
├─────────────────────────────────────────────────────────┤
│               Conditionally Free                        │
│  · Workspace operations → WORKSPACE authority required  │
│  · Dynamic Surface registration → DYNAMIC_SURFACE       │
│  · PTY process → PTY_PROCESS authority required         │
│  · Filesystem → WASM: Filesystem capability required    │
│  · Transform chain ordering → controlled by priority    │
│  · Key consumption priority → registration order        │
├─────────────────────────────────────────────────────────┤
│                    Impossible                           │
│  · Direct AppState mutation                             │
│  · Read/write other plugins' internal state             │
│  · Side effects during view phase                       │
│  · Composition result visibility (context-dep. compose) │
│  · Synchronous inter-plugin RPC                         │
│  · WASM PaintHook / network I/O                         │
│  · Rendering pipeline intermediate access               │
└─────────────────────────────────────────────────────────┘
```

---

## Key Files

| File | Content |
|------|---------|
| `kasane-core/src/plugin/state.rs` | `Plugin` trait (recommended API) |
| `kasane-core/src/plugin/traits.rs` | `PluginBackend` trait (internal API) |
| `kasane-core/src/plugin/compose.rs` | Monoid composition framework |
| `kasane-core/src/plugin/app_view.rs` | `AppView` (read interface) |
| `kasane-core/src/plugin/bridge.rs` | `Plugin` → `PluginBackend` adapter |
| `kasane-core/src/plugin/registry.rs` | `PluginRuntime` (registration, dispatch) |
| `kasane-core/src/plugin/effects.rs` | Effect type definitions |
| `kasane-core/src/plugin/command.rs` | `Command` enum |
| `kasane-core/src/plugin/context.rs` | Extension point context types |
| `kasane-core/src/surface/traits.rs` | `Surface` trait |
| `kasane-core/src/display/mod.rs` | `DisplayMap`, `DisplayDirective` |
| `kasane-wasm/src/capability.rs` | WASI capability resolution |
| `kasane-wasm/src/authority.rs` | Kasane authority resolution |
| `kasane-wasm/wit/plugin.wit` | WIT interface definition |
| `kasane-plugin-sdk/src/lib.rs` | WASM plugin SDK |
