# Glossary

This document is a reference list of terminology used in Kasane.
For authoritative definitions of semantics and responsibilities, see [semantics.md](./semantics.md) and [layer-responsibilities.md](./layer-responsibilities.md).

## Protocol & Rendering

| Term | Description |
|------|-------------|
| JSON UI | Kakoune's JSON-RPC 2.0 based external UI protocol |
| Face | Text decoration information (foreground color, background color, underline color, attributes) |
| Atom | A pair of Face and string. The smallest unit of rendering |
| Line | An array of Atoms. Corresponds to one display line |
| Coord | A pair of line number and column number. Represents a position on the screen |
| Anchor | The reference coordinate for floating window positioning. Derived from the Kakoune protocol. Serves as the basis for OverlayAnchor in the Element tree |
| Inline style | Floating display that follows an anchor position within the buffer |
| Prompt style | Fixed display in the status bar area |
| Gutter | The area on the left edge of the editor for line numbers and icons. Extensible via Slot::BufferLeft |
| Double buffering | A technique of rendering to an off-screen buffer before transferring all at once. Prevents flickering |
| CellGrid | A two-dimensional array of cells. Implements differential rendering via double buffering |

## Declarative UI

| Term | Description |
|------|-------------|
| Element | The smallest unit of declarative UI description. An enum with variants: Text, StyledLine, Flex, Grid, Stack, Scrollable, Container, Interactive, Empty, BufferRef. Building blocks of the tree returned by view() |
| Element tree | A nested structure of Elements. The return value of view(&State). Used by the framework for layout calculation and CellGrid rendering |
| view() | A pure function that takes State and returns an Element tree. The core of TEA |
| paint() | The process that takes an Element tree and layout results to render onto a CellGrid |
| Overlay | A child element positioned on top of other elements within a Stack container in the Element tree. Used for menus, info popups, etc. |
| OverlayAnchor | Position specification for Overlays. Absolute (absolute coordinates), Relative (relative position), AnchorPoint (Kakoune-compatible anchor-based positioning) |
| InteractiveId | An identifier attached to an Element for mouse hit-testing. Matched against layout results to determine click targets |
| Owned Element | A memory model where Element has no lifetime parameter and owns all its data (ADR-009-3). Minimizes cognitive load for plugin authors. Clone cost is mitigated by the BufferRef pattern |
| BufferRef | A performance optimization pattern. Instead of cloning buffer lines, renders directly from State during paint |

## TEA (The Elm Architecture)

| Term | Description |
|------|-------------|
| TEA | The Elm Architecture. A unidirectional data flow: State -> view() -> Element, Event -> Msg -> update() -> State |
| State | The entire application state. Holds CoreState (from Kakoune) + plugin state |
| Msg | A message that triggers state changes. Includes Kakoune messages, input events, plugin messages, etc. |
| update() | A function that takes State and Msg, updates State, and returns a Command. Side effects are made explicit as Commands |
| Command | A description of side effects returned by update(). SendToKakoune, Paste, Quit, RequestRedraw, ScheduleTimer, PluginMessage, SetConfig |
| DirtyFlags | Bit flags (u16) indicating which parts of AppState have changed. 8 types: BUFFER_CONTENT, STATUS, MENU_STRUCTURE, MENU_SELECTION, INFO, OPTIONS, BUFFER_CURSOR, PLUGIN_STATE. Used for invalidation decisions in on_state_changed() and PluginSlotCache |
| CoreState | State derived from the Kakoune protocol (buffer lines, cursor, menus, status, etc.). Read-only from plugins |

## Plugin System

| Term | Description |
|------|-------------|
| Plugin | The primary user-facing plugin trait (state-externalized). Framework owns state; all methods are pure functions `(&self, &State) → (State, effects)`. Recommended for new plugins. Formerly called `PurePlugin` (renamed in ADR-022) |
| PluginBackend | The internal framework plugin trait (mutable `&mut self`). Full access to all extension points including Surface, PaintHook, pane lifecycle. Formerly called `Plugin` (renamed in ADR-022) |
| PluginId | A unique identifier for a plugin |
| PluginRegistry | Manages all registered plugins, performing Slot collection, Decorator application, and Replacement resolution |
| Slot | An extension point defined by the framework. Plugins insert Elements into Slots to extend the UI |
| Decorator | An extension pattern that receives and wraps an existing Element. Used for adding line numbers, changing borders, etc. |
| Replacement | An extension pattern that completely replaces an existing component. Used for fzf-style menu replacement, etc. |
| DecorateTarget | The target of Decorator application (Buffer, StatusBar, Menu, Info, BufferLine) |
| ReplaceTarget | The target of Replacement application (MenuPrompt, MenuInline, InfoPrompt, StatusBar, etc.) |
| proc macro | Procedural macros such as `#[kasane::plugin]` and `#[kasane::component]`. Automate boilerplate generation and compile-time validation |
| LineDecoration | Decoration provided by a plugin for each buffer line. Composed of 3 optional elements: left_gutter (left gutter Element), right_gutter (right gutter Element), and background (line background Face) |
| contribute_overlay | A method on the Plugin/PluginBackend traits. An extension point where a plugin provides a single Overlay (a floating Element with position specification). Independent of Slot::Overlay |
| contribute_line | A method on the Plugin/PluginBackend traits. Returns LineDecoration for a specified line. Used for implementing gutter icons and line backgrounds |
| on_state_changed | A lifecycle method on the Plugin/PluginBackend traits. Called with DirtyFlags when AppState is updated. Used for synchronizing plugin internal state |
| observe_key / observe_mouse | Input observation methods on the PluginBackend trait. Notified to all plugins but cannot consume events. Used for tracking internal state |
| state_hash | A method on the PluginBackend trait. Returns a u64 hash of internal state. Used for differential evaluation in the L1 cache layer of PluginSlotCache. `Plugin` (state-externalized) does not require manual `state_hash()` — the framework tracks changes via `PartialEq` |
| slot_deps | A method on the PluginBackend trait. Returns the DirtyFlags that a contribute() for a given Slot depends on. Used in the L3 cache layer of PluginSlotCache |
| PluginSlotCache | An in-memory cache in PluginRegistry. Caches contribute() results across two tiers, L1 (state_hash) and L3 (slot_deps), to avoid unnecessary recalculation |
| transform_menu_item | A method on the Plugin trait. Pre-rendering transformation of menu items (Atom arrays). Used for adding icons, etc. |
| cursor_line | An example WASM plugin. Highlights the cursor line background. A practical example of annotate_line_with_ctx(). Source: `examples/wasm/cursor-line/` |
| color_preview | An example WASM plugin. Detects color codes (#RRGGBB, #RGB, rgb:RRGGBB) in the buffer and provides gutter swatches and an interactive color picker. A practical example of annotate_line_with_ctx() + contribute_overlay_with_ctx() + handle_mouse(). Source: `examples/wasm/color-preview/` |

## Layer Responsibilities

| Term | Description |
|------|-------------|
| Three-layer responsibility model | A model that classifies feature responsibilities across three layers: upstream (Kakoune) / core (kasane-core) / plugin. A decision flowchart determines which layer a feature belongs to. See [layer-responsibilities.md](./layer-responsibilities.md) for details |
| Example WASM plugin | Example plugins embedded in the binary via `include_bytes!` (cursor_line, color_preview, sel_badge, fuzzy_finder). Can be overridden by FS-discovered plugins. Source: `examples/wasm/` |
| API proof | Verifying unproven Plugin trait extension points with real plugins. `examples/` serves as reference implementations |
| Frontend-native | Capabilities specific to the OS or window system (focus detection, D&D, clipboard, etc.). A category of features belonging to the core layer |

## Layout

| Term | Description |
|------|-------------|
| Flex | A simplified flexbox layout model. Positions child elements using Direction (Row/Column) + flex-grow + min/max |
| Constraints | Constraints during layout calculation. Min/max width and height |
| measure() | The first phase of layout calculation (bottom-up). Each element reports its size within constraints |
| place() | The second phase of layout calculation (top-down). The parent determines the concrete position of children |
| LayoutResult | The result of layout calculation. The on-screen rectangle (Rect) for each element |

## Surface & Workspace

| Term | Description |
|------|-------------|
| Surface | A rendering unit that owns a screen region. A trait with methods such as `id()`, `size_hint()`, `view()`, `handle_event()`. The foundation for a design where core UI components and plugins own screen regions equally |
| SurfaceId | A unique identifier for a Surface (u32). Constant definitions: BUFFER=0, STATUS=1, MENU=2, INFO_BASE=10, PLUGIN_BASE=100 |
| SurfaceRegistry | Manages Surface instances and the Workspace layout tree. Builds a unified Element tree from all Surfaces via `compose_view()` / `compose_full_view()` |
| ViewContext / EventContext | Context passed to a Surface (AppState, Rect, focus state, PluginRegistry) |
| WorkspaceNode | A node in the Workspace layout tree. 4 types: Leaf / Split / Tabs / Float |
| Workspace | Root node management, focus tracking (history stack), `compute_rects()` / `surface_at()` |
| WorkspaceCommand | Workspace operation commands: AddSurface / RemoveSurface / Focus / FocusDirection / Resize / Swap / Float / Unfloat |
| Placement | Placement specification for a new Surface: SplitFocused / SplitFrom / Tab / TabIn / Dock / Float |
| SlotId | Open slot system. Replaces the legacy `Slot` enum (deprecated). Custom slots can be defined with `SlotId::new("myplugin.sidebar")` |
| PaintHook | A trait for directly modifying the CellGrid after paint. Controls targets via DirtyFlags-based + Surface filter |
| PluginCapabilities | Bitflags (14 types) indicating which extension points a plugin participates in. Used as an optimization to skip WASM boundary calls for non-participating plugins. Shared by both `Plugin` and `PluginBackend` |
| PluginState | Marker trait for externalized plugin state. Blanket-implemented for `T: Clone + PartialEq + Debug + Send + 'static`. Supports trait-object cloning (via `dyn-clone`) and dynamic equality comparison |
| PluginBridge | Adapter that wraps a `Plugin` into the `PluginBackend` trait interface. Holds framework-owned state and uses a generation counter for `state_hash()`. Formerly called `PurePluginBridge` (renamed in ADR-022) |
| IsBridgedPlugin | Marker trait for runtime detection of `Plugin`-backed `dyn PluginBackend` objects. Provides access to the externalized state. Formerly called `IsPurePlugin` (renamed in ADR-022) |
| State Externalization | Design pattern where plugin state is owned by the framework rather than the plugin. Used by the `Plugin` trait (primary API). Enables structural equality comparison, automatic cache invalidation, and future Salsa memoization |
| DirtyFlags::PLUGIN_STATE | Dirty flag (bit 7) indicating that plugin internal state has changed. Used for `Plugin` (state-externalized) state change signaling |

## Rendering Optimization

| Term | Description |
|------|-------------|
| ViewCache | Section-level cache for the Element tree (base, menu, info). Invalidated based on DirtyFlags |
| ComponentCache\<T\> | A generic memoization wrapper. Caches values via `get_or_insert()` / `invalidate()` |
| SceneCache | Section-level cache at the DrawCommand level (for GPU). Same invalidation rules as ViewCache |
| LayoutCache | Caches base_layout, status_row, root_area. The foundation for section-level redraws |
| PaintPatch | A trait for modifying CellGrid with minimal cell updates. 3 built-in types: StatusBarPatch (~80 cells), MenuSelectionPatch (~10 cells), CursorPatch (2 cells) |

## Related Documents

- [semantics.md](./semantics.md) — Semantics in which terms are used
- [plugin-api.md](./plugin-api.md) — API terminology in the plugin context
- [architecture.md](./architecture.md) — Positioning within the system architecture
- [layer-responsibilities.md](./layer-responsibilities.md) — Terminology for responsibility boundaries
