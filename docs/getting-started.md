# Getting Started

## Prerequisites

- [Kakoune](https://kakoune.org/) 2024.12.09 or later
- [Rust](https://rustup.rs/) stable toolchain (for building from source)

## Installation

### From Source (recommended)

```bash
git clone https://github.com/Yus314/kasane.git
cd kasane

# TUI only
cargo install --path kasane

# With GPU backend
cargo install --path kasane --features gui
```

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
