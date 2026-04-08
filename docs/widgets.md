# Widgets

Declarative UI widgets defined in KDL. Widgets customize the status bar, add gutters, apply color transforms, and provide line backgrounds — all without writing a plugin.

## File Location

Widget definitions live in the same file as configuration:

```
~/.config/kasane/kasane.kdl
```

Or `$XDG_CONFIG_HOME/kasane/kasane.kdl` if the variable is set. Widget definitions go inside a `widgets { }` block. Top-level nodes whose names match a known config section (e.g., `ui`, `scroll`, `theme`, `plugins`) are parsed as configuration.

Kasane watches this file and applies changes within 2 seconds of saving. If the file has a KDL syntax error, the previous widgets remain active (last-known-good). Per-node semantic errors skip the invalid node but keep the rest.

## Quick Start

Show cursor position in the status bar:

```kdl
widgets {
    position slot="status-right" text=" {cursor_line}:{cursor_col} "
}
```

Add this to `~/.config/kasane/kasane.kdl`. The status bar updates live as you move the cursor.

> **Note:** Widget definitions must be placed inside a `widgets { }` block. Top-level widget definitions outside the block still work but emit deprecation warnings.

## Widget Kinds

Each node inside `widgets { }` defines one widget. The node name is an identifier (used in diagnostics). The `kind=` attribute selects the widget type; it defaults to `"contribution"`.

### Contribution

Adds content to a slot in the UI layout.

```kdl
widgets {
    mode slot="status-left" text=" {editor_mode} " face="white,blue+b"
}
```

| Attribute | Required | Description |
|-----------|----------|-------------|
| `slot` | yes | Target slot (see [Slots](#slots)) |
| `text` | yes* | Template string (see [Template Syntax](#template-syntax)) |
| `face` | no | Face or theme token (see [Face Specification](#face-specification)) |
| `size` | no | Size hint (see [Size Hints](#size-hints)), default `auto` |
| `when` | no | Condition (see [Condition Syntax](#condition-syntax)) |

*`text` is required for the shorthand form. For multi-part widgets, use `part` children instead:

```kdl
widgets {
    status-info slot="status-right" {
        part text=" {editor_mode} " face="default,blue+b"
        part text=" {cursor_line}:{cursor_col} "
        part text=" multi:{cursor_count} " when="cursor_count > 1"
    }
}
```

Each `part` node accepts `text=`, `face=`, and `when=`. Multiple contributions to the same slot are combined into a row.

### Background

Applies a background face to specific lines.

```kdl
widgets {
    cursorline kind="background" line="cursor" face="default,rgb:303030"
}
```

| Attribute | Required | Description |
|-----------|----------|-------------|
| `line` | no | `"cursor"` (default) or `"selection"` |
| `face` | yes | Face applied as background layer |
| `when` | no | Condition |

### Transform

Modifies the face of an existing UI element.

```kdl
widgets {
    insert-status kind="transform" target="status" face="default,blue" when="editor_mode == 'insert'"
}
```

| Attribute | Required | Description |
|-----------|----------|-------------|
| `target` | yes | Transform target (see [Transform Targets](#transform-targets)) |
| `face` | yes | Face to apply |
| `patch` | no | `"modify-face"` (default) or `"wrap"` |
| `when` | no | Condition |

`modify-face` overlays the face onto the existing element. `wrap` wraps the element in a container with the given face (useful for full-width backgrounds).

### Gutter

Adds per-line annotations in the gutter area.

```kdl
widgets {
    line-numbers kind="gutter" side="left" text="{line_number:4} " face="rgb:888888"
}
```

| Attribute | Required | Description |
|-----------|----------|-------------|
| `side` | no | `"left"` (default) or `"right"` |
| `text` | yes | Template (has access to per-line variables) |
| `face` | no | Face for gutter cells |
| `when` | no | Global condition (disables entire gutter) |
| `line-when` | no | Per-line condition (hides individual lines) |

Gutter templates can use [per-line variables](#per-line-variables) in addition to the global variables.

## Slots

Slots are regions in the UI layout where contribution widgets can place content.

| Slot name | Location |
|-----------|----------|
| `status-left` | Left side of the status bar |
| `status-right` | Right side of the status bar |
| `buffer-left` | Left of the buffer area |
| `buffer-right` | Right of the buffer area |
| `above-buffer` | Above the buffer |
| `below-buffer` | Below the buffer |
| `above-status` | Between the buffer and the status bar |

Source: `kasane-core/src/widget/parse.rs:275-284`

### Widget Ordering

Widgets appear in each slot in the order they are defined in the file. To reorder widgets in the status bar, reorder them in `kasane.kdl`.

### Status Bar Composition

```
┌─────────────┬──────────────────────────────┬──────────────────┐
│ status-left │   Kakoune status (flex fill)  │   status-right   │
└─────────────┴──────────────────────────────┴──────────────────┘
```

The status bar has three regions. `status-left` and `status-right` are sized to their content (or explicit `size=`). The center area is Kakoune's native status line, which flex-fills the remaining space.

## Transform Targets

Targets identify which UI element a transform widget modifies.

| Target name | UI element |
|-------------|------------|
| `status` | Status bar |
| `buffer` | Buffer area |
| `menu` | Completion menu |
| `menu-prompt` | Prompt-mode menu |
| `menu-inline` | Inline completion menu |
| `menu-search` | Search menu |
| `info` | Info popup |
| `info-prompt` | Prompt-mode info popup |
| `info-modal` | Modal info popup |

`status-bar` is an alias for `status`.

Source: `kasane-core/src/widget/parse.rs:288-299`

## Variables

Variables are referenced in templates as `{name}` and in conditions as bare names.

### Global Variables

| Variable | Type | Description |
|----------|------|-------------|
| `cursor_line` | number | Current cursor line (1-indexed) |
| `cursor_col` | number | Current cursor column (1-indexed) |
| `cursor_count` | number | Number of cursors/selections |
| `editor_mode` | string | `normal`, `insert`, `replace`, `prompt`, `unknown` |
| `line_count` | number | Total lines in the buffer |
| `is_focused` | bool | Whether the window has focus |
| `cols` | number | Terminal width |
| `rows` | number | Terminal height |
| `has_menu` | bool | Whether a menu is currently shown |
| `has_info` | bool | Whether an info popup is currently shown |
| `is_prompt` | bool | Whether in prompt mode |
| `status_style` | string | `status`, `command`, `search`, `prompt` |
| `cursor_mode` | string | `buffer` or `prompt` |
| `is_dark` | bool | Whether the background is dark |
| `session_count` | number | Number of active sessions |
| `active_session` | string | Key of the active session |
| `filetype` | string | Alias for `opt.filetype` |
| `bufname` | string | Alias for `opt.bufname` |
| `opt.<name>` | string | Any Kakoune `ui_option` value |

Source: `kasane-core/src/widget/variables.rs:28-63`

### Per-line Variables

Available only in gutter widget templates and `line-when` conditions.

| Variable | Type | Description |
|----------|------|-------------|
| `line_number` | number | Line number (1-indexed) |
| `relative_line` | number | Distance from cursor line |
| `is_cursor_line` | bool | Whether this is the cursor line |

Source: `kasane-core/src/widget/variables.rs:134-141`

### Truthiness

Boolean variables resolve to `"true"` or `""` (empty string). In conditions, empty strings and `"0"` are falsy; everything else is truthy.

## Condition Syntax

The `when=` attribute accepts a condition expression that determines whether a widget is active.

```kdl
// Simple truthy check
mode slot="status-left" text=" MULTI " when="cursor_count > 1"

// Equality
insert-bg kind="transform" target="status" face="default,blue" when="editor_mode == 'insert'"

// Logical operators
prompt-info slot="status-right" text=" prompt " when="is_prompt && has_info"

// Parentheses for grouping
special slot="status-right" text=" ! " when="(is_prompt || has_menu) && cursor_count > 1"
```

### Operators

| Operator | Description |
|----------|-------------|
| `==` | Equal |
| `!=` | Not equal |
| `>` | Greater than |
| `<` | Less than |
| `>=` | Greater than or equal |
| `<=` | Less than or equal |
| `&&` | Logical AND |
| `\|\|` | Logical OR |
| `!` | Logical NOT |
| `(` `)` | Grouping (controls precedence) |

Precedence (lowest to highest): `||`, `&&`, `!`. Use parentheses to override: `(a || b) && c`.

Numeric values are compared numerically; otherwise lexicographic comparison is used. String values in comparisons can be quoted with single quotes: `editor_mode == 'insert'`.

Conditions are limited to 16 nodes and 256 characters.

Source: `kasane-core/src/widget/condition.rs`

## Template Syntax

Templates expand variables inline within text content.

```
{variable_name}            → expanded value
{variable_name:N}          → right-aligned, padded to N columns
{variable_name:<N}         → left-aligned, padded to N columns
{variable_name:.N}         → truncated to N characters (with trailing …)
{variable_name:<N.M}       → left-aligned to N columns, truncated to M chars
literal text               → passed through as-is
```

Examples:

```kdl
// Simple expansion
pos slot="status-right" text=" {cursor_line}:{cursor_col} "

// Right-aligned with padding (default)
line-numbers kind="gutter" side="left" text="{line_number:4} "

// Left-aligned
filename slot="status-left" text=" {bufname:<20} "

// Truncate long values (adds … when exceeded)
path slot="status-left" text=" {bufname:.30} "

// Combined: left-align to 20 columns, truncate at 30 characters
info slot="status-left" text=" {bufname:<20.30} "
```

Unknown variables expand to an empty string and produce a warning with a fuzzy suggestion (e.g., `unknown variable 'cursor_lint', did you mean 'cursor_line'?`).

Source: `kasane-core/src/widget/template.rs`

## Face Specification

Faces follow the same `"fg,bg+attrs"` format used in [config.md § Face specification format](config.md#face-specification-format). See that section for the full syntax (colors, attributes, RGB values).

### Theme Token References

Instead of specifying colors directly, you can reference a theme token from the `theme` section in `kasane.kdl`:

```kdl
// Direct face
mode slot="status-left" face="white,blue+b" text=" {editor_mode} "

// Theme token reference (uses @ prefix)
mode slot="status-left" face="@status_line" text=" {editor_mode} "

// Custom token defined in theme { }
mode slot="status-left" face="@my_accent" text=" {editor_mode} "
```

The `@` prefix indicates a theme token reference. Both underscores and dots are accepted — `@menu_item_normal` and `@menu.item.normal` resolve to the same token. The underscore form is conventional.

Any key in the `theme { }` block becomes a valid token — both built-in and custom. See [config.md § Custom tokens](config.md#custom-tokens) for details.

When the theme changes (via config hot-reload), widgets using `@token` references automatically pick up the new colors.

If the referenced token does not exist in the theme, the default face is used.

## Size Hints

Size hints control how contribution widgets share space within a slot.

| Format | Meaning |
|--------|---------|
| `auto` | Size to content (default) |
| `Ncol` | Fixed width of N columns (e.g., `20col`) |
| `Nfr` | Flex fraction (e.g., `1fr`, `2.5fr`) |

## Hot-Reload

Kasane polls `kasane.kdl` every 2 seconds. On change:

- **Valid KDL, all nodes valid**: all widgets replaced immediately.
- **Valid KDL, some nodes invalid**: valid nodes load, invalid nodes are skipped. Warnings are logged.
- **Invalid KDL syntax**: the entire file is rejected. Previous widgets remain active (last-known-good).

Use `kasane widget check` to validate a file without starting Kasane:

```
kasane widget check                              # checks default path
kasane widget check path/to/kasane.kdl           # checks specific file
kasane widget check --watch                      # re-check on every save
```

The `--watch` flag monitors the file for changes and re-validates automatically, useful during widget development.

## Recipes

### Bridging Kakoune options via `opt.*`

Kakoune can expose arbitrary data to widgets through `ui_options`. Set an option in your `kakrc` with `set-option`, then read it in widget templates as `{opt.<key>}`:

**Example: show the current git branch**

In your `kakrc`:

```kak
hook global WinDisplay .* %{
    try %{
        set-option -add window ui_options \
            "git_branch=%sh{ git branch --show-current 2>/dev/null }"
    }
}
```

In `kasane.kdl`:

```kdl
theme {
    git_face "green,default"
}

widgets {
    git-branch slot="status-right" text=" {opt.git_branch} " face="@git_face" when="opt.git_branch"
}
```

The `when="opt.git_branch"` condition hides the widget when the value is empty (i.e., not in a git repo). The `opt.*` namespace is open-ended — any key in Kakoune's `ui_options` map is accessible as `opt.<key>`. Variable names with `opt.` prefix are always valid and never produce unknown-variable warnings.

### Mode-dependent status bar color

```kdl
widgets {
    insert-bg kind="transform" target="status" face="default,blue" when="editor_mode == 'insert'"
    normal-bg kind="transform" target="status" face="default,rgb:303030" when="editor_mode == 'normal'"
}
```

### Relative line numbers

```kdl
widgets {
    relnum kind="gutter" side="left" text="{relative_line:3} " face="rgb:666666"
}
```

## How do I...?

| Goal | Widget kind | Key attributes |
|------|-------------|----------------|
| Show text in the status bar | contribution | `slot="status-left"` or `status-right`, `text=` |
| Add line numbers | gutter | `kind="gutter"`, `text="{line_number:4} "` |
| Highlight the cursor line | background | `kind="background"`, `line="cursor"` |
| Change status bar color per mode | transform | `kind="transform"`, `target="status"`, `when=` |
| Show/hide a widget conditionally | any | `when=` attribute |
| Show a Kakoune option value | contribution | `text="{opt.my_option}"` (see [Recipes](#bridging-kakoune-options-via-opt)) |
| Display content beside the buffer | contribution | `slot="buffer-left"` or `buffer-right` |

## Constraints

- Maximum 64 widget definitions per file.

## Limitations

Widgets are intentionally simple — they combine templates, conditions, and faces. For anything beyond this, write a plugin:

- **Dynamic content** — widgets can only display variable values; they cannot compute derived values, call external commands, or maintain state.
- **Syntax highlighting** — widgets cannot modify per-token faces within the buffer. Use a Kakoune highlighter or a transform plugin.
- **Interactive elements** — widgets are display-only. Clickable actions, input fields, and focus management require the Plugin trait.
- **Cross-widget communication** — widgets are independent. If you need one widget to react to another, use plugins with the pub/sub system.
- **Custom overlays** — floating panels, popups, and modals require the overlay extension point (plugin-only).

See [Plugin Development](plugin-development.md) for the full Plugin trait and WASM SDK.

## Next Steps

- [Configuration](config.md) — theme colors, UI settings
- [Plugin Development](plugin-development.md) — write plugins for capabilities beyond what widgets offer
