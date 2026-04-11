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
| Display transformation | P-032 Level 2 — `Inference` / `Policy` read-side projections (ADR-030 staged rollout) |

### 2.2 Backlog

| Workstream | Notes |
|---|---|
| External plugin candidates | indent guides, clickable links, built-in splits, floating panels, code folding, display-line navigation, URL detection, region-specific text policy, etc. |
| Session-affine plugin surfaces | Plugin API for declaring session affinity on `surfaces()` return values. No consumer exists yet; deferred until a plugin requires it |
| Type-level isolation of Kakoune-writing Commands | Marker trait or sub-enum that distinguishes `{SendToKakoune, InsertText, EditBuffer}` from other `Command` variants, enabling automatic derivation of the "Kakoune-Transparent?" column in semantics §9.1. See semantics §13.13, §13.15 |
| RecoveryWitness contract for destructive display directives | Associated type or registration-time check witnessing the Visual Faithfulness condition on fold/hide-style directive contributors. See semantics §10.2a, §13.14 |
| Element ↔ §2.6 P(X) synchronisation regression test | Mechanise the §15.1 sync obligation between `Element` variants and the polynomial functor P(X) in semantics §2.6, so variant additions force a semantics update. See semantics §13.16 |
| Explicit free monad of Commands | Replace `Vec<Command>` with an explicit `Free<CommandSig>` representation so effect sequences become statically analysable (T12). Large refactor, low priority. See semantics §13.17 |

## 3. Open Workstreams

### 3.1 Display transformation — remaining work

- P-032: Formal observed/policy separation
  - Level 1 — `Truth<'a>` projection: **✓ Complete** (ADR-030). Read-side write denial for `#[epistemic(observed)]` fields, structural coverage witness, A9 property test, and Salsa projection fix (`status_prompt` / `status_content` / `status_content_cursor_pos`).
  - Level 2 — `Inference<'a>` / `Policy<'a>` projections: derived+heuristic and config+runtime projections analogous to Level 1.
  - Levels 3–6 — Kakoune-writing `Command` marker trait, `RecoveryWitness` for destructive directives, explicit free monad of `Command`, and type-level `&mut AppState` ownership on the protocol ingestion path. Tracked in §2.2 Backlog.

Next deliverable: Design and land `Inference<'a>` / `Policy<'a>` (ADR-030 Level 2), mirroring the `Truth<'a>` pattern from Level 1. Blocks the type-level Kakoune-transparency work listed in §2.2 Backlog.

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
- [semantics.md](./semantics.md) — Current semantics authority (referenced by backlog entries for gap identifiers)
- [plugin-api.md](./plugin-api.md) — Current API from a plugin's perspective
- [plugin-development.md](./plugin-development.md) — Practical guide for plugin authoring
- [performance.md](./performance.md) — Performance implementation progress
