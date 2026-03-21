# Kasane Semantics

This document is the authoritative reference for Kasane's current semantics and correctness conditions.
What is defined here is "what Kasane means." Benchmark values, implementation progress, upstream issue tracking, and API signature listings are out of scope.

## 1. Document Responsibilities

### 1.1 What This Document Defines

- Kasane's system boundaries
- The meaning of state, update, rendering, and invalidation
- Plugin composition and Surface/Workspace semantics
- WASM plugin constraints and state access model
- Formal correctness theorems for optimization paths
- Currently known theoretical gaps

### 1.2 What This Document Does Not Define

- Benchmark values or performance measurement listings
- History of when features were implemented
- User-facing configuration details
- Complete plugin API reference
- Detailed design of future proposals

### 1.3 Related Documents

- [requirements.md](./requirements.md): Authoritative reference for requirements
- [index.md](./index.md): Documentation entry point and architecture overview
- [plugin-development.md](./plugin-development.md): Guide for plugin authors
- [performance.md](./performance.md): Performance principles, benchmarks, and optimization status
- [decisions.md](./decisions.md): History of design decisions
- [kakoune-protocol-constraints.md](./kakoune-protocol-constraints.md): Analysis of upstream protocol constraints

## 2. Fundamental Model

### 2.1 System Boundaries

Kasane is a JSON UI frontend for Kakoune. Kakoune sends drawing commands and UI state as JSON-RPC messages, and Kasane reflects them into `AppState`, rendering through a declarative UI and backend.

Kasane is not a general-purpose UI framework. It is designed to be tightly coupled to Kakoune's JSON UI protocol.

### 2.2 Division of Responsibilities Between Kakoune and Kasane

Kakoune manages the editing model, buffer contents, selections, triggering of menus and info popups, and the protocol truth.
Kasane manages how to display those through a declarative UI and backend implementation, how to perform plugin composition, and how to handle frontend-native capabilities not expressed by the protocol.

Kasane's core is responsible for "what to display and where," while the backend is responsible for "how to draw it."

### 2.3 Resolution Layer (HOW) and Responsibility Layer (WHERE)

Kasane classifies functionality along two axes.

- Resolution Layer (HOW)
  - Renderer
  - Configuration
  - Infrastructure
  - Protocol constraints
- Responsibility Layer (WHERE)
  - Upstream (Kakoune)
  - Core (`kasane-core`)
  - Plugins

The resolution layer represents "which mechanism resolves it," and the responsibility layer represents "which layer is responsible." The two are independent and must not be conflated.

### 2.4 Default Frontend Semantics and Extended Frontend Semantics

Kasane has a two-tier semantics.

- Default Frontend Semantics
  - The semantics when Kasane acts as an alternative frontend to `kak` for general users
  - Prioritizes conservatively displaying Kakoune's protocol truth and maintaining compatibility with existing configurations, plugins, and workflows
- Extended Frontend Semantics
  - The semantics when display structure, interaction policy, and surface composition are significantly reconfigured through plugins or explicit display policies
  - Builds additionally upon Default Frontend Semantics as its foundation

Kasane's primary purpose as a product lies in Default Frontend Semantics. Extended Frontend Semantics is a capability of Kasane and an important goal, but it is not a precondition for overriding the standard semantics for ordinary users.

## 3. State Semantics

### 3.1 Role of AppState

`AppState` is a single state space that holds facts observable from Kakoune, values derived from them, values estimated through heuristics, and frontend runtime state.

`AppState` does not treat "everything as the same kind of truth." Each field has a different epistemological strength.

### 3.2 Observed State

Observed State is information explicitly communicated by Kakoune's protocol. These are Kasane's first-class facts and must not be altered by Kasane-side policy.

Examples:

- Buffer lines received via `draw`
- `draw.cursor_pos`
- `menu_show` / `menu_hide`
- `info_show` / `info_hide`
- `draw_status` and `draw_status.cursor_pos`

### 3.3 Derived State

Derived State is information that can be deterministically recomputed from Observed State. Derived State may be held for caching or convenience purposes, but semantically it is uniquely determined from Observed State.

Examples:

- Layout results
- Contents of various caches
- Per-section rendering data

### 3.4 Heuristic State

Heuristic State is information estimated from patterns in display data that Kakoune does not explicitly provide. It exists for convenience, but its accuracy is not guaranteed by the upstream protocol.

Examples:

- Cursor count estimation via `FINAL_FG + REVERSE`
- Cursor style estimation from mode line strings
- Info identity estimation

Heuristic State does not carry the same strength of truth as Observed State. Fallback behavior and non-goals for heuristic failures must be explicitly stated.

### 3.5 Runtime State

Runtime State is state that exists only during frontend execution. It includes backend caches, animations, focus, plugin internal state, and so on.

Runtime State must not override Kakoune's truth, but it is held to inform rendering and input handling strategies.

### 3.6 Display Policy State

Display Policy State is the frontend-side policy that determines how Observed State is projected into display. It includes overlay visibility policies, display transformations, proxy displays, display unit grouping, and reconfiguration rules introduced by plugins.

Display Policy State is not Observed State itself. Kasane may use it to omit, proxy-display, supplement, or reconfigure Observed State, but must not treat the result as "facts stated by Kakoune."

In Default Frontend Semantics, Display Policy State is in principle Observed-preserving. That is, Kasane's standard behavior preserves the visible structure of protocol truth while improving placement, decoration, supplementary display, and overlay. Observed-eliding transformations and large-scale reconfiguration belong to Extended Frontend Semantics and are introduced through explicit policy or plugins.

### 3.7 Principles of State Updates

Input from external sources is in principle processed through the following flow.

1. Receive protocol or frontend input
2. Update `AppState`
3. Generate `DirtyFlags`
4. Notify plugins and the rendering pipeline

State is updated before rendering. Rendering is always a function of state, and rendering results must not generate state truth.

### 3.8 Treatment of Heuristics

Heuristics follow these principles.

- Do not override protocol facts
- Accept explicit degraded mode on failure
- Separate problems that should be resolved upstream as upstream dependencies
- Features derived from heuristics may weaken their exactness targets

In Default Frontend Semantics, heuristic failure should be treated as graceful degradation rather than UI collapse. Even when heuristics do not hold, Kasane prioritizes maintaining its meaning as a core frontend, with only extended features degrading.

## 4. Update Semantics

### 4.1 From External Input to State Update

Kasane's update system receives protocol input from Kakoune and key/mouse/focus inputs from the frontend, converting them into state updates and command sequences.

The basic flow is as follows.

1. Receive a message from Kakoune
2. Update `AppState` via `state.apply()` and compute dirty flags
3. If necessary, perform additional state transitions and `Command` generation via `update()`
4. Perform plugin notification and rendering based on dirty flags

### 4.2 Role of TEA Update

Kasane adopts TEA as its runtime model. `update()` aggregates inputs and centralizes state transitions and side-effect instructions.

The semantic benefits of TEA are as follows.

- Clear entry point for state transitions
- Makes it easier to keep `view` as a pure function of state
- Aligns well with Rust's ownership model
- Enables testable state transition units

### 4.3 Meaning of Command

`Command` is not a side effect itself but a description of a side-effect request. `Command` is not generated from view; it is generated from the update system or plugin hooks.

Commands fall into the following categories.

- Protocol commands: `SendToKakoune` (key forwarding, command execution), `EditBuffer` (structured buffer edits translated to key sequences)
- Frontend commands: `Paste`, `Quit`, `RequestRedraw(DirtyFlags)`, `InjectInput` (synthetic input re-dispatch with depth guard)
- Timer and scheduling: `ScheduleTimer`
- Plugin communication: `PluginMessage` (inter-plugin messaging)
- Configuration: `SetConfig`, `RegisterThemeTokens`
- Process management: `SpawnProcess`, `WriteToProcess`, `CloseProcessStdin`, `KillProcess`, `ResizePty`
- Surface management: `RegisterSurface`, `UnregisterSurface`, `RegisterSurfaceRequested`, `UnregisterSurfaceKey`
- Structural commands: `Session(SessionCommand)`, `Workspace(WorkspaceCommand)`, `Pane(PaneCommand)`

The runtime receives Commands and executes them as side effects. The important invariant is that Command generation is deterministic given the same state and input, even though Command execution may involve I/O.

### 4.4 Generation of DirtyFlags

`DirtyFlags` is a coarse-grained change set representing "which observable aspects have changed." `DirtyFlags` serves as input for cache invalidation and selective redraw, not as a complete proof of state differences.

The important point is that `DirtyFlags` represents "what kind of information has changed," not "the detailed content of the change."

### 4.5 Semantic Split of Buffer Flags

`DirtyFlags` splits buffer-related changes into two independent flags.

- `BUFFER_CONTENT`: Buffer lines, faces, and structural changes received via `draw`
- `BUFFER_CURSOR`: Cursor position, cursor mode, and secondary cursor coordinates

This split is a semantic design decision, not merely an optimization. It encodes the invariant that cursor movement alone does not change the meaning of the buffer body. Consequently, `BUILD_BASE_DEPS` includes `BUFFER_CONTENT` but excludes `BUFFER_CURSOR`, enabling cursor-only redraws without rebuilding the base section Element tree.

The composite flag `BUFFER` is defined as `BUFFER_CONTENT | BUFFER_CURSOR` for convenience.

### 4.6 Frame Structure and Phase Ordering

A frame is one iteration of the backend event loop. Each frame processes input and optionally renders. The phases execute in strict order:

1. **Event batch**: Drain channel up to 256 events or 16ms deadline, whichever comes first. Each event is processed sequentially via `update()`, accumulating `DirtyFlags` via bitwise OR.
2. **Plugin cache**: `prepare_plugin_cache(dirty)` — compare each plugin's generation counter against previous frame to set `any_plugin_state_changed`.
3. **Salsa sync**: `sync_inputs_from_state()` unconditionally projects all `AppState` fields to Salsa inputs (PartialEq early-cutoff). `sync_plugin_epoch()` increments epoch if any plugin changed. `sync_plugin_contributions()` and `sync_display_directives()` refresh Salsa-tracked extension point data.
4. **Render**: `render_pipeline_cached()` (Salsa demand-driven) → `draw_grid()` → `rebuild_hit_map()`.

If dirty flags are empty after the batch phase, phases 2–4 are skipped entirely.

```text
Invariant (Intra-Frame Plugin Isolation):
  During the render phase, plugin view methods (contribute_to, transform,
  annotate_line, contribute_overlay) operate on a frozen PluginView<'_>
  (immutable borrow). No plugin can observe state changes made by other
  plugins within the same render phase. Inter-plugin state effects
  propagate only via the next frame's event processing.
```

The plugin system enforces a two-phase lifecycle per frame:

- **Mutable phase**: event processing, state transitions (`&mut PluginRuntime`)
- **Immutable phase**: rendering, view queries (`PluginView<'_>`)

This boundary is enforced at compile time by Rust's borrow checker. The two phases never overlap within a frame.

## 5. Rendering Semantics

### 5.1 Exact Semantics

Under Exact Semantics, the rendering result for a given state `S` is defined by the complete rendering result produced by the reference path.

Conceptually, this can be expressed as follows.

```text
render_exact(S) = view(S) -> layout -> paint
```

Correctness here means that the observable rendering result is consistent with the meaning of `S`.

### 5.2 Policy Semantics

Kasane's actual fast paths do not always recompute `render_exact(S)` itself. They perform policy-relative incremental rendering based on `DirtyFlags`, various caches, and `stable()`.

Therefore, practical correctness is divided into two tiers.

- Exact Semantics: The meaning of complete re-rendering
- Policy Semantics: The meaning of incremental rendering under the current invalidation policy

Where `stable()` is involved, Policy Semantics is weaker than Exact Semantics. This is not a defect but an intentional specification.

However, in Default Frontend Semantics, policy-permitted staleness must remain within the range that does not break "the meaning existing users expect from a `kak` replacement." Staleness tolerance may exist for the freedom of plugin-defined extensions, but it must not take priority over the semantic consistency of the core frontend.

### 5.3 Separation of Responsibilities: view, layout, paint

- `view`: Constructs a declarative `Element` tree from state
- `layout`: Computes rectangular placement from `Element` and constraints
- `paint`: Converts `Element` and layout results into a representation for the drawing backend

In TUI, the output of `paint` is `CellGrid`; in GUI, it is a sequence of `DrawCommand`. Differences exist per backend, but both share the same UI semantics.

### 5.4 Common Semantics Between TUI and GUI

TUI and GUI differ in output representation.

- TUI: Diffs `CellGrid` and converts to terminal I/O
- GUI: Converts scene descriptions to GPU drawing

However, both are required to display the same UI structure and the same semantic content for the same state. The backend's freedom is limited to "how to draw it."

### 5.5 What Constitutes an Observable Result

Kasane's observational equivalence is defined not by the state of internal caches but by the finally observable rendering result.

Examples of observable targets:

- Displayed text
- Faces and styles
- Display positions
- Presence and placement of overlays/menus/info popups
- Cursor display

### 5.6 Rendering Faithfulness

Under Default Frontend Semantics, every element of Observed State must appear in the final rendering output unless explicitly elided by Display Policy State.

```text
Invariant (Rendering Faithfulness):
  For all observable elements e in Observed State S:
    e is visible in render(S)
    ∨ e is elided by an active Display Policy
```

This invariant does not apply to Extended Frontend Semantics, where Observed-eliding transformations are permitted.

### 5.7 Diff and Incremental Drawing

In TUI, the output of the rendering pipeline is not drawn in full each frame. Instead, `CellGrid` maintains both the current and previous frame states.

1. `paint` writes into the current grid
2. `diff()` computes the set of changed cells between current and previous grids
3. The backend emits terminal I/O only for changed cells
4. `swap()` promotes the current grid to become the previous grid for the next frame

Correctness of this cycle requires that `diff()` detects all cells that have changed and that `swap()` is called exactly once per frame.

## 6. Invalidation and Caching

### 6.1 Meaning of DirtyFlags

`DirtyFlags` is the input for state dependency tracking and cache invalidation. It does not represent the full diff of the entire state but rather an approximation of which observable aspects require recomputation.

### 6.2 Section-Level Invalidation

The current core view is primarily divided into `base`, `menu`, and `info` sections. Cache invalidation is performed at this section granularity.

This design means that a menu change does not always require rebuilding the buffer body.

### 6.3 ViewCache

> **Superseded.** `ViewCache` has been removed. Salsa incremental computation is now the sole caching layer for element tree construction (see ADR-020). This section is retained for historical reference.

`ViewCache` holds `Element` trees or their subtrees and skips reconstruction when the corresponding dirty flags are not set.

`ViewCache` performs policy-driven reuse based on `DirtyFlags` and component deps, not exact dependency analysis.

### 6.4 SceneCache

`SceneCache` holds `DrawCommand` sequences per section for the GUI backend. Like `ViewCache`, it has an invalidation mask, but it is used for GUI-specific fast paths.

### 6.5 PaintPatch

> **Superseded.** `PaintPatch` has been removed. Salsa incremental computation handles all memoization (see ADR-020). This section is retained for historical reference.

`PaintPatch` is a compiled fast path on the TUI side that performs direct cell updates for specific change patterns. It is an alternative to the full pipeline.

The correctness of a PaintPatch is defined by three conditions.

```text
For a PaintPatch P with guard predicate guard_P(dirty):

  (1) Guard soundness:
      P.can_apply(dirty) = true  →  guard_P(dirty) holds
      P.can_apply(dirty) = false →  P is not applied

  (2) Observational equivalence:
      P.apply(S, dirty, grid) ≡obs full_pipeline(S, dirty, grid)
      for all states S where guard_P(dirty) holds

  (3) Spatial isolation:
      For all cells c outside P's designated region:
        grid[c] after P.apply = grid[c] before P.apply
```

Current built-in patches and their guard predicates:

- `StatusBarPatch`: `dirty == STATUS` — repaints only the status row (~80 cells vs full grid)
- `MenuSelectionPatch`: `dirty == MENU_SELECTION` — swaps face on old/new selected menu items (~10 cells)
- `CursorPatch`: `dirty == ∅ ∧ cursor_moved` — swaps face at old/new cursor positions (2 cells)

All patches are disabled when `plugins_changed` is true, as plugin state changes may affect any region.

In debug builds, a correctness assertion (`patch_correctness_assertion`) verifies condition (2) by comparing patch output against the full pipeline output for every patched frame.

### 6.6 LayoutCache

> **Superseded.** `LayoutCache` has been removed. Salsa incremental computation handles layout memoization (see ADR-020). This section is retained for historical reference.

`LayoutCache` supports section-level redraws and patched paths through layout reuse. Which parts of the state the layout depends on is controlled by the invalidation policy.

### 6.7 Meaning of `stable()`

`stable()` is a policy declaration that "this component does not request reconstruction in response to specific state changes." It does not mean "this component never reads that state."

Therefore, a component with `stable(x)` may read `x`. In that case, the component may become stale relative to Exact Semantics, but this is permitted under Policy Semantics.

The distinction between `stable()` and `allow()` is as follows.

- `stable(field)`: The macro can detect the field access, but staleness is intentionally permitted. This is a **policy declaration**.
- `allow(field)`: The macro cannot detect the field access (e.g., access through helper functions, macros, or opaque references). This is a **verification escape hatch**.

Example: `build_info_section` is declared `stable(cursor_pos)`. It reads `cursor_pos` for overlap avoidance logic, but info popups are permitted to be stale relative to cursor movement. This avoids rebuilding info overlays on every cursor move.

### 6.8 Meaning of `allow()`

`allow()` is an explicit escape hatch for the static dependency analysis of `#[kasane::component]`. It is not a function that strengthens soundness; rather, it is a function for intentionally exempting dependencies that the verifier cannot handle.

When the AST analysis cannot detect a field access (e.g., field read inside a `format!()` macro, through a helper function, or via an opaque reference), `allow(field)` suppresses the diagnostic. Unlike `stable()`, `allow()` does not imply that staleness is intentionally permitted — it indicates that the dependency exists but cannot be statically verified.

### 6.9 Locations Where Exactness Is Intentionally Weakened

Current Kasane does not require step-by-step equivalence with complete re-rendering for all fast paths. Particularly where `stable()` is involved, warm/cold cache consistency becomes the primary correctness condition (see §11.2 for the formal statement).

This weakening is a design trade-off and is treated as a documented specification. Previously, PaintPatch outputs were verified against the full pipeline in debug builds (§6.5); this verification was removed along with PaintPatch itself.

### 6.10 ComponentCache

> **Superseded.** `ComponentCache<T>` has been removed along with `ViewCache`. Salsa provides equivalent memoization with automatic dependency tracking (see ADR-020). This section is retained for historical reference.

`ComponentCache<T>` is a generic memoization wrapper used by `ViewCache` and other caches. It stores a value of type `T` and invalidates it when the corresponding dirty flags are set.

`ComponentCache` provides `get_or_insert()` for lazy construction and `invalidate()` for manual clearing. The distinction between a warm cache (value present) and a cold cache (value absent) is observable via `cache_dirty_snapshot()`, which is used in tests to verify warm/cold equivalence (§11.2).

## 7. Dependency Tracking Semantics

### 7.1 Contract of `#[kasane::component(deps(...))]`

`#[kasane::component(deps(...))]` is a contract declaring which dirty flags a component depends on. Declared dependencies are interpreted as part of the conditions under which reconstruction should occur.

### 7.2 Guarantees of AST-Based Verification

The proc macro analyzes the AST and partially verifies consistency between declared deps and state field references. This verification enables compile-time detection of simple field access omissions.

### 7.3 Role of Hand-Written Dependency Information

In the current implementation, not all dependency information is generated from a single macro. Since hand-written dependency tables and section deps coexist, the dependency theory is not yet a single source of truth.

### 7.4 Limits of Soundness

The current dependency tracking is not fully sound.

Main reasons:

- Dependencies across helper functions may not be automatically detected
- Hand-written deps constants and macro analysis are dual-managed
- `allow()` is an explicit exemption
- WASM plugins declare dependencies via WIT functions (`contribute_deps()`, `transform_deps()`, `annotate_deps()`) with no compile-time verification (see §8.12)

Therefore, dependency tracking is effective as "strong discipline" but is not a "complete proof."

For native plugins, the `#[kasane::component(deps(...))]` macro provides partial compile-time verification. For WASM plugins, dependency declarations are entirely manual and unverified. Incorrect WASM dependency declarations may cause plugin output to become stale without detection.

## 8. Plugin Composition Semantics

### 8.1 Overview of Extension Points

Kasane's UI extensions are primarily composed of the following mechanisms.

- Contribution (`contribute_to`)
- Line Annotation (`annotate_line_with_ctx`)
- Overlay (`contribute_overlay_with_ctx`)
- Transform (`transform`)
- Menu Item Transform (`transform_menu_item`)
- PaintHook

These are not at the same level of abstraction; they differ in degrees of freedom and responsibilities.

These extension points are available to both native plugins (`Plugin` / `PluginBackend` traits) and WASM plugins (via WIT interface). The semantic contract is identical regardless of the plugin runtime; differences exist only in state access mechanisms and dependency declaration (see §8.11, §8.12).

### 8.2 Contribution

`contribute_to()` is the most constrained extension, contributing `Element`s to framework-defined extension points (`SlotId`). Contributions carry `priority` and `size_hint`, making it easiest to maintain structural consistency. It is preferred whenever possible.

### 8.3 Line Annotation

`annotate_line_with_ctx()` is a mechanism for extending the gutter and background of each buffer line. It does not modify the buffer content itself but provides per-line visual contributions (`LineAnnotation`). Contributions from multiple plugins are composed through `BackgroundLayer` and `z_order`.

### 8.4 Overlay

`contribute_overlay_with_ctx()` is a floating element overlaid separately from the normal layout flow. Overlays add display layers but do not modify the underlying protocol state. Display order is controlled via `z_index`.

### 8.5 Transform

`transform()` is a mechanism that receives an existing `Element` and returns a transformed version. It fulfills the roles of both the former Decorator (wrapping/decoration) and Replacement (substitution). The target is specified via `TransformTarget` and the application order via `transform_priority()`.

Element-level transforms are unified in the plugin composition pipeline as `apply_transform_chain`.

`transform_menu_item()` is a separate extension point that transforms individual menu items before rendering. It shares the concept of element transformation but operates on a different pipeline with its own trait method. It is not part of `apply_transform_chain`.

### 8.6 Composition Order and Priority

The rendering pipeline composes plugin outputs in three phases:

1. Build the seed default elements (framework-provided base UI)
2. Apply the transform chain in priority order
3. Compose contributions, annotations, and overlays

Each extension point has its own ordering rule. All multi-plugin results use stable, deterministic sorting. When priorities are equal, `PluginId` (lexicographic string comparison) breaks ties.

| Extension Point | Sort Key | Direction | Semantics |
|---|---|---|---|
| Contribution | `(priority, plugin_id)` | ASC | Lower priority → earlier in layout |
| Transform | `(priority, plugin_id)` | **DESC** (priority reversed) | Higher priority → applied first (innermost) |
| Annotation gutter | `(priority, plugin_id)` | ASC | Lower priority → leftmost |
| Annotation background | `(z_order, plugin_id)` | ASC, **last wins** | Highest z_order takes the line background |
| Overlay | `(z_index, plugin_id)` | ASC | Lower z_index → behind; higher → front |
| Display directive | `(priority, plugin_id)` | `resolve()` composition | Multi-plugin composable (P-031) |
| Menu item transform | registration order | sequential chain | Output of previous = input of next |
| Cursor style override | registration order | first non-None wins | Single winner |

> **Transform priority inversion**: Transform priority is intentionally inverted from contribution priority. High-priority transforms are applied first (closest to the seed element), so low-priority transforms control the final appearance. This matches the decorator pattern: the outermost decorator has the last word.

> **Effects merge**: When multiple plugins produce `RuntimeEffects` in the same notification cycle, effects are merged by OR-ing `DirtyFlags` and appending `commands` and `scroll_plans` in plugin registration order.

### 8.7 Input Dispatch and Key Consumption

Plugin input handling follows a defined dispatch order.

1. `observe_key()` is called on **all** plugins (observation only, no consumption)
2. `handle_key()` is called on plugins in registration order; the **first** plugin to return a non-None result consumes the key
3. If no plugin consumes the key, built-in handlers (PageUp, PageDown, etc.) are tried
4. If no built-in handler matches, the key is forwarded to Kakoune

This is a first-wins dispatch model. Plugin registration order determines priority for key consumption. `observe_key` is always exhaustive; `handle_key` is short-circuiting.

### 8.8 Inter-Plugin Messaging

Plugins may communicate via `PluginMessage`, which carries a target plugin ID and an opaque payload. The runtime delivers the message to the target plugin's `update()` (for `Plugin` trait) or a dedicated handler (for `PluginBackend` trait).

Message delivery returns `(DirtyFlags, Vec<Command>)`, allowing the receiving plugin to trigger state changes and side effects in response. Message delivery is synchronous within a single update cycle.

### 8.9 What Plugins May and May Not Change

Plugins may change display and interaction where policy can diverge.
Plugins may not change protocol truth, the core state machine, the semantics of the backend itself, or fabricate facts not provided by upstream.

Plugin-defined UI is not a precondition for core frontend semantics. Even in the absence of plugins, Kasane's standard frontend semantics must be self-contained. Display transformations introduced by plugins should in principle be additive, and must not capture core semantics by replacing the sole truth for standard users.

### 8.10 Plugin State Model (State-Externalized)

The `Plugin` trait externalizes plugin state ownership to the framework. The key semantic properties are:

- **State ownership**: The framework holds `Box<dyn PluginState>` for each Plugin. State transitions produce new values; the old state is replaced atomically.
- **Transition semantics**: All state-mutating operations return `(NewState, Vec<Command>)`. The framework detects changes via `PartialEq` on the concrete state type and increments a generation counter for `state_hash()`.
- **Invalidation**: `DirtyFlags::PLUGIN_STATE` (bit 7) signals plugin state changes to the view cache. `BUILD_BASE_DEPS` includes `PLUGIN_STATE` to trigger base section rebuilds when plugin state changes.
- **DynCompare**: `dyn PluginState` supports equality comparison via downcasting. Two states of different concrete types are always unequal.
- **Compatibility**: `PluginBridge` adapts `Plugin` to `PluginBackend`, preserving all existing cache invalidation behavior (L1 state_hash, L3 slot_deps).

#### Dual Change Detection

Plugin state changes are tracked by two independent tiers:

- **Tier 1 (coarse)**: `PluginBridge` compares current state against a previous snapshot via `PartialEq` after every mutable hook. If different, increments a monotonic generation counter (`state_hash()`). `PluginRuntime::prepare_plugin_cache()` reads generation counters to set `any_plugin_state_changed`.
- **Tier 2 (fine)**: `sync_plugin_epoch()` increments a Salsa input epoch when `any_plugin_state_changed` is true. Salsa tracked functions re-evaluate, but individual inputs use `PartialEq` early-cutoff — unchanged contributions produce cached outputs even when the epoch bumps.

Both tiers are necessary. The generation counter provides the coarse "did anything change?" gate. Salsa provides fine-grained memoization. Removing the generation counter would force Salsa to re-evaluate all plugin queries every frame. Removing Salsa would lose incremental computation.

> **Naming history**: This model was originally introduced as `PurePlugin` (ADR-021), with the mutable trait called `Plugin`. In ADR-022, the traits were renamed: `PurePlugin` became `Plugin` (primary API) and the old `Plugin` became `PluginBackend` (internal). The adapter was renamed from `PurePluginBridge` to `PluginBridge`, and the marker trait from `IsPurePlugin` to `IsBridgedPlugin`.

### 8.11 Plugin Model Positioning

Kasane provides two plugin trait models with different levels of abstraction.

- **`Plugin` trait** (recommended, primary API): State-externalized model. The framework owns plugin state; all methods are pure functions. Automatic cache invalidation via `PartialEq`. Suitable for most plugins.
- **`PluginBackend` trait** (internal, advanced): Mutable state model with `&mut self`. Full access to all extension points including `Surface`, `PaintHook`, and workspace observation. Intended for framework-internal use and advanced scenarios.

`PluginBridge` adapts `Plugin` to `PluginBackend`, enabling both models to coexist in `PluginRuntime`. The semantic guarantees (extension point contracts, composition ordering, input dispatch) are identical for both models.

WASM plugins implement the equivalent of `PluginBackend` via WIT interface, with the host providing the adaptation layer.

### 8.12 WASM Plugin Semantics

WASM plugins participate in the same composition model as native plugins but operate under additional constraints imposed by the Component Model boundary.

#### 8.12.1 Snapshot-Based State Access

WASM plugins do not access `AppState` directly. Before each WASM call, the host creates a snapshot of relevant state fields in `HostState`. The plugin reads this snapshot via host-imported functions.

```text
Invariant (WASM State Isolation):
  Within a single WASM call, the state snapshot is immutable.
  The plugin cannot observe state changes made by other plugins
  in the same frame.
```

State fields are organized into tiers (Tier 0–8), reflecting the evolutionary history of the WIT interface. All tiers are refreshed before each call.

#### 8.12.2 Element Arena Lifecycle

WASM plugins construct `Element` trees via `element_builder` host calls. Elements are stored in a per-call arena that is cleared at the start of each WASM invocation.

Element handles returned by builder calls are valid only within the current invocation. They must not be cached or reused across calls.

#### 8.12.3 Dependency Declaration

WASM plugins declare `DirtyFlags` dependencies via WIT functions: `contribute_deps()`, `transform_deps()`, `annotate_deps()`. Unlike native `#[kasane::component(deps(...))]`, these declarations have no compile-time verification.

If a WASM plugin does not implement a dependency function, the default is `DirtyFlags::ALL` (always recompute). This is conservative but may negate caching benefits.

#### 8.12.4 State Hash and Cache Invalidation

WASM plugins must implement `state_hash() → u64` to enable the host-side plugin slot cache. The host compares state hashes across frames to determine whether plugin contributions need recomputation.

Unlike the `Plugin` trait, where `PartialEq`-based change detection is automatic, WASM plugins bear full responsibility for state hash correctness. An incorrect state hash may cause stale contributions to persist.

#### 8.12.5 Capability Gating

Privileged operations (process spawning, filesystem access) require explicit capability grants declared via `requested_capabilities()`. The host constructs a per-plugin WASI context based on these grants. Native plugins default to full access; WASM plugins default to sandboxed.

## 9. Display Transformation and Display Units

### 9.1 Meaning of Display Transformation

Display Transformation is a policy that takes Observed State as material and constructs a different display structure. It can include omission, proxy display, supplementary display, and reconfiguration. Display Transformation is a view policy, not a falsification of protocol truth.

### 9.2 Observed-preserving and Observed-eliding

There are at least two types of Display Transformation.

- Observed-preserving transformation
  - Preserves the visible elements of Observed State while adding decoration, placement, overlay, and supplementary display
- Observed-eliding transformation
  - Omits some Observed State and reconfigures using proxy display or summaries

Kasane may permit the latter. However, elided facts are not lost; they are simply not directly displayed due to display policy.

In Default Frontend Semantics, Observed-eliding transformation is not the standard behavior. To maintain Kasane's substitutability where `kak = kasane`, strong omission, proxy display, and reconfiguration are positioned on the Extended Frontend Semantics side.

### 9.3 Boundaries of Display Transformation

Display Transformation may change display structure and interaction policy. What it may not change is falsifying Observed State content as "facts given by upstream."

For example, a fold summary may summarize multiple lines into one, but that summary must not be treated as the actual buffer lines sent by Kakoune.

**Multi-plugin composition (P-031):** Multiple plugins may contribute display directives simultaneously. The `resolve()` function composes them deterministically:

- **InsertAfter**: All kept; same-line ordering by `(priority, plugin_id)`.
- **Hide**: Set union of all ranges (idempotent).
- **Fold overlap**: Higher `(priority, plugin_id)` wins entirely; lower-priority overlapping fold dropped whole (protects summary integrity).
- **Fold-Hide partial overlap**: Fold removed (conservative — partial hide invalidates fold summary).
- **InsertAfter suppression**: Inserts targeting hidden or folded lines removed.

Plugins declare priority via `display_directive_priority()` (default 0). The resolved `Vec<DisplayDirective>` is passed to `DisplayMap::build()` unchanged.

### 9.4 Meaning of Display Unit

A Display Unit is an operable display unit within the reconfigured UI. A Display Unit is not merely a layout box; it collectively represents the display target, its relationship to the source, and the availability of interaction.

A Display Unit can carry the following information.

- geometry
- semantic role
- source mapping
- interaction policy
- navigation relationships with other Display Units

### 9.5 Meaning of Source Mapping

A Display Unit may have a mapping to corresponding buffer positions, buffer ranges, selections, derived objects, or plugin-defined objects.

This mapping is not necessarily one-to-one. A single Display Unit may represent multiple lines, and conversely, a single line may be split into multiple Display Units.

### 9.6 Restricted Interaction

If a Display Unit does not have a complete inverse mapping to its source, that unit may be treated as read-only or with restricted interaction.

The important point is to not leave "undefined operation results" implicit. Kasane should be able to explicitly represent units where interaction is impossible or restricted.

### 9.7 Responsibilities of Plugins and Display Transformation

Plugins can introduce Display Transformations and Display Units, but they bear the following responsibilities.

- Do not fabricate protocol truth
- Keep interaction policy within definable bounds
- Accept degraded mode when source mapping is weak

The core guarantees the following in return.

- Transformations are treated as view policy
- Display units can be represented as targets for hit testing, focus, and navigation
- Plugin-defined UI can participate in the same composition rules as standard UI
- Plugin-defined UI builds upon standard frontend semantics as its foundation, and must not break the meaning of the core frontend in its absence

## 10. Surface, Workspace, and Session

### 10.1 Meaning of Surface

A Surface is an abstraction that owns a rectangular region on screen and handles its own view construction, event processing, and state change notifications. The core's main screen elements are represented as Surfaces.

### 10.2 Meaning of SurfaceId

`SurfaceId` is a stable ID that identifies a surface. Buffer, status, menu, info, and plugin surfaces belong to different `SurfaceId` spaces.

### 10.3 Meaning of Workspace

A Workspace is a layout structure that manages surface placement, focus, splitting, and floating. A Workspace represents "which surface is where."

### 10.4 Relationship Between Surfaces and the Existing View Layer

Surface lifecycle has been integrated into the core view pipeline via `surface_view_sections_cached()`, which delegates base element composition to `SurfaceRegistry::compose_base_result()`. The legacy view layer remains as an explicit fallback (`legacy_surface_compose_result()`).

Therefore, Surface is partially integrated as a first-class abstraction. The rendering pipeline uses Surfaces when registered, falling back to the legacy direct-construction path otherwise. Full unification (where all core UI elements are Surfaces) is not yet complete.

### 10.5 Current Constraints

The current implementation has at least the following constraints.

- Invalidation is still centered on global `DirtyFlags`
- There are places where the `rect` received by a `Surface` and the final rendering are not fully consistent
- Overlay positioning and parts of the core view coexist with legacy paths

### 10.6 Relationship to Future Per-Surface Invalidation

`SurfaceId`-based invalidation is a promising future direction, but this document does not treat it as part of the current semantics. What is addressed here is solely the fact that the current system assumes global dirty.

### 10.7 Meaning of Session

A Session represents a single managed Kakoune process and its associated UI state. `SessionManager` assigns a stable `SessionId` to each session and tracks multiple sessions concurrently. At any given time, exactly one session is active and rendered; inactive sessions are held in the background with their Kakoune readers still alive.

A session is not a Surface, a Workspace layout, or a buffer. It is the runtime container that binds a Kakoune process, an `AppState` snapshot, and (in the future) a set of session-bound surfaces into a single switchable unit.

### 10.8 Session State Preservation

When the active session switches, `SessionStateStore` saves a full `AppState` clone of the outgoing session and restores the stored snapshot of the incoming session. This transition is atomic from the perspective of the rendering pipeline: the pipeline always sees a complete, consistent `AppState`.

Inactive sessions continue to receive Kakoune protocol messages. Their `AppState` snapshots are updated in the background, so when an inactive session is activated, it reflects the latest state from its Kakoune process rather than a stale snapshot from the moment of deactivation.

### 10.9 Session and Surface Binding (Current Constraints)

In the current implementation, the relationship between sessions and surfaces is not yet formalized. Surfaces are registered globally in `SurfaceRegistry` and are not scoped to a specific session. Automatic generation of session-bound surface groups (buffer, status, supplemental) and deterministic surface detachment on session switch are planned but not yet implemented.

Until session-bound surface generation is in place, multi-session operation relies solely on `AppState` snapshot swapping, and the surface composition does not change on session switch.

## 11. Equivalence and Proof Obligations

### 11.1 Trace-Equivalence

Kasane has multiple rendering optimization paths. These are required to be equivalent in observable results, even though their internal procedures differ.

```text
Theorem (Trace-Equivalence):
  For all valid states S, dirty flags D, and cache states C
  consistent with prior state history:

    render_pipeline(S, ALL, ∅)
      ≡obs render_pipeline_direct(S, D, C)
      ≡obs render_pipeline_sectioned(S, D, C)
      ≡obs render_pipeline_patched(S, D, C)

  where ≡obs denotes identity of the final CellGrid / DrawCommand
  sequence as observable output.
```

This is Kasane's central correctness theorem. All optimization paths — cached view reuse, section-level repaints, and compiled paint patches — must produce output identical to the reference full-pipeline path.

Trace-equivalence is verified empirically via property-based tests (`trace_equivalence.rs`) across randomly generated states and dirty flag combinations.

### 11.2 Warm/Cold Cache Equivalence

When `stable()` is involved, the trace-equivalence theorem as stated in §11.1 requires qualification. A component declared `stable(x)` may produce stale output relative to changes in `x`. Therefore, the practical correctness criterion is warm/cold cache equivalence under the same dirty flags.

```text
Theorem (Warm/Cold Equivalence):
  For all valid states S and dirty flags D:

    render(S, D, warm_cache) ≡obs render(S, D, cold_cache)

  where warm_cache has been populated by prior rendering,
  and cold_cache is freshly constructed.
```

Note what this does **not** require:

```text
  render(S, D, warm_cache) ≡obs render(S, ALL, ∅)

  — This may NOT hold when stable() permits staleness.
  — This is by specification, not a defect (§6.7, §6.9).
```

The distinction is critical: warm/cold equivalence for a given `D` is the soundness oracle; full-pipeline equivalence with `ALL` flags is the exactness oracle. Where `stable()` weakens exactness, warm/cold equivalence remains the binding correctness criterion.

### 11.3 PaintPatch Correctness

> **Superseded.** PaintPatch has been removed (see §6.5). This section is retained for historical reference.

Each PaintPatch has its own correctness obligation, derived from the general trace-equivalence theorem.

```text
Theorem (PaintPatch Correctness):
  For a PaintPatch P with guard predicate guard_P:

    guard_P(dirty) ∧ ¬plugins_changed
      → P.apply(S, dirty, grid) ≡obs full_pipeline(S, dirty, grid)

  Additionally (spatial isolation):
    ∀ cell c ∉ P.region:
      grid[c] is unchanged by P.apply
```

See §6.5 for the specific guard predicates of built-in patches.

### 11.4 What Tests Guarantee

What tests primarily guarantee are the following properties.

- Trace-equivalence across pipeline variants (property-based, §11.1)
- Warm/cold cache equivalence for each atomic dirty flag (§11.2)
- ~~PaintPatch guard soundness, observational equivalence, and spatial isolation (§11.3)~~ (removed with PaintPatch)
- Plugin cache invalidation consistency (L1 state hash)
- Preservation of semantics shared across backends

### 11.5 Contracts Expressible Only in Prose

The following contracts are difficult to fully express through tests alone.

- That weakening exactness via `stable()` is by specification (§6.7, §6.9)
- That heuristic state is not on par with protocol truth (§3.4)
- The boundaries that plugins may and may not cross (§8.9)
- That WASM state snapshot isolation holds across the Component Model boundary (§8.12)

As a non-goal of Kasane, requiring existing Kakoune users to participate in a Kasane-specific ecosystem within the standard frontend semantics is not included. Kasane has a plugin platform, but Default Frontend Semantics is not subordinate to it.

These are maintained through both prose and tests.

### 11.6 What Must Be Consistent Across Backends

TUI and GUI differ in output methods, but at least the following semantics must be consistent.

- What is displayed
- Where it is displayed
- Which state changes produce which view changes
- Which overlays/menus/info popups are visible

## 12. Known Gaps

### 12.1 Non-Strictness Due to `stable()`

`stable()` intentionally weakens strict equivalence with exact semantics. This is a specification at the policy level, but which locations permit staleness must be carefully managed. See §6.7 for the definition and §11.2 for the formal correctness criterion under staleness.

### 12.2 Limits of Dependency Tracking

AST-based verification and hand-written deps are useful but do not guarantee complete soundness. The dependency theory is not yet a single source of truth. See §7.4 for the detailed enumeration.

### 12.3 Mismatch Between Global DirtyFlags and Surface Theory

Surfaces have been introduced as localized rectangular abstractions, but invalidation still heavily depends on global dirty.

### 12.4 Mismatch Between Workspace Ratio and Actual Rendering

There is room for the split ratios computed on the Workspace side and the final flex allocation on the view composition side to not fully agree.

### 12.5 Gap in Plugin Overlay Invalidation

The GUI-side scene invalidation and plugin overlay dependencies are not fully integrated, leaving theoretical room for overlays to become stale.

### 12.6 Display Transformation and Core Invalidation

The `DisplayMap` is now integrated into the rendering pipeline. `display_directives_deps()` contributes to `EffectiveSectionDeps.base`, ensuring that the base view cache is invalidated when display directive dependencies change. The `DisplayMap` is rebuilt each frame via `collect_display_map()` and propagated through `Element::BufferRef`, `ViewSections`, and cursor/input functions.

Remaining gap: the display unit model (P-040..P-043) has not yet been introduced as a first-class invalidation unit. Per-display-unit dirty tracking and navigation are not implemented.

### 12.7 Incomplete Display-Oriented Navigation

Visual unit-based navigation is required as a future foundation, but the current implementation still centers on buffer-oriented navigation, and a complete unification theory with display units is unfinished.

### 12.8 Unverified WASM Dependency Declarations

WASM plugins declare `DirtyFlags` dependencies via WIT functions with no compile-time verification (§8.12.3). Incorrect declarations may cause plugin output to become stale without detection. The default fallback (`DirtyFlags::ALL`) is conservative but negates caching benefits.

### 12.9 WASM Snapshot Consistency Across Plugins

WASM plugins receive a frozen state snapshot before each call (§8.12.1). Multiple plugins' state changes within a single frame are not atomically visible to subsequent WASM calls; each call sees a fresh snapshot. This means WASM plugin ordering may affect observable output when plugins have state dependencies on each other.

### 12.10 Menu Item Transform Outside Unified Pipeline

`transform_menu_item()` operates separately from the Element-level `apply_transform_chain` (§8.5). The two transform mechanisms have independent priority orderings and are not subject to the same composition rules.

### 12.11 HitMap Frame Delay

Mouse routing uses the previous frame's HitMap. The HitMap is rebuilt after rendering (`rebuild_hit_map()`), so input events within a batch are routed using a potentially stale hit map. This introduces at most one frame of stale mouse routing (~16ms).

This is an accepted tradeoff documented in the frame loop code. It is recorded here because it represents a deviation from the "current frame reflects current state" ideal.

### Resolved Gaps

The following gaps have been resolved and are retained for historical reference.

- **Transform and Replacement unification**: At the Plugin trait level, `transform()` has absorbed both decorator and replacement, and is unified as `apply_transform_chain`. The old APIs (`decorate()`, `replace()`) have been removed from the Plugin trait.
- **Session invisibility to plugins**: Session observability infrastructure has been implemented: `AppState.session_descriptors` and `active_session_key` expose session state, `DirtyFlags::SESSION` notifies plugins of lifecycle changes, and `SessionCommand::Switch` allows plugins to request session activation. WASM plugins access these via WIT Tier 8 host-state functions and the `switch-session` command variant.
- **P-031 Single-plugin display directive exclusivity**: Display directives now support multi-plugin composition via `DirectiveSet` monoid and `resolve()`. Priority-based fold conflict resolution, hide union, and insert suppression enable combining code folding + virtual text from different plugins.

## 13. Non-Goals

### 13.1 Optimizations Not Covered in This Document

Individual micro-optimizations and benchmark tuning are not covered here. What is covered is only the semantics that such optimizations must preserve.

### 13.2 User-Facing Configuration Not Covered in This Document

Configuration methods for themes, layout, keybindings, etc. are not covered. Only which semantic boundary a given configuration belongs to is addressed.

### 13.3 Future Proposals Not Covered in This Document

Proposals for Phase 5 and beyond, or ideal designs after upstream changes, are explicitly distinguished from the current semantics.

## 14. Change Policy

### 14.1 When to Update This Document

This document is updated when any of the following change.

- Meaning of state classification
- DirtyFlags or invalidation policy
- Plugin composition order
- Surface/Workspace semantics
- Definition of observational equivalence

### 14.2 Relationship with ADRs

ADRs preserve the history of "why that decision was made." This document is the authoritative reference for "what is currently the specification." When the two conflict, this document takes priority as the current specification, and notes are added to the ADR as needed.

### 14.3 Synchronization with Test Updates

When semantics change, the following should also be updated in the same change whenever possible.

- Related prose
- Related tests
- Necessary invalidation / cache implementation

Changes that advance only semantics or only tests are avoided in principle.

## 15. Related Documents

- [index.md](./index.md) — Documentation entry point and architecture overview
- [plugin-api.md](./plugin-api.md) — Plugin API reference
- [requirements.md](./requirements.md) — Authoritative reference for requirements
- [decisions.md](./decisions.md) — History of design decisions
