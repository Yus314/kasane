# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## What is Kasane

Kasane is an alternative frontend for the [Kakoune](https://kakoune.org/) text editor, written in Rust. It communicates with Kakoune via the JSON-RPC protocol (`kak -ui json`) and provides both a TUI (crossterm) and GPU (wgpu+glyphon) backend. The UI is built using a declarative architecture: Element tree + TEA (The Elm Architecture) + a plugin system with Contribution/Transform/Annotation/Overlay extension points.

## Build & Test Commands

```bash
# Build
cargo build                              # TUI only
cargo build --features gui               # Include GPU backend

# Test
cargo test                               # All tests (~660)
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
| `kasane-wasm-bench/` | WASM benchmarks — wasmtime Component Model overhead measurement (Phase W0) |

## Architecture

### Rendering Pipeline

```
Kakoune (kak -ui json)
  → JSON-RPC parse (simd-json)
  → AppState.apply()          # protocol → state
  → view(&state, &registry)   # state → Element tree (with plugin contributions)
  → place(&element, rect)     # Element tree → Layout (flexbox + overlay positioning)
  → paint(&element, &layout)  # Layout → CellGrid (2D cell buffer)
  → backend.draw_grid()       # TUI: zero-copy diff + incremental SGR (or GPU: wgpu)
```

### TEA (The Elm Architecture)

- **State**: `AppState` in `kasane-core/src/state/mod.rs` — buffer, cursor, menus, info popups, options
- **Update**: `update()` in `state/update.rs` — `Msg` enum → state mutation + `Command` side-effects, with `DirtyFlags` for selective redraws
- **View**: `view()` in `render/view/mod.rs` — pure function from state to `Element` tree

### Element Tree

Defined in `kasane-core/src/element.rs`: `Text`, `StyledLine`, `Flex`, `Grid`, `Stack`, `Scrollable`, `Container`, `Interactive`, `Empty`. Layout uses flexbox (`layout/flex.rs`) for main content, grid (`layout/grid.rs`) for 2D table layouts, and overlay positioning (`layout/position.rs`) for menus/info popups.

### Plugin System

`Plugin` trait in `kasane-core/src/plugin/traits.rs` defines the plugin interface. Four main extension mechanisms:
- **Contribution** (`contribute_to`): Inject elements at named `SlotId` insertion points (e.g., `BUFFER_LEFT`, `STATUS_RIGHT`)
- **Transform** (`transform`): Modify or replace existing elements by `TransformTarget`, with priority ordering
- **Line Annotation** (`annotate_line_with_ctx`): Add per-line gutter/background decorations
- **Overlay** (`contribute_overlay_with_ctx`): Add floating overlay elements

`PluginRegistry` in `kasane-core/src/plugin/registry.rs` collects and applies contributions during `view()`.

External crates can create plugins using `kasane_core::plugin_prelude` and register them via `kasane::run(|registry| { ... })`. See `docs/plugin-development.md` and `examples/line-numbers/`.

## Conventions

- **Commit messages**: English, conventional commits (`feat(scope):`, `fix:`, `refactor:`, `perf:`, `docs:`)
- **Documentation**: `docs/` directory (requirements, architecture, ADRs, roadmap) — currently in Japanese, being migrated to English
- **Rust edition**: 2024
- **Dev environment**: Nix flake + direnv (provides Rust toolchain, GUI deps, pre-commit hooks)
- **Performance**: ~49 μs CPU per frame at 80×24 (~21 μs with line-dirty optimization). TUI backend I/O: ~58 μs at 80×24 (ADR-015 draw_grid). Benchmarks tracked in CI with 115% alert threshold
