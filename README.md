# Kasane

Extensible Kakoune frontend — independent rendering, GPU backend, WASM plugins.
Drop in, then grow.

<p align="center">
  <img src="docs/assets/demo.gif" alt="Kasane demo" width="800"><br>
  <sub>GPU backend · Cursor line highlight and fuzzy finder are WASM plugins running sandboxed</sub>
</p>

[![CI](https://github.com/Yus314/kasane/actions/workflows/ci.yml/badge.svg)](https://github.com/Yus314/kasane/actions/workflows/ci.yml)
[![License: MIT OR Apache-2.0](https://img.shields.io/badge/license-MIT%2FApache--2.0-blue)](LICENSE-MIT)

[Getting Started](docs/getting-started.md) · [What's Different](docs/whats-different.md) · [Configuration](docs/config.md) · [Using Plugins](docs/using-plugins.md) · [Plugin Development](docs/plugin-development.md) · [Plugin API](docs/plugin-api.md) · [Vision](docs/vision.md)

## What You Get

Your kakrc works unchanged. `alias kak=kasane` and these improvements
apply automatically:

- **Flicker-free rendering** — independent pipeline at ~59 μs per frame
- **Multi-pane without tmux** — native splits with per-pane status bars
- **Clipboard that just works** — Wayland, X11, macOS, SSH forwarding
- **Correct Unicode** — independent width calculation, CJK and emoji handled

Opt in to smooth scrolling, GPU backend (`--ui gui`), themes, border
styles, and search dropdown.
See [What's Different](docs/whats-different.md) for the full list.

## Quick Start

```bash
# Requires Rust toolchain and Kakoune (2024.12.09+)
cargo install --path kasane

# Use it — your Kakoune config works unchanged
kasane file.txt

# Make it your default
alias kak=kasane  # add to .bashrc / .zshrc
```

Arch Linux: `yay -S kasane-bin`
· macOS: `brew install Yus314/kasane/kasane`
· Nix: `nix run github:Yus314/kasane`

GPU backend: `cargo install --path kasane --features gui`, then
`kasane --ui gui`.

See [Getting Started](docs/getting-started.md) for detailed setup.

## Plugins

Kakoune's `-ui json` protocol decouples editor from renderer. Kasane
builds on this with a plugin system that opens the full UI to extension —
floating overlays, line annotations, virtual text, code folding, gutter
decorations, input handling, scroll policies, and session management.

The repository includes [example plugins](examples/wasm/) you can
try today:

| Plugin | What it does |
|---|---|
| [cursor-line](examples/wasm/cursor-line/) | Highlight the active line with theme-aware colors |
| [fuzzy-finder](examples/wasm/fuzzy-finder/) | fzf-powered file picker as a floating overlay |
| [sel-badge](examples/wasm/sel-badge/) | Show selection count in the status bar |
| [color-preview](examples/wasm/color-preview/) | Inline color swatches next to hex values |
| [smooth-scroll](examples/wasm/smooth-scroll/) | Animated scrolling |
| [prompt-highlight](examples/wasm/prompt-highlight/) | Visual feedback when entering prompt mode |

Each plugin ships as a single `.wasm` file — sandboxed, composable,
auto-cached. Here is the full source of sel-badge:

```rust
kasane_plugin_sdk::define_plugin! {
    id: "sel_badge",

    state {
        #[bind(host_state::get_cursor_count(), on: dirty::BUFFER)]
        cursor_count: u32 = 0,
    },

    slots {
        STATUS_RIGHT(dirty::BUFFER) => |_ctx| {
            (state.cursor_count > 1).then(|| {
                auto_contribution(text(&format!(" {} sel ", state.cursor_count), default_face()))
            })
        },
    },
}
```

Start writing your own:

```bash
kasane plugin new my-plugin    # scaffold from 5 templates
kasane plugin dev              # hot-reload while you edit
```

See [Plugin Development](docs/plugin-development.md) and
[Plugin API](docs/plugin-api.md).

## Status

Kasane is stable as a Kakoune frontend — `alias kak=kasane` and use it
daily. The plugin API is still evolving; expect breaking changes if
you write plugins. The current WASM plugin ABI is `kasane:plugin@0.22.0`;
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

## Contributing

See [CONTRIBUTING.md](CONTRIBUTING.md) for development setup and guidelines.

```bash
cargo test                             # Run all tests
cargo clippy -- -D warnings            # Lint
cargo fmt --check                      # Format check
```

## License

MIT OR Apache-2.0
