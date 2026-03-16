# Kasane - Requirements Traceability

This document is a tracker for each requirement's resolution layer, status, Phase, and upstream dependencies.
The authoritative requirement text is in [requirements.md](./requirements.md).

## 1. Document Scope

This document tracks how the requirements defined in [requirements.md](./requirements.md) are resolved,
to what extent they have been implemented or proven, and which items are blocked by upstream constraints.

The authoritative requirement text is in [requirements.md](./requirements.md).
For current semantics, see [semantics.md](./semantics.md). For implementation order, see [roadmap.md](./roadmap.md).
For upstream blockers, see [upstream-dependencies.md](./upstream-dependencies.md).

## 2. Resolution Layer Classification

Each requirement is classified according to which Kasane mechanism resolves it.
This classification does not define "which layer is responsible."
For responsibility boundaries, see [layer-responsibilities.md](./layer-responsibilities.md).

| Resolution Layer | Description | Impact on Foundation Design |
|------------------|-------------|----------------------------|
| **Renderer** | Automatically resolved by the base implementation of the rendering engine and input handling | No foundation mechanism needed. Resolved simply by correct implementation |
| **Config** | Resolved through configuration via `config.toml` / `ui_options` | Builds a configuration interface on top of foundation mechanisms |
| **Foundation** | Resolved by Kasane's UI foundation and extension mechanisms | Plugin authors can also use the same mechanisms |
| **Protocol constraint** | Cannot be fully resolved due to Kakoune protocol limitations | Heuristic workarounds. Tracks contributions to upstream |

> **Complementary model:** Resolution layers classify "by what mechanism is it resolved" (HOW). The complementary
> classification of "which layer is responsible" (WHERE) is in the
> [three-layer responsibility model](./layer-responsibilities.md).

## 3. Core Functional Requirements Traceability

### 3.1 Basic Rendering (R-001, R-003 through R-009)

| ID | Resolution Layer | Notes | Status |
|----|------------------|-------|--------|
| R-001 | Foundation | Built with Element tree (BufferRef) | ✓ Phase 1 |
| R-003 | Renderer | Software cursor rendering | ✓ Phase 1 |
| R-004 | Renderer | Rendering with padding_face | ✓ Phase 1 |
| R-005 | Renderer | Resize detection and `resize` message sending | ✓ Phase 1 |
| R-006 | Renderer | 24-bit RGB direct rendering | ✓ Phase 1 |
| R-007 | Renderer | Double buffering (`CellGrid`) | ✓ Phase 1 |
| R-008 | Renderer | Width calculation based on `unicode-width` | ✓ Phase 1 |
| R-009 | Renderer | Placeholder glyph rendering | ✓ Phase 1 |

### 3.2 Standard Floating UI (R-010 through R-032)

| ID | Resolution Layer | Notes | Status |
|----|------------------|-------|--------|
| R-010 | Foundation | Built with `Stack` + `Overlay` | ✓ Phase 1 |
| R-011 | Foundation | Style-specific positioning via `OverlayAnchor` | ✓ Phase 1 |
| R-012 | Foundation | Reflects `selected` from `MenuState` | ✓ Phase 1 |
| R-013 | Foundation | Immediately hidden on `MenuState` clear | ✓ Phase 1 |
| R-014 | Config + Foundation | Placement policy via `MenuPlacement` / `InfoPlacement` | ✓ Phase 2 |
| R-016 | Renderer | Event batching (`recv + try_recv`, with safety valve) | ✓ Phase 2 |
| R-020 | Foundation | Built with `Stack` + `Overlay` | ✓ Phase 1 |
| R-021 | Foundation | `OverlayAnchor` + `InfoStyle` switching | ✓ Phase 1 |
| R-022 | Foundation | Immediately hidden on `InfoState` clear | ✓ Phase 1 |
| R-023 | Foundation | Concurrent management via `infos: Vec<InfoState>` + `InfoIdentity` | ✓ Phase 2 |
| R-024 | Foundation | `scroll_offset` + `InteractiveId` + mouse wheel | ✓ Phase 2 |
| R-025 | Foundation | Generalization of `compute_pos` to `&[Rect]` + avoid rect | ✓ Phase 2 |
| R-030 | Foundation | `OverlayAnchor::AnchorPoint` | ✓ Phase 1 |
| R-031 | Foundation | Clamping logic in `compute_pos` | ✓ Phase 1 |
| R-032 | Foundation | Drawing order of `Stack` | ✓ Phase 1 |

### 3.3 Standard Status / Prompt UI (R-060, R-061, R-063, R-064)

| ID | Resolution Layer | Notes | Status |
|----|------------------|-------|--------|
| R-060 | Foundation | Built with Element tree | ✓ Phase 1 |
| R-061 | Config | `Column` ordering changed by `status_at_top` | ✓ Phase 2 |
| R-063 | Foundation | Parsing `{face_spec}text{default}` in `markup.rs` | ✓ Phase 2 |
| R-064 | Foundation | `cursor_count` badge (`FINAL_FG+REVERSE` detection) | ✓ Phase 2 |

### 3.4 Input Handling (R-040 through R-047)

| ID | Resolution Layer | Notes | Status |
|----|------------------|-------|--------|
| R-040 through R-045 | Renderer | `crossterm` event conversion | ✓ Phase 1 |
| R-046 | Renderer | Coordinate calculation for scrolling during selection | ✓ Phase 3 |
| R-047 | Renderer | Right-click drag event handling | ✓ Phase 3 |

### 3.5 Cursor and Text Decoration (R-050, R-051, R-053)

| ID | Resolution Layer | Notes | Status |
|----|------------------|-------|--------|
| R-050 | Renderer | Software rendering | ✓ Phase 4a |
| R-051 | Renderer | Focus tracking | ✓ Phase 4a |
| R-053 | Renderer | Protocol parser, TUI backend, and GUI backend all supported. GUI uses DecorationPipeline (solid/curly/double underline + strikethrough) | ✓ Phase G |

### 3.6 UI Options / Clipboard / Scroll / Standard UI Style

| ID | Resolution Layer | Notes | Status |
|----|------------------|-------|--------|
| R-070, R-071 | Renderer | State reflection and redraw | ✓ Phase 1 |
| R-080 through R-082 | Renderer | Direct system clipboard API access (`arboard`) | ✓ Phase 3 |
| R-090 through R-093 | Renderer | Custom scroll implementation (smooth scroll + `PageUp` / `PageDown`) | ✓ Phase 3 |
| R-028 | Config + Foundation | `StyleToken` + `Theme` + `ThemeConfig` | ✓ Phase 2 |

## 4. Extension Foundation Requirements Traceability

P-series items track extension foundation capabilities guaranteed by the core.
For specific use cases, see [requirements.md](./requirements.md#4-validation-targets-and-representative-use-cases).

| ID | Resolution Layer | Notes | Status | Phase |
|----|------------------|-------|--------|-------|
| P-001 | Foundation | `Slot::Overlay` + `Decorator(Buffer)` | ○ Partially proven (`color_preview`) | 4b |
| P-002 | Foundation | `OverlayAnchor::Absolute` | ○ Infrastructure implemented (plugin proof pending) | 4b |
| P-003 | Foundation | Drawing order and clip in `Stack` | ✓ Phase 1 | 1 |
| P-010 | Foundation + Protocol constraint | `Slot::BufferLeft/Right`. `BUFFER_LEFT` partially proven with `line_numbers` / `color_preview`. `BUFFER_RIGHT` unproven | ○ Partially proven | 4b |
| P-011 | Foundation | Contribution API for auxiliary regions. Left gutter / line background proven with `color_preview`, `cursor_line` | ○ Partially proven | 4b |
| P-012 | Foundation + Protocol constraint | Full mapping to document-wide position partially depends on upstream | - In progress | [Upstream dep.](./upstream-dependencies.md) |
| P-020 | Foundation | Hit-testing with `Interactive Element` | ○ Partially proven (`color_preview`) | 4b |
| P-021 | Foundation | Event routing and target resolution | ○ Partially implemented | 4b |
| P-022 | Foundation | Semantic recognizer / binding | - Candidate | 5c |
| P-023 | Renderer + Foundation | GUI file drop -> `:edit` implemented. Generic drop routing to UI elements / plugins not yet done | ○ Partial | Separate track |
| P-030 | Foundation | Display transformation hook | - Candidate | 5c |
| P-031 | Foundation | Composition rules for elision, proxy display, and additional display | - Candidate | 5c |
| P-032 | Foundation + Semantics | Observed / policy separation. Authoritative source is [semantics.md](./semantics.md) | ○ Theory organized | 5c |
| P-033 | Foundation | Plugin-defined transformation API | - Candidate | 5c |
| P-034 | Foundation + Semantics | Read-only / restricted interaction policy | - Candidate | 5c |
| P-040 | Foundation | Display unit model | - Candidate | 5c |
| P-041 | Foundation | Geometry / source mapping / role | - Candidate | 5c |
| P-042 | Foundation + Renderer | Visual navigation / hit test | - Candidate | 5c |
| P-043 | Foundation | Plugin-defined navigation policy | - Candidate | 5c |
| P-050 | Foundation | Split layout with `Flex` | - Candidate | 5a |
| P-051 | Foundation | Focus / input routing across surfaces | - Candidate | 5a |
| P-052 | Foundation | Workspace / tab / pane manager abstraction | - Candidate | 5a |
| P-060 | Renderer + Foundation | Kasane-specific decoration capabilities | - Candidate | 5c |
| P-061 | Config + Foundation | Semantic style tokens | ○ Basic implementation exists | 5a |
| P-062 | Renderer | Text policy assuming GPU backend (`glyphon`) | - Candidate | 5c |

## 5. Upstream Dependency / Degraded Behavior Traceability

| ID | Resolution Layer | Notes | Status |
|----|------------------|-------|--------|
| D-001 | Protocol constraint | Startup `info` awaiting isolation of Kakoune-side behavior | [Upstream dep.](./upstream-dependencies.md) |
| D-002 | Protocol constraint | Insufficient completeness of out-of-viewport information | [Upstream dep.](./upstream-dependencies.md) |
| D-003 | Protocol constraint | `draw_status` context insufficient. Heuristic fallback only | [Upstream dep.](./upstream-dependencies.md) |
| D-004 | Protocol constraint | Scroll information insufficient; full right-side navigation UI not possible | [Upstream dep.](./upstream-dependencies.md) |

## 6. Tracking Notes

- Resolution layers indicate "by what mechanism is it resolved." The authoritative source for
  responsibility boundaries is [layer-responsibilities.md](./layer-responsibilities.md).
- For the meaning and implementation order of Phases, see [roadmap.md](./roadmap.md).
- For reintegration conditions of upstream-dependent items, see
  [upstream-dependencies.md](./upstream-dependencies.md).
- For non-functional requirements, particularly measurement results and implementation status of
  performance requirements, see [performance.md](./performance.md) and
  [performance-benchmarks.md](./performance-benchmarks.md).

## 7. Related Documents

- [requirements.md](./requirements.md) — Authoritative source for requirement text
- [roadmap.md](./roadmap.md) — Phases and incomplete items
- [upstream-dependencies.md](./upstream-dependencies.md) — Upstream dependency tracking
- [layer-responsibilities.md](./layer-responsibilities.md) — Criteria for responsibility boundaries
