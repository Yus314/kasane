# Kasane

An operating system for [Kakoune](https://kakoune.org/)'s UI — a plugin-extensible frontend with dual TUI/GPU backends.

> **Status: Alpha** — Core features work but the API is unstable. Expect breaking changes.

<!-- TODO: add screenshot or GIF demo here -->

## Philosophy

Kakoune follows the Unix philosophy: it does one thing well — code editing — and composes with external tools for everything else. Window management is left to tmux or system window managers. Extensions are shell scripts interacting via `%sh{}` and `kak -p`, not plugins — by deliberate design.

Its client-server architecture also exposes a `kak -ui json` protocol that enables alternative frontends. This means Kakoune users who want richer UI — git gutter signs, LSP diagnostics, fuzzy finders, or custom pane layouts — must assemble solutions across tmux, shell scripts, and window managers, each with its own interface and no way to share or combine them.

Kasane builds on this foundation to fill the gap. It is an **operating system for editor UI**: a platform that provides primitives — elements, layout, state access, commands, and input hooks — so that plugins can build anything from small decorations to entire window management systems.

- **Plugin-first** — Kasane itself is minimal. Features belong in plugins, not the core.
- **Graduated freedom** — Three extension mechanisms (Slot, Decorator, Replacement) offer increasing levels of control, from injecting elements at named points to replacing entire components.
- **Declarative** — TEA (The Elm Architecture) with a pure `view()` function. Plugins declare what to render, not how to render it.
- **Performance as prerequisite** — A plugin platform that slows down the editor is not viable. ~49 µs/frame at 80×24, leaving plugins ample headroom.

## Features

- **Dual backend** — TUI (crossterm) and GPU (wgpu + glyphon)
- **Plugin system** — Slot / Decorator / Replacement extension points; WASM (Component Model) and native (`#[kasane::plugin]`) plugins
- **Compiled rendering** — PaintPatch, section caching, DirtyFlags-based memoization
- **System clipboard** — direct integration via arboard (no xclip/xsel)
- **Smooth scrolling** — with inertia support
- **True 24-bit color** — no palette approximation
- **CJK & emoji** — independent width calculation

## Installation

Requires [Rust](https://rustup.rs/) (stable) and [Kakoune](https://kakoune.org/).

```bash
git clone https://github.com/Yus314/kasane.git
cd kasane

# TUI only (default)
cargo install --path kasane

# With GPU backend
cargo install --path kasane --features gui
```

## Usage

```
kasane [options] [kak-options] [file]... [+<line>[:<col>]|+:]
```

```bash
kasane file.txt              # Edit with default backend
kasane --ui gui file.txt     # Edit with GPU backend
kasane -c project            # Connect to existing session
```

Non-UI kak flags (`-l`, `-f`, `-p`, etc.) are delegated directly to `kak`.

Configuration: `~/.config/kasane/config.toml` — see [docs/config.md](docs/config.md).

## Architecture

See [docs/architecture.md](docs/architecture.md).

## Performance

See [docs/performance.md](docs/performance.md).

## Plugins

Kasane supports external plugins as standalone Rust binaries and WASM modules. See [docs/plugin-development.md](docs/plugin-development.md) and [examples/line-numbers/](examples/line-numbers/).

## Contributing

```bash
cargo test                             # Run all tests
cargo clippy -- -D warnings            # Lint
cargo fmt --check                      # Format check
```

## License

MIT OR Apache-2.0
