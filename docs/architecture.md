# Architecture Design Document

This document describes Kasane's system boundaries, runtime composition, and separation of responsibilities.
For a detailed workspace tree, see [repo-layout.md](./repo-layout.md). For state and rendering semantics, see [semantics.md](./semantics.md).

## System Overview

```text
┌──────────────────────────────────────────────────────────┐
│                   Kasane (Frontend)                      │
│                                                          │
│  ┌────────────────────────────────────────────────────┐  │
│  │                 kasane-core                        │  │
│  │  JSON-RPC parser / State management / Layout      │  │
│  │  engine / Input mapping / Config / RenderBackend  │  │
│  │  trait                                            │  │
│  └──────────┬───────────────────────┬─────────────────┘  │
│             │                       │                    │
│  ┌──────────▼──────────┐ ┌─────────▼────────────────┐   │
│  │    kasane-tui        │ │     kasane-gui           │   │
│  │  (direct crossterm)  │ │ (winit + wgpu + glyphon) │   │
│  │  Cell grid mgmt      │ │ GPU text rendering       │   │
│  │  Diff-based drawing  │ │ Scene-based drawing      │   │
│  └──────────────────────┘ └──────────────────────────┘   │
│                                                          │
│  ┌────────────────────────────────────────────────────┐  │
│  │  Declarative UI + Display Policy + Plugin compose  │  │
│  │ Buffer / Status / Menu / Info / Overlay / Surface  │  │
│  │ Display Transformation / Display Unit / Interaction│  │
│  └────────────────────────────────────────────────────┘  │
│           ▲ Drawing               │ Key/mouse input      │
│           │ TUI: stdout           ▼ TUI: stdin           │
│           │ GUI: winit + GPU        GUI: winit           │
│  ┌────────────────────────────────────────────────────┐  │
│  │             Kakoune (Editor engine)               │  │
│  │             kak -ui json                          │  │
│  └────────────────────────────────────────────────────┘  │
└──────────────────────────────────────────────────────────┘
```

## Runtime Data Flow

```text
Kakoune message / frontend input
  -> protocol parse / input conversion
  -> state.apply() / update()
  -> DirtyFlags
  -> plugin notification
  -> view construction
  -> display policy application
  -> layout / display-unit resolution
  -> paint / scene build
  -> backend draw
```

For the semantic details of this flow, see [semantics.md](./semantics.md).

## Communication Protocol

- Protocol: JSON-RPC 2.0
- Kakoune -> Kasane: Drawing and state messages such as `draw`, `draw_status`, `menu_show`, `info_show`
- Kasane -> Kakoune: Input messages such as `keys`, `resize`, `mouse_press`
- Startup: Kakoune is launched as a child process via `kak -ui json`, with stdin/stdout connected

For protocol details, see [json-ui-protocol.md](./json-ui-protocol.md).

## Abstraction Boundaries

The core is responsible for "what to display, where, and under which display policy," while the backend is responsible for "how to draw it."

### Three-Layer Responsibility Model

| Layer | Definition | Decision Criteria |
|---|---|---|
| Upstream (Kakoune) | Protocol-level concerns | Does it require a protocol change? |
| Core (`kasane-core`) | Faithful rendering of the protocol + frontend-native capabilities | Does a single correct implementation exist? |
| Plugin | Features where policy may vary | Everything else |

For detailed decision criteria, see [layer-responsibilities.md](./layer-responsibilities.md).

### Declarative UI Layer Responsibilities

| Component | Owner | Description |
|---|---|---|
| `view` construction | `kasane-core` | Builds the `Element` tree from state and composes plugin contributions |
| Display policy application | `kasane-core` | Applies overlays, transformations, proxy display, and display unit generation as view policy |
| Layout calculation | `kasane-core` | Computes rectangular placement from `Element` |
| TUI paint | `kasane-core` | `Element + LayoutResult -> CellGrid` |
| GUI scene build | `kasane-core` | `Element + LayoutResult -> DrawCommand` |
| Plugin dispatch | `kasane-core` | Delivers state changes and input to plugin hooks |
| Hit test / interaction routing | `kasane-core` | Identifies interaction targets based on `InteractiveId` and the future display unit model |

### Backend Responsibilities

| Component | `kasane-core` | `kasane-tui` | `kasane-gui` |
|---|---|---|---|
| JSON-RPC parsing | Responsible | - | - |
| State management (TEA) | Responsible | - | - |
| `Element` construction | Responsible | - | - |
| Layout calculation | Responsible | - | - |
| Paint to `CellGrid` | Responsible | - | - |
| Terminal output | - | crossterm | - |
| GPU drawing | - | - | wgpu + glyphon |
| Key/mouse input capture | - | crossterm | winit |
| Clipboard | - | arboard | arboard |
| IME / D&D and other GUI-native capabilities | - | Not possible or terminal-dependent | winit-based |

## Rendering Paths

### TUI Path

```text
view_cached -> display_policy -> place -> paint -> CellGrid -> diff -> backend.draw
```

The TUI performs diff-based drawing on a cell grid, converting to escape sequences via crossterm.

### GUI Path

```text
view_sections_cached -> display_policy -> scene_paint_section -> SceneCache -> SceneRenderer
```

The GUI generates a scene description based on `DrawCommand` and draws directly to the GPU.

### Cache Layers

| Layer | Target | Role |
|---|---|---|
| `ViewCache` | `Element` tree | Per-section view reuse |
| `LayoutCache` | Layout results | Per-section redraw support |
| `SceneCache` | `DrawCommand` sequence | GUI scene reuse |
| `PaintPatch` | `CellGrid` partial updates | TUI fast path |

For the semantics and invalidation policy of each cache, see [semantics.md](./semantics.md).

## Display Policy Layer

The Display Policy layer determines what display structure to project onto before drawing the Observed State directly. This includes overlay composition, contributions to auxiliary regions, display transformations, proxy display, display unit grouping, and interaction policy.

The roles of this layer are as follows:

- Separate protocol truth from display policy
- Allow plugin-driven restructuring to participate in the core's composition rules
- Assemble hit test / focus / navigation targets before paint
- Serve as an intermediate layer to accommodate the future display transformation / display unit API

For semantic details, see the `Display Policy State` and `Display Transformation and Display Units` sections in [semantics.md](./semantics.md).

## Related Documents

- [repo-layout.md](./repo-layout.md): Detailed workspace and source tree
- [semantics.md](./semantics.md): State, rendering, invalidation, and equivalence
- [plugin-api.md](./plugin-api.md): API reference for plugin authors
- [plugin-development.md](./plugin-development.md): Quick start guide
