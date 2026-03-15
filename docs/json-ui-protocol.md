# Kakoune JSON UI Protocol Specification

This document serves as the reference specification for the Kakoune JSON UI protocol that Kasane relies on.
For constraint analysis, see [kakoune-protocol-constraints.md](./kakoune-protocol-constraints.md). For Kasane-side semantics, see [semantics.md](./semantics.md).

Specification for Kakoune's JSON-RPC 2.0 based external UI protocol.

## Data Structures

### Face
```json
{
  "fg": "Color",
  "bg": "Color",
  "underline": "Color",
  "attributes": ["Attribute", "..."]
}
```

**Color:** `"default"` | Named colors (`"red"`, `"blue"`, ...) | HEX (`"#rrggbb"`)

**Attribute:** `"underline"` | `"curly_underline"` | `"double_underline"` | `"reverse"` | `"blink"` | `"bold"` | `"dim"` | `"italic"` | `"strikethrough"` | `"final_fg"` | `"final_bg"` | `"final_attr"`

### Atom
```json
{
  "face": "Face",
  "contents": "string"
}
```

### Line
```json
["Atom", "Atom", "..."]
```

### Coord
```json
{
  "line": 0,
  "column": 0
}
```

## Kakoune → Kasane Messages

| Method | Parameters |
|---------|-----------|
| `draw` | `[lines: Line[], cursor_pos: Coord, default_face: Face, padding_face: Face, widget_columns: int]` |
| `draw_status` | `[prompt: Line, content: Line, cursor_pos: int, mode_line: Line, default_face: Face]` |
| `menu_show` | `[items: Line[], anchor: Coord, selected_item_face: Face, menu_face: Face, style: string]` |
| `menu_select` | `[selected: int]` |
| `menu_hide` | `[]` |
| `info_show` | `[title: Line, content: Line[], anchor: Coord, face: Face, style: string]` |
| `info_hide` | `[]` |
| `set_ui_options` | `[options: Map<string, string>]` |
| `refresh` | `[force: bool]` |

## Kasane → Kakoune Messages

| Method | Parameters |
|---------|-----------|
| `keys` | `[key: string, ...]` |
| `resize` | `[rows: int, columns: int]` |
| `scroll` | `[amount: int, line: int, column: int]` |
| `mouse_press` | `[button: string, line: int, column: int]` |
| `mouse_release` | `[button: string, line: int, column: int]` |
| `mouse_move` | `[line: int, column: int]` |
| `menu_select` | `[index: int]` |

## Upstream Protocol Extension Tracking

Unmerged upstream PRs/Issues that could enhance Kasane's capabilities.

| PR/Issue | Description | Impact on Kasane | Status |
|----------|-------------|------------------|--------|
| [Commit 3dd6f30d](https://github.com/mawww/kakoune/commit/3dd6f30d) | Removed `set_cursor`, integrated `cursor_pos` into `draw`/`draw_status` | **Supported** | Merged |
| [PR #5455](https://github.com/mawww/kakoune/pull/5455) | Added `widget_columns` parameter to `draw` | **Supported** — Can be used for auxiliary region contributions in `P-010` / `P-011` | Merged |
| [PR #4707](https://github.com/mawww/kakoune/pull/4707) | Additional face names | Improves semantic differentiation for `P-001`, `P-010`, `P-011` | Open |
| [#4686](https://github.com/mawww/kakoune/issues/4686) | Incremental draw | Enables more efficient differential rendering | Open |
| [#4687](https://github.com/mawww/kakoune/issues/4687) | Atom type differentiation | Improves accuracy of separated rendering and overlay compositing for `P-001`, `P-010`, `P-011` | Open |
| [#5428](https://github.com/mawww/kakoune/issues/5428) | Status line context | Resolves the `D-003` degeneration | Open |

## Related Documents

- [kakoune-protocol-constraints.md](./kakoune-protocol-constraints.md) — Constraint analysis
- [upstream-dependencies.md](./upstream-dependencies.md) — Upstream watchlist
- [semantics.md](./semantics.md) — Kasane-side semantics
- [architecture.md](./architecture.md) — Where the protocol fits in the architecture
