# Kasane Plugin API Reference

This document is a reference for looking up the Kasane plugin API.
For a quickstart guide to writing a working plugin, see [plugin-development.md](./plugin-development.md). For composition ordering and correctness conditions, see [semantics.md](./semantics.md).

## 0. Scope of the Plugin API

The Kasane plugin API is primarily designed for **UI decoration, transformation, and extension**.

Plugins construct Element trees based on state received from Kakoune and provide supplementary visual information to users.
Side effects are issued indirectly via `Command`, and are limited to UI-side coordination such as sending keys to Kakoune, requesting redraws, inter-plugin messages, and timers.

The following operations are currently outside the scope of the plugin API.

| Out-of-scope operation | Reason |
|---|---|
| External process execution | No corresponding `Command` variant |
| File system access | WASM is prohibited by the sandbox. Native is technically possible but lacks an async infrastructure |
| Network communication | Same as above |
| Text input widgets | No input elements in `Element`. Text editing is delegated to Kakoune by design |

Native plugins run within the host process and can therefore technically use `std::process`, `std::fs`, etc. However, Plugin trait hook functions are called synchronously, so the plugin developer bears the design responsibility for avoiding main thread blocking.

Kasane's long-term strategy is to **make WASM the first-class distribution and execution path, with capabilities as close to native as possible**. Accordingly, native-only APIs are treated not as "permanent advantages" but as one of the following:

- A provisional escape hatch not yet stably exposed via WIT
- A host-integration API requiring redesign to achieve WASM parity
- An API intentionally kept native-only based on security boundary decisions

File system access is provided via WASI capability declarations (Phase P-1), and external process execution is provided via host-mediated `Command` + `IoEvent` (Phase P-2). See [ADR-019](./decisions.md#adr-019-plugin-io-infrastructure--hybrid-model) for design rationale.

## 1. Extension Points

### 1.1 Core Surfaces and Built-in Slots

The core UI is structured around surfaces. The extension points available to plugins are declared by each surface.

| SurfaceId | Surface | Description |
|---|---|---|
| `BUFFER` (0) | `KakouneBufferSurface` | Main buffer display |
| `STATUS` (1) | `StatusBarSurface` | Status bar |
| `MENU` (2) | `MenuSurface` | Menu |
| `INFO_BASE`+ (10+) | `InfoSurface` | Info popups |
| `PLUGIN_BASE`+ (100+) | Plugin-defined | Plugin-provided surfaces |

| SlotId | Position | Declaring Surface |
|---|---|---|
| `kasane.buffer.left` | Left of buffer | `KakouneBufferSurface` |
| `kasane.buffer.right` | Right of buffer | `KakouneBufferSurface` |
| `kasane.buffer.above` | Above buffer | `KakouneBufferSurface` |
| `kasane.buffer.below` | Below buffer | `KakouneBufferSurface` |
| `kasane.buffer.overlay` | Overlay on buffer | `KakouneBufferSurface` |
| `kasane.status.above` | Above status bar | `StatusBarSurface` |
| `kasane.status.left` | Left of status bar | `StatusBarSurface` |
| `kasane.status.right` | Right of status bar | `StatusBarSurface` |

### 1.2 Choosing a Mechanism

| Goal | Mechanism to use |
|---|---|
| Add UI at a predefined location | `contribute_to()` |
| Decorate individual buffer lines | `annotate_line_with_ctx()` |
| Display floating UI | `contribute_overlay_with_ctx()` |
| Modify or replace existing UI appearance | `transform()` |
| Transform individual menu items | `transform_menu_item()` |
| Draw directly without going through the Element tree | `PaintHook` |

As a principle, prefer the least flexible mechanism that suffices. Do not use `transform()` if `contribute_to()` can achieve the goal.

### 1.2.1 Display Transformations and Display Units

As described in `P-030..P-043` of [requirements.md](./requirements.md) and `Display Transformations and Display Units` in [semantics.md](./semantics.md), Kasane's long-term direction is to allow plugins to treat display transformations and display units as first-class concepts.

Dedicated APIs for this are not yet complete. Therefore, plugins should currently use the following combination of existing mechanisms to approximate the desired behavior:

- UI contribution: `contribute_to()`
- Modifying or replacing existing UI: `transform()`
- Local per-item transformation: `transform_menu_item()`
- Overlay display: `contribute_overlay_with_ctx()`
- Line / gutter contribution: `annotate_line_with_ctx()`
- Independent UI context: `Surface`

However, these are not fully synonymous with the future display transformation API. In particular, source mapping, display-oriented navigation, and restricted interaction policies have not yet been established as dedicated abstractions.

### 1.3 Composition Rules

The composition order for extensions is as follows:

1. Build the seed default elements
2. Apply the transform chain in priority order (processing decoration and replacement in a unified manner)
3. Compose contributions and overlays

For detailed semantics, see `Plugin Composition Semantics` in [semantics.md](./semantics.md).

### 1.4 Contribution (`contribute_to`)

`contribute_to()` is the most constrained extension, contributing `Element`s to framework-provided extension points (`SlotId`).

**Native:**

```rust
fn contribute_to(&self, region: &SlotId, state: &AppState, _ctx: &ContributeContext) -> Option<Contribution> {
    if region != &SlotId::BUFFER_LEFT { return None; }
    Some(Contribution {
        element: Element::text("★", Face::default()),
        priority: 0,
        size_hint: ContribSizeHint::Auto,
    })
}

fn contribute_deps(&self, _region: &SlotId) -> DirtyFlags {
    DirtyFlags::BUFFER_CONTENT
}
```

**WASM:**

```rust
fn contribute_to(region: SlotId, _ctx: ContributeContext) -> Option<Contribution> {
    kasane_plugin_sdk::route_slot_ids!(region, {
        BUFFER_LEFT => {
            Some(Contribution {
                element: element_builder::create_text("★", face),
                priority: 0,
                size_hint: ContribSizeHint::Auto,
            })
        },
    })
}

fn contribute_deps(region: SlotId) -> u16 {
    kasane_plugin_sdk::route_slot_id_deps!(region, {
        BUFFER_LEFT => dirty::BUFFER,
    })
}
```

`ContributeContext` provides layout-aware constraints. The main fields are `min_width` / `max_width` / `min_height` / `max_height`, where `None` represents unbounded. `Contribution` consists of `element`, `priority` (composition order), and `size_hint` (`Auto` / `Fixed(u16)` / `Flex(f32)`).

The legacy `u8` constants from `slot::BUFFER_LEFT` through `slot::OVERLAY` remain in the `kasane_plugin_sdk::slot` module, but the canonical API uses first-class `SlotId`. Custom slots can be specified in both Native and WASM via `SlotId::new("...")` / `SlotId::Named("...".into())`.

### 1.5 Line Annotation (`annotate_line_with_ctx`)

`annotate_line_with_ctx()` contributes gutter elements and backgrounds to individual buffer lines.

**Native:**

```rust
fn annotate_line_with_ctx(&self, line: usize, state: &AppState, _ctx: &AnnotateContext) -> Option<LineAnnotation> {
    if line == state.cursor_pos.line as usize {
        Some(LineAnnotation {
            left_gutter: None,
            right_gutter: None,
            background: Some(BackgroundLayer {
                face: Face { bg: Color::Rgb(RgbColor { r: 40, g: 40, b: 50 }), ..Face::default() },
                z_order: 0,
            }),
        })
    } else {
        None
    }
}

fn annotate_deps(&self) -> DirtyFlags {
    DirtyFlags::BUFFER_CONTENT
}
```

`LineAnnotation` consists of three elements: `left_gutter`, `right_gutter`, and `background`. `BackgroundLayer` has `face` and `z_order`; background contributions from multiple plugins are composited in `z_order` order. Gutter contributions are composited horizontally.

### 1.6 Overlay (`contribute_overlay_with_ctx`)

`contribute_overlay_with_ctx()` provides floating elements that are overlaid outside the normal layout flow.

**Native:**

```rust
fn contribute_overlay_with_ctx(&self, state: &AppState, _ctx: &OverlayContext) -> Option<OverlayContribution> {
    Some(OverlayContribution {
        element: Element::container(child, style),
        anchor: OverlayAnchor::AnchorPoint { coord, prefer_above: true, avoid: vec![] },
        z_index: 0,
    })
}
```

**WASM:**

```rust
fn contribute_overlay_v2(_ctx: OverlayContext) -> Option<OverlayContribution> {
    Some(OverlayContribution {
        element: element_builder::create_container_styled(child, ...),
        anchor: OverlayAnchor::Absolute(AbsoluteAnchor { x: 10, y: 5, w: 30, h: 10 }),
        z_index: 0,
    })
}
```

`OverlayContribution` consists of `element`, `anchor`, and `z_index`. There are two types of `OverlayAnchor`:

- `Absolute { x, y, w, h }`: Absolute position in screen coordinates
- `AnchorPoint { coord, prefer_above, avoid }`: Kakoune-compatible anchor-based positioning

### 1.7 Transform (`transform`)

`transform()` is a unified mechanism that receives an existing `Element`, transforms it, and returns the result. It serves as both decoration (formerly Decorator) and replacement (formerly Replacement).

**Native:**

```rust
fn transform(&self, target: &TransformTarget, element: Element, state: &AppState, _ctx: &TransformContext) -> Element {
    match target {
        TransformTarget::Buffer => Element::container(element, Style::from(Face::default())),
        _ => element,
    }
}

fn transform_priority(&self) -> i16 { 100 }

fn transform_deps(&self, _target: &TransformTarget) -> DirtyFlags {
    DirtyFlags::BUFFER_CONTENT
}
```

**WASM:**

```rust
fn transform_element(target: TransformTarget, element: ElementHandle, _ctx: TransformContext) -> ElementHandle {
    element_builder::create_container(element, Some(BorderLineStyle::Single), false, edges)
}

fn transform_priority() -> s16 { 100 }
```

`TransformTarget` includes `Buffer`, `StatusBar`, `Menu`, `Info`, and others.

Guidelines:

- Do not assume the internal structure of the received `Element`
- For lightweight decoration, prefer wrapping the `Element` as-is
- Full replacement is also performed via `transform()` (ignore the received element and return a new one)
- Use `transform_priority()` to control the application order

### 1.8 Menu Transform (`transform_menu_item`)

`transform_menu_item()` is a per-menu-item transformation corresponding to the `MENU_TRANSFORM` capability. Use it when you want to locally transform the label or style of individual items. If you need to replace the entire menu structure, use `transform()` with `TransformTarget::Menu`.

### 1.10 Future Display Transformation API

Display transformation is a future API for omitting, substituting, supplementing, and restructuring the Observed State. It is more powerful than simple decorators or replacements, and presupposes the semantic distinction between source truth and display policy.

Current direction:

- Transformations do not falsify protocol truth
- Transformations are treated as display policy
- Transformation results will connect to the future display unit model
- When the inverse mapping to source is weak, read-only or restricted interaction is permitted

This API is incomplete, and the current approach is to proceed with incremental validation using `Decorator`, `Replacement`, `Overlay`, and `Surface`.

## 2. Element API

### 2.1 Element variants

| Type | Purpose | WASM builder | Native |
|---|---|---|---|
| `Text` | Text + style | `create_text(content, face)` | `Element::text(s, face)` |
| `StyledLine` | Atom sequence | `create_styled_line(atoms)` | `Element::styled_line(line)` |
| `Flex` (Column) | Vertical layout | `create_column(children)` / `create_column_flex(entries, gap)` | `Element::column(children)` |
| `Flex` (Row) | Horizontal layout | `create_row(children)` / `create_row_flex(entries, gap)` | `Element::row(children)` |
| `Grid` | 2D table | `create_grid(cols, children, col_gap, row_gap)` | `Element::grid(columns, children)` |
| `Container` | border/shadow/padding | `create_container(...)` / `create_container_styled(...)` | `Element::container(child, style)` |
| `Stack` | Z-axis stacking | `create_stack(base, overlays)` | `Element::stack(base, overlays)` |
| `Scrollable` | Scrollable region | `create_scrollable(child, offset, vertical)` | `Element::Scrollable { ... }` |
| `Interactive` | Mouse hit test | `create_interactive(child, id)` | `Element::Interactive { child, id }` |
| `Empty` | Empty element | `create_empty()` | `Element::Empty` |
| `BufferRef` | Buffer line reference | Host-internal only | `Element::buffer_ref(range)` |

### 2.2 WASM element-builder API

All functions are imported from the `element_builder` module. The returned `ElementHandle` is valid only within the current plugin invocation scope.

```rust
use kasane::plugin::element_builder;

let text = element_builder::create_text("hello", face);
let col = element_builder::create_column(&[text]);
let container = element_builder::create_container(
    col,
    Some(BorderLineStyle::Single),
    false,
    Edges { top: 0, right: 1, bottom: 0, left: 1 },
);
```

For proportional distribution, use `create_column_flex` / `create_row_flex` with `FlexEntry { child, flex }`.

### 2.3 Native element construction

```rust
use kasane_core::plugin_prelude::*;

let text = Element::text("hello", Face::default());
let col = Element::column(vec![
    FlexChild::fixed(text),
    FlexChild::flexible(Element::Empty, 1.0),
]);
```

`FlexChild::fixed(element)` is fixed, and `FlexChild::flexible(element, factor)` is proportionally distributed.

## 3. State Access and Events

### 3.1 AppState overview

Native plugins can directly reference `&AppState`.

| Field | Type | Description |
|---|---|---|
| `lines` | `Vec<Line>` | Buffer lines |
| `cursor_pos` | `Coord` | Cursor position |
| `status_line` | `Line` | Status bar |
| `menu` | `Option<MenuState>` | Menu state |
| `infos` | `Vec<InfoState>` | Info popups |
| `cols`, `rows` | `u16` | Terminal size |
| `focused` | `bool` | Focus state |

Dirty flags primarily notify the following observable aspects:

| Flag | Description |
|---|---|
| `BUFFER` | Buffer lines and cursor |
| `STATUS` | Status bar |
| `MENU_STRUCTURE` | Menu structure |
| `MENU_SELECTION` | Menu selection |
| `INFO` | Info popups |
| `OPTIONS` | UI options |

For semantic classification, see [semantics.md](./semantics.md).

### 3.2 WASM host-state API

`kasane::plugin::host_state` provides a tiered read API.

**Basic state (Tier 0):**

| Function | Return type |
|---|---|
| `get_cursor_line()` | `s32` |
| `get_cursor_col()` | `s32` |
| `get_line_count()` | `u32` |
| `get_cols()` | `u16` |
| `get_rows()` | `u16` |
| `is_focused()` | `bool` |

**Buffer lines (Tier 0.5):**

| Function | Return type |
|---|---|
| `get_line_text(line)` | `Option<String>` |
| `is_line_dirty(line)` | `bool` |

**Status bar (Tier 1):**

| Function | Return type |
|---|---|
| `get_status_prompt()` | `Vec<Atom>` |
| `get_status_content()` | `Vec<Atom>` |
| `get_status_line()` | `Vec<Atom>` |
| `get_status_mode_line()` | `Vec<Atom>` |
| `get_status_default_face()` | `Face` |

**Menu/Info state (Tier 2):**

| Function | Return type |
|---|---|
| `has_menu()` | `bool` |
| `get_menu_item_count()` | `u32` |
| `get_menu_item(index)` | `Option<Vec<Atom>>` |
| `get_menu_selected()` | `s32` |
| `has_info()` | `bool` |
| `get_info_count()` | `u32` |

**General state (Tier 3):**

| Function | Return type |
|---|---|
| `get_ui_option(key)` | `Option<String>` |
| `get_cursor_mode()` | `u8` |
| `get_widget_columns()` | `u16` |
| `get_default_face()` | `Face` |
| `get_padding_face()` | `Face` |

**Multi-cursor (Tier 4):**

| Function | Return type |
|---|---|
| `get_cursor_count()` | `u32` |
| `get_secondary_cursor_count()` | `u32` |
| `get_secondary_cursor(index)` | `Option<Coord>` |

**Configuration (Tier 5):**

| Function | Return type |
|---|---|
| `get_config_string(key)` | `Option<String>` |

**Info details (Tier 6):**

| Function | Return type |
|---|---|
| `get_info_title(index)` | `Option<Vec<Atom>>` |
| `get_info_content(index)` | `Option<Vec<Vec<Atom>>>` |
| `get_info_style(index)` | `Option<String>` |
| `get_info_anchor(index)` | `Option<Coord>` |

**Menu details (Tier 7):**

| Function | Return type |
|---|---|
| `get_menu_anchor()` | `Option<Coord>` |
| `get_menu_style()` | `Option<String>` |
| `get_menu_face()` | `Option<Face>` |
| `get_menu_selected_face()` | `Option<Face>` |

### 3.3 Lifecycle hooks

| Hook | Timing | Purpose |
|---|---|---|
| `on_init` | Immediately after `PluginRegistry` registration | Initialization, theme token registration |
| `on_shutdown` | At application exit | Cleanup |
| `on_state_changed(dirty)` | After `AppState` update | Synchronize plugin internal state |

### 3.4 Input handling

The processing order for key input is as follows:

1. Notify all plugins via `observe_key()`
2. Call `handle_key()` in order
3. The first plugin to return `Some(commands)` wins
4. If all return `None`, proceed to built-in key bindings
5. If still unhandled, forward to Kakoune

Mouse input is passed to `handle_mouse(event, id, state)` after `observe_mouse()`, followed by `InteractiveId` hit testing.

### 3.4.1 Display Units and Interaction Policy

In the future, restructured UI introduced by plugins is expected to have hit test, focus, navigation, and source mapping on a per-display-unit basis.

This model is not yet exposed as a dedicated API, and plugins should use existing APIs under the following constraints:

- `InteractiveId` is a hit test target identifier and does not yet represent the full semantics of a display unit
- `handle_mouse()` may need to interpret source mapping on its own
- UI without a complete inverse mapping to source should be designed as read-only or with limited operations
- Plugins must not fabricate facts that Kakoune has not provided as the result of interactions

### 3.5 Commands

Hook functions issue side-effect requests by returning `Vec<Command>`.

| Command | Description |
|---|---|
| `SendToKakoune(req)` | Send a request to Kakoune |
| `Paste` | Paste from clipboard |
| `Quit` | Quit the application |
| `RequestRedraw(flags)` | Request a redraw |
| `ScheduleTimer { delay, target, payload }` | Send a message to target after a delay |
| `PluginMessage { target, payload }` | Send a message to another plugin |
| `SetConfig { key, value }` | Change a runtime configuration |
| `SpawnProcess { job_id, program, args, stdin_mode }` | Spawn an external process (Phase P-2) |
| `Session(SessionCommand)` | Create or close a Kakoune session managed by the host runtime |
| `WriteToProcess { job_id, data }` | Write to the stdin of a spawned process |
| `CloseProcessStdin { job_id }` | Close a process's stdin (EOF) |
| `KillProcess { job_id }` | Force-kill a process |
| `Pane(PaneCommand)` | Pane operations |
| `Workspace(WorkspaceCommand)` | Workspace operations |
| `RegisterThemeTokens(tokens)` | Register custom theme tokens |

`SessionCommand` has the following variants:

- `Spawn { key, session, args, activate }`: Open a new managed session. `key` is a stable key within the host, `session` is the session name corresponding to `kak -c <name>`, and `activate = true` immediately switches to that session as the active session.
- `Close { key }`: Close the session with the specified key. `key = None` closes the current active session. If the last session is closed, the host runtime terminates. If the active session is closed and other sessions remain, the host runtime promotes the next session in creation order to active.

The V1 session runtime can hold multiple sessions, but only one active session is rendered at a time. The Kakoune reader for inactive sessions remains alive, and its events continue to be reflected in the off-screen session snapshot. When activated, that snapshot is restored, but automatic generation of session-bound surfaces and multi-session dedicated UI are not yet implemented.

In WASM, these are represented as `command` variants. `Pane`, `Workspace`, and `RegisterThemeTokens` are currently not supported in WASM. Process execution commands (`SpawnProcess`, etc.) and session management commands (`spawn-session`, `close-session`) have been introduced on the WIT side.

WASM plugins are sandboxed by default. The host constructs WASM instances without granting capabilities via `WasiCtxBuilder`, so access to host resources such as file system and network is unavailable. The host functions available to WASM plugins are limited to the two WIT interfaces: `host-state` (state reading) and `element-builder` (element construction). Per Phase P ([ADR-019](./decisions.md#adr-019-plugin-io-infrastructure--hybrid-model)), `preopened_dir` / `env` are unlocked based on capability declarations (P-1), and process execution is provided via host mediation (`Command::SpawnProcess` + `IoEvent`) (P-2). Process execution requires declaring `Capability::Process`, which can be denied via `deny_capabilities` in `config.toml`.

## 4. Capabilities and Caching

### 4.1 PluginCapabilities

`PluginCapabilities` is a bitflag declaring the features a plugin implements, used to skip unnecessary method calls.

| Flag | Description |
|---|---|
| `CONTRIBUTOR` | `contribute_to()` |
| `TRANSFORMER` | `transform()` |
| `ANNOTATOR` | `annotate_line_with_ctx()` |
| `OVERLAY` | `contribute_overlay_with_ctx()` |
| `MENU_TRANSFORM` | `transform_menu_item()` |
| `CURSOR_STYLE` | `cursor_style_override()` |
| `INPUT_HANDLER` | `handle_key()` / `handle_mouse()` |
| `PANE_LIFECYCLE` | Pane lifecycle hooks |
| `PANE_RENDERER` | `render_pane()` |
| `SURFACE_PROVIDER` | `surfaces()` |
| `WORKSPACE_OBSERVER` | `on_workspace_changed()` |
| `PAINT_HOOK` | `paint_hooks()` |
| `IO_HANDLER` | `on_io_event()` |

The default for native plugins is `all()`, and the WASM adapter is configured from WIT call results.

`PANE_LIFECYCLE`, `PANE_RENDERER`, `WORKSPACE_OBSERVER`, and `PAINT_HOOK` are currently native-only, but `SURFACE_PROVIDER` has also been introduced on the WIT side as hosted surface descriptors / `render-surface`. It is not assumed that the same trait signatures will be directly mapped to WIT.

### 4.2 State hash and dependency tracking

Plugin contribution results are primarily cached through the following mechanisms:

- `state_hash()`: Hash of the plugin's internal state
- `contribute_deps(region)`: `DirtyFlags` that the specified region depends on
- `transform_deps(target)`: `DirtyFlags` that the transform depends on
- `annotate_deps()`: `DirtyFlags` that line annotation depends on

```rust
// WASM
fn state_hash() -> u64 {
    MY_STATE.get() as u64
}

fn contribute_deps(region: SlotId) -> u16 {
    kasane_plugin_sdk::route_slot_id_deps!(region, {
        BUFFER_LEFT => dirty::BUFFER,
        STATUS_RIGHT => dirty::STATUS,
    })
}
```

Native plugins implement `state_hash()` and dependency tracking methods directly.

### 4.3 PaintHook

`PaintHook` is a native-only hook that directly manipulates the `CellGrid` after paint, bypassing the `Element` tree. This is a **provisional escape hatch** and not intended as a long-term public API. It should be treated with the assumption that it will be redesigned into a higher-level render hook accessible from WASM, rather than direct `CellGrid` manipulation.

```rust
fn paint_hooks(&self) -> Vec<Box<dyn PaintHook>> {
    vec![Box::new(MyHighlightHook)]
}

impl PaintHook for MyHighlightHook {
    fn id(&self) -> &str { "myplugin.highlight" }
    fn deps(&self) -> DirtyFlags { DirtyFlags::BUFFER }
    fn surface_filter(&self) -> Option<SurfaceId> { Some(SurfaceId::BUFFER) }
    fn apply(&self, grid: &mut CellGrid, region: &Rect, state: &AppState) {
        // mutate grid directly
    }
}
```

## 5. Styling

### 5.1 StyleToken

`StyleToken` is a semantic style token that maps to a `Face` from the theme configuration.

| Token name | Purpose |
|---|---|
| `buffer.text` | Buffer text |
| `buffer.padding` | Buffer padding |
| `status.line` | Status bar |
| `status.mode` | Mode display |
| `menu.item.normal` | Normal menu item |
| `menu.item.selected` | Selected menu item |
| `menu.scrollbar` / `menu.scrollbar.thumb` | Scrollbar |
| `info.text` / `info.border` | Info popup |
| `border` / `shadow` | Border / shadow |

Custom tokens can be created and registered by plugins.

```rust
StyleToken::new("myplugin.highlight")

fn on_init(&mut self, _state: &AppState) -> Vec<Command> {
    vec![Command::RegisterThemeTokens(vec![
        ("myplugin.highlight".into(), Face {
            fg: Color::Named(NamedColor::Yellow),
            ..Face::default()
        }),
    ])]
}
```

### 5.2 config.toml integration

```toml
[theme]
"menu.selected" = { fg = "black", bg = "blue" }
"myplugin.highlight" = { fg = "yellow" }
```

## 6. Advanced API

### 6.1 Surface provider

Plugins with the `SURFACE_PROVIDER` capability can provide their own surfaces. In Native, they return `Box<dyn Surface>`, while in WASM, they map to a hosted surface model returning static `surface-descriptor` groups, `render-surface(surface-key, ctx)`, `handle-surface-event(surface-key, event, ctx)`, and `handle-surface-state-changed(surface-key, dirty-flags)`.

```rust
impl Plugin for MyPlugin {
    fn capabilities(&self) -> PluginCapabilities {
        PluginCapabilities::SURFACE_PROVIDER
    }

    fn surfaces(&mut self) -> Vec<Box<dyn Surface>> {
        vec![Box::new(MySidebar::new())]
    }
}
```

| Method | Description |
|---|---|
| `id() -> SurfaceId` | Unique ID |
| `surface_key() -> CompactString` | Stable semantic key |
| `size_hint() -> SizeHint` | Preferred size |
| `initial_placement() -> Option<SurfacePlacementRequest>` | Static initial placement |
| `view(ctx: &ViewContext) -> Element` | Build `Element` tree |
| `handle_event(event, ctx) -> Vec<Command>` | Event handling |
| `on_state_changed(state, dirty) -> Vec<Command>` | Shared state change notification |
| `state_hash() -> u64` | Hash for view cache |
| `declared_slots() -> &[SlotDeclaration]` | Extension point declarations |

`ViewContext` provides `state`, `rect`, `focused`, `registry`, and `surface_id`. Collection/registration of plugin-owned surfaces and `initial_placement()` are evaluated during bootstrap preflight, and `workspace_request()` is used only as a legacy fallback during the transition period. The descriptor's `initial_placement()` reflects `SplitFocused` / `SplitFrom` / `Tab` / `TabIn` / `Dock` / `Float` directly from the surface path into the workspace. `Dock` uses `SizeHint`'s preferred/min size to determine the ratio when the root rect is known, and falls back to a default ratio otherwise. Commands returned by `handle_event()` / `handle-surface-event(...)` / `handle-surface-state-changed(...)` are executed in the context of the surface owner plugin, so capability-gated deferred commands such as `SpawnProcess` are evaluated under the owner plugin's permissions. `on_state_changed(...)` is called at least on shared state updates originating from the Kakoune protocol, allowing the surface owner to return additional commands based on dirty flags.

### 6.2 Workspace commands

`WorkspaceCommand` manipulates surface placement and layout.

| WorkspaceCommand | Description |
|---|---|
| `AddSurface { surface_id, placement }` | Add a surface |
| `RemoveSurface(id)` | Remove a surface |
| `Focus(id)` | Move focus |
| `FocusDirection(dir)` | Directional focus |
| `Resize { delta }` | Adjust split ratio. Split divider drag also internally falls through to this command |
| `Swap(id1, id2)` | Swap surfaces |
| `Float { surface_id, rect }` | Make a surface floating |
| `Unfloat(id)` | Return to tiled mode. If split metadata from the previous float remains, it is preferentially used for restoration |

| Placement | Description |
|---|---|
| `SplitFocused { direction, ratio }` | Split the focused surface |
| `SplitFrom { target, direction, ratio }` | Split from a specific surface |
| `Tab` / `TabIn { target }` | Add a tab |
| `Dock(position)` | Dock to Left/Right/Bottom/Panel |
| `Float { rect }` | Add as floating |

### 6.3 Custom slots

Surfaces can define custom slots that other plugins can contribute to by returning `declared_slots()`.

```rust
impl Surface for MySurface {
    fn declared_slots(&self) -> &[SlotDeclaration] {
        &[
            SlotDeclaration::new("myplugin.sidebar.top", SlotKind::AboveBand),
            SlotDeclaration::new("myplugin.sidebar.bottom", SlotKind::BelowBand),
        ]
    }
}
```

`SlotDeclaration.kind` is advisory metadata; the actual placement is determined by `Element::SlotPlaceholder`. Other plugins use `contribute_to(&SlotId::new("myplugin.sidebar.top"), state, ctx)`. In WASM, the same slot name is specified via `SlotId::Named("myplugin.sidebar.top".into())`.

### 6.4 Plugin messages and timers

`Command::PluginMessage { target, payload }` enables inter-plugin message passing.

- Native: Downcast in `update(msg: Box<dyn Any>, state)`
- WASM: Receive byte array in `update(payload: Vec<u8>)`

`Command::ScheduleTimer { delay, target, payload }` performs delayed message sending.

### 6.5 Pane lifecycle

Plugins with the `PANE_LIFECYCLE` capability can observe pane creation, deletion, and focus changes.

| Hook | Description |
|---|---|
| `on_pane_created(pane_id, state)` | Pane creation notification |
| `on_pane_closed(pane_id)` | Pane deletion notification |
| `on_focus_changed(from, to, state)` | Focus change notification |

With the `PANE_RENDERER` capability, `render_pane(pane_id, cols, rows)` can render plugin-owned panes.

## 7. Related Documents

- [plugin-development.md](./plugin-development.md) — Quickstart guide
- [semantics.md](./semantics.md) — Composition ordering and semantics
- [architecture.md](./architecture.md) — Positioning of surfaces and backends
- [index.md](./index.md) — Entry point for all docs
