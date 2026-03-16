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
| Session / Surface parity | Active | Automatic generation of session-bound surfaces | Surface groups are automatically generated per active / inactive session, and surfaces consistently follow session switching |
| Multi-session UI parity | Active | Minimal UI for session switcher or session list | Multiple sessions can be switched in a user-visible manner, and the existence of non-active sessions is apparent from the UI |
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

Remaining work:

- Automatic generation of session-bound surfaces
- Mechanism to consistently attach / detach surface groups per session on session switch
- Organize surface registry / workspace side to treat session identity as first-class

Next deliverable:

- Minimal implementation that automatically generates buffer/status/supplemental surfaces per active session
- Proof that surface composition switches deterministically on session switch

Proof / completion criteria:

- Surface composition does not break when there are two or more sessions
- No stale surfaces remain after session switch
- Correspondence between active / inactive session snapshots and surfaces is locked down by automated tests

### 3.2 Multi-session UI parity

Current status:

- Runtime can hold multiple sessions
- State snapshots of inactive sessions are retained
- Rendering target is still only the single active session
- Session observability and control are available to plugins: session descriptors in `AppState`, `DirtyFlags::SESSION` for lifecycle notifications, `SessionCommand::Switch` for session activation, WIT Tier 8 for WASM plugins

Remaining work:

- Bundled WASM plugin providing session list / session switcher UI
- User-visible active session display (e.g., status bar indicator via slot contribution)
- UI feedback for session close / promote

Next deliverable:

- Bundled session UI plugin showing session list and active state
- Command path to switch sessions from the UI

Proof / completion criteria:

- Multiple sessions are identifiable in the UI
- Active session can be switched from the UI
- Creation-order promotion on close is observable in the UI

### 3.3 Display transformation / display unit model

Remaining work:

- P-030 through P-043
- Display transformation
- Display unit model
- Navigation policy

Next deliverable:

- Select the smallest single slice from the above and introduce it with a proof artifact

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

- [requirements-traceability.md](./requirements-traceability.md) — Status per requirement
- [upstream-dependencies.md](./upstream-dependencies.md) — Upstream blockers
- [layer-responsibilities.md](./layer-responsibilities.md) — Organization of shared API validation and native escape hatches
- [plugin-api.md](./plugin-api.md) — Current API from a plugin's perspective
- [plugin-development.md](./plugin-development.md) — Practical guide for plugin authoring
- [performance-benchmarks.md](./performance-benchmarks.md) — Performance implementation progress
