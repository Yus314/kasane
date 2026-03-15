# Kakoune Issue/PR Investigation Report — Evidence Base for Validation Use Cases

## Investigation Overview

We investigated GitHub Issues and PRs from Kakoune (mawww/kakoune), and organized the problem areas that serve as demand evidence for Kasane's validation targets and representative use cases. Over 100 Issues/PRs were analyzed and categorized below.

> **Note:** This document is not a "list of features that Kasane itself should implement directly." The Issues listed here contain a mix of demand evidence for: features Kasane directly provides as core functionality, features Kasane enables as an extension platform, and features that remain in degraded state due to upstream dependencies. For the authoritative requirements, see [requirements.md](./requirements.md); for status tracking, see [requirements-traceability.md](./requirements-traceability.md); for semantics, see [semantics.md](./semantics.md).

### How to Read This Document

- Read Issues as evidence of demand, not as requirements themselves
- Each category primarily corresponds to the representative use case groups in [requirements.md](./requirements.md#4-validation-targets-and-representative-use-cases)
- Whether a specific feature is standard Kasane core or expected as an external plugin follows the classification in [requirements.md](./requirements.md)
- Items with strong upstream dependencies use [upstream-dependencies.md](./upstream-dependencies.md) as the authoritative source

---

## 1. Floating Windows and Popups

Primarily demand evidence for `2.2 Standard Floating UI`, with some items also connecting to the representative use cases of `3.1 UI Composition and Layers` and `3.7 Extended Styling`.

### 1.1 Info Popup Limitations

| Issue | Title | Status | Summary |
|-------|-------|--------|---------|
| [#1516](https://github.com/mawww/kakoune/issues/1516) | Simultaneous display of multiple info boxes | OPEN | Only one info box can be displayed at a time. Lint errors and LSP hover overwrite each other |
| [#4043](https://github.com/mawww/kakoune/issues/4043) | Scrollable info box | OPEN | Long LSP hover documentation is truncated. No means to scroll |
| [#5398](https://github.com/mawww/kakoune/issues/5398) | Popup obscures selection | OPEN | Info popup covers the selection, making it impossible to see what is selected |
| [#5294](https://github.com/mawww/kakoune/issues/5294) | Cannot display info at startup | OPEN | `info -style modal` is ignored in kakrc / KakBegin hooks |
| [#3944](https://github.com/mawww/kakoune/issues/3944) | Info window border clashes with color scheme | OPEN | Cannot change or disable border color |
| [#2676](https://github.com/mawww/kakoune/issues/2676) | Visual inconsistency between menu and info | OPEN | User mode key hints and command mode menu have different appearances |

**Positioning in Kasane:**
- Simultaneous rendering of multiple floating windows using Element `Stack` + `Overlay` with Z-axis layer management
- Scrollable popups realized through the `Scrollable` Element
- `OverlayAnchor::AnchorPoint`'s collision avoidance logic (avoid) prevents overlap with selections
- Startup info is useful, but currently tracked as a degraded item in [upstream-dependencies.md](./upstream-dependencies.md)
- Unified design via `Container` Element border/shadow properties + semantic style tokens
- Plugins can fully customize info popup display via `Replacement(InfoPrompt)`

### 1.2 Completion Menu Limitations

| Issue | Title | Status | Summary |
|-------|-------|--------|---------|
| [#3938](https://github.com/mawww/kakoune/issues/3938) | Completion menu display position change | OPEN | Covers the code below the cursor. Want to display elsewhere |
| [#4396](https://github.com/mawww/kakoune/issues/4396) | Filtering of `:menu` candidates | OPEN | Cannot narrow down numerous candidates such as code actions via fuzzy search |
| [#5068](https://github.com/mawww/kakoune/issues/5068) | Completion preview (without buffer write) | OPEN | Selecting a completion candidate writes to the buffer. Want preview only |
| [#5277](https://github.com/mawww/kakoune/issues/5277) | Auto-selection of first completion list item | OPEN | No option to auto-select the first candidate |
| [#5410](https://github.com/mawww/kakoune/issues/5410) | Way to dismiss prompt completion | OPEN | Cannot dismiss completion during prompt input |
| [#1491](https://github.com/mawww/kakoune/issues/1491) | Completion menu flash during macro execution | OPEN | Menu briefly appears during macro playback |
| [#2170](https://github.com/mawww/kakoune/issues/2170) | Search completion as dropdown display | OPEN | Search candidates displayed side by side on the prompt line are hard to read |
| [#1531](https://github.com/mawww/kakoune/issues/1531) | Horizontal completion display at screen bottom | OPEN | Want horizontal display like command-line completion |

**Positioning in Kasane:**
- Freely adjustable completion menu display position via `OverlayAnchor` settings
- Plugins can fully replace menus with fzf-style etc. via `Replacement(MenuPrompt/MenuInline/MenuSearch)`
- Completion preview display by inserting ghost text Elements into `Slot::Overlay`
- Auto-selection and completion dismiss modes controlled by referencing configuration in TEA's `update()`
- Popup flash suppression during macro playback via event batching (try_recv)
- Switchable dropdown/horizontal completion display using `Grid` Element

---

## 2. Rendering and Display Quality

Rendering issues caused by terminal dependency that can be fundamentally resolved through custom rendering.

### 2.1 Flickering and Redraw Issues

| Issue | Title | Status | Summary |
|-------|-------|--------|---------|
| [#3429](https://github.com/mawww/kakoune/issues/3429) | Screen flickering | CLOSED | Intermediate states visible during differential scroll optimization with relative line numbers |
| [#4320](https://github.com/mawww/kakoune/issues/4320) | Flickering (line position shift) | CLOSED | Malfunction due to synchronized output detection bug |
| [#4317](https://github.com/mawww/kakoune/issues/4317) | Visual glitches on Linux console | CLOSED | Linux console displays all intermediate rendering states |
| [#3185](https://github.com/mawww/kakoune/issues/3185) | Inconsistent redraw on st terminal | OPEN | Rendering issues due to terminfo database mismatch |
| [#4689](https://github.com/mawww/kakoune/issues/4689) | Redraw issues inside aerc | OPEN | Terminal compatibility issues when embedded in another application |

**Kasane's solution:**
- Atomic frame rendering via double buffering. No intermediate states are ever displayed
- Complete elimination of dependency on terminal escape sequences
- No need for terminfo or synchronized output protocol

### 2.2 Color and Color Scheme Issues

| Issue | Title | Status | Summary |
|-------|-------|--------|---------|
| [#3554](https://github.com/mawww/kakoune/issues/3554) | Poor contrast in default theme | OPEN | Appearance varies greatly depending on terminal palette |
| [#2842](https://github.com/mawww/kakoune/issues/2842) | Text markup broken in Solarized | OPEN | Color mismatch caused by bold-as-bright behavior |
| [#4193](https://github.com/mawww/kakoune/issues/4193) | Blank screen with tmux + solarized | OPEN | Depends on tmux color settings |
| [#3763](https://github.com/mawww/kakoune/issues/3763) | Incorrect color calculation | CLOSED | 256-color approximation when True Color is unavailable |

**Kasane's solution:**
- Native 24-bit RGB color rendering. No palette approximation needed
- Bold and color handled completely independently (bold-as-bright issue does not occur)
- Direct rendering independent of multiplexers such as tmux
- Ships with a default theme that guarantees consistent color rendering

### 2.3 Unicode / CJK / Emoji Display Issues

| Issue | Title | Status | Summary |
|-------|-------|--------|---------|
| [#3598](https://github.com/mawww/kakoune/issues/3598) | CJK character completion candidate display corruption | OPEN | Rendering collapse from double-width characters overlapping with popups |
| [#4257](https://github.com/mawww/kakoune/issues/4257) | Emoji barely works on macOS | OPEN | `iswprint()` has an outdated Unicode database |
| [#3059](https://github.com/mawww/kakoune/issues/3059) | Emoji support | CLOSED | Same as above. Kakoune depends on the system libc |
| [#1941](https://github.com/mawww/kakoune/issues/1941) | Scrollbar and info area broken with CJK widths | OPEN | Layout collapse due to character width calculation mismatch |
| [#3570](https://github.com/mawww/kakoune/issues/3570) | Zero-width characters are invisible | OPEN | U+200B etc. are invisible but affect cursor movement |
| [#2936](https://github.com/mawww/kakoune/issues/2936) | Display control characters as ^A, ^M | OPEN | Control characters are invisible and hard to identify |
| [#3364](https://github.com/mawww/kakoune/issues/3364) | UTF-8 rendering corruption | OPEN | Character corruption due to terminal encoding issues |

**Kasane's solution:**
- Accurate character width calculation using a custom Unicode text layout library
- Proper emoji display through system font fallback chains
- No dependency on libc's `iswprint()` / `wcwidth()`
- Visible display of zero-width and control characters (placeholder glyphs)
- End-to-end character data integrity through JSON (UTF-8) communication

### 2.4 Cursor Rendering

| Issue | Title | Status | Summary |
|-------|-------|--------|---------|
| [#3652](https://github.com/mawww/kakoune/issues/3652) | No cursor change when inactive | OPEN | No cursor style change on focus loss |
| [#5377](https://github.com/mawww/kakoune/issues/5377) | Kitty multi-cursor protocol | OPEN | Native cursor overlaps with UI widgets |
| [#1524](https://github.com/mawww/kakoune/issues/1524) | Cursor flickering | CLOSED | Hardware cursor appearing at random positions during draw updates |
| [#2727](https://github.com/mawww/kakoune/issues/2727) | Off-screen cursor display | CLOSED | Forgetting off-screen selections and corrupting files |

**Kasane's solution:**
- Software cursor rendering (block/bar/underline/outline)
- Automatic active/inactive cursor switching via focus tracking
- Native rendering of multiple cursors (no terminal protocol required)
- Off-screen selection indicators displayed at viewport edges

---

## 3. Terminal Compatibility Issues

A category of problems entirely eliminated by Kasane's custom rendering approach.

| Issue | Title | Status | Summary |
|-------|-------|--------|---------|
| [#4079](https://github.com/mawww/kakoune/issues/4079) | Terminal.app ignores synchronized output | CLOSED | DCS sequences displayed as text |
| [#3705](https://github.com/mawww/kakoune/issues/3705) | Screen corruption in PuTTY | CLOSED | DCS not supported |
| [#4260](https://github.com/mawww/kakoune/issues/4260) | tmux interprets italic differently | CLOSED | Escape sequence interpretation differences |
| [#4616](https://github.com/mawww/kakoune/issues/4616) | Backspace not recognized in Xterm | OPEN | Keycode differences |
| [#4834](https://github.com/mawww/kakoune/issues/4834) | Shift-Tab not working in WezTerm | OPEN | Keycode differences |
| [#1307](https://github.com/mawww/kakoune/issues/1307) | Kakoune is slow in iTerm2 | OPEN | Terminal emulator overhead |
| [#5333](https://github.com/mawww/kakoune/issues/5333) | Rendering in GNU Screen | CLOSED | GNU Screen below 5.0 does not support True Color |

**Kasane's solution:**
- All issues are automatically resolved because no terminal escape sequences are used
- Keyboard input obtained directly from the window system (no terminal keycode conversion needed)
- Elimination of terminal emulator rendering overhead

---

## 4. Window Management and Layout

| Issue | Title | Status | Summary |
|-------|-------|--------|---------|
| [#1363](https://github.com/mawww/kakoune/issues/1363) | Portable horizontal/vertical splits | OPEN | Split commands independent of tmux/WM. High demand with 29 comments |
| [#3878](https://github.com/mawww/kakoune/issues/3878) | tmux popup support | OPEN | Floating terminal for fzf etc. |
| [#3942](https://github.com/mawww/kakoune/issues/3942) | No focus/unfocus distinction in tmux | CLOSED | Cannot tell which client is active with multiple clients |

**Positioning in Kasane:**
- Built-in split/pane system constructed with `Flex` Element (draggable `Interactive` borders)
- Floating panel plugins (file picker, terminal) placed in `Slot::Overlay`
- Visual distinction between focused/unfocused via semantic style tokens

> In the current requirements framework, this category is primarily read as representative use cases for `3.6 Multi-surface / Workspace / Pane Abstraction`.

---

## 5. Virtual Text and Overlays

| Issue | Title | Status | Summary |
|-------|-------|--------|---------|
| [#1813](https://github.com/mawww/kakoune/issues/1813) | Virtual text within windows | OPEN | LSP code lens, inlay hints, inline diagnostics. 11 comments |
| [#5382](https://github.com/mawww/kakoune/issues/5382) | Inserting virtual newlines with replace-ranges | OPEN | Cannot display inline diagnostics below lines |
| [#4387](https://github.com/mawww/kakoune/issues/4387) | Code action indicator (lightbulb) | OPEN | Proposal by matklad (rust-analyzer developer). 10 comments |
| [#2323](https://github.com/mawww/kakoune/issues/2323) | Indent guides | CLOSED | Thin vertical lines cannot be drawn in terminal. 21 comments |
| [#3937](https://github.com/mawww/kakoune/issues/3937) | Indent guide lines | OPEN | Request from community survey |
| [#4316](https://github.com/mawww/kakoune/issues/4316) | Clickable links (OSC 8) | OPEN | Want to make URLs in info boxes and documentation clickable |
| [#1820](https://github.com/mawww/kakoune/issues/1820) | Window-relative highlights | OPEN | Needed for overlay features such as easymotion |
| [#1909](https://github.com/mawww/kakoune/issues/1909) | Extend selection display to end of line | OPEN | Selections including newline characters are hard to see |

**Positioning in Kasane:**
- Virtual text overlaid on buffer using `Slot::Overlay` + `Stack` Element
- Plugins add code lens and inlay-type annotation layers via `Decorator(Buffer)`
- Gutter icon plugins (lightbulb, error/warning, git diff) inserted into `Slot::BufferLeft`
- Sub-pixel indent guide line rendering in the GUI backend
- Clickable hyperlinks via `Interactive` Element (hit test using InteractiveId)
- Viewport-relative overlays (easymotion etc.) via `OverlayAnchor::Absolute`
- Window-width extended selection display via `Decorator(BufferLine)`

> In the current requirements framework, this category corresponds to `3.1 UI Composition and Layers`, `3.2 Auxiliary Regions and Extension Slots`, `3.3 Interactivity and Event Dispatch`, and `4.1 Auxiliary Display on Buffer`.

---

## 6. Scroll Behavior

| Issue | Title | Status | Summary |
|-------|-------|--------|---------|
| [#4028](https://github.com/mawww/kakoune/issues/4028) | Uneven mouse scroll with high scrolloff | OPEN | Scroll amount inconsistency |
| [#4027](https://github.com/mawww/kakoune/issues/4027) | Cursor cannot reach first line with high scrolloff | OPEN | Boundary condition bug |
| [#4030](https://github.com/mawww/kakoune/issues/4030) | Line shift with high scrolloff + mouse click | OPEN | Click coordinates are offset |
| [#4155](https://github.com/mawww/kakoune/issues/4155) | Scroll becomes Up/Down keys when mouse is disabled | OPEN | Incorrect event conversion |
| [#3951](https://github.com/mawww/kakoune/issues/3951) | Scroll even when target line is already visible | OPEN | Unnecessary scrolling occurs |
| [#1517](https://github.com/mawww/kakoune/issues/1517) | PageUp not working with wrapped lines | OPEN | Scroll amount calculation does not account for display lines |

**Kasane's solution:**
- Independent control of viewport scroll and cursor movement
- Pixel-level smooth scrolling / inertia scrolling
- Accurate mouse coordinate to buffer position mapping
- Page scroll calculation that accurately accounts for display lines

---

## 7. Mouse Operations

| Issue | Title | Status | Summary |
|-------|-------|--------|---------|
| [#2051](https://github.com/mawww/kakoune/issues/2051) | Cannot scroll during text selection | OPEN | Selection breaks when scrolling |
| [#5339](https://github.com/mawww/kakoune/issues/5339) | Selection extension via right-click drag | OPEN | Right-click down works but drag is unresponsive |
| [#4135](https://github.com/mawww/kakoune/issues/4135) | Whitespace display interferes with URL click detection | OPEN | `·` character breaks terminal URL detection |
| [#3928](https://github.com/mawww/kakoune/issues/3928) | Drag & drop support | OPEN | File drop from file manager |

**Kasane's solution:**
- Properly extends selection range when scrolling during drag
- Full implementation of selection extension via right-click drag
- Custom URL detection (unaffected by whitespace display)
- Native drag & drop support

---

## 8. Clipboard Integration

| Issue | Title | Status | Summary |
|-------|-------|--------|---------|
| [#3935](https://github.com/mawww/kakoune/issues/3935) | Built-in clipboard integration | OPEN | Want to eliminate dependency on xclip/xsel |
| [#4620](https://github.com/mawww/kakoune/issues/4620) | Native OSC 52 support | OPEN | Paste does not work |
| [#4497](https://github.com/mawww/kakoune/issues/4497) | Clipboard newlines and special characters | OPEN | Escaping issues via shell commands |
| [#1743](https://github.com/mawww/kakoune/issues/1743) | Slow paste from X11 clipboard | OPEN | Overhead from spawning external processes |

**Kasane's solution:**
- Direct access to the system clipboard API
- Instant copy/paste without spawning external processes
- Accurate clipboard handling of Unicode/binary data

---

## 9. Status Line and Mode Line

| Issue | Title | Status | Summary |
|-------|-------|--------|---------|
| [#5428](https://github.com/mawww/kakoune/issues/5428) | Cannot distinguish status line context in JSON UI | OPEN | Cannot distinguish between command/search/info messages |
| [#4445](https://github.com/mawww/kakoune/issues/4445) | Status line customization | OPEN | Limited access to individual components (mode, selection count, etc.) |
| [#4507](https://github.com/mawww/kakoune/issues/4507) | Markup not parsed in mode line | OPEN | `{green}text` displayed as literal |
| [#5425](https://github.com/mawww/kakoune/issues/5425) | Cursor count indicator | CLOSED | Visualization of multi-cursor state |
| [#235](https://github.com/mawww/kakoune/issues/235) | Place status line at top | CLOSED | Position is hardcoded |

**Kasane's solution (declarative UI):**
- Fully customizable status bar via `Replacement(StatusBar)` (position, layout, widgets)
- Plugins insert widgets into `Slot::StatusLeft` / `Slot::StatusRight`
- Markup parsing and rendering added via `Decorator(StatusBar)`
- Command palette / notification area separated into `Slot::AboveStatus`

---

## 10. Soft Wrap and Display Line Navigation

| Issue | Title | Status | Summary |
|-------|-------|--------|---------|
| [#5163](https://github.com/mawww/kakoune/issues/5163) | Up/down navigation in soft-wrapped text | OPEN | No equivalent to vim's `gj`/`gk`. 21 comments |
| [#1425](https://github.com/mawww/kakoune/issues/1425) | Movement by display lines | OPEN | mawww notes design challenges (off-screen multiple selections) |
| [#3649](https://github.com/mawww/kakoune/issues/3649) | Cursor navigation in soft-wrapped text | OPEN | Needed for prose editing |
| [#5328](https://github.com/mawww/kakoune/issues/5328) | Exposing buffer_display_width / split_line | OPEN | To implement display line navigation via script |

**Positioning in Kasane:**
- Since Kasane knows the exact visual layout, it can convert display line coordinates to buffer coordinates to implement `gj`/`gk`
- However, wrapping information for off-screen multiple selections requires coordination with the Kakoune side

> In the current requirements framework, this category corresponds to the representative use cases of `3.5 Display Unit Model and Navigation` and `4.2 Navigation Auxiliary UI`.

---

## 11. Code Folding, Scrollbar, and Minimap

| Issue | Title | Status | Summary |
|-------|-------|--------|---------|
| [#453](https://github.com/mawww/kakoune/issues/453) | Code folding | OPEN | Request since 2016. 27 comments |
| [#165](https://github.com/mawww/kakoune/issues/165) | Add scrollbar | CLOSED | Request for text-mode scrollbar |
| [#4014](https://github.com/mawww/kakoune/issues/4014) | Directory exploration feature | OPEN | File browser like netrw |

**Positioning in Kasane:**
- Display-level line folding plugin implemented via `Decorator(Buffer)` (gutter `Interactive` icons)
- Scrollbar plugin in `Slot::BufferRight` (`Scrollable` + annotation markers)
- Minimap plugin placed in `Slot::BufferRight`
- File tree / fuzzy finder plugin in `Slot::BufferLeft` or `Slot::Overlay`

> In the current requirements framework, this category spans the representative use cases of `3.4 Display Transformation and Restructuring`, `3.5 Display Unit Model and Navigation`, and `3.2 Auxiliary Regions and Extension Slots`, with some items involving upstream dependencies tracked in [upstream-dependencies.md](./upstream-dependencies.md).

---

## 12. Font Rendering and Text Size

| Issue | Title | Status | Summary |
|-------|-------|--------|---------|
| [#5295](https://github.com/mawww/kakoune/issues/5295) | Kitty text-sizing protocol | OPEN | Want to change font size per region |
| [#4138](https://github.com/mawww/kakoune/issues/4138) | Underline variations | CLOSED | Wavy/dotted/double line. Unstable terminal support |
| [#3946](https://github.com/mawww/kakoune/issues/3946) | Right-to-Left text support | OPEN | RTL text display |

**Kasane's solution:**
- Per-region font size (larger for headings, smaller for inlay hints)
- Consistent rendering of all underline styles (no terminal support needed)
- BiDi text rendering (future extension)

---

## 13. JSON UI Protocol Extension Proposals (Upstream Contributions)

Protocol improvements to propose to upstream Kakoune in parallel with Kasane's implementation. For detailed constraint impact analysis, see [Kakoune Protocol Constraint Analysis](./kakoune-protocol-constraints.md).

| PR/Issue | Title | Status | Summary |
|----------|-------|--------|---------|
| [PR #4707](https://github.com/mawww/kakoune/pull/4707) | Add Face names to JSON UI | OPEN | Send semantic Face names (PrimaryCursor, etc.) to the frontend |
| [#4686](https://github.com/mawww/kakoune/issues/4686) | Incremental draw notifications | OPEN | Differential sending of only changed lines |
| [#4687](https://github.com/mawww/kakoune/issues/4687) | Distinguish code from virtual text/line numbers | OPEN | Enable distinguishing Atom types |
| [#5428](https://github.com/mawww/kakoune/issues/5428) | Add status line context | OPEN | Addition of `status_style` parameter |
| [#2019](https://github.com/mawww/kakoune/issues/2019) | Summary of JSON UI limitations | OPEN | Clipboard, character width, command execution, etc. |

**Kasane's strategy:**
- First complete an implementation that works with the current protocol
- Address limitations with heuristic workarounds
- Participate in upstream PR review and feedback
- Submit new PRs as needed

---

## Priority Ranking

Comprehensive evaluation of user demand (comment count, re-request frequency) and feasibility within Kasane.

### Tier 1 — Kasane's Core Value (Direct Differentiators)

| Rank | Category | Representative Issues | Comments |
|------|----------|----------------------|----------|
| 1 | Floating windows (menu/info) | #1516, #4043, #3938, #5398 | Many |
| 2 | Elimination of flickering/redraw issues | #3429, #4320, #3185 | — |
| 3 | Proper Unicode/CJK/emoji display | #3598, #4257, #3059 | Many |
| 4 | Consistent True Color display | #3554, #2842 | 16+ |
| 5 | Complete elimination of terminal compatibility issues | #4079, #3705, #4616, etc. | — |

### Tier 2 — High-demand Feature Extensions

| Rank | Category | Representative Issues | Comments |
|------|----------|----------------------|----------|
| 6 | Built-in split management | #1363 | 29 |
| 7 | Code folding | #453 | 27 |
| 8 | Display line navigation | #5163, #1425 | 21 |
| 9 | Indent guides | #2323 | 21 |
| 10 | Clipboard integration | #3935, #4620, #1743 | Many |

### Tier 3 — UX Improvements

| Rank | Category | Representative Issues | Comments |
|------|----------|----------------------|----------|
| 11 | Virtual text/code lens | #1813, #4387 | 11, 10 |
| 12 | Status line customization | #4445, #5428 | 7 |
| 13 | Scroll behavior improvements | #4028, #4027, #1517 | — |
| 14 | Mouse operation improvements | #2051, #5339, #3928 | — |
| 15 | Scrollbar/minimap | #165, PR #5304 | — |
| 16 | Enhanced cursor rendering | #3652, #5377, #2727 | — |
| 17 | Font size/underline | #5295, #4138 | — |

---

## Existing Alternative Frontend Projects

| Project | Technology | Status | Features |
|---------|-----------|--------|----------|
| [kakoune-gtk](https://gitlab.com/Screwtapello/kakoune-gtk) | GTK | PoC | Pioneer that spawned the #2019 Issue |
| [kakoune-electron](https://github.com/Delapouite/kakoune-electron) | Electron/Canvas | Experimental | Canvas rendering |
| [Kakoune Qt](https://discuss.kakoune.com/t/announcing-kakoune-qt/2522) | Qt | Active (2024) | Splits, borders, multi-font-size |
| [kakoune-arcan](https://github.com/cipharius/kakoune-arcan) | Arcan/Zig | Experimental | Arcan display server frontend |
| [kak-ui](https://docs.rs/kak-ui/latest/kak_ui/) | Rust crate | Published | Rust wrapper for the JSON-RPC protocol |
