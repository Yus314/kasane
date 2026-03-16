# Layer Responsibility Model

This document defines the criteria for determining whether a new feature belongs to upstream, core, or plugins.
For the classification of implementation mechanisms, refer to the resolution layer in [requirements-traceability.md](./requirements-traceability.md).

## Overview

This document establishes the criteria for determining "which layer a feature belongs to" when adding new features to Kasane.

The "resolution layer" in [requirements-traceability.md](./requirements-traceability.md) is a classification of **implementation mechanisms** (HOW: renderer/configuration/infrastructure/protocol constraints), defining "by what mechanism to resolve it." This model is a classification of **responsibility boundaries** (WHERE: upstream/core/plugin), defining "which layer is responsible." Both axes are necessary for feature classification.

**Related documents:**
- [architecture.md](./architecture.md) — Abstraction boundaries
- [decisions.md](./decisions.md) — ADR-012: Layer Responsibility Model
- [upstream-dependencies.md](./upstream-dependencies.md) — Tracking upstream dependencies

---

## Three-Layer Model

> **Background:** Originally this was a four-layer model of "upstream / core / built-in plugins / external plugins," but built-in plugins (`kasane-core/src/plugins/`) were migrated to WASM, and `kasane-core/src/plugins/` was removed. The roles that built-in plugins served (API validation, reference implementations, default UX) are now absorbed by example plugins (`examples/wasm/`, `examples/line-numbers/`) respectively, so the plugin layers were unified and the model was revised to three layers.

### Upstream (Kakoune)

**Definition:** Protocol-level concerns. Features that require protocol changes.

**Principle:** Core should not, as a rule, build heuristic workarounds for information that does not exist in the protocol.

**Tracking:** Record in [upstream-dependencies.md](./upstream-dependencies.md) and monitor upstream PRs/Issues.

**Examples:**
- Completeness of right-side navigation UI (D-004) — `draw` message does not include scroll position
- Atom types (PR #4707) — No distinction between auxiliary regions / overlays / body text
- Off-screen cursor / selection auxiliary display (D-002) — `draw` message does not include total cursor count

### Core (kasane-core)

**Definition:** Faithful rendering of the protocol + frontend-native capabilities.

**Decision criterion:** "Does a single correct implementation exist?" — If Yes, it belongs in core.

**Protocol rendering:**
- Faithful rendering of `draw` / `menu_show` / `info_show` / `draw_status`
- Layout calculation (Flex + Overlay + Grid)
- Differential rendering (CellGrid → diff → backend)

**Frontend-native:**
- Focus detection (R-051) — Window focus gain/loss is frontend-specific
- D&D (P-023 proof use case) — GUI window events are frontend-specific
- Clipboard (R-080–R-082) — Direct system API access
- Multi-cursor rendering (R-050) — Face analysis derived from protocol
- Faithful rendering of text decorations (R-053) — Faithful rendering of underline types, underline colors, and strikethrough sent by the protocol

**What core does NOT do:**
- Display decisions where policy may vary (cursor line highlight color, gutter display items, etc.) — Plugin territory
- Heuristic guessing for features where the protocol lacks information — Upstream territory

### Plugin

**Definition:** Features where policy may vary. Areas that can be customized according to user preferences and use cases.

**Decision criterion:** "Does a single correct implementation exist?" — If No, it belongs in plugins.

**Distribution forms:**

| Form | Mechanism | Use case |
|------|-----------|----------|
| **Example WASM** | Embedded in binary via `include_bytes!` | Included examples (cursor_line, color_preview, sel_badge, fuzzy_finder) |
| **FS-discovered WASM** | `~/.local/share/kasane/plugins/*.wasm` | WASM plugins placed by users |
| **Native** | Compile-time binding via `kasane::run(\|registry\| { ... })` | Performance-critical or uses Surface/PaintHook/Pane |

**Registration order:** Example WASM → FS-discovered WASM (can override with same ID) → User callback

**Reference implementations:** `examples/` (native and WASM) serve as implementation examples for plugin authors.

---

## Decision Flowchart

```
Want to add feature F
  │
  ▼
1. Does it require a protocol change?
  │  Yes → Upstream (record in upstream-dependencies.md)
  │  No ↓
  ▼
2. Does a single correct implementation exist?
  │  Yes → Core (kasane-core)
  │  No ↓
  ▼
3. Plugin
  │  Otherwise → External plugin (WASM or native)
  │  Insufficient API? → Plugin trait / WIT extension comes first
```

---

## Shared Plugin API Validation

Phase 4 addresses validation of the **shared Plugin API reachable from WASM**.
Proof artifacts are not limited to distributable samples; WASM fixtures, `examples/`, and plugins within integration tests are treated equivalently.

| Shared Extension Point | Proof Artifact | Status |
|------------------------|----------------|--------|
| `contribute_to(SlotId::BUFFER_LEFT)` | color_preview (gutter swatch), line-numbers (line numbers) | Proven |
| `contribute_to(SlotId::STATUS_RIGHT)` | sel-badge (selection count badge) | Proven |
| `annotate_line_with_ctx()` | cursor_line (line background highlight), color_preview (gutter swatch) | Proven |
| `contribute_overlay_with_ctx()` | color_preview (color picker) | Proven |
| `handle_mouse()` | color_preview (color value editing) | Proven |
| `handle_key()` | test plugin in `kasane-core/tests/plugin_integration.rs` | Proven |
| `transform_menu_item()` | test plugin in `kasane-core/tests/plugin_integration.rs` | Proven |
| `contribute_to(SlotId::OVERLAY)` | Internal use (info/menu) | Implemented (external plugin proof pending) |
| `contribute_to(SlotId::BUFFER_RIGHT)` | — | Unproven (full version deferred due to upstream blocker) |
| `contribute_to(SlotId::ABOVE_BUFFER / BELOW_BUFFER)` | test plugin in `kasane-core/tests/plugin_integration.rs` | Proven |
| `transform(TransformTarget::Buffer)` | test plugin in `kasane-core/tests/plugin_integration.rs` | Proven |
| `cursor_style_override()` | test plugin in `kasane-core/tests/plugin_integration.rs` | Proven |
| `contribute_to(SlotId::Named(...))` | `surface_probe` hosted surface E2E in `kasane-wasm/src/tests.rs` | Proven |
| `OverlayAnchor::Absolute` | `fuzzy_finder` overlay test in `kasane-wasm/src/tests.rs` | Proven |

## Native Escape Hatches

Native-only APIs are handled separately from shared validation. The long-term policy is WASM parity, but exposing the same trait directly to WIT is not a goal.

| Native-only API | Current positioning | Parity policy |
|-----------------|---------------------|---------------|
| `PaintHook` | Provisional escape hatch | Needs redesign from direct `CellGrid` manipulation to high-level render hooks |
| `Surface` / `SURFACE_PROVIDER` | Hosted surface model introduced | `surface-descriptor` / `render-surface` / `handle-surface-event` / `handle-surface-state-changed` are introduced. Runtime wiring of `SessionManager` with `spawn-session` / `close-session`, and retention of inactive session snapshots are introduced. See [roadmap.md](./roadmap.md) for prioritization of remaining session/surface parity |
| `Pane` / `Workspace` advanced API | Native-only but parity target | Aiming for parity via command / observer model rather than object access |

---

## Concrete Examples: Item Classification

| Item | Classification | Rationale |
|------|----------------|-----------|
| D-001 | Under upstream verification → Core candidate | After verifying upstream behavior, minimal core implementation (TEA update() queuing). Could be the single correct implementation |
| R-050 | Core | Multi-cursor rendering is face analysis derived from protocol; the single correct implementation. However, Primary/Secondary distinction awaits PR #4707 |
| R-051 | Core (implemented) | Window focus detection is a frontend-native capability. The single correct implementation |
| D-002 | Upstream-dependent | `draw` message does not include total cursor count, making accurate detection of off-viewport cursors impossible |
| R-053 | Core | Faithful rendering of text decorations sent by the protocol is the rendering system's responsibility; the single correct implementation |
| P-002 proof | Plugin | Proof artifact for `OverlayAnchor::Absolute`. Can be implemented as a WASM guest or plugin within integration tests |
| Line/range decoration proof | Plugin | Proof artifact via `annotate_line_with_ctx()` or `transform()`. Can be implemented as a WASM guest or plugin within integration tests |
| P-023 implementation | Core | D&D is a native capability of the GUI backend (winit). The single correct implementation |

---

## Correspondence with Existing "Resolution Layer"

Relationship with the resolution layer in [requirements-traceability.md](./requirements-traceability.md):

| | Resolution Layer (HOW) | Three-Layer Model (WHERE) |
|---|---|---|
| **Question** | By what mechanism to resolve? | Which layer is responsible? |
| **Classification** | Renderer / Configuration / Infrastructure / Protocol constraint | Upstream / Core / Plugin |
| **Example** | R-050 → Renderer (software rendering) | R-050 → Core (the single correct implementation) |
| **Example** | Line/range decoration → Infrastructure (Transform) | Line/range decoration proof → Plugin |

Both axes are necessary for feature classification:
- **Resolution layer** determines the technical mechanism of implementation
- **Three-layer model** determines code placement and responsibility boundaries

## Related Documents

- [requirements-traceability.md](./requirements-traceability.md) — Resolution layer (HOW) tracking
- [semantics.md](./semantics.md) — Current semantics
- [architecture.md](./architecture.md) — System boundaries
- [upstream-dependencies.md](./upstream-dependencies.md) — Upstream dependency tracking
