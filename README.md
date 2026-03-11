# Kasane

An operating system for [Kakoune](https://kakoune.org/)'s UI — a plugin-extensible frontend with dual TUI/GPU backends.

> **Status: Alpha** — Core features work but the API is unstable. Expect breaking changes.

<!-- TODO: add screenshot or GIF demo here -->

## Philosophy

Kakoune's design delegates UI extension to plugins and external tools. In practice, this means relying on tmux, window managers, and ad-hoc scripts — with no unified platform for plugin authors to build on. The editor's own `kak -ui json` protocol cleanly separates the engine from the frontend, yet no frontend has leveraged this to provide a true plugin infrastructure.

Kasane fills this gap. It is an **operating system for editor UI**: a platform that provides primitives — declarative elements, layout, state access, side-effect commands, and input hooks — so that plugins can build anything from gutter decorations to fuzzy finders to entire window management systems.

- **Plugin-first** — Kasane itself is minimal. Features belong in plugins, not the core.
- **Graduated freedom** — Three extension mechanisms (Slot, Decorator, Replacement) offer increasing levels of control, from injecting elements at named points to replacing entire components.
- **Declarative** — TEA (The Elm Architecture) with a pure `view()` function. Plugins declare what to render, not how to render it.
- **Performance as prerequisite** — A plugin platform that slows down the editor is not viable. The rendering pipeline runs in ~49 µs/frame, leaving plugins ample headroom.
- **Separation as virtue** — Kakoune's engine/frontend split is a deliberate design choice. Kasane honors it: the editor engine is untouched, and the frontend is fully replaceable.

## Features

- **Declarative UI** — Element tree + TEA (The Elm Architecture)
- **Plugin system** — Slot / Decorator / Replacement extension points with `#[kasane::plugin]` proc macro
- **Dual backend** — TUI (crossterm) and GPU (wgpu + glyphon)
- **~49 µs/frame** — 328x headroom vs 16 ms budget at 60 FPS
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
cargo build --release

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

~49 µs/frame CPU pipeline at 80×24 (328x headroom vs 16 ms @ 60 FPS). See [docs/performance.md](docs/performance.md) for details.

## Plugins

Kasane supports external plugins as standalone Rust binaries. See [docs/external-plugins.md](docs/external-plugins.md) and [examples/line-numbers/](examples/line-numbers/).

## Contributing

```bash
cargo test                             # Run all tests
cargo clippy -- -D warnings            # Lint
cargo fmt --check                      # Format check
```

## License

MIT OR Apache-2.0
