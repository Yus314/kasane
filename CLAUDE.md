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
| `kasane-wasm/` | WASM plugin runtime — wasmtime Component Model host, pre-built example plugins (`bundled/`) |
| `kasane-plugin-sdk/` | SDK for WASM guest plugins — WIT bindings, constants, helper macros |
| `kasane-plugin-sdk-macros/` | Proc macros for WASM SDK — `define_plugin!` all-in-one macro |
| `kasane-wasm-bench/` | WASM benchmarks — wasmtime Component Model overhead measurement (Phase W0) |
| `examples/wasm/` | WASM plugin examples — cursor-line, color-preview, sel-badge, fuzzy-finder, prompt-highlight, session-ui |
| `examples/line-numbers/` | Native plugin example — direct `PluginBackend` trait implementation |
| `tools/wasm-test/` | WASM integration test binary |

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
- **View**: `view()` in `render/view/mod.rs` — pure function from state to `Element` tree. Salsa incremental computation is the sole caching layer for the rendering pipeline

### Element Tree

Defined in `kasane-core/src/element.rs`: `Text`, `StyledLine`, `Flex`, `Grid`, `Stack`, `Scrollable`, `Container`, `Interactive`, `Empty`. Layout uses flexbox (`layout/flex.rs`) for main content, grid (`layout/grid.rs`) for 2D table layouts, and overlay positioning (`layout/position.rs`) for menus/info popups.

### Plugin System

Two native plugin models in `kasane-core/src/plugin/`:
- **`Plugin` trait** (`pure.rs`): State-externalized model (primary user-facing API). Framework owns state; all methods are pure functions `(&self, &State) → (State, effects)`. Automatic cache invalidation via `PartialEq`. Register via `registry.register()`.
- **`PluginBackend` trait** (`traits.rs`): Mutable state model (`&mut self`). Internal framework trait with full access to all extension points including `Surface`, `PaintHook`, pane lifecycle. Register via `registry.register_backend(Box::new(...))`.

`PluginBridge` (`pure.rs`) adapts `Plugin` to `PluginBackend`, enabling both models to coexist in `PluginRegistry`.

Five main extension mechanisms (shared by both models):
- **Contribution** (`contribute_to`): Inject elements at named `SlotId` insertion points (e.g., `BUFFER_LEFT`, `STATUS_RIGHT`)
- **Transform** (`transform`): Modify or replace existing elements by `TransformTarget`, with priority ordering
- **Line Annotation** (`annotate_line_with_ctx`): Add per-line gutter/background decorations
- **Overlay** (`contribute_overlay_with_ctx`): Add floating overlay elements
- **Display Transform** (`display_directives`): Declare display-level transformations (fold, virtual text, hide) via `DisplayDirective`. The core builds a `DisplayMap` providing O(1) bidirectional mapping between buffer lines and display lines. Requires `PluginCapabilities::DISPLAY_TRANSFORM`.

Cache invalidation for plugin contributions is handled entirely by Salsa incremental computation. Plugins no longer need to declare dependency flags via `*_deps()` methods.

`PluginRegistry` in `kasane-core/src/plugin/registry.rs` collects and applies contributions during `view()`.

External crates can create plugins using `kasane_core::plugin_prelude` and register them via `kasane::run(|registry| { ... })`. The `Plugin` trait (state-externalized) is the recommended API for new plugins; `PluginBackend` is for internal/advanced use cases. See `docs/plugin-development.md`, `examples/line-numbers/`, and `examples/virtual-text-demo/`.

## Conventions

- **Commit messages**: English, conventional commits (`feat(scope):`, `fix:`, `refactor:`, `perf:`, `docs:`)
- **Documentation**: `docs/` directory (requirements, architecture, ADRs, roadmap) — in English
- **Rust edition**: 2024
- **Dev environment**: Nix flake + direnv (provides Rust toolchain, GUI deps, pre-commit hooks)
- **Performance**: ~49 μs CPU per frame at 80×24 (~21 μs with line-dirty optimization). TUI backend I/O: ~58 μs at 80×24 (ADR-015 draw_grid). Benchmarks tracked in CI with 115% alert threshold
