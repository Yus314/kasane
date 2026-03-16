# Kasane

[![CI](https://github.com/Yus314/kasane/actions/workflows/ci.yml/badge.svg)](https://github.com/Yus314/kasane/actions/workflows/ci.yml)
[![License: MIT OR Apache-2.0](https://img.shields.io/badge/license-MIT%2FApache--2.0-blue)](LICENSE)

Extensible Kakoune frontend. Drop in, then grow.

Your kakrc works unchanged. Kasane adds a GPU backend, independent
rendering, and a plugin system spanning the full UI — all optional,
always compatible.

<p align="center">
  <img src="docs/assets/demo.gif" alt="Kasane demo" width="800"><br>
  <sub>GPU backend · Cursor line highlight and fuzzy finder are example WASM plugins</sub>
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

Requires [Rust](https://rustup.rs/) (stable) and [Kakoune](https://kakoune.org/) (2024.12.09 or later).

```bash
git clone https://github.com/Yus314/kasane.git
cd kasane
cargo install --path kasane               # TUI only
cargo install --path kasane --features gui # With GPU backend
```

Nix: `nix run github:Yus314/kasane`

See [Getting Started](docs/getting-started.md) for detailed setup instructions.

## What's Different

Out of the box, Kasane provides:

- **Flicker-free rendering** — double-buffered with synchronized updates
- **CJK & emoji** — independent Unicode width calculation
- **System clipboard** — direct integration (no xclip/xsel needed)
- **True 24-bit color** — no palette approximation
- **Mouse drag scrolling** — works immediately

Opt-in via configuration: smooth scrolling, themes, border styles, and search dropdown. A [WASM plugin system](docs/using-plugins.md) lets you build fuzzy finders, line decorations, overlay pickers, and more — several [example plugins](examples/wasm/) are included.

See [What's Different](docs/whats-different.md) for the full list.

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

Kasane's plugin system supports WASM and native plugins. Plugins are
distributed as single `.wasm` files, auto-discovered at startup. Each
plugin runs sandboxed, composes with others without conflict, and adds
no overhead to the rendering pipeline thanks to automatic caching.

The repository includes [example plugins](examples/wasm/) demonstrating
the available extension points. See [Plugin Development](docs/plugin-development.md)
and [Plugin API](docs/plugin-api.md). The plugin API is unstable —
expect breaking changes.

## Documentation

See [docs/index.md](docs/index.md) for the full documentation index.

## Contributing

See [CONTRIBUTING.md](CONTRIBUTING.md) for development setup and guidelines.

```bash
cargo test                             # Run all tests
cargo clippy -- -D warnings            # Lint
cargo fmt --check                      # Format check
```

## License

MIT OR Apache-2.0
