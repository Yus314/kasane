# Widgets

Declarative UI widgets defined in KDL. Widgets customize the status bar, add gutters, apply color transforms, and provide line backgrounds — all without writing a plugin.

## File Location

Widget definitions live in the same file as configuration:

```
~/.config/kasane/kasane.kdl
```

Or `$XDG_CONFIG_HOME/kasane/kasane.kdl` if the variable is set. Widget definitions go inside a `widgets { }` block. Top-level nodes whose names match a known config section (e.g., `ui`, `scroll`, `theme`, `plugins`) are parsed as configuration.

Kasane watches this file using filesystem notifications and applies changes within ~100ms of saving. If the file has a KDL syntax error, the previous widgets remain active (last-known-good). Per-node semantic errors skip the invalid node but keep the rest.

### Widget Includes

You can split widget definitions across multiple files using `include` directives inside the `widgets {}` block:

```kdl
widgets {
    include "~/.config/kasane/widgets/*.kdl"
    include "./my-statusline.kdl"

    // Inline widgets still work alongside includes
    position slot="status-right" text=" {cursor_line}:{cursor_col} "
}
```

Included files contain bare widget definitions (no `widgets {}` wrapper):

```kdl
// ~/.config/kasane/widgets/mode.kdl
mode slot="status-left" text=" {editor_mode} " face="white,blue+b"
```

Paths are relative to the config file directory. Glob patterns and `~` expansion are supported. Circular includes are detected and skipped. Included files are also watched for hot-reload.

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

### Multi-Effect Widgets

A single widget block can combine multiple effects. Each effect child becomes a separate plugin instance sharing the widget's `when` condition:

```kdl
widgets {
    insert-mode when="editor_mode == 'insert'" {
        contribution slot="status-left" text=" INSERT "
        background line="cursor" face="default,rgb:202040"
        transform target="status" face="default,blue"
    }
}
```

This is equivalent to three separate widgets but shares the `when` condition and groups related effects under one name.

### Widget Groups

A `group` block shares a `when` condition across multiple independent widgets:

```kdl
widgets {
    group when="editor_mode == 'insert'" {
        insert-status slot="status-left" text=" INSERT " face="white,blue+b"
        insert-bg kind="background" line="cursor" face="default,rgb:202040"
    }
}
```

Unlike multi-effect widgets, each child in a group is a separate named widget. Groups can be nested — conditions are combined with AND.

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
    line-numbers kind="gutter" side="left" text="{line_number:>4} " face="rgb:888888"
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

### Inline

Applies a face to pattern matches within visible lines.

```kdl
widgets {
    // Substring match
    todo-highlight kind="inline" pattern="TODO" face="yellow+b"

    // Regex match (delimited by /)
    url-highlight kind="inline" pattern="/https?://[^ ]+/" face="cyan+u"
}
```

| Attribute | Required | Description |
|-----------|----------|-------------|
| `pattern` | yes | Substring or `/regex/` pattern to match |
| `face` | yes | Face applied to matched ranges |
| `when` | no | Condition |

Patterns delimited by `/` are compiled as regular expressions at parse time. Invalid regex syntax produces a parse error.

### Virtual Text

Appends virtual text at the end of lines.

```kdl
widgets {
    eol-marker kind="virtual-text" text=" ⏎" face="rgb:555555"
}
```

| Attribute | Required | Description |
|-----------|----------|-------------|
| `text` | yes | Template string for the virtual text content |
| `face` | no | Face for the virtual text |
| `when` | no | Condition |

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

Widgets appear in each slot in file-definition order by default. To override, use the `order=` attribute:

```kdl
widgets {
    // Explicit ordering: lower values appear first
    position slot="status-right" text=" {cursor_line}:{cursor_col} " order=10
    mode slot="status-right" text=" {editor_mode} " order=0
}
```

Negative values are allowed. Widgets without `order=` use their file-order position (0, 1, 2, ...).

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
| `plugin.<name>` | any | Variable exposed by a plugin via `Command::ExposeVariable` |

The `opt.*` and `plugin.*` namespaces are open-ended — they never produce unknown-variable warnings.

Source: `kasane-core/src/widget/variables.rs:28-85`

### Per-line Variables

Available only in gutter widget templates and `line-when` conditions.

| Variable | Type | Description |
|----------|------|-------------|
| `line_number` | number | Line number (1-indexed) |
| `relative_line` | number | Distance from cursor line |
| `is_cursor_line` | bool | Whether this is the cursor line |

Source: `kasane-core/src/widget/variables.rs:134-141`

### Truthiness

Boolean variables resolve to `Bool(true)` or `Bool(false)`. The `opt.*` namespace performs type inference: numeric strings like `"42"` become `Int(42)`, `"true"`/`"false"` become `Bool`, and everything else remains `Str`. This means `opt.tabstop = "0"` resolves to `Int(0)` (falsy), not `Str("0")` (which would be truthy).

In conditions, the following values are falsy:
- `Bool(false)`
- `Int(0)`
- `Str("")` (empty string)
- `Empty` (missing variable)

Everything else is truthy.

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
| `=~` | Regex match |
| `in` | Set membership |
| `&&` | Logical AND |
| `\|\|` | Logical OR |
| `!` | Logical NOT |
| `(` `)` | Grouping (controls precedence) |

Precedence (lowest to highest): `||`, `&&`, `!`. Use parentheses to override: `(a || b) && c`.

Numeric values are compared numerically; otherwise lexicographic comparison is used. String values in comparisons can be quoted with single quotes: `editor_mode == 'insert'`.

#### Regex Match (`=~`)

Tests a variable's value against a regular expression:

```kdl
widgets {
    rust-badge slot="status-right" text=" RS " when="filetype =~ 'rs|rust'"
}
```

The regex is compiled at parse time; invalid patterns produce a parse error.

#### Set Membership (`in`)

Tests whether a variable's value is in a set of values:

```kdl
widgets {
    lang-badge slot="status-right" text=" {filetype} " when="filetype in ('rust', 'go', 'python')"
}
```

Values are comma-separated inside parentheses. Both strings and numbers are supported.

Conditions are limited to 16 nodes and 256 characters.

### Condition Layers

Multi-effect widgets support conditions at multiple levels, composed with implicit AND:

| Layer | Attribute | Scope | Description |
|-------|-----------|-------|-------------|
| 1 | `when=` on widget block | Shared | Disables all effects when false |
| 2 | `when=` on effect child | Per-effect | Disables a single effect when false |
| 3 | `when=` on face rule / `line-when=` on gutter | Per-face/per-line | Controls individual face rules or gutter lines |

Example:

```kdl
widgets {
    insert-mode when="editor_mode == 'insert'" {
        contribution slot="status-left" text=" INSERT " when="is_focused"
        background line="cursor" face="default,rgb:202040"
    }
}
```

Here, Layer 1 (`editor_mode == 'insert'`) must be true for any effect to activate. Layer 2 (`is_focused`) further gates only the contribution. The background activates whenever Layer 1 is true regardless of focus.

Source: `kasane-core/src/widget/condition.rs`

## Template Syntax

Templates expand variables inline within text content.

```
{variable_name}            → expanded value
{variable_name:N}          → left-aligned, padded to N columns
{variable_name:>N}         → right-aligned, padded to N columns
{variable_name:.N}         → truncated to N characters (with trailing …)
{variable_name:>N.M}       → right-aligned to N columns, truncated to M chars
literal text               → passed through as-is
```

Examples:

```kdl
// Simple expansion
pos slot="status-right" text=" {cursor_line}:{cursor_col} "

// Left-aligned with padding (default)
filename slot="status-left" text=" {bufname:20} "

// Right-aligned (gutter line numbers)
line-numbers kind="gutter" side="left" text="{line_number:>4} "

// Truncate long values (adds … when exceeded)
path slot="status-left" text=" {bufname:.30} "

// Combined: right-align to 20 columns, truncate at 30 characters
info slot="status-left" text=" {bufname:>20.30} "
```

### Conditional Expansion

Templates support inline conditionals with `{?condition => then => else}`:

```
{?is_focused => active => inactive}
{?cursor_count > 1 => multi => single}
```

Branches can contain variables and formatting:

```
{?is_focused => {cursor_line} => N/A}
{?is_focused => {cursor_line:4} => ---}
```

Conditionals can be nested:

```
{?is_focused => active => {?has_menu => menu => buffer}}
```

The `=>` separator allows colons and other punctuation in branches without ambiguity:

```
{?is_focused => 12:34 => --:--}
{?has_file => https://example.com => N/A}
```

Unknown variables expand to an empty string and produce a warning with a fuzzy suggestion (e.g., `unknown variable 'cursor_lint', did you mean 'cursor_line'?`).

Boolean `false` values expand to the string `"false"` (not empty). Use a conditional `{?var => text}` to show text only when a variable is truthy.

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

Kasane uses filesystem notifications (`notify` crate) to detect changes to `kasane.kdl` and any included widget files. Changes are applied within ~100ms of saving, with a debounce window to coalesce rapid-fire editor writes. If filesystem watching is unavailable, it falls back to 2-second polling.

A content hash check skips re-parsing when the file content hasn't actually changed (e.g., touch without modification).

On change:

- **Valid KDL, all nodes valid**: all widgets replaced immediately.
- **Valid KDL, some nodes invalid**: valid nodes load, invalid nodes are skipped. Warnings are logged.
- **Invalid KDL syntax**: the entire file is rejected. Previous widgets remain active (last-known-good).

Use `kasane widget check` to validate a file without starting Kasane:

```
kasane widget check                              # checks default path
kasane widget check path/to/kasane.kdl           # checks specific file
kasane widget check --watch                      # re-check on every save
kasane widget check -v                           # show per-widget details
kasane widget variables                          # list available template variables
kasane widget slots                              # list available slots and targets
```

The `--watch` flag monitors the file for changes and re-validates automatically, useful during widget development. The `-v`/`--verbose` flag shows per-widget details (kind, slot/target, referenced variables).

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
| Add line numbers | gutter | `kind="gutter"`, `text="{line_number:>4} "` |
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
