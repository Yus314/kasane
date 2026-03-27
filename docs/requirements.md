# Kasane - Requirements Specification

This document is the authoritative source for the requirements that Kasane must satisfy.
For tracking of implementation status and phases, see [roadmap.md](./roadmap.md); for upstream dependencies, see [upstream-dependencies.md](./upstream-dependencies.md).

## 1. Project Overview

**Project Name:** Kasane (重ね)

**Purpose:** To provide an extensible, high-performance frontend UI foundation for the Kakoune text editor via its JSON UI protocol. Kasane prioritizes extensibility, configurability, and semantic consistency over providing features directly, enabling displays and interactions that are difficult to achieve with the standard terminal UI.

**Design Principles:**
- **Extensibility:** Plugins can extend the frontend through UI contributions, decorations, overlays, transforms, and custom region provisioning
- **Configurability:** Users can change themes, layouts, keybindings, and display policies through configuration
- **High Performance:** Maintain practical responsiveness and smooth rendering even under high-frequency updates
- **Semantic Consistency:** Display UI with the same meaning for the same state regardless of which backend is used
- **Ease of Adoption:** Prioritize allowing existing Kakoune users to adopt Kasane without major configuration or workflow changes
- **Compatibility First:** Operate coherently with `kakrc`, autoload, existing plugins, and existing session workflows as standard behavior
- **Conservative Defaults:** In the default state, do not unnecessarily deviate from the existing Kakoune user experience
- **Optional Extensions:** Kasane's advanced features and plugins are added value, not prerequisites for normal usage
- **Kakoune-Specific:** Designed specifically for Kakoune's JSON UI protocol. No unnecessary abstractions
- Communication with Kakoune via the JSON UI (JSON-RPC 2.0) protocol
- A pure JSON UI frontend (no dependency on specific plugins)

**Supplementary Documents:**
- [Current Semantics](./semantics.md) — State, rendering, redraw policy, and extensibility norms
- [Implementation Roadmap](./roadmap.md) — Implementation order and upcoming phases
- [Upstream Dependencies](./upstream-dependencies.md) — Upstream blockers and reintegration conditions

---

## 2. Core Functional Requirements

This chapter defines the capabilities that Kasane must directly provide and guarantee as standard behavior in its role as a JSON UI frontend. Core functional requirements here include only capabilities for which Kasane itself bears implementation responsibility: rendering, input, standard UI, state reflection, standard style system, and so on. Concrete features achievable through external plugins, or application examples enabled by Kasane's foundation, are not included in this chapter but are covered in [3. Extension Foundation Requirements](#3-extension-foundation-requirements) and [4. Validation Targets and Representative Use Cases](#4-validation-targets-and-representative-use-cases). Items that cannot be fully guaranteed due to insufficient upstream information, or degraded behaviors that rely on heuristics, are covered in [6. Upstream Dependencies and Degraded Behaviors](#6-upstream-dependencies-and-degraded-behaviors).

### 2.1 Basic Rendering

Kasane provides the basic rendering capabilities to accurately and reliably reflect the drawing facts observed from Kakoune onto the screen.

| ID | Requirement | Description | Related Issue |
|----|-------------|-------------|---------------|
| R-001 | Buffer rendering | Rendering of the main buffer based on `draw` messages. Accurately reflects Face (fg, bg, underline, attributes) | — |
| R-003 | Cursor display | Software cursor rendering (block/bar/underline). Priority control between buffer cursor and prompt cursor | [#1524](https://github.com/mawww/kakoune/issues/1524) |
| R-004 | Padding display | Renders lines beyond the end of the buffer with `padding_face` | — |
| R-005 | Resize handling | Detects window size changes and sends `resize` messages to Kakoune. Handles redrawing appropriately | — |
| R-006 | True Color rendering | Renders 24-bit RGB colors directly. No terminal palette approximation | [#3554](https://github.com/mawww/kakoune/issues/3554), [#2842](https://github.com/mawww/kakoune/issues/2842), [#3763](https://github.com/mawww/kakoune/issues/3763) |
| R-007 | Double buffering | Performs frame rendering atomically, completely eliminating flicker | [#3429](https://github.com/mawww/kakoune/issues/3429), [#4320](https://github.com/mawww/kakoune/issues/4320), [#4317](https://github.com/mawww/kakoune/issues/4317), [#3185](https://github.com/mawww/kakoune/issues/3185) |
| R-008 | Unicode character width calculation | Custom Unicode text layout for accurate width calculation of CJK/emoji/zero-width characters. Independent of libc's `wcwidth()` | [#3598](https://github.com/mawww/kakoune/issues/3598), [#4257](https://github.com/mawww/kakoune/issues/4257), [#3059](https://github.com/mawww/kakoune/issues/3059), [#1941](https://github.com/mawww/kakoune/issues/1941) |
| R-009 | Special character visualization | Visually displays zero-width characters (U+200B, etc.) and control characters (^A, ^M) with placeholder glyphs | [#3570](https://github.com/mawww/kakoune/issues/3570), [#2936](https://github.com/mawww/kakoune/issues/2936) |

### 2.2 Standard Floating UI

Kasane guarantees display, positioning, readability, and visual stability for its standard floating UI elements such as menus and info panels.

| ID | Requirement | Description | Related Issue |
|----|-------------|-------------|---------------|
| R-010 | Standard menu display | Standard floating display of the completion menu based on `menu_show` messages | — |
| R-011 | Standard menu styles | Switches the standard UI according to `inline`, `prompt`, and `search` styles | — |
| R-012 | Menu selection display | Highlights the selected item in the standard menu UI based on `menu_select` messages | — |
| R-013 | Menu hiding | Immediately hides the standard menu UI based on `menu_hide` messages | — |
| R-014 | Standard floating UI positioning policy | Standard menu/info UI supports configurable anchor selection, positioning, and collision avoidance policies | [#3938](https://github.com/mawww/kakoune/issues/3938), [#2170](https://github.com/mawww/kakoune/issues/2170), [#1531](https://github.com/mawww/kakoune/issues/1531) |
| R-016 | Intermediate state suppression under high-frequency updates | Standard floating UI suppresses unnecessary intermediate state display and temporary flashes even under high-frequency UI updates | [#1491](https://github.com/mawww/kakoune/issues/1491) |
| R-020 | Standard info display | Standard floating display of documentation/help information based on `info_show` messages | — |
| R-021 | Standard info styles | Supports `prompt`, `inline`, `inlineAbove`, `inlineBelow`, `menuDoc`, and `modal` styles | — |
| R-022 | Info hiding | Immediately hides the standard info UI based on `info_hide` messages | — |
| R-023 | Simultaneous display of multiple info elements | The standard floating UI can hold and display multiple independent info elements simultaneously | [#1516](https://github.com/mawww/kakoune/issues/1516) |
| R-024 | Browsing means for long content | The standard info UI provides scrolling or other browsing means for content that exceeds the display area | [#4043](https://github.com/mawww/kakoune/issues/4043) |
| R-025 | Occlusion suppression of important observation targets | The standard floating UI can adjust positioning to avoid unnecessarily occluding important observation targets such as cursor, selection, and anchor areas | [#5398](https://github.com/mawww/kakoune/issues/5398) |
| R-030 | Anchor tracking | Floating UI in `inline` style tracks and displays at the `anchor` coordinates | — |
| R-031 | Screen boundary control | Automatically adjusts the display position when floating UI exceeds the screen boundary | — |
| R-032 | Z-axis layer management | Properly manages the drawing order (Z-order) of menus, info popups, and the main buffer | — |

### 2.3 Standard Status / Prompt UI

Kasane guarantees display, context reflection, and standard readability for its standard status/prompt UI elements.

| ID | Requirement | Description | Related Issue |
|----|-------------|-------------|---------------|
| R-060 | Status bar rendering | Rendering of prompt, content, and mode line based on `draw_status` | — |
| R-061 | Status bar position | Status bar display position is configurable as top or bottom | [#235](https://github.com/mawww/kakoune/issues/235) |
| R-063 | Markup rendering | Parses and renders `{Face}` markup syntax within status lines | [#4507](https://github.com/mawww/kakoune/issues/4507) |
| R-064 | Cursor count badge | Displays cursor count in the status bar when multiple cursors/selections are present | [#5425](https://github.com/mawww/kakoune/issues/5425) |
| R-065 | Per-pane status bar | In multi-pane mode, each pane displays its own status bar reflecting the pane's independent Kakoune client state (mode, prompt, status line) | — |

### 2.4 Compatibility and Ease of Adoption

Kasane prioritizes being adoptable by existing Kakoune users as a replacement for standard `kak`, while providing advanced extensibility capabilities. The requirements defined in this section are not Kasane-specific advanced features, but rather compatibility and maintainability requirements for establishing Kasane as a viable standard frontend candidate.

| ID | Requirement | Description |
|----|-------------|-------------|
| R-100 | Existing configuration compatibility | Kasane is coherent with Kakoune's normal configuration loading paths and does not impede workflows that assume existing `kakrc` / autoload configurations |
| R-101 | Existing plugin compatibility | Kasane does not require Kasane-specific APIs, and prioritizes the operability of existing plugins that use only standard Kakoune mechanisms |
| R-102 | Session workflow compatibility | Kasane operates coherently with Kakoune's existing startup, connection, and session usage workflows |
| R-103 | Conservative defaults | Kasane's default UI does not impose unexpected large-scale restructuring or persistent UI additions on existing Kakoune users |
| R-104 | Optionality of advanced features | Kasane's proprietary plugins, extended UI, and additional capabilities are not mandatory prerequisites for normal usage |
| R-105 | Incremental enhancement | Kasane uses compatible standard behavior as its foundation, upon which opt-in advanced features can be layered |
| R-106 | Representative workflow preservation | Kasane prioritizes handling representative workflows such as plugin managers, LSP clients, and tmux/SSH without becoming an adoption barrier |

### 2.5 Input Handling

Kasane accurately receives keyboard, mouse, scroll, and other input, and dispatches them coherently to Kakoune and the standard UI.

| ID | Requirement | Description | Related Issue |
|----|-------------|-------------|---------------|
| R-040 | Keyboard input | Converts all key input to Kakoune's key format and sends it | [#4616](https://github.com/mawww/kakoune/issues/4616), [#4834](https://github.com/mawww/kakoune/issues/4834) |
| R-041 | Modifier keys | Accurate parsing of Control (`c-`), Alt (`a-`), and Shift (`s-`) modifier keys | — |
| R-042 | Special keys | Support for `<ret>`, `<esc>`, `<tab>`, `<backspace>`, `<del>`, function keys, etc. | — |
| R-043 | Mouse clicks | Sending `mouse_press` / `mouse_release` events (left, middle, right). Accurate coordinate mapping | [#4030](https://github.com/mawww/kakoune/issues/4030) |
| R-044 | Mouse movement | Sending `mouse_move` events | — |
| R-045 | Scrolling | Sending scroll events via mouse wheel. Configurable scroll speed | [#4155](https://github.com/mawww/kakoune/issues/4155) |
| R-046 | Scrolling during selection | Correctly extends the selection range when scrolling with the mouse wheel during text selection | [#2051](https://github.com/mawww/kakoune/issues/2051) |
| R-047 | Right-click drag | Extends the selection range via right-click drag | [#5339](https://github.com/mawww/kakoune/issues/5339) |

### 2.6 Cursor and Text Decoration

Kasane faithfully visualizes cursors, selections, underlines, strikethrough, and other observed editing states and decorations.

| ID | Requirement | Description | Related Issue |
|----|-------------|-------------|---------------|
| R-050 | Multi-cursor rendering | Software rendering of all cursors (primary/secondary) | [#5377](https://github.com/mawww/kakoune/issues/5377) |
| R-051 | Focus-linked cursor | Switches cursor to outline style upon window focus loss | [#3652](https://github.com/mawww/kakoune/issues/3652) |
| R-053 | Faithful text decoration rendering | Faithfully renders underline types, underline colors, strikethrough, and other text decorations sent by Kakoune, to the extent the backend permits | [#4138](https://github.com/mawww/kakoune/issues/4138) |

### 2.7 UI Options and Refresh

Kasane receives UI option changes and redraw requests from Kakoune and reflects them in the standard UI and rendering state.

| ID | Requirement | Description | Related Issue |
|----|-------------|-------------|---------------|
| R-070 | UI option reception | Receives `set_ui_options` messages and reflects them in rendering | — |
| R-071 | Refresh | Screen redraw based on `refresh` messages (normal/forced) | — |

### 2.8 Clipboard Integration

Kasane integrates directly with the system clipboard, providing low-latency and accurate copy/paste.

| ID | Requirement | Description | Related Issue |
|----|-------------|-------------|---------------|
| R-080 | System clipboard integration | Direct access to the system clipboard API for copy/paste without external processes (xclip/xsel) | [#3935](https://github.com/mawww/kakoune/issues/3935), [#4620](https://github.com/mawww/kakoune/issues/4620) |
| R-081 | Fast paste | Instant paste without launching external processes. No delay even with large text | [#1743](https://github.com/mawww/kakoune/issues/1743) |
| R-082 | Accurate special character handling | Handles newlines and special characters in clipboard content without shell escaping issues | [#4497](https://github.com/mawww/kakoune/issues/4497) |

### 2.9 Scrolling

Kasane guarantees correctness and user experience for viewport movement, page movement, scrolloff, and standard scrolling behavior.

| ID | Requirement | Description | Related Issue |
|----|-------------|-------------|---------------|
| R-090 | Smooth scrolling | Pixel-level smooth scrolling / inertial scrolling (optional) | [#4028](https://github.com/mawww/kakoune/issues/4028) |
| R-091 | Accurate scrolloff handling | Correctly handles boundary conditions with high scrolloff values, allowing the cursor to reach the first/last line | [#4027](https://github.com/mawww/kakoune/issues/4027) |
| R-092 | Display-line-aware page scrolling | Accurately accounts for soft-wrapped display lines in PageUp/PageDown calculations | [#1517](https://github.com/mawww/kakoune/issues/1517) |
| R-093 | Unnecessary scroll suppression | Suppresses unnecessary scrolling when the target line is already within the viewport | [#3951](https://github.com/mawww/kakoune/issues/3951) |

### 2.10 Standard UI Style System

Kasane composes its standard UI elements on a consistent theme / style token / container style foundation.

| ID | Requirement | Description | Related Issue |
|----|-------------|-------------|---------------|
| R-028 | Common style system | Standard UI elements such as menus, info, and key hints share a common style token / theme system | [#2676](https://github.com/mawww/kakoune/issues/2676), [#3944](https://github.com/mawww/kakoune/issues/3944) |

---

## 3. Extension Foundation Requirements

This chapter defines the extension foundation capabilities that Kasane must guarantee to enable external plugins and future standard UI extensions. The requirements in this chapter do not mean that Kasane itself provides specific concrete features as standard. What this chapter defines are capabilities — UI composition, auxiliary regions, interactivity, display transforms, display units, multiple surfaces, extended styling, and so on — that make it possible to implement concrete features. Representative features and demands realized on top of these foundations are covered in [4. Validation Targets and Representative Use Cases](#4-validation-targets-and-representative-use-cases).

### 3.1 UI Composition and Layers

Kasane provides a foundation for overlaying and coexisting multiple visual elements — including standard UI and plugin UI — on a common composition model and layer rules.

| ID | Requirement | Description |
|----|-------------|-------------|
| P-001 | Overlay composition | Overlays independent rendering layers on the buffer, allowing standard UI and plugin UI to participate in the same composition model |
| P-002 | Viewport-relative positioning | Can express positioning of overlays / markers / plugin UI relative to viewport coordinates |
| P-003 | Layer order and visibility | Each UI element can have independent Z-order, visibility, and clipping rules |

### 3.2 Auxiliary Regions and Extension Slots

Kasane provides auxiliary and peripheral regions beyond the main text, along with extension slots that enable plugins to meaningfully contribute to them.

| ID | Requirement | Description |
|----|-------------|-------------|
| P-010 | Auxiliary region model | Can express auxiliary regions beyond the main text (gutters, right-side regions, peripheral regions, etc.) |
| P-011 | Region contribution | Plugins can contribute custom display elements to auxiliary regions |
| P-012 | Region-to-source/viewport correspondence | Elements in auxiliary regions can be mapped to source, viewport, and document-wide positions |

### 3.3 Interactivity and Event Dispatch

Kasane provides an interactivity foundation where any UI element — including plugin-defined elements — can be a target for hit testing, focus, click, drag, wheel, drop, and other interactions.

| ID | Requirement | Description |
|----|-------------|-------------|
| P-020 | Interactive elements | Can express any UI element as a target for hit testing, hover, click, drag, wheel, and focus |
| P-021 | Event routing | Can dispatch native input events to the appropriate target |
| P-022 | Semantic recognizer and binding | Plugins can define custom semantic regions and event bindings |
| P-023 | Native drop event | Can dispatch OS-originated drop events to UI elements or plugins |

### 3.4 Display Transform and Restructuring

Kasane provides a foundation for defining elision, proxy display, supplementary display, and restructuring — on the premise that the Observed State is not falsified but treated as display policy.

| ID | Requirement | Description |
|----|-------------|-------------|
| P-030 | Display transform definition | Can define and apply display transforms on the Observed State |
| P-031 | Transform types | Display transforms can include elision, proxy display, supplementary display, and restructuring |
| P-032 | Separation of fact and display policy | Display transforms are treated as display policy, not as falsification of the Observed State |
| P-033 | Plugin-defined transforms | Plugins can define custom display transforms |
| P-034 | Explicit limited operations | Displays that do not have a complete inverse mapping to source can be expressed as limited operations or read-only |

### 3.5 Display Unit Model and Navigation

Kasane provides a foundation for expressing the restructured UI as navigable display units, supporting movement, selection, hit testing, and source mapping.

| ID | Requirement | Description |
|----|-------------|-------------|
| P-040 | Display unit model | Provides a model for expressing display units of the restructured UI |
| P-041 | Geometry and source mapping | Display units can have geometry, semantic role, source mapping, and interaction policy |
| P-042 | Operations on display units | Supports movement, selection, hit testing, and focus management on display units |
| P-043 | Plugin-defined navigation | Plugins can define custom display units and navigation policies |

### 3.6 Multiple Surface / Workspace / Pane Abstraction

Kasane provides an abstraction for holding and arranging multiple surfaces or panes, enabling plugins to build custom workspace management models.

| ID | Requirement | Description |
|----|-------------|-------------|
| P-050 | Multiple surface holding | Can simultaneously hold and arrange multiple surfaces / panes |
| P-051 | Focus and input routing | Can handle focus, visibility, and input dispatch between surfaces |
| P-052 | Layout abstraction | Plugins can build custom pane / workspace / tab management models |

### 3.7 Extended Styling and Text Representation

Kasane provides a foundation for richer styling and region-specific text representation beyond the direct expressions of the Kakoune protocol, available for common use by both standard UI and plugin UI.

| ID | Requirement | Description |
|----|-------------|-------------|
| P-060 | Richer styling | Can have decorations and visual expressions beyond protocol-derived ones |
| P-061 | Semantic style token | Styles can be treated as semantic tokens according to role, focus, context, etc. |
| P-062 | Region-specific text rendering policy | Different text rendering policies can be applied per region |

---

## 4. Validation Targets and Representative Use Cases

This chapter presents representative use cases that demonstrate the feasibility enabled by [3. Extension Foundation Requirements](#3-extension-foundation-requirements). These are not a list of features that Kasane must provide as standard, but rather targets for validating and explaining the expressiveness and applicability of the foundation. Each use case may be provided as standard or implemented as an external plugin. Related issues are referenced as evidence of real-world demands that Kasane can address.

### 4.1 Auxiliary Displays on the Buffer

| Use Case | Overview | Required Foundation Capabilities | Delivery Form | Related Issue |
|----------|----------|----------------------------------|---------------|---------------|
| Gutter icons | Renders code action, error, git diff, and other icons in the line number gutter | P-010, P-011, P-020 | Expected as external plugin | [#4387](https://github.com/mawww/kakoune/issues/4387) |
| Indent guides | Displays indent levels and current scope with thin sub-pixel vertical lines | P-001, P-060 | Expected as external plugin | [#2323](https://github.com/mawww/kakoune/issues/2323), [#3937](https://github.com/mawww/kakoune/issues/3937) |
| Clickable links | Makes URLs in info boxes and buffers clickable with hover effects | P-020, P-021, P-022 | Expected as external plugin | [#4316](https://github.com/mawww/kakoune/issues/4316) |
| Extended selection display | Highlights selections containing newlines across the full window width | P-030, P-060 | Expected as external plugin | [#1909](https://github.com/mawww/kakoune/issues/1909) |

### 4.2 Navigation Assistance UI

| Use Case | Overview | Required Foundation Capabilities | Delivery Form | Related Issue |
|----------|----------|----------------------------------|---------------|---------------|
| Scrollbar | Displays a scrollbar with a proportional handle, providing click/drag operations | P-010, P-012, P-020 | Expected as external plugin | [#165](https://github.com/mawww/kakoune/issues/165), [PR #5304](https://github.com/mawww/kakoune/pull/5304) |
| Scrollbar annotations | Displays markers for search results, errors, and selection positions on the scrollbar | P-010, P-012, P-020 | Expected as external plugin | [#2727](https://github.com/mawww/kakoune/issues/2727) |
| Code folding | Provides display-level line folding, gutter open/close UI, and click-to-expand | P-030, P-040, P-020, P-010 | Expected as external plugin | [#453](https://github.com/mawww/kakoune/issues/453) |
| Display line navigation | Provides `gj/gk`-equivalent movement for soft-wrapped or restructured display units | P-040, P-042, P-043 | Expected as external plugin | [#5163](https://github.com/mawww/kakoune/issues/5163), [#1425](https://github.com/mawww/kakoune/issues/1425), [#3649](https://github.com/mawww/kakoune/issues/3649) |

### 4.3 Multiple View / Workspace UI

| Use Case | Overview | Required Foundation Capabilities | Delivery Form | Related Issue |
|----------|----------|----------------------------------|---------------|---------------|
| Built-in splits | Builds horizontal/vertical splits and arbitrary layouts without depending on tmux/WM | P-050, P-051, P-052 | Expected as external plugin | [#1363](https://github.com/mawww/kakoune/issues/1363) |
| Floating panels | Displays independent surfaces such as file pickers and terminals as floating panels | P-001, P-050, P-051 | Expected as external plugin | [#3878](https://github.com/mawww/kakoune/issues/3878) |
| Focus visual feedback | Provides visual differentiation between focused and unfocused surfaces | P-051, P-061 | Standard or external plugin | [#3942](https://github.com/mawww/kakoune/issues/3942), [#3652](https://github.com/mawww/kakoune/issues/3652) |

### 4.4 External Input and Interaction

| Use Case | Overview | Required Foundation Capabilities | Delivery Form | Related Issue |
|----------|----------|----------------------------------|---------------|---------------|
| File drag & drop | Opens a buffer by dropping files from a GUI file manager | P-023, P-021 | Standard + future extension | [#3928](https://github.com/mawww/kakoune/issues/3928) |
| URL detection | Independently detects URLs in the buffer and treats them as interactive regions | P-022, P-020 | Expected as external plugin | [#4135](https://github.com/mawww/kakoune/issues/4135) |

### 4.5 Advanced Text Representation

| Use Case | Overview | Required Foundation Capabilities | Delivery Form | Related Issue |
|----------|----------|----------------------------------|---------------|---------------|
| Plugin UI using Kasane-specific decorations | Applies richer decorations independent of the protocol to plugin UI | P-060, P-061 | Expected as external plugin | [#4138](https://github.com/mawww/kakoune/issues/4138) |
| Region-specific font sizing | Applies region-specific text policies such as smaller inlay hints and larger headings | P-062, P-060 | Expected as external plugin | [#5295](https://github.com/mawww/kakoune/issues/5295) |

---

## 5. Non-Functional Requirements

### 5.1 Performance

| ID | Requirement | Target | Related Issue |
|----|-------------|--------|---------------|
| NF-001 | Rendering latency | 16ms or less from receiving a draw command from Kakoune to screen reflection (equivalent to 60fps) | [#1307](https://github.com/mawww/kakoune/issues/1307) |
| NF-002 | Input latency | 1ms or less from key input to sending to Kakoune | — |
| NF-003 | Memory usage | Minimize memory consumption during normal usage | — |
| NF-004 | Localized redrawing | Suppress redraw scope according to the changed region | — |
| NF-005 | Asynchronous I/O | Process communication with Kakoune non-blockingly | — |
| NF-006 | Visual stability under high-frequency updates | Suppress visual flashes and unnecessary intermediate state display even during high-frequency continuous updates (e.g., macro playback) | [#1491](https://github.com/mawww/kakoune/issues/1491) |

### 5.2 UI/UX

| ID | Requirement | Description | Related Issue |
|----|-------------|-------------|---------------|
| NF-012 | Flicker elimination | Zero flicker through double buffering | [#3429](https://github.com/mawww/kakoune/issues/3429) |
| NF-013 | Unicode support | Accurately calculates Unicode character widths (fullwidth/halfwidth/emoji) and performs alignment | [#3598](https://github.com/mawww/kakoune/issues/3598) |
| NF-014 | True Color | 24-bit True Color (RGB) support. Independent of terminal palette | [#3554](https://github.com/mawww/kakoune/issues/3554) |
| NF-015 | Kakoune compatibility | Maintains the same operational feel as the standard terminal UI | — |
| NF-016 | Terminal independence | No dependency whatsoever on terminal escape sequences or terminfo | [#4079](https://github.com/mawww/kakoune/issues/4079), [#3705](https://github.com/mawww/kakoune/issues/3705), [#4260](https://github.com/mawww/kakoune/issues/4260) |

### 5.3 Correctness and Extensibility

| ID | Requirement | Description | Related Issue |
|----|-------------|-------------|---------------|
| NF-020 | Semantic consistency across backends | For the same state, TUI and GUI display UI with the same meaning | — |
| NF-021 | Observational equivalence of optimized rendering paths | Fast paths using incremental rendering or caching are observationally equivalent to reference rendering under the documented redraw policy | — |
| NF-022 | Extension boundary preservation | Plugins can extend UI and interactions, but must not destroy the facts given by the protocol or the core's state transitions | — |
| NF-023 | Explicit degraded behavior | When the upstream protocol does not provide necessary information, Kasane may perform estimation or limited display, but must not treat such results as equivalent to protocol facts | — |

---

## 6. Upstream Dependencies and Degraded Behaviors

This chapter presents items among Kasane's targeted capabilities that currently cannot be fully guaranteed due to insufficient upstream protocol information or behavioral constraints, and are treated as limited implementations or heuristic degraded behaviors. Items in this chapter are not non-goals, but cannot be strictly promised as core functional requirements under the current protocol. Kasane may provide useful fallbacks for these items where possible, but must not treat such results as equivalent to facts provided by the upstream. When upstream improvements satisfy the necessary conditions, these items may be reintegrated into core requirements or extension foundation requirements.

| ID | Item | Current Treatment | Related Issue |
|----|------|-------------------|---------------|
| D-001 | Retention of startup info | Retaining and re-displaying info received at startup is useful, but the implementation approach depends on Kakoune's startup behavior | [#5294](https://github.com/mawww/kakoune/issues/5294) |
| D-002 | Off-screen cursor / selection auxiliary display | Completeness of off-screen information depends on the information provided by the upstream protocol, so this is treated as a limited fallback rather than a full guarantee | [#2727](https://github.com/mawww/kakoune/issues/2727), [#5425](https://github.com/mawww/kakoune/issues/5425) |
| D-003 | Status line context estimation | Distinguishing command/search/info messages from `draw_status` alone is treated as a heuristic fallback | [#5428](https://github.com/mawww/kakoune/issues/5428) |
| D-004 | Completeness of right-side navigation UI | Scrollbar and document-wide position UI may not guarantee full accuracy with the information available in the current protocol | [#165](https://github.com/mawww/kakoune/issues/165), [PR #5304](https://github.com/mawww/kakoune/pull/5304), [#2727](https://github.com/mawww/kakoune/issues/2727) |

---

## 7. Known Constraints

> For detailed analysis of each constraint (implementation distortions and protocol limitations), see [Kakoune Protocol Constraint Analysis](./kakoune-protocol-constraints.md).

| ID | Constraint | Impact | Workaround |
|----|------------|--------|------------|
| C-001 | No clipboard integration | No clipboard events exist in the protocol | Frontend directly accesses the system clipboard API (R-080) |
| C-002 | No character width information | Atoms do not contain display width information | Frontend implements custom Unicode width calculation (R-008) |
| C-003 | No option change notification | Kakoune-side option changes are not notified in real time | Periodic polling of `set_ui_options` or re-acquisition on `refresh` triggers |
| C-004 | Mouse modifier keys not supported | Cannot attach Ctrl/Alt modifier keys to mouse events | Frontend handles Ctrl+click etc. independently |
| C-005 | Positional parameters only | JSON-RPC supports only positional parameters | Parser handles positional parameters accurately |
| C-006 | Status line context unknown | Cannot distinguish command/search/message | Heuristic estimation (D-003). Tracking upstream [#5428](https://github.com/mawww/kakoune/issues/5428) resolution |
| C-007 | No incremental draw | All display lines are sent every time | Frontend-side diff detection (NF-004). Tracking upstream [#4686](https://github.com/mawww/kakoune/issues/4686) resolution |
| C-008 | Atom type unknown | Cannot distinguish line numbers/virtual text/code | Face name-based heuristics. Tracking upstream [#4687](https://github.com/mawww/kakoune/issues/4687), [PR #4707](https://github.com/mawww/kakoune/pull/4707) |

## 8. Related Documents

- [roadmap.md](./roadmap.md) — Implementation status and phases
- [semantics.md](./semantics.md) — Current semantics
- [roadmap.md](./roadmap.md) — Implementation order and outstanding items
- [upstream-dependencies.md](./upstream-dependencies.md) — Upstream blockers
