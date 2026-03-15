# Kasane

Extensible Kakoune frontend. Drop in, then grow.

Your kakrc works unchanged. Kasane adds a plugin system, GPU backend,
and independent rendering — all optional, always compatible.

<p align="center">
  <img src="docs/assets/demo.gif" alt="Kasane demo" width="800"><br>
  <sub>GPU backend · Cursor line highlight and fuzzy finder are bundled WASM plugins</sub>
</p>

## Status

Kasane is stable as a Kakoune frontend — `alias kak=kasane` and use it
daily. The plugin API is still evolving; expect breaking changes if
you write plugins.

## Quick Start

```bash
# Install (requires Rust toolchain and Kakoune)
cargo install --path kasane

# Use it
kasane file.txt

# Make it your default
alias kak=kasane  # add to .bashrc / .zshrc
```

## Installation

### From Source

Requires [Rust](https://rustup.rs/) (stable) and [Kakoune](https://kakoune.org/) (2024.12.09 or later).

```bash
git clone https://github.com/Yus314/kasane.git
cd kasane

# TUI only (default)
cargo install --path kasane

# With GPU backend
cargo install --path kasane --features gui
```

### Nix

```bash
nix run github:Yus314/kasane
```

## What's Different

Out of the box, Kasane provides:

- **Flicker-free rendering** — double-buffered with synchronized updates
- **CJK & emoji** — independent Unicode width calculation
- **System clipboard** — direct integration (no xclip/xsel needed)
- **True 24-bit color** — no palette approximation
- **Mouse drag scrolling** — works immediately

See [What's Different](docs/whats-different.md) for the full list including opt-in features.

## Usage

```
kasane [options] [kak-options] [file]... [+<line>[:<col>]|+:]
```

All Kakoune arguments work — `kasane` passes them through to `kak`.

```bash
kasane file.txt              # Edit a file
kasane -c project            # Connect to existing session
kasane -s myses file.txt     # Named session
kasane --ui gui file.txt     # GPU backend
kasane -l                    # List sessions (delegates to kak)
```

Configuration: `~/.config/kasane/config.toml` — see [docs/config.md](docs/config.md).

## Going Further

- [Getting Started](docs/getting-started.md) — installation and first run
- [What's Different](docs/whats-different.md) — discover improvements
- [Configuration](docs/config.md) — customize behavior
- [Using Plugins](docs/using-plugins.md) — extend with plugins
- [GPU Backend](docs/gpu-backend.md) — try GPU rendering
- [Compatibility](docs/compatibility.md) — version requirements and known differences

## For Plugin Authors

Kasane supports WASM and native plugins. See [Plugin Development](docs/plugin-development.md) and [Plugin API](docs/plugin-api.md). The plugin API is unstable — expect breaking changes.

## Documentation

See [docs/index.md](docs/index.md) for the full documentation index.

## Contributing

```bash
cargo test                             # Run all tests
cargo clippy -- -D warnings            # Lint
cargo fmt --check                      # Format check
```

## License

MIT OR Apache-2.0
