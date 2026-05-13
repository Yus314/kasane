# ADR-012: Layer Responsibility Model

**Status:** Decided (revised from four layers to three)

**Context:**
During Phase 4a/4b item classification, a systematic criterion for determining which layer a feature belongs to was needed. The existing "resolution layer" was a classification of implementation mechanisms (HOW) and insufficient as a criterion for responsibility boundaries (WHERE).

Originally four layers (upstream / core / built-in plugin / external plugin), but since built-in plugins (`kasane-core/src/plugins/`) were migrated to WASM bundles and removed, the distinction between built-in and external became unnecessary. Revised to a three-layer model.

**Decision:** Adopt the three-layer responsibility model.

### 12-1: Three-Layer Definitions

| Layer | Definition | Criteria |
|-------|-----------|----------|
| Upstream (Kakoune) | Protocol-level concerns | Does it require protocol changes? |
| Core (kasane-core) | Faithful protocol rendering + frontend-native capabilities | Does a single correct implementation exist? |
| Plugin | Features where policy can diverge | Everything else |

The Plugin layer is subdivided by distribution form: bundled WASM (default UX) / FS-discovered WASM / native (`kasane::run()`).

### 12-2: Core Criteria — "A Single Correct Implementation"

Determined by "whether policy divergence exists."

- **Single policy:** Multi-cursor rendering (R-050) — there is only one way to parse faces → Core
- **Multiple policies:** Cursor line background color — color choice is user preference → Plugin
- **Frontend-native:** Focus detection (R-051), D&D (`P-023` proof-of-concept use case) — OS/window system specific → Core

### 12-3: API Parity

WASM plugins use a subset of the Plugin trait API via WIT interface. `contribute_to`, `transform`, `annotate_line_with_ctx`, `contribute_overlay_with_ctx`, `transform_menu_item`, and `render_ornaments` are available in WASM (WIT v0.4.0+). `Surface` and `Pane` APIs are available only in native plugins.

### 12-4: Upstream Criteria

Heuristic workarounds for information absent from the protocol are not constructed in principle.

**Exceptions:** Existing high-reliability heuristics are maintained:
- Cursor detection via FINAL_FG+REVERSE (R-064) — de facto standard behavior
- Estimation of auxiliary region contributions via face name pattern matching (`P-010` / `P-011` partial proof) — full version depends on upstream

**Rationale:**
- Heuristics risk breaking on upstream implementation changes
- Maintains motivation to encourage upstream toward formal solutions
- Features based on unreliable guesses degrade user experience

**Trade-offs:**
- Some features are unavailable while waiting for upstream changes
- Existing heuristics (FINAL_FG+REVERSE, etc.) are reliable and practical, so maintained as exceptions
- New heuristics are evaluated individually for reliability

### 12-5: Phase 4 Shared Plugin API Validation (Completed)

Proof artifacts for extension points reachable from WASM:

| Shared Extension Point | Proof Artifact | Status |
|------------------------|----------------|--------|
| `contribute_to(SlotId::BUFFER_LEFT)` | color_preview (gutter swatch) | Proven |
| `contribute_to(SlotId::STATUS_RIGHT)` | sel-badge (selection count badge) | Proven |
| `annotate_line_with_ctx()` | cursor_line (line background highlight), color_preview (gutter swatch) | Proven |
| `contribute_overlay_with_ctx()` | color_preview (color picker) | Proven |
| `handle_mouse()` | color_preview (color value editing) | Proven |
| `handle_key()` | test plugin in `kasane-core/tests/plugin_integration.rs` | Proven |
| `transform_menu_item()` | test plugin in `kasane-core/tests/plugin_integration.rs` | Proven |
| `contribute_to(SlotId::OVERLAY)` | Internal use (info/menu) | Implemented (external plugin proof pending) |
| `contribute_to(SlotId::BUFFER_RIGHT)` | — | Unproven (full version deferred due to upstream blocker) |
| `contribute_to(SlotId::ABOVE_BUFFER / BELOW_BUFFER)` | test plugin in `kasane-core/tests/plugin_integration.rs` | Proven |
| `transform(TransformTarget::BUFFER)` | test plugin in `kasane-core/tests/plugin_integration.rs` | Proven |
| `transform(TransformTarget::STATUS_BAR)` | prompt-highlight (status bar wrap in prompt mode) | Proven |
| `render_ornaments()` | test plugin in `kasane-core/tests/plugin_integration.rs` | Proven |
| `contribute_to(SlotId::Named(...))` | `surface_probe` hosted surface E2E in `kasane-wasm/src/tests.rs` | Proven |
| `OverlayAnchor::Absolute` | `fuzzy_finder` overlay test in `kasane-wasm/src/tests.rs` | Proven |
