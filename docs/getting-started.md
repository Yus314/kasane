# Getting Started

## Prerequisites

- [Kakoune](https://kakoune.org/) 2024.12.09 or later (AUR package installs this automatically)

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

> macOS and Linux x86_64 (glibc) builds include the GPU backend. Use `kasane --ui gui` to launch the GUI, or set `backend = "gui"` in config.toml. Other targets (musl, aarch64-linux) are TUI-only.

### From Source

```bash
git clone https://github.com/Yus314/kasane.git
cd kasane

# TUI only
cargo install --path kasane

# With GPU backend
cargo install --path kasane --features gui
```

Requires [Rust](https://rustup.rs/) stable toolchain.

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

Kasane reads configuration from:

```
~/.config/kasane/config.toml
```

No config file is needed — all defaults match Kakoune's standard behavior. Create a config file only when you want to change something.

Example:

```toml
[ui]
shadow = false
border_style = "double"

[scroll]
smooth = true
```

See [config.md](config.md) for the full configuration reference.

## Next Steps

- [What's Different](whats-different.md) — discover improvements available in Kasane
- [Using Plugins](using-plugins.md) — extend Kasane with plugins
- [Troubleshooting](troubleshooting.md) — common issues and solutions
