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

| Workstream | Next deliverable |
|---|---|
| Display transformation | Initial enforcement of P-032 observed/policy separation |

### 2.2 Backlog

| Workstream | Notes |
|---|---|
| External plugin candidates | indent guides, clickable links, built-in splits, floating panels, code folding, display-line navigation, URL detection, region-specific text policy, etc. |
| Session-affine plugin surfaces | Plugin API for declaring session affinity on `surfaces()` return values. No consumer exists yet; deferred until a plugin requires it |

## 3. Open Workstreams

### 3.1 Display transformation — remaining work

- P-032: Formal observed/policy separation (theory organized, not yet enforced)

Next deliverable: Define the initial enforcement boundary for observed state vs. display policy and land the minimal enforcement skeleton in transform paths

## 4. Phase Status Summary

| Phase | Primary objective | Status | Notes |
|---|---|---|---|
| Phase 0 | Development environment and CI foundation | ✓ Complete | project bootstrap |
| Phase 1 | MVP (TUI core features + declarative UI foundation) | ✓ Complete | Element + TEA + basic slots |
| Phase 2 | Enhanced floating windows + plugin foundation | ✓ Complete | Some items moved to subsequent workstreams |
| Phase 3 | Input, clipboard, and scroll enhancements | ✓ Complete | Basic input features on the TUI side are complete |
| Phase G | GUI backend | ✓ Complete | Foundation complete. R-053 text decoration rendering (DecorationPipeline) complete. Image element GPU pipeline + texture cache landed |
| Phase W | WASM plugin runtime foundation | ✓ Complete | Foundation + operational follow-ups (plugin manifest, settings API, precompiled cache) |
| Phase 4 | Shared Plugin API validation | ✓ Complete | Proof artifacts for public extension points are sufficient |
| Phase 5 | Surface / Workspace / multi-pane foundation | ✓ Complete | Session/surface + multi-session UI complete; multi-pane split/focus/routing landed (5b/5c); UI polish (pane layout persistence) complete |
| Phase P | Plugin I/O foundation | ✓ Complete | P-1 / P-2 / P-3 complete |
| Plugin Redesign | Plugin architecture redesign (HandlerRegistry, ElementPatch, annotation decomposition, per-plugin invalidation, pub/sub, extension points, WASM capability inference, proc macro v2) | ✓ Complete | ADR-025 through ADR-029 |

## 5. Items Separated to Upstream Dependencies

The following items are not tracked in this roadmap; [upstream-dependencies.md](./upstream-dependencies.md) is the source of truth.

- D-001: Startup info retention
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
