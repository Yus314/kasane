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
for the division of responsibilities between the shared API and native escape hatches, see
[layer-responsibilities.md](./layer-responsibilities.md); for the current specification from a plugin's perspective, see
[plugin-api.md](./plugin-api.md); for performance numbers and implementation status, see
[performance-benchmarks.md](./performance-benchmarks.md).

## 2. Current Priorities

### 2.1 Now

| Workstream | Status | Next deliverable | Completion criteria |
|---|---|---|---|
| Session / Surface parity | Active | Per-session surface filtering (multi-pane prerequisite) | Infrastructure landed; correctness proof complete via automated tests |
| Multi-session UI parity | **Complete** | Session-ui example plugin with status bar + switcher overlay | Multiple sessions can be switched in a user-visible manner, and the existence of non-active sessions is apparent from the UI |
| Display transformation / display unit model | Active | First slice of P-030 through P-043 | Minimal implementation and proof of display transformation / navigation policy are in place |

### 2.2 Next

| Workstream | Status | Next deliverable |
|---|---|---|
| WASM runtime operations | Open | Add operational features in order: plugin manifest, plugin settings API, precompiled component cache |
| Native escape hatch redesign | Open | Higher-level `PaintHook`, definition of `Pane` / `Workspace` parity model |
| Core event / degraded behavior | Open | Minimal queuing for D-001, introduction of P-023 `DropEvent` |

### 2.3 Backlog

| Workstream | Status | Notes |
|---|---|---|
| External plugin candidates | Open | Maintained as candidates: indent guides, clickable links, built-in splits, floating panels, code folding, display-line navigation, URL detection, region-specific text policy, etc. |

## 3. Open Workstreams

### 3.1 Session / Surface parity

Current status:

- `SessionManager` foundation, primary session linkage, and runtime `spawn-session` / `close-session` wiring are in place
- Kakoune events from inactive sessions continue to be reflected in off-screen snapshots
- Hosted surface `render-surface` / `handle-surface-event` / `handle-surface-state-changed` are in place
- Session observability infrastructure complete: `AppState.session_descriptors` / `active_session_key`, `DirtyFlags::SESSION`, `SessionCommand::Switch`, WIT Tier 8 host-state functions (ADR-023 step 1)

Design analysis (2026-03-16):

Built-in surfaces (`KakouneBufferSurface`, `StatusBarSurface`) are stateless renderers of `AppState`.
Ephemeral surfaces (`MenuSurface`, `InfoSurface`) are auto-managed via `sync_ephemeral_surfaces()` based on `AppState`.
Because `AppState` is already swapped atomically on session switch, current single-pane surface composition
is correct without per-session surface instances. Per-session surface generation and per-session workspace trees
are deferred to multi-pane implementation, where they become necessary.

Completed:

- `session_id: Option<SessionId>` field in `RegisteredSurface` — enables future per-session surface binding
- `SurfaceRegistry::remove_surfaces_for_session()` — close-time cleanup of session-bound surfaces
- `SurfaceRegistry::surface_session_id()` — query session binding of a registered surface
- Automated correctness proof: 7 integration tests + 4 unit tests covering session switch, ephemeral surface lifecycle, round-trip stability, session close/promote, dirty flags, metadata consistency, and compose_view correctness

Remaining work:

- `compose_view` filtering by active session (deferred — co-designed with multi-pane, where per-session surfaces become necessary)
- Plugin API for declaring session affinity on `surfaces()` return values (deferred — no consumer exists yet; correct model depends on multi-pane design)

Deferred to multi-pane:

- Per-session `KakouneBufferSurface` / `StatusBarSurface` instances (stateless surfaces do not benefit from duplication)
- Per-session workspace trees (single-pane layout is session-agnostic)
- Surface internal state snapshots (no stateful plugin surfaces exist yet)

Next deliverable:

- Multi-pane integration: per-session surface filtering in `compose_view`, workspace tree per session

Proof / completion criteria:

- ✓ Surface composition does not break when there are two or more sessions
- ✓ No stale surfaces remain after session switch
- ✓ Correspondence between active / inactive session snapshots and surfaces is locked down by automated tests
- ✓ Session affinity API is available for plugin surfaces (even if no plugin uses it yet)

### 3.2 Multi-session UI parity

Current status: **Complete**

Delivered:

- `SessionDescriptor` enriched with `buffer_name` and `mode_line` from per-session `AppState` snapshots
- WIT `session-descriptor` record extended with `buffer-name` and `mode-line` fields
- `on_state_changed(SESSION)` notification after deferred session commands (Spawn/Close/Switch)
- `send_initial_resize` recovery after session commands to prevent input suppression
- Session key deduplication in `spawn_session_core` to avoid Kakoune process orphaning
- Example plugin `examples/wasm/session-ui/`: status bar indicator + Ctrl+T session switcher overlay
- 14 WASM fixture tests in `kasane-wasm/src/tests/session_ui.rs`

Proof / completion criteria (all met):

- Multiple sessions are identifiable in the UI (status bar shows `[count:key]`)
- Active session can be switched from the UI (Ctrl+T → select → Enter)
- Creation-order promotion on close is observable in the UI (switcher reflects remaining sessions)
- Session create (`n`), close (`d`), and switch (`Enter`) all work from within the switcher

### 3.3 Display transformation / display unit model

**Completed (first slice):**

- P-030: Display transformation hook — `display_directives()` API on `Plugin` / `PluginBackend`
- P-033: Plugin-defined transformation API — `DisplayDirective` enum (`Fold`, `InsertAfter`, `Hide`)
- P-034: Read-only / restricted interaction policy — `InteractionPolicy` enum, `SourceMapping`
- Core `DisplayMap` with O(1) bidirectional mapping, integrated into paint, cursor, input, and patch layers
- Proof artifact: `examples/virtual-text-demo/` (virtual text insertion with keyword detection)

**Remaining work:**

- P-031: Composition rules for multi-plugin display directives (currently single-plugin constraint)
- P-032: Formal observed/policy separation (theory organized, not yet enforced)
- P-040 through P-043: Display unit model, geometry/source mapping/role, visual navigation, plugin-defined navigation policy
- WASM WIT extension: `display-directives` function for WASM plugins

### 3.4 WASM runtime operations

Remaining work:

- Plugin manifest
- Plugin settings API
- Precompiled component cache

Next deliverable:

- Decide on either manifest or settings API as the first implementation

### 3.5 Native escape hatch redesign

Remaining work:

- Redesign `PaintHook` into a high-level render hook that does not depend on direct `CellGrid` manipulation
- Definition of `Pane` / `Workspace` parity model

Next deliverable:

- Finalize the redesign direction for `PaintHook` and put in place the minimal skeleton of the migration target API

### 3.6 Core event / degraded behavior

Remaining work:

- D-001: Minimal queuing based on `update()`
- P-023: Introduce `DropEvent` into `InputEvent` / plugin API / WIT

Next deliverable:

- Select either D-001 or P-023 as the first slice and land it on the core path

## 4. Phase Status Summary

| Phase | Primary objective | Status | Notes |
|---|---|---|---|
| Phase 0 | Development environment and CI foundation | ✓ Complete | project bootstrap |
| Phase 1 | MVP (TUI core features + declarative UI foundation) | ✓ Complete | Element + TEA + basic slots |
| Phase 2 | Enhanced floating windows + plugin foundation | ✓ Complete | Some items moved to subsequent workstreams |
| Phase 3 | Input, clipboard, and scroll enhancements | ✓ Complete | Basic input features on the TUI side are complete |
| Phase G | GUI backend | ✓ Complete | Foundation complete. R-053 text decoration rendering (DecorationPipeline) complete |
| Phase W | WASM plugin runtime foundation | ✓ Foundation complete | Remaining operational issues consolidated into `WASM runtime operations` |
| Phase 4 | Shared Plugin API validation | ✓ Complete | Proof artifacts for public extension points are sufficient |
| Phase 5 | Surface / Workspace / display restructuring foundation | Open | Session/surface parity and display transformation are ongoing |
| Phase P | Plugin I/O foundation | ✓ Complete | P-1 / P-2 / P-3 complete |

## 5. Items Separated to Upstream Dependencies

The following items are not tracked in this roadmap; [upstream-dependencies.md](./upstream-dependencies.md) is the source of truth.

- D-002: Auxiliary display for off-screen cursors / selections
- D-003: Status line context inference
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
- [layer-responsibilities.md](./layer-responsibilities.md) — Organization of shared API validation and native escape hatches
- [plugin-api.md](./plugin-api.md) — Current API from a plugin's perspective
- [plugin-development.md](./plugin-development.md) — Practical guide for plugin authoring
- [performance-benchmarks.md](./performance-benchmarks.md) — Performance implementation progress
