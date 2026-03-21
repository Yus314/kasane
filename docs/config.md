# Configuration Reference

This document is the reference for user-facing configuration keys and defaults.
For architecture and semantics behind these settings, see [architecture.md](./architecture.md) and [semantics.md](./semantics.md).

Kasane reads its configuration from a TOML file at:

```
~/.config/kasane/config.toml
```

Or, if `$XDG_CONFIG_HOME` is set:

```
$XDG_CONFIG_HOME/kasane/config.toml
```

Partial configs are fine — any omitted field uses its default value. If no config file exists, all defaults apply. Changes require restarting Kasane (no hot reload).

## `[ui]`

General UI settings.

| Key | Type | Default | Description |
|-----|------|---------|-------------|
| `backend` | string | `"tui"` | UI backend: `"tui"` or `"gui"`. CLI `--ui` overrides this. |
| `shadow` | bool | `true` | Shadow effect on floating windows (menus, info popups) |
| `padding_char` | string | `"~"` | Character shown on empty lines below buffer content |
| `border_style` | string | `"rounded"` | Border style: `"single"`, `"rounded"`, `"double"`, `"heavy"`, `"ascii"` |
| `status_position` | string | `"bottom"` | Status bar position: `"top"` or `"bottom"` |
| `scene_renderer` | bool or null | `null` | Enable the scene-based GPU renderer (bypasses CellGrid). `null` = auto (`true` for GUI, `false` for TUI). |

```toml
[ui]
backend = "tui"
shadow = false
padding_char = " "
border_style = "double"
status_position = "top"
```

## `[scroll]`

Scroll behavior settings.

| Key | Type | Default | Description |
|-----|------|---------|-------------|
| `lines_per_scroll` | integer | `3` | Lines per mouse wheel / scroll event |
| `smooth` | bool | `false` | Enable smooth scroll policy plugins that honor `smooth-scroll.enabled` |
| `inertia` | bool | `false` | Momentum/inertia scrolling (reserved, not yet implemented) |

```toml
[scroll]
lines_per_scroll = 5
smooth = true
```

Runtime/plugin note:

- The canonical runtime key for smooth scroll policy plugins is `smooth-scroll.enabled`.
- `SetConfig { key: "smooth_scroll", ... }` is still accepted as a deprecated alias and is normalized internally.
- The TOML file continues to use `[scroll].smooth`; it seeds `smooth-scroll.enabled` during startup.

## `[log]`

Logging configuration. Log files are written as daily-rotating files named `kasane.log` in the log directory.

| Key | Type | Default | Description |
|-----|------|---------|-------------|
| `level` | string | `"warn"` | Log level: `"trace"`, `"debug"`, `"info"`, `"warn"`, `"error"` |
| `file` | string or omit | *(auto)* | Log file directory. Default: `$XDG_STATE_HOME/kasane/` or `~/.local/state/kasane/` |

The `KASANE_LOG` environment variable overrides the configured `level`.

```toml
[log]
level = "info"
file = "/tmp/kasane-logs"
```

## `[menu]`

Completion menu settings.

| Key | Type | Default | Description |
|-----|------|---------|-------------|
| `position` | string | `"auto"` | Menu placement: `"auto"`, `"above"`, `"below"` |
| `max_height` | integer | `10` | Maximum menu height in rows |

```toml
[menu]
position = "below"
max_height = 15
```

## `[search]`

Search prompt settings.

| Key | Type | Default | Description |
|-----|------|---------|-------------|
| `dropdown` | bool | `false` | Show search completions as a vertical dropdown instead of inline |

```toml
[search]
dropdown = true
```

## `[clipboard]`

System clipboard integration.

| Key | Type | Default | Description |
|-----|------|---------|-------------|
| `enabled` | bool | `true` | Enable system clipboard integration |

```toml
[clipboard]
enabled = false
```

## `[mouse]`

Mouse behavior settings.

| Key | Type | Default | Description |
|-----|------|---------|-------------|
| `drag_scroll` | bool | `true` | Enable mouse drag scrolling |

```toml
[mouse]
drag_scroll = false
```

## `[theme]`

Override the default face (color + attributes) for UI elements. Each key is a style token name and the value is a face specification string.

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

```toml
[theme]
menu_item_normal = "cyan,black"
menu_item_selected = "black,cyan+b"
info_border = "bright-blue,default"
status_mode = "white,red+b"
shadow = "default,default+d"
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

The following sections only apply when using `--ui gui` or `backend = "gui"`. They are ignored by the TUI backend. Requires building with `--features gui` and GPU drivers supporting Vulkan, Metal, or DX12 (handled automatically by wgpu).

### `[window]`

Window settings.

| Key | Type | Default | Description |
|-----|------|---------|-------------|
| `initial_cols` | integer | `80` | Initial window width in columns |
| `initial_rows` | integer | `24` | Initial window height in rows |
| `fullscreen` | bool | `false` | Start in borderless fullscreen mode |
| `maximized` | bool | `false` | Start with window maximized |

When `fullscreen` is `true`, `initial_cols` and `initial_rows` are ignored (the window fills the entire monitor). Fullscreen can be toggled at runtime with F11.

```toml
[window]
initial_cols = 120
initial_rows = 36
fullscreen = false
maximized = true
```

### `[font]`

Font settings for the GUI renderer.

| Key | Type | Default | Description |
|-----|------|---------|-------------|
| `family` | string | `"monospace"` | Font family name |
| `size` | float | `14.0` | Font size in points |
| `style` | string | `"Regular"` | Font style variant |
| `fallback_list` | array of strings | `[]` | Fallback fonts for missing glyphs |
| `line_height` | float | `1.2` | Line height multiplier |
| `letter_spacing` | float | `0.0` | Extra letter spacing in points |

```toml
[font]
family = "JetBrains Mono"
size = 15.0
fallback_list = ["Noto Sans CJK JP", "Noto Color Emoji"]
line_height = 1.3
```

### `[colors]`

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
| `white` | `#e5e5e5` | White |
| `bright_black` | `#666666` | Bright black |
| `bright_red` | `#f14c4c` | Bright red |
| `bright_green` | `#23d18b` | Bright green |
| `bright_yellow` | `#f5f543` | Bright yellow |
| `bright_blue` | `#3b8eea` | Bright blue |
| `bright_magenta` | `#d670d6` | Bright magenta |
| `bright_cyan` | `#29b8db` | Bright cyan |
| `bright_white` | `#e5e5e5` | Bright white |

```toml
# Gruvbox-inspired palette
[colors]
default_fg = "#ebdbb2"
default_bg = "#282828"
black = "#282828"
red = "#cc241d"
green = "#98971a"
yellow = "#d79921"
blue = "#458588"
magenta = "#b16286"
cyan = "#689d6a"
white = "#a89984"
bright_black = "#928374"
bright_red = "#fb4934"
bright_green = "#b8bb26"
bright_yellow = "#fabd2f"
bright_blue = "#83a598"
bright_magenta = "#d3869b"
bright_cyan = "#8ec07c"
bright_white = "#ebdbb2"
```

## `[plugins]`

Plugin discovery and loading settings.

| Key | Type | Default | Description |
|-----|------|---------|-------------|
| `enabled` | array of strings | `[]` | Bundled plugin IDs to enable (opt-in). See [using-plugins.md](./using-plugins.md#bundled-wasm-plugins) for the full list |
| `auto_discover` | bool | `true` | Automatically discover `.wasm` plugins from the plugins directory |
| `path` | string or omit | *(auto)* | Custom plugins directory. Default: `$XDG_DATA_HOME/kasane/plugins/` or `~/.local/share/kasane/plugins/` |
| `disabled` | array of strings | `[]` | Plugin IDs to disable (applies to discovered and user-registered plugins) |

Example plugins are embedded in the Kasane binary but are **not loaded by default**. Add their IDs to `enabled` to activate them:

```toml
[plugins]
enabled = ["cursor_line", "color_preview"]
```

Discovered plugins (from the plugins directory) can be disabled individually:

```toml
[plugins]
disabled = ["some_plugin"]
```

### `[plugins.deny_capabilities]`

Restrict WASI capabilities for specific WASM plugins. Key: plugin ID. Value: list of denied capability names.

Valid capability names: `"filesystem"`, `"environment"`, `"monotonic-clock"`.

```toml
[plugins.deny_capabilities]
untrusted_plugin = ["filesystem", "environment"]
```

See [Using Plugins](using-plugins.md) for more details.

## See also

- [README.md](../README.md) — installation and basic usage
- [architecture.md](./architecture.md) — where configuration is applied
- [semantics.md](./semantics.md) — runtime semantics affected by config and ui_options
- [index.md](./index.md) — docs entry point
