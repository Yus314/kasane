# Getting Started

## Prerequisites

- [Kakoune](https://kakoune.org/) v2026.04.12 or later (AUR package installs this automatically)

## Installation

### Arch Linux (AUR)

```bash
yay -S kasane-bin    # or: paru -S kasane-bin
```

Installs a prebuilt binary. Kakoune is pulled in automatically as a dependency.

### macOS (Homebrew)

```bash
brew install Yus314/kasane/kasane
```

Installs a prebuilt binary. Kakoune is pulled in automatically as a dependency.

### Binary Release

Download a prebuilt binary from [GitHub Releases](https://github.com/Yus314/kasane/releases/latest):

```bash
# x86_64 Linux (glibc) — replace the URL with your target from the releases page
curl -LO "$(curl -s https://api.github.com/repos/Yus314/kasane/releases/latest \
  | grep -oP 'https://[^"]*x86_64-linux-gnu\.tar\.gz')"
tar xzf kasane-*-x86_64-linux-gnu.tar.gz
install -Dm755 kasane ~/.local/bin/kasane
```

Other targets: `aarch64-linux-gnu`, `x86_64-linux-musl`, `x86_64-macos`, `aarch64-macos`. See the [releases page](https://github.com/Yus314/kasane/releases/latest) for all available archives.

> macOS and Linux x86_64 (glibc) builds include the GPU backend. Use `kasane --ui gui` to launch the GUI, or set `backend "gui"` in the `ui` section of `kasane.kdl`. Other targets (musl, aarch64-linux) are TUI-only.

### From Source

```bash
git clone https://github.com/Yus314/kasane.git
cd kasane

# Default: TUI + WASM plugin support (cursor-line and color-preview bundled)
cargo install --path kasane

# Include the GPU backend
cargo install --path kasane --features gui

# Add tree-sitter syntax highlighting
cargo install --path kasane --features syntax

# Combine
cargo install --path kasane --features "gui syntax"
```

Requires [Rust](https://rustup.rs/) stable toolchain.

#### Slim-Build Matrix

The `kasane` binary composes from four optional feature flags:

| Flag | Default | Adds |
|---|---|---|
| `wasm-plugins` | **on** | WASM plugin runtime (`kasane-wasm`) + bundled `cursor_line` / `color_preview` + the lock-file resolver + plugin reload watcher (`notify`). Disable with `--no-default-features` for a TUI-only build with native plugins only. |
| `gui` | off | GPU backend (`kasane-gui`: winit + wgpu + Parley + swash). Significantly larger binary. |
| `syntax` | off | Tree-sitter syntax highlighting (`kasane-syntax`). Pre-render hook attached automatically. |
| `tui-image` | off (in `kasane-core`) | TUI image protocol support (kitty / sixel). Off by default to keep `kasane-core` slim. |

Common build invocations:

```bash
# Slimmest: TUI only, no WASM, no GUI, no syntax
cargo build --no-default-features

# TUI + WASM (the default)
cargo build

# TUI + WASM + syntax
cargo build --features syntax

# TUI + WASM + GUI + syntax (the maximum surface)
cargo build --features "gui syntax"
```

`--no-default-features` retires the entire `kasane-wasm` dependency (lock
resolver, bundled plugins, hot-reload watcher) — useful for embedded
targets or environments where the WASM runtime is not desired. Custom
native plugins built via `kasane::run()` continue to work in this mode.

### Nix

```bash
nix run github:Yus314/kasane
```

## First Run

```bash
kasane file.txt
```

You should see your file in Kakoune — same keybindings, same behavior. Kasane connects to Kakoune via the `-ui json` protocol and renders the UI independently.

## Making It Your Default

Add to your `.bashrc` or `.zshrc`:

```bash
alias kak=kasane
```

All Kakoune arguments work unchanged:

```bash
kak file.txt              # Edit a file
kak -c project            # Connect to existing session
kak -s myses file.txt     # Named session
kak -l                    # List sessions (delegates to kak)
```

## Configuration

Kasane reads configuration and widget definitions from a single file:

```
~/.config/kasane/kasane.kdl
```

No config file is needed — all defaults match Kakoune's standard behavior. Create a config file only when you want to change something.

Example:

```kdl
ui {
    shadow #false
    border_style "double"
}

scroll {
    smooth #true
}
```

See [config.md](config.md) for the full configuration reference.

## Next Steps

- [What's Different](whats-different.md) — discover improvements available in Kasane
- [Customize the UI](widgets.md) — status bar, gutters, and transforms (in the same `kasane.kdl` file)
- [Using Plugins](using-plugins.md) — extend Kasane with plugins
- [Troubleshooting](troubleshooting.md) — common issues and solutions
