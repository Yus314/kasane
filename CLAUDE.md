# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## What is Kasane

Kasane is an alternative frontend for the [Kakoune](https://kakoune.org/) text editor, written in Rust. It communicates with Kakoune via the JSON-RPC protocol (`kak -ui json`) and provides both a TUI (crossterm) and GPU (wgpu+glyphon) backend. The UI is built using a declarative architecture: Element tree + TEA (The Elm Architecture) + a plugin system with Slot/Decorator/Replacement extension points.

## Build & Test Commands

```bash
# Build
cargo build                              # TUI only
cargo build --features gui               # Include GPU backend

# Test
cargo test                               # All tests (~520)
cargo test -p kasane-core                # Single crate
cargo test -p kasane-core -- test_name   # Single test by name

# Lint (CI enforces -D warnings)
cargo clippy -- -D warnings              # TUI
cargo clippy --features gui -- -D warnings  # TUI + GUI
cargo fmt --check                        # Format check

# Benchmarks
cargo bench --bench rendering_pipeline   # Core rendering (criterion)
cargo bench --bench iai_pipeline         # Instruction counts (iai-callgrind, needs valgrind)
cargo bench -p kasane-tui --bench backend
cargo bench --bench replay
cargo bench -p kasane-gui --bench cpu_rendering
cargo test -p kasane-core --test latency_budget -- --ignored  # Latency budget regression test
```

## Workspace Structure

| Crate | Purpose |
|---|---|
| `kasane/` | Main binary + library — CLI parsing, Kakoune process management, `kasane::run()` entry point for custom plugin binaries |
| `kasane-core/` | Core library — protocol, state (TEA), element tree, layout, rendering, plugin system |
| `kasane-tui/` | TUI backend — crossterm-based terminal rendering |
| `kasane-gui/` | GPU backend — winit+wgpu+glyphon (feature-gated via `--features gui`) |
| `kasane-macros/` | Proc macros — `#[kasane::plugin]` and `#[kasane::component]` |
| `kasane-wasm/` | WASM plugin runtime — wasmtime Component Model host, bundled plugins (`bundled/`), guest sources (`guests/`) |
| `kasane-plugin-sdk/` | SDK for WASM guest plugins — WIT bindings, constants, helper macros |

## Architecture

### Rendering Pipeline

```
Kakoune (kak -ui json)
  → JSON-RPC parse (simd-json)
  → AppState.apply()          # protocol → state
  → view(&state, &registry)   # state → Element tree (with plugin contributions)
  → place(&element, rect)     # Element tree → Layout (flexbox + overlay positioning)
  → paint(&element, &layout)  # Layout → CellGrid (2D cell buffer)
  → grid.diff()               # Dirty cell detection
  → backend.draw()            # TUI (crossterm) or GPU (wgpu)
```

### TEA (The Elm Architecture)

- **State**: `AppState` in `kasane-core/src/state/mod.rs` — buffer, cursor, menus, info popups, options
- **Update**: `update()` in `state/update.rs` — `Msg` enum → state mutation + `Command` side-effects, with `DirtyFlags` for selective redraws
- **View**: `view()` in `render/view/mod.rs` — pure function from state to `Element` tree

### Element Tree

Defined in `kasane-core/src/element.rs`: `Text`, `StyledLine`, `Flex`, `Grid`, `Stack`, `Scrollable`, `Container`, `Interactive`, `Empty`. Layout uses flexbox (`layout/flex.rs`) for main content, grid (`layout/grid.rs`) for 2D table layouts, and overlay positioning (`layout/position.rs`) for menus/info popups.

### Plugin System

`#[kasane::plugin]` proc macro generates `Plugin` trait impls. Three extension mechanisms:
- **Slot**: Inject elements at named insertion points (e.g., `BufferLeft`, `StatusRight`, `Overlay`)
- **Decorator**: Wrap elements with modifications (borders, styles) in priority order
- **Replacement**: Replace entire components (menus, status bar)

`PluginRegistry` in `kasane-core/src/plugin.rs` collects and applies contributions during `view()`.

External crates can create plugins using `kasane_core::plugin_prelude` and register them via `kasane::run(|registry| { ... })`. See `docs/plugin-development.md` and `examples/line-numbers/`.

## Conventions

- **Commit messages**: English, conventional commits (`feat(scope):`, `fix:`, `refactor:`, `perf:`, `docs:`)
- **Documentation**: `docs/` directory (requirements, architecture, ADRs, roadmap) — currently in Japanese, being migrated to English
- **Rust edition**: 2024
- **Dev environment**: Nix flake + direnv (provides Rust toolchain, GUI deps, pre-commit hooks)
- **Performance**: ~40 μs CPU per frame at 80×24. Benchmarks tracked in CI with 115% alert threshold
