# Kasane

[![CI](https://github.com/Yus314/kasane/actions/workflows/ci.yml/badge.svg)](https://github.com/Yus314/kasane/actions/workflows/ci.yml)
[![License: MIT OR Apache-2.0](https://img.shields.io/badge/license-MIT%2FApache--2.0-blue)](LICENSE)

Extensible Kakoune frontend. Drop in, then grow.

Your kakrc works unchanged. Kasane sits between you and Kakoune,
providing independent rendering, a GPU backend, and a plugin system
that opens the full UI to extension — all optional, always compatible.

<p align="center">
  <img src="docs/assets/demo.gif" alt="Kasane demo" width="800"><br>
  <sub>GPU backend · Cursor line highlight and fuzzy finder are example WASM plugins</sub>
</p>

## Out of the Box

Day-to-day editing feels the same — but small annoyances quietly
disappear. An independent rendering pipeline handles flicker, Unicode
edge cases, and clipboard integration directly, without terminal
multiplexer or window manager dependencies. Zero perceptible overhead.

Opt in to smooth scrolling, themes, border styles, and search dropdown.
See [What's Different](docs/whats-different.md) for the full list.

## Quick Start

```bash
# Requires Rust toolchain and Kakoune (2024.12.09+)
cargo install --path kasane

# Use it
kasane file.txt

# Make it your default
alias kak=kasane  # add to .bashrc / .zshrc
```

Arch Linux: `yay -S kasane-bin`

macOS: `brew install Yus314/kasane/kasane`

Nix: `nix run github:Yus314/kasane`

GPU backend: `cargo install --path kasane --features gui`, then
`kasane --ui gui`.

See [Getting Started](docs/getting-started.md) for detailed setup.

## For Plugin Authors

Kakoune's `-ui json` protocol decouples the editor from its renderer,
opening the door to rich UI extension. Kasane builds on this foundation
with a plugin system spanning the full UI.

Every layer of the UI is open to extension — floating overlays, per-line
decorations, gutter annotations, display transforms like code folding
and virtual text. What terminal rendering constrains, plugins can freely
shape. The framework handles state management, caching, and lifecycle,
so plugin code focuses on what to render.

Plugins are distributed as single `.wasm` files, auto-discovered at
startup. Each runs sandboxed, composes with others without conflict, and
imposes no overhead on the rendering pipeline thanks to automatic
caching. Any language that compiles to WebAssembly works.

The repository includes [example plugins](examples/wasm/) demonstrating
the available extension points. See [Plugin Development](docs/plugin-development.md)
and [Plugin API](docs/plugin-api.md).

## Status

Kasane is stable as a Kakoune frontend — `alias kak=kasane` and use it
daily. The plugin API is still evolving; expect breaking changes if
you write plugins. The current WASM plugin ABI is `kasane:plugin@0.17.0`;
plugins built against an older ABI must be rebuilt before they will load.

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
- [What's Different](docs/whats-different.md) — full feature comparison
- [Configuration](docs/config.md) — customize behavior
- [Using Plugins](docs/using-plugins.md) — install and manage plugins
- [Plugin Development](docs/plugin-development.md) — write your own plugins
- [Plugin API](docs/plugin-api.md) — API reference
- [Vision](docs/vision.md) — project philosophy and direction

## Contributing

See [CONTRIBUTING.md](CONTRIBUTING.md) for development setup and guidelines.

```bash
cargo test                             # Run all tests
cargo clippy -- -D warnings            # Lint
cargo fmt --check                      # Format check
```

## License

MIT OR Apache-2.0
