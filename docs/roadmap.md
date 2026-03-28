# Implementation Roadmap

This document is a tracker that follows the **currently open implementation workstreams** of Kasane.
It records only "what is currently incomplete and what ships next," not detailed design rationale or current semantics.

## 1. Scope of This Document

This document is limited to the following three concerns.

- Currently open / active workstreams
- Next deliverable
- Delegation targets for backlog / upstream dependencies

The following are NOT the responsibility of this document.

- Explanation of current semantics
- Detailed specification of the shared Plugin API
- Lengthy design explanations of native escape hatches
- Detailed history of completed phases

For detailed design rationale, see [decisions.md](./decisions.md); for current semantics, see [semantics.md](./semantics.md);
for the current specification from a plugin's perspective, see
[plugin-api.md](./plugin-api.md); for performance numbers and implementation status, see
[performance.md](./performance.md).

## 2. Current Priorities

### 2.1 Now

| Workstream | Next deliverable | Completion criteria |
|---|---|---|
| Multi-pane UI polish | ✓ Complete | Pane layout persistence landed |
| Display transformation / display unit model | ✓ Complete | Display unit abstraction, visual navigation, and plugin-defined navigation policy are in place |

### 2.2 Next

| Workstream | Next deliverable |
|---|---|
| WASM runtime operations | Plugin manifest, plugin settings API, precompiled component cache (in order) |
| Native escape hatch redesign | Higher-level `PaintHook` redesign |
| Core event / degraded behavior | Minimal queuing for D-001, introduction of P-023 `DropEvent` |

### 2.3 Backlog

| Workstream | Notes |
|---|---|
| External plugin candidates | indent guides, clickable links, built-in splits, floating panels, code folding, display-line navigation, URL detection, region-specific text policy, etc. |
| Session-affine plugin surfaces | Plugin API for declaring session affinity on `surfaces()` return values. No consumer exists yet; deferred until a plugin requires it |

## 3. Open Workstreams

### 3.1 Multi-pane UI polish

Foundation delivered in Phase 5b/5c:

- `PaneMap` per-session surface binding with `ClientBufferSurface`
- `Workspace` split tree with flexbox-based pane layout
- Per-pane rendering via `PaneStates` + `BufferRefState`
- Overlay offset for multi-pane (menu/info positioned in pane-local → screen coordinates)
- Focused pane command routing (`focused_writer!` macro) across all dispatch sites and scroll runtime
- `<C-w>v/s/w/W/h/j/k/l/q` key bindings (split, focus, close)
- Pane border / separator glyphs — box-drawing characters (`│` / `─`) with `SPLIT_DIVIDER` / `SPLIT_DIVIDER_FOCUSED` theme tokens; edge-adjacency detection for focused pane highlight
- Mouse divider drag-to-resize (`handle_workspace_divider_mouse()`, `WorkspaceCommand::Resize`)
- Pane resize key bindings — `<C-w>+/-` via pane-manager plugin (`WorkspaceCommand::Resize`)
- Pane resize direction key bindings — `<C-w>>/<` for width-axis resize via `WorkspaceCommand::ResizeDirection` (direction-filtered tree-walk; skips cross-axis splits)
- TUI and GUI backend support

- Per-pane status bar — each pane displays its own mode, file name, and status via singleton N-render of `StatusBarSurface` (reuses `resolve_surface_tree()` with `PaneContext`; Kakoune clients resized to `rect.h - 1`; prompt cursor positioned relative to focused pane rect)
- Pane layout persistence — workspace tree (splits, ratios, focus, buffer names) saved to `$XDG_STATE_HOME/kasane/layout/{session}.json` on structural changes and shutdown; automatically restored on `kasane -c <session>` reconnect

Status: ✓ Complete

### 3.2 Display transformation / display unit model

Completed (first slice):

- P-030: Display transformation hook — `display_directives()` API on `Plugin` / `PluginBackend`
- P-031: Composition rules — `DirectiveSet` monoid + `resolve()` with priority-based composition
- P-033: Plugin-defined transformation API — `DisplayDirective` enum (`Fold`, `InsertAfter`, `InsertBefore`, `Hide`)
- P-034: Read-only / restricted interaction policy — `InteractionPolicy` enum, `SourceMapping`
- Core `DisplayMap` with O(1) bidirectional mapping, integrated into paint, cursor, input, and patch layers
- WASM WIT extension: `display-directives` function for WASM plugins (host conversion, adapter, roundtrip tests)
- `InsertBefore` directive for virtual text before buffer lines (WIT v0.17.0)
- `InlineOp::Insert` for inline virtual text insertion within buffer lines (WIT v0.16.0)
- EOL virtual text (Phase VT-1) — append virtual atoms after buffer line content
- Display scroll offset for virtual line overflow
- Proof artifact: `examples/virtual-text-demo/`

Remaining work:

- P-032: Formal observed/policy separation (theory organized, not yet enforced)

Completed (second slice):

- P-040 through P-043: Display unit model (`DisplayUnit`, `DisplayUnitId`, `SemanticRole`, `UnitSource`), geometry/source mapping/role, visual navigation (`NavigationPolicy`, `NavigationAction`, `ActionResult`), plugin-defined navigation policy via `HandlerRegistry` dispatch

### 3.3 WASM runtime operations

Remaining work:

- Plugin manifest
- Plugin settings API
- Precompiled component cache

Next deliverable: Decide on either manifest or settings API as the first implementation

### 3.4 Native escape hatch redesign

Completed:

- `Pane` / `Workspace` parity model — `Workspace` split tree, `PaneMap`, workspace observation on `PluginBackend` (landed in Phase 5)
- Plugin transforms integrated into Salsa rendering path — info overlays return `Vec<(InfoStyle, Overlay)>` for style-specific transform targets; menu path falls back to non-Salsa builder when `MENU_TRANSFORM` plugins are present

Remaining work:

- Redesign `PaintHook` into a high-level render hook that does not depend on direct `CellGrid` manipulation

Next deliverable: Finalize the redesign direction for `PaintHook` and land the minimal skeleton of the migration target API

### 3.5 Core event / degraded behavior

Remaining work:

- D-001: Minimal queuing based on `update()`
- P-023: Introduce `DropEvent` into `InputEvent` / plugin API / WIT

Next deliverable: Select either D-001 or P-023 as the first slice and land it on the core path

## 4. Phase Status Summary

| Phase | Primary objective | Status | Notes |
|---|---|---|---|
| Phase 0 | Development environment and CI foundation | ✓ Complete | project bootstrap |
| Phase 1 | MVP (TUI core features + declarative UI foundation) | ✓ Complete | Element + TEA + basic slots |
| Phase 2 | Enhanced floating windows + plugin foundation | ✓ Complete | Some items moved to subsequent workstreams |
| Phase 3 | Input, clipboard, and scroll enhancements | ✓ Complete | Basic input features on the TUI side are complete |
| Phase G | GUI backend | ✓ Complete | Foundation complete. R-053 text decoration rendering (DecorationPipeline) complete. Image element GPU pipeline + texture cache landed |
| Phase W | WASM plugin runtime foundation | ✓ Foundation complete | Remaining operational issues consolidated into `WASM runtime operations` |
| Phase 4 | Shared Plugin API validation | ✓ Complete | Proof artifacts for public extension points are sufficient |
| Phase 5 | Surface / Workspace / multi-pane foundation | ✓ Complete | Session/surface + multi-session UI complete; multi-pane split/focus/routing landed (5b/5c); UI polish (pane layout persistence) complete |
| Phase P | Plugin I/O foundation | ✓ Complete | P-1 / P-2 / P-3 complete |
| Plugin Redesign | Plugin architecture redesign (HandlerRegistry, ElementPatch, annotation decomposition, per-plugin invalidation, pub/sub, extension points, WASM capability inference, proc macro v2) | ✓ Complete | ADR-025 through ADR-029 |

## 5. Items Separated to Upstream Dependencies

The following items are not tracked in this roadmap; [upstream-dependencies.md](./upstream-dependencies.md) is the source of truth.

- D-002: Auxiliary display for off-screen cursors / selections
- P-001: Overlay composition (full version)
- P-010 / P-011: Supplemental area contributions (full version)
- D-004: Completeness of right-side navigation UI

## 6. Update Rules

This document is updated when:

- Priorities among `Now` / `Next` / `Backlog` change
- Deliverables or completion criteria for an open workstream change
- A phase status changes
- The source of truth for the tracker is moved to another document

## 7. Related Documents

- [upstream-dependencies.md](./upstream-dependencies.md) — Upstream blockers
- [plugin-api.md](./plugin-api.md) — Current API from a plugin's perspective
- [plugin-development.md](./plugin-development.md) — Practical guide for plugin authoring
- [performance.md](./performance.md) — Performance implementation progress
