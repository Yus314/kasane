# GPU Backend

## What It Offers

The GPU backend renders Kasane using wgpu and glyphon instead of terminal escape sequences:

- **System font rendering** — use any monospace font installed on your system, with fallback chains for CJK and emoji
- **Smooth animations** — cursor blinking, scroll animations at native refresh rate
- **Native window management** — resizable window, fullscreen toggle (F11), maximize

## Prerequisites

- GPU drivers supporting Vulkan, Metal, or DX12 (handled automatically by wgpu)
- Kasane built with the `gui` feature:

```bash
cargo install --path kasane --features gui
```

## Usage

```bash
kasane --ui gui file.txt
```

Or set as default in configuration:

```toml
# ~/.config/kasane/config.toml
[ui]
backend = "gui"
```

## Configuration

### Window

```toml
[window]
initial_cols = 120
initial_rows = 36
fullscreen = false
maximized = true
```

When `fullscreen = true`, `initial_cols` and `initial_rows` are ignored. Toggle fullscreen at runtime with F11.

### Font

```toml
[font]
family = "JetBrains Mono"
size = 15.0
style = "Regular"
fallback_list = ["Noto Sans CJK JP", "Noto Color Emoji"]
line_height = 1.3
letter_spacing = 0.0
```

### Color Palette

In TUI mode, named colors use the terminal's palette. The GUI needs explicit RGB values. Default palette is VS Code Dark+ inspired:

```toml
# Gruvbox example
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
```

See [config.md](config.md) for the full color palette reference.

## Limitations

- Not available over SSH (requires a local display server)
- The `gui` feature adds build dependencies (wgpu, winit, glyphon)
- Some features may have minor differences from the TUI backend
