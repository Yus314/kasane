# Configuration Reference

This document is the reference for user-facing configuration keys and defaults.
For semantics behind these settings, see [semantics.md](./semantics.md).

Kasane reads its configuration from a KDL file at:

```
~/.config/kasane/kasane.kdl
```

Or, if `$XDG_CONFIG_HOME` is set:

```
$XDG_CONFIG_HOME/kasane/kasane.kdl
```

Partial configs are fine — any omitted field uses its default value. If no config file exists, all defaults apply. Most settings take effect within 2 seconds when the file is saved. The following require a restart: `ui.backend`, `ui.border_style`, `ui.image_protocol`, `scroll.lines_per_scroll`, `window`, `font`, `log`, `plugins`.

Configuration and widget definitions live in the same file. Top-level nodes whose names match a known config section (`ui`, `scroll`, `log`, `theme`, `menu`, `search`, `clipboard`, `mouse`, `window`, `font`, `colors`, `plugins`, `settings`) are parsed as configuration. Everything else is treated as a [widget definition](widgets.md).

## `ui`

General UI settings.

| Key | Type | Default | Description |
|-----|------|---------|-------------|
| `backend` | string | `"tui"` | UI backend: `"tui"` or `"gui"`. CLI `--ui` overrides this. |
| `shadow` | bool | `#true` | Shadow effect on floating windows (menus, info popups) |
| `padding_char` | string | `"~"` | Character shown on empty lines below buffer content |
| `border_style` | string | `"rounded"` | Border style: `"single"`, `"rounded"`, `"double"`, `"heavy"`, `"ascii"` |
| `status_position` | string | `"bottom"` | Status bar position: `"top"` or `"bottom"` |
| `scene_renderer` | bool or null | `#null` | Enable the scene-based GPU renderer (bypasses CellGrid). `#null` = auto (`#true` for GUI, `#false` for TUI). |
| `image_protocol` | string | `"auto"` | Image rendering protocol: `"auto"` (detect terminal), `"halfblock"`, `"kitty"` |

```kdl
ui {
    backend "tui"
    shadow #false
    padding_char " "
    border_style "double"
    status_position "top"
}
```

## `scroll`

Scroll behavior settings.

| Key | Type | Default | Description |
|-----|------|---------|-------------|
| `lines_per_scroll` | integer | `3` | Lines per mouse wheel / scroll event |
| `smooth` | bool | `#false` | *Deprecated.* Use `settings { smooth_scroll { enabled #true } }` instead. Kept for backward compatibility; seeds the plugin setting at startup. |
| `inertia` | bool | `#false` | Momentum/inertia scrolling (reserved, not yet implemented) |

```kdl
scroll {
    lines_per_scroll 5
    smooth #true
}
```

## `settings`

Per-plugin typed settings. Each plugin declares its settings schema in its `kasane-plugin.toml` manifest (type, default, description). You can override defaults here.

```kdl
settings {
    smooth_scroll {
        enabled #true
    }
    my_custom_plugin {
        threshold 42
        label "custom"
    }
}
```

Values must match the type declared in the plugin's manifest (`bool`, `integer`, `float`, `string`). Unknown keys or type mismatches produce a warning at startup and fall back to the manifest default.

Plugins read settings via `get_setting_bool`, `get_setting_integer`, `get_setting_float`, or `get_setting_string` host functions.

## `log`

Logging configuration. Log files are written as daily-rotating files named `kasane.log` in the log directory.

| Key | Type | Default | Description |
|-----|------|---------|-------------|
| `level` | string | `"warn"` | Log level: `"trace"`, `"debug"`, `"info"`, `"warn"`, `"error"` |
| `file` | string or omit | *(auto)* | Log file directory. Default: `$XDG_STATE_HOME/kasane/` or `~/.local/state/kasane/` |

The `KASANE_LOG` environment variable overrides the configured `level`.

Set `KASANE_LOG_STDERR=1` to redirect tracing output to stderr instead of
the daily-rotating file. The TUI owns stdout for ANSI escapes, so callers
should redirect stderr (e.g. `KASANE_LOG=debug KASANE_LOG_STDERR=1 kasane file 2> trace.log`)
unless they are running a non-TUI subcommand.

```kdl
log {
    level "info"
    file "/tmp/kasane-logs"
}
```

## `menu`

Completion menu settings.

| Key | Type | Default | Description |
|-----|------|---------|-------------|
| `position` | string | `"auto"` | Menu placement: `"auto"`, `"above"`, `"below"` |
| `max_height` | integer | `10` | Maximum menu height in rows |

```kdl
menu {
    position "below"
    max_height 15
}
```

## `search`

Search prompt settings.

| Key | Type | Default | Description |
|-----|------|---------|-------------|
| `dropdown` | bool | `#false` | Show search completions as a vertical dropdown instead of inline |

```kdl
search {
    dropdown #true
}
```

## `clipboard`

System clipboard integration.

| Key | Type | Default | Description |
|-----|------|---------|-------------|
| `enabled` | bool | `#true` | Enable system clipboard integration |

```kdl
clipboard {
    enabled #false
}
```

## `mouse`

Mouse behavior settings.

| Key | Type | Default | Description |
|-----|------|---------|-------------|
| `drag_scroll` | bool | `#true` | Enable mouse drag scrolling |

```kdl
mouse {
    drag_scroll #false
}
```

## `theme`

Override the default face (color + attributes) for UI elements. Each key is a style token name and the value is a face specification string.

Any key in the `theme` block becomes a theme token that can be referenced by widgets with the `@token` prefix (see [widgets.md § Theme Token References](widgets.md#theme-token-references)). The built-in tokens below have default values; custom keys are accepted and start as `Face::default()`.

### Token name normalization

Theme token names use **dot notation** internally (e.g., `menu.item.normal`). In the config file, underscores and dots are interchangeable — `menu_item_normal` and `menu.item.normal` both resolve to the same token. The underscore form is conventional.

### Built-in tokens

| Token | Description | Default face |
|-------|-------------|--------------|
| `buffer_text` | Main editor text | *(inherits)* |
| `buffer_padding` | Padding lines (`~`) | *(inherits)* |
| `status_line` | Status bar | `default,default` |
| `status_mode` | Mode indicator | `default,default` |
| `menu_item_normal` | Unselected menu item | `white,blue` |
| `menu_item_selected` | Selected menu item | `blue,white` |
| `menu_scrollbar` | Scrollbar track | `white,blue` |
| `menu_scrollbar_thumb` | Scrollbar handle | `white,blue` |
| `info_text` | Info popup text | `default,default` |
| `info_border` | Info popup border | `default,default` |
| `border` | Container borders | *(inherits)* |
| `shadow` | Floating window shadow | `default,default+d` |

### Custom tokens

Any additional key you add becomes a custom theme token:

```kdl
theme {
    my_accent "cyan,default+b"
    git_status "green,default"
}
```

Widgets can then reference these as `face="@my_accent"` or `face="@git_status"`. When the theme changes via hot-reload, widgets using `@token` references automatically pick up the new values. If a referenced token does not exist, the default face is used.

Source: `kasane-core/src/render/theme.rs:132-141`, `kasane-core/src/render/theme.rs:214-216`

```kdl
theme {
    menu_item_normal "cyan,black"
    menu_item_selected "black,cyan+b"
    info_border "bright-blue,default"
    status_mode "white,red+b"
    shadow "default,default+d"
}
```

### Face specification format

Face strings follow Kakoune's format:

```
foreground,background+attributes
```

Either part can be omitted: `"red"` sets only the foreground, `",blue"` sets only the background, `"red,blue+bi"` sets both colors with bold+italic.

#### Colors

| Category | Names |
|----------|-------|
| Named | `black`, `red`, `green`, `yellow`, `blue`, `magenta`, `cyan`, `white` |
| Bright | `bright-black`, `bright-red`, `bright-green`, `bright-yellow`, `bright-blue`, `bright-magenta`, `bright-cyan`, `bright-white` |
| Special | `default` (terminal default color) |
| RGB | `rgb:rrggbb` (e.g., `rgb:ff8000` for orange) |

In TUI mode, named colors use the terminal's palette. In GUI mode, named colors are mapped to concrete values via the `[colors]` section.

#### Attributes

Attributes are specified after `+`. Multiple attributes can be combined (e.g., `+bi` for bold + italic).

| Char | Attribute |
|------|-----------|
| `b` | Bold |
| `i` | Italic |
| `u` | Underline |
| `r` | Reverse (swap fg/bg) |
| `d` | Dim |

## GUI Backend

The following sections only apply when using `--ui gui` or `backend "gui"`. They are ignored by the TUI backend. Requires building with `--features gui` and GPU drivers supporting Vulkan, Metal, or DX12 (handled automatically by wgpu).

### `window`

Window settings.

| Key | Type | Default | Description |
|-----|------|---------|-------------|
| `initial_cols` | integer | `80` | Initial window width in columns |
| `initial_rows` | integer | `24` | Initial window height in rows |
| `fullscreen` | bool | `#false` | Start in borderless fullscreen mode |
| `maximized` | bool | `#false` | Start with window maximized |
| `present_mode` | string or null | `#null` | Override GPU present mode: `"Fifo"`, `"Mailbox"`, `"AutoVsync"`, `"AutoNoVsync"`. `#null` = wgpu default. |

When `fullscreen` is `#true`, `initial_cols` and `initial_rows` are ignored (the window fills the entire monitor). Fullscreen can be toggled at runtime with F11.

```kdl
window {
    initial_cols 120
    initial_rows 36
    fullscreen #false
    maximized #true
}
```

### `font`

Font settings for the GUI renderer.

| Key | Type | Default | Description |
|-----|------|---------|-------------|
| `family` | string | `"monospace"` | Font family name |
| `size` | float | `14.0` | Font size in points |
| `style` | string | `"Regular"` | Font style variant |
| `fallback_list` | array of strings | `[]` | Fallback fonts for missing glyphs |
| `line_height` | float | `1.2` | Line height multiplier |
| `letter_spacing` | float | `0.0` | Extra letter spacing in points |

```kdl
font {
    family "JetBrains Mono"
    size 15.0
    fallback_list "Noto Sans CJK JP" "Noto Color Emoji"
    line_height 1.3
}
```

### `colors`

Defines concrete RGB values for named colors in the GUI backend. The TUI backend uses the terminal's own palette, but the GUI needs explicit values. All values are `#rrggbb` hex strings.

Default palette (VS Code Dark+ inspired):

| Key | Default | Description |
|-----|---------|-------------|
| `default_fg` | `#d4d4d4` | Default foreground |
| `default_bg` | `#1e1e1e` | Default background |
| `black` | `#000000` | Black |
| `red` | `#cd3131` | Red |
| `green` | `#0dbc79` | Green |
| `yellow` | `#e5e510` | Yellow |
| `blue` | `#2472c8` | Blue |
| `magenta` | `#bc3fbc` | Magenta |
| `cyan` | `#11a8cd` | Cyan |
| `white` | `#cccccc` | White |
| `bright_black` | `#666666` | Bright black |
| `bright_red` | `#f14c4c` | Bright red |
| `bright_green` | `#23d18b` | Bright green |
| `bright_yellow` | `#f5f543` | Bright yellow |
| `bright_blue` | `#3b8eea` | Bright blue |
| `bright_magenta` | `#d670d6` | Bright magenta |
| `bright_cyan` | `#29b8db` | Bright cyan |
| `bright_white` | `#e5e5e5` | Bright white |

```kdl
// Gruvbox-inspired palette
colors {
    default_fg "#ebdbb2"
    default_bg "#282828"
    black "#282828"
    red "#cc241d"
    green "#98971a"
    yellow "#d79921"
    blue "#458588"
    magenta "#b16286"
    cyan "#689d6a"
    white "#a89984"
    bright_black "#928374"
    bright_red "#fb4934"
    bright_green "#b8bb26"
    bright_yellow "#fabd2f"
    bright_blue "#83a598"
    bright_magenta "#d3869b"
    bright_cyan "#8ec07c"
    bright_white "#ebdbb2"
}
```

## `plugins`

Plugin discovery and loading settings.

| Key | Type | Default | Description |
|-----|------|---------|-------------|
| `enabled` | array of strings | `[]` | Bundled plugin IDs to enable (opt-in). See [using-plugins.md](./using-plugins.md#bundled-wasm-plugins) for the full list |
| `path` | string or omit | *(auto)* | Custom plugins directory. Default: `$XDG_DATA_HOME/kasane/plugins/` or `~/.local/share/kasane/plugins/` |
| `disabled` | array of strings | `[]` | Plugin IDs to disable when building `plugins.lock` via `kasane plugin resolve` / `install` / `dev` |

Example plugins are embedded in the Kasane binary but are **not loaded by default**. Add their IDs to `enabled`, then run `kasane plugin resolve` (or `install` / `dev`) to write them into `plugins.lock`:

```kdl
plugins {
    enabled "cursor_line" "color_preview"
}
```

Installed packages and bundled plugins can be disabled individually:

```kdl
plugins {
    disabled "some_plugin"
}
```

### `plugins` / `selection`

Pin a filesystem package selection for a specific plugin ID.

```kdl
plugins {
    selection {
        sel_badge {
            mode "pin-digest"
            digest "sha256:abc123..."
        }
    }
}
```

```kdl
plugins {
    selection {
        cursor_line {
            mode "pin-package"
            package "builtin/cursor-line"
            version "0.4.0"
        }
    }
}
```

Available modes:

- `auto`: use the current lock entry when valid, otherwise require a single installed candidate
- `pin-digest`: select an exact installed artifact digest
- `pin-package`: select an installed package by name, optionally constrained to a specific version

### `plugins` / `deny_capabilities`

Restrict WASI capabilities for specific WASM plugins. Key: plugin ID. Value: list of denied capability names.

Valid capability names: `"filesystem"`, `"environment"`, `"monotonic-clock"`, `"process"`.

```kdl
plugins {
    deny_capabilities {
        untrusted_plugin "filesystem" "environment"
    }
}
```

See [Using Plugins](using-plugins.md) for more details.

### `plugins` / `deny_authorities`

Restrict workspace authorities for specific WASM plugins. Key: plugin ID. Value: list of denied authority names.

Valid authority names: `"dynamic-surface"`, `"pty-process"`.

```kdl
plugins {
    deny_authorities {
        untrusted_plugin "dynamic-surface"
        another_plugin "pty-process"
    }
}
```

## Migrating from v0.4.0

Kasane 0.5.0 replaces the TOML-based `config.toml` with KDL-based `kasane.kdl`. On startup, Kasane detects a stale `config.toml` and prints a warning — your old config is not read, but also not deleted.

There is no automatic migrator: the TOML configs are small in practice and the structural mapping is mechanical. Start fresh with `kasane init`, then port each section using the examples below.

### Structural mapping

- TOML `[section]` → KDL `section { ... }`
- TOML `key = value` inside a section → KDL `key value` (no `=`)
- TOML `true`/`false` → KDL `#true`/`#false`
- TOML nested `[section.subsection]` → KDL `section { subsection { ... } }`
- TOML arrays `key = ["a", "b"]` → KDL `key "a" "b"` (variadic args)

### `ui`

Before (`config.toml`):

```toml
[ui]
shadow = false
border_style = "double"
status_position = "top"
padding_char = " "
```

After (`kasane.kdl`):

```kdl
ui {
    shadow #false
    border_style "double"
    status_position "top"
    padding_char " "
}
```

### `theme`

Before:

```toml
[theme]
menu_item_normal = "cyan,black"
menu_item_selected = "black,cyan+b"
info_border = "bright-blue,default"
```

After:

```kdl
theme {
    menu_item_normal "cyan,black"
    menu_item_selected "black,cyan+b"
    info_border "bright-blue,default"
}
```

Note: v0.5.0 also supports `@token` references and `variant "dark"/"light"` blocks — see [Theme](#theme) above.

### `plugins`

Before:

```toml
[plugins]
enabled = ["cursor_line", "color_preview"]
disabled = ["some_plugin"]
```

After:

```kdl
plugins {
    enabled "cursor_line" "color_preview"
    disabled "some_plugin"
}
```

### `plugins.deny_capabilities` (nested table)

Before:

```toml
[plugins.deny_capabilities]
untrusted_plugin = ["filesystem", "environment"]
```

After:

```kdl
plugins {
    deny_capabilities {
        untrusted_plugin "filesystem" "environment"
    }
}
```

### `settings.<plugin_id>`

Before:

```toml
[settings.cursor_line]
highlight_color = "rgb:303030"
enabled = true
intensity = 42
```

After:

```kdl
settings {
    cursor_line {
        highlight_color "rgb:303030"
        enabled #true
        intensity 42
    }
}
```

### Other sections

`scroll`, `log`, `menu`, `search`, `clipboard`, `mouse`, `window`, `font`, `colors` follow the same mapping rules. See the reference sections above for field names and defaults.

When you are done, delete the old `config.toml` to silence the startup warning.

## See also

- [README.md](../README.md) — installation and basic usage
- [semantics.md](./semantics.md) — runtime semantics affected by config and ui_options
- [widgets.md](./widgets.md) — declarative widget definitions (in the same `kasane.kdl` file)
- [index.md](./index.md) — docs entry point
