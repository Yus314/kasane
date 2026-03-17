# Kakoune Protocol Constraint Analysis — Impact on Kasane and Implementation Distortions

This document analyzes how the Kakoune JSON UI protocol distorts Kasane's implementation.
For tracking current upstream status and reintegration conditions, see [upstream-dependencies.md](./upstream-dependencies.md).

## 1. Overview

This document systematically analyzes the constraints that the Kakoune JSON UI protocol (`kak -ui json`) imposes on Kasane's design and implementation. Rather than merely enumerating constraints (see [requirements.md §7](./requirements.md#7-known-constraints)), its purpose is to clarify **how the constraints distort the implementation** and **which properties represent fundamental protocol limitations**.

**Related documents:**
- [Requirements §7 Known Constraints](./requirements.md#7-known-constraints) — Concise list of constraints
- [Upstream Dependencies](./upstream-dependencies.md) — Current upstream status and reintegration conditions
- [Technical Decision Records](./decisions.md) — Design decisions driven by constraints
- [Implementation Roadmap](./roadmap.md) — Kasane-side tracking and implementation order

---

## 2. Fundamental Design Philosophy of the Protocol and Its Consequences

The Kakoune JSON UI protocol is essentially **"a JSON representation of terminal escape sequences."** It is designed to send the drawing commands that Kakoune's built-in terminal UI (`terminal_ui.cc`) performs, almost verbatim, as JSON-RPC messages.

This design philosophy leads to the following consequences:

1. **Display commands only, no semantic information**: The protocol only says "draw this" and never says "this is what it is"
2. **The frontend is passive**: There is no means to actively query Kakoune for information
3. **The coordinate system is implicit**: It assumes terminal cell coordinates, with no known correspondence to buffer coordinates

Upstream Issue [#2019](https://github.com/mawww/kakoune/issues/2019) (since 2018, 7 comments) comprehensively documents this problem, with multiple frontend developers participating in the discussion, including Screwtapello of kakoune-gtk and casimir of Kakoune Qt.

---

## 3. Classification of Implementation Distortions

The distortions Kasane experiences can be classified into three layers.

### 3.1 Inference Layer — Heuristic Estimation of Information Not Provided by Kakoune

This layer estimates semantic information not conveyed by the protocol through pattern matching on display data. Since it **implicitly depends on Kakoune's internal implementation**, there is a risk of breakage without notice on version upgrades.

### 3.2 Redundant Computation Layer — Independent Recalculation Separate from Kakoune

This layer independently recalculates results that Kakoune holds internally (character widths, menu scroll positions, etc.) because they are not included in the protocol. There is a risk that **precision divergences** manifest as layout misalignment.

### 3.3 Bypass Layer — Direct Access Circumventing the Protocol

This layer implements features unsupported by the protocol through direct access to OS APIs and similar mechanisms. It **cannot synchronize** with Kakoune's internal state.

---

## 4. Detailed Analysis of the Inference Layer

### 4.1 Cursor Detection — `FINAL_FG + REVERSE` Heuristic

**Constraint:** Kakoune conveys the cursor coordinates via `cursor_pos` in the `draw` message, but does not provide the following information:
- Total number of multi-cursors
- Cursor type (Primary / Secondary)
- Cursor face name (PrimaryCursor / SecondaryCursor, etc.)

**Implementation distortion** (`kasane-core/src/state/apply.rs:13-21`):

```rust
self.cursor_count = lines
    .iter()
    .flat_map(|line| line.iter())
    .filter(|atom| {
        atom.face.attributes.contains(Attributes::FINAL_FG)
            && atom.face.attributes.contains(Attributes::REVERSE)
    })
    .count();
```

The simultaneous presence of `FINAL_FG` + `REVERSE` attributes is used as the cursor signature. This depends on the internal implementation knowledge that Kakoune's `terminal_ui.cc` sets `FINAL_FG | REVERSE` on the Atom at the cursor position.

**Impact scope:**
- R-050 (multi-cursor rendering) — Cannot distinguish Primary/Secondary
- R-064 (cursor count badge) — Functions visually but without guarantees

**Upstream resolution:** [PR #4707](https://github.com/mawww/kakoune/pull/4707) (adding semantic Face names to Atoms). However, mawww recommends the DisplayAtom flag approach in [PR #4737](https://github.com/mawww/kakoune/pull/4737), and the prospect of #4707 itself being merged is unclear.

---

### 4.2 Edit Mode Inference — String Matching on the Status Mode Line

**Constraint:** Kakoune has no message that explicitly notifies the current edit mode (normal / insert / replace).

**Implementation distortion** (`kasane-core/src/render/mod.rs:74-100`):

```rust
pub fn cursor_style(state: &AppState) -> CursorStyle {
    // 1. Explicit override via ui_option
    if let Some(style) = state.ui_options.get("kasane_cursor_style") { ... }
    // 2. On focus loss
    if !state.focused { return CursorStyle::Outline; }
    // 3. Prompt mode
    if state.cursor_mode == CursorMode::Prompt { return CursorStyle::Bar; }
    // 4. Inference via string matching on the mode line
    let mode = state.status_mode_line.iter()
        .find_map(|atom| match atom.contents.as_str() {
            "insert" => Some(CursorStyle::Bar),
            "replace" => Some(CursorStyle::Underline),
            _ => None,
        });
    mode.unwrap_or(CursorStyle::Block)
}
```

Step 4 checks whether the Atom content in the mode line matches the strings `"insert"` / `"replace"`. This breaks if the user changes `modelinefmt` to display mode names in Japanese, or if a plugin modifies the mode line.

**Mitigation:** The `kasane_cursor_style` ui_option takes highest priority as an explicit override, providing a fallback when the heuristic fails.

---

### 4.3 Status Line Context Inference (Deferred)

**Constraint:** The `draw_status` message only sends two `Line` values, `status_line` and `mode_line`, without indicating whether they represent:
- A command prompt (`:`)
- A search prompt (`/`, `?`)
- A message from `echo`
- File information display

**Impact:** D-003 (status line context inference) is treated as degraded behavior. UI branching based on prompt type (command palette-style display, search highlighting, etc.) cannot be implemented as an exact core requirement.

**Upstream:** [#5428](https://github.com/mawww/kakoune/issues/5428) — Proposal to add a `status_style` parameter to `draw_status`. Zero comments; discussion has not progressed.

**Status of other frontends:** In [#2019](https://github.com/mawww/kakoune/issues/2019), casimir attempted detection of the `:` prefix but reported low reliability.

---

### 4.4 Info Popup Identity Determination

**Constraint:** Kakoune does not assign a unique ID to info windows. `info_show` / `info_hide` assume a single stack-like operation.

**Implementation distortion** (`kasane-core/src/state/mod.rs:181-197`):

```rust
pub struct InfoIdentity {
    pub style: InfoStyle,
    pub anchor_line: u32,
}
```

The tuple `(InfoStyle, anchor_line)` is used as an approximate ID; info with the same identity is overwritten, while info with different identities coexists.

**Known collision patterns:**
- Lint errors and LSP hover information on the same line (both `Inline` style)
- Multiple `Modal` style infos (when anchor_line is the same)

**Upstream:** [#1516](https://github.com/mawww/kakoune/issues/1516) — Simultaneous display of multiple info boxes. A fundamental fix requires the introduction of info IDs on the Kakoune side.

---

## 5. Detailed Analysis of the Redundant Computation Layer

### 5.1 Independent Character Width Calculation

**Constraint (C-002):** Atoms contain only strings and carry no display width information.

**Structure of redundant computation:**

| Computing entity | Width calculation source | Usage |
|---------|------------|------|
| Kakoune | libc `wcwidth()` / `wcswidth()` | In-buffer cursor movement, line wrapping decisions, Atom splitting |
| Kasane | `unicode-width` crate + compatibility patches | Layout calculation, cell grid placement |

**Divergence risks:**
- The Unicode database of the libc version may differ from the Unicode version of the `unicode-width` crate
- Particularly the interpretation of CJK Ambiguous Width characters
- Width calculation differences for emoji sequences (ZWJ, Variation Selector)
- macOS `iswprint()` depends on an outdated Unicode database ([#4257](https://github.com/mawww/kakoune/issues/4257))

**Examples of manifestation:**
- If Kasane judges a character as 2-cell width but Kakoune treats it as 1-cell width, the cursor position shifts
- Item boundaries shift in CJK text within menus ([#3598](https://github.com/mawww/kakoune/issues/3598))

**Upstream:** In [#2019](https://github.com/mawww/kakoune/issues/2019), Screwtapello proposed that "Atoms should include expected width." mawww has not responded.

---

### 5.2 Recalculation of Menu Scroll Position

**Constraint:** Kakoune only sends the selection index via `menu_select(index)` and does not convey the scroll position.

**Implementation distortion:** Kasane ports Kakoune's `terminal_ui.cc` scroll logic to Rust in `MenuState::scroll_column_based()` and `MenuState::scroll_search()`.

```
// Reverse-engineering Kakoune's terminal_ui.cc:
// stride = win_height
// first_item = (selected / stride) * stride
```

If the logic on Kakoune's side changes, the menu scroll position will drift.

---

### 5.3 Incremental Diff Detection

**Constraint (C-007):** The `draw` message sends all display lines every time. No differential transmission of changed lines only is performed.

**Redundant computation:** As NF-004 (differential rendering), Kasane compares the previous frame's `CellGrid` with the current frame and sends only changed cells to the backend. Kakoune internally performs similar diff detection (for its terminal UI), but the results are not included in the protocol.

**Upstream:** [#4686](https://github.com/mawww/kakoune/issues/4686) — Proposal for incremental draw notifications. Zero comments.

---

## 6. Detailed Analysis of the Bypass Layer

### 6.1 Clipboard

**Constraint (C-001):** No clipboard-related messages exist in the protocol.

**Bypass:** Direct access to the system clipboard API via the `arboard` crate (R-080).

**Synchronization problems:**
- Kakoune's yank register (`"`) and Kasane's clipboard are independent
- Content yanked with `y` within Kakoune is not reflected in Kasane's clipboard
- Content pasted via Kasane does not enter Kakoune's `"` register

In [#2019](https://github.com/mawww/kakoune/issues/2019), Screwtapello enumerated five clipboard integration scenarios and pointed out that all of them are impossible with the JSON UI protocol.

**Upstream:** [#3935](https://github.com/mawww/kakoune/issues/3935) — Request for built-in clipboard integration.

---

### 6.2 Mouse Modifier Keys

**Constraint (C-004):** The protocol message for mouse events has no modifier key field.

```rust
// Mouse event in the protocol:
MousePress { button: String, line: u32, column: u32 }
// ← No Ctrl/Alt/Shift information
```

**Bypass:** Kasane inspects the OS key state when receiving mouse events and handles `Ctrl+Click` etc. independently on the frontend side. There is no means to convey this modifier key information to Kakoune.

---

## 7. Operations Fundamentally Impossible via the Protocol

The following are operations that are **impossible** for Kasane to perform due to the protocol's design.

| Operation | Current alternative | Limitation |
|------|-------------|------|
| Command execution (`evaluate-commands`) | Simulating key input via `keys` messages | Difficult to issue complex commands. Cannot obtain execution results |
| Buffer metadata retrieval | None | No means to know file path, modification state, or list of open buffers |
| Register monitoring | None | Cannot detect content changes from yank/delete |
| Arbitrary range retrieval of buffer contents | None | Can only access the portion displayed on screen |
| Viewport position retrieval | Only sending `resize` messages | Unknown which line of the buffer display starts from |
| Command execution response confirmation | None | No ACK for `keys` transmission (fire-and-forget) |
| Active retrieval of option values | Waiting for `set_ui_options` reception | Cannot query the value of a specific option |

---

## 8. Impact Matrix

This section organizes which Kasane features each constraint blocks.

| Constraint | Blocked feature | Distortion layer | Severity |
|------|---------------------|---------|-------|
| No cursor type distinction | R-050 multi-cursor rendering | Inference | **High** — If broken, all cursor rendering collapses |
| No edit mode notification | Automatic cursor style switching | Inference | Medium — ui_option fallback available |
| No status context | D-003 status line context inference | Inference | Medium — Degraded behavior |
| No info ID | Accurate management of multiple infos | Inference | Low — Collisions are rare cases |
| No character width information | All text layout | Redundant computation | **High** — Divergence manifests as cursor position shifts |
| No scroll position | Menu display | Redundant computation | Medium — Implemented but can break on Kakoune changes |
| No incremental draw | Performance | Redundant computation | Low — Maintains 60fps in current state |
| No clipboard notification | Clipboard synchronization | Bypass | Medium — One direction functions |
| No mouse modifier keys | Ctrl+Click etc. | Bypass | Low — Can be handled on the frontend side |
| No command execution RPC | Buffer operation abstraction | Fundamentally impossible | **High** — No alternative means |
| No viewport position | D-002 off-screen cursor / selection range auxiliary display, P-040–P-043 series | Fundamentally impossible | **High** — Cannot be obtained with current protocol |

---

## Appendix A: Approaches Taken by Other Frontend Projects

| Project | Technology | Approach to constraints |
|------------|------|------------|
| [kakoune-gtk](https://gitlab.com/Screwtapello/kakoune-gtk) | GTK | Led discussion on #2019. Requested protocol improvements upstream |
| [Kakoune Qt](https://discuss.kakoune.com/t/announcing-kakoune-qt/2522) | Qt | Independently implemented splitting, borders, and multi-font sizes |
| [kak-ui](https://docs.rs/kak-ui/latest/kak_ui/) | Rust crate | Protocol wrapper only. Constraints are left to consumers |

---

## Appendix B: Upstream Issue Cross-Reference

Complete list of upstream Issues/PRs referenced in this document.

| Number | Title | Section referenced |
|------|---------|----------------|
| [#2019](https://github.com/mawww/kakoune/issues/2019) | JSON UI limitations summary | §2, §4.3, §5.1, §6.1 |
| [#5428](https://github.com/mawww/kakoune/issues/5428) | Status line context | §4.3 |
| [#4686](https://github.com/mawww/kakoune/issues/4686) | Incremental draw notifications | §5.3 |
| [#4687](https://github.com/mawww/kakoune/issues/4687) | Atom type distinction | [upstream-dependencies.md](./upstream-dependencies.md) |
| [#1516](https://github.com/mawww/kakoune/issues/1516) | Simultaneous display of multiple info boxes | §4.4 |
| [#3935](https://github.com/mawww/kakoune/issues/3935) | Built-in clipboard integration | §6.1 |
| [#3598](https://github.com/mawww/kakoune/issues/3598) | CJK character completion candidate display corruption | §5.1 |
| [#4257](https://github.com/mawww/kakoune/issues/4257) | macOS emoji issue | §5.1 |
| [PR #4707](https://github.com/mawww/kakoune/pull/4707) | Add Face names to JSON UI | §4.1, [upstream-dependencies.md](./upstream-dependencies.md) |
| [PR #4737](https://github.com/mawww/kakoune/pull/4737) | Add DisplaySetup to draw | §4.1 |

## Related Documents

- [upstream-dependencies.md](./upstream-dependencies.md) — Current upstream status and reintegration conditions
- [requirements.md](./requirements.md) — Authoritative source for core requirements / extension infrastructure / constraints
- [roadmap.md](./roadmap.md) — Kasane-side tracking
