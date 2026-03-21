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
cargo test                               # All tests (~1100)
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
| `examples/wasm/` | WASM plugin examples — cursor-line, color-preview, sel-badge, fuzzy-finder, line-numbers, prompt-highlight, session-ui, smooth-scroll |
| `examples/line-numbers/` | Native plugin example — `Plugin` trait with `kasane::run()` |
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

### Key Module Locations

- **State (TEA)**: `kasane-core/src/state/mod.rs` (AppState), `state/update.rs` (Msg enum + update)
- **Element tree**: `kasane-core/src/element.rs`
- **Rendering**: `kasane-core/src/render/` — `pipeline_salsa.rs` (Salsa-backed entry), `view/mod.rs`, `paint.rs`
- **Layout**: `kasane-core/src/layout/flex.rs` (flexbox), `grid.rs`, `position.rs` (overlay)
- **Plugin system**: `kasane-core/src/plugin/` — `state.rs` (Plugin trait, recommended), `traits.rs` (PluginBackend, internal), `registry.rs` (PluginRuntime), `bridge.rs` (PluginBridge adapter)
- **Event loop**: `kasane-core/src/event_loop.rs` (~80KB, main dispatch)
- **Salsa integration**: `kasane-core/src/salsa_sync.rs`, `salsa_inputs.rs`, `salsa_views/`
- **Plugin prelude**: `kasane-core/src/plugin_prelude.rs` (public API for external plugins)
- **Display transform**: `kasane-core/src/display/mod.rs` (DisplayMap, DisplayDirective)

For architecture details, see `docs/architecture.md`. For plugin API reference, see `docs/plugin-api.md`. For plugin development guide, see `docs/plugin-development.md`.

## Conventions

- **Commit messages**: English, conventional commits (`feat(scope):`, `fix:`, `refactor:`, `perf:`, `docs:`)
- **Documentation**: `docs/` directory (requirements, architecture, ADRs, roadmap) — in English
- **Rust edition**: 2024
- **Dev environment**: Nix flake + direnv (provides Rust toolchain, GUI deps, pre-commit hooks)
- **Performance**: ~49 μs CPU per frame at 80×24 (~21 μs with line-dirty optimization). TUI backend I/O: ~58 μs at 80×24 (ADR-015 draw_grid). Benchmarks tracked in CI with 115% alert threshold
