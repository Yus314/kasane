# What's Different

Kasane's defaults match Kakoune's standard UI. The following features
are available — some enabled by default, others opt-in.

## Enabled by Default

These improvements are active without any configuration:

- **Flicker-free rendering** — double-buffered output with synchronized updates eliminates visual tearing during redraws
- **Independent Unicode width calculation** — correct layout for CJK characters, emoji, and other wide glyphs using the `unicode-width` crate
- **System clipboard integration** — copy/paste via the `arboard` crate without needing xclip, xsel, or pbcopy
- **True 24-bit color** — RGB colors are passed directly to the terminal with no palette approximation
- **Mouse drag scrolling** — click and drag to scroll the buffer

## Opt-in via Configuration

These features are available but disabled by default. Enable them in `~/.config/kasane/config.toml`:

### Smooth scrolling

```toml
[scroll]
smooth = true
```

Animated scrolling instead of instant jumps. Configurable scroll speed via `lines_per_scroll`.

### Search dropdown

```toml
[search]
dropdown = true
```

Show search completions as a vertical dropdown instead of inline.

### Shadow on floating windows

Enabled by default. Disable with:

```toml
[ui]
shadow = false
```

### Status bar position

```toml
[ui]
status_position = "top"  # default: "bottom"
```

### Border styles

```toml
[ui]
border_style = "double"  # "single", "rounded", "double", "heavy", "ascii"
```

### Theme customization

Override colors for any UI element:

```toml
[theme]
menu_item_normal = "cyan,black"
menu_item_selected = "black,cyan+b"
info_border = "bright-blue,default"
```

See [config.md](config.md) for the full configuration reference.

## Opt-in: Multi-Pane

Split the editor into multiple panes, each connected to an independent Kakoune session. Works in both TUI and GUI backends.

| Key | Action |
|-----|--------|
| `<C-w>v` | Vertical split |
| `<C-w>s` | Horizontal split |
| `<C-w>h/j/k/l` | Move focus directionally |
| `<C-w>w` / `<C-w>W` | Cycle focus forward / backward |
| `<C-w>q` | Close focused pane |

Pane dividers can also be dragged with the mouse to resize.

## Opt-in: Plugin System

Kasane has a WASM plugin system for UI extensions. Plugins can add visual elements, decorations, overlays, and input handling that Kakoune's shell-based plugins cannot — for example, line highlights, gutter annotations, floating pickers, and status bar widgets.

Several example plugins are bundled in the binary. Enable them in `~/.config/kasane/config.toml`:

```toml
[plugins]
enabled = ["cursor_line", "color_preview"]
```

See [Using Plugins](using-plugins.md) for the full list of bundled plugins and details, and [Plugin Development](plugin-development.md) to write your own.

## Opt-in: GPU Backend

Kasane includes a GPU rendering backend built on wgpu and glyphon:

```bash
kasane --ui gui file.txt
```

- **System font rendering** — use any monospace font installed on your system, with fallback chains for CJK and emoji
- **Smooth animations** — cursor blinking, scroll animations at native refresh rate
- **Native window management** — resizable window, fullscreen toggle (F11), maximize

See [config.md](config.md#gui-backend) for setup and configuration.
