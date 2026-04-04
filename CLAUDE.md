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
| `kasane-plugin-model/` | Shared plugin model types — `PluginId`, `SettingValue`, serialization formats |
| `kasane-plugin-package/` | Plugin package format — `.kpk` build/inspect/verify, manifest parsing, filesystem utilities |
| `kasane-plugin-sdk/` | SDK for WASM guest plugins — WIT bindings, constants, helper macros |
| `kasane-plugin-sdk-macros/` | Proc macros for WASM SDK — `define_plugin!` all-in-one macro |
| `kasane-wasm-bench/` | WASM benchmarks — wasmtime Component Model overhead measurement (Phase W0) |
| `examples/wasm/` | WASM plugin examples — cursor-line, color-preview, sel-badge, fuzzy-finder, pane-manager, prompt-highlight, session-ui, smooth-scroll, image-preview |
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
  → backend.present()          # TUI: incremental diff + SGR (or GPU: wgpu)
```

### Key Module Locations

- **State (TEA)**: `kasane-core/src/state/mod.rs` (AppState), `state/update.rs` (Msg enum + update)
- **Element tree**: `kasane-core/src/element.rs`
- **Rendering**: `kasane-core/src/render/` — `pipeline_salsa.rs` (Salsa-backed entry), `view/mod.rs`, `paint.rs`
- **Layout**: `kasane-core/src/layout/flex.rs` (flexbox), `grid.rs`, `position.rs` (overlay)
- **Plugin system**: `kasane-core/src/plugin/` — `state.rs` (Plugin trait, 3 methods, HandlerRegistry-based), `handler_registry.rs` (HandlerRegistry, handler registration API including `on_transform_for()`), `handler_table.rs` (type-erased dispatch table), `bridge.rs` (PluginBridge adapter, Plugin→PluginBackend), `traits.rs` (PluginBackend, internal), `registry.rs` (PluginRuntime), `element_patch.rs` (declarative transform algebra), `compose.rs` (monoidal composition traits + types), `channel.rs` (ChannelValue cross-boundary serialization), `pubsub.rs` (topic-based inter-plugin pub/sub), `extension_point.rs` (plugin-defined extension points)
- **Event loop**: `kasane-core/src/event_loop/` — `mod.rs` (re-exports), `dispatch.rs` (command dispatch), `context.rs` (deferred context), `session.rs` (session lifecycle), `surface.rs` (surface lifecycle)
- **Workspace persistence**: `kasane-core/src/workspace/persist.rs` (layout save/restore across sessions)
- **Salsa integration**: `kasane-core/src/salsa_sync.rs`, `salsa_inputs.rs`, `salsa_views/`
- **Plugin prelude**: `kasane-core/src/plugin_prelude.rs` (public API for external plugins)
- **Display transform**: `kasane-core/src/display/mod.rs` (DisplayMap, DisplayDirective)

For architecture details, see `docs/index.md`. For plugin API reference, see `docs/plugin-api.md`. For plugin development guide, see `docs/plugin-development.md`.

## Design Philosophy

- Plugin API expressiveness is a first-class goal: building infrastructure for future plugin authors is intentional, not speculative. Do not argue against extensibility work on the basis that no current consumer exists.
- The roadmap documents planned work. Do not suggest deferring items that appear in the roadmap or that the user has explicitly requested.
- When a simpler alternative exists, present it alongside the requested approach as an option — not as a reason to defer or reject.

## Conventions

- **Commit messages**: English, conventional commits (`feat(scope):`, `fix:`, `refactor:`, `perf:`, `docs:`)
- **Documentation**: `docs/` directory (requirements, architecture, ADRs, roadmap) — in English
- **Rust edition**: 2024
- **Dev environment**: Nix flake + direnv (provides Rust toolchain, GUI deps, pre-commit hooks)
- **Performance**: ~57 μs CPU per frame at 80×24. TUI backend I/O: ~60 μs full redraw, ~30 μs incremental (1-line change) at 80×24. Benchmarks tracked in CI with 115% alert threshold. Performance policy: perceptual imperceptibility as goal and stopping condition (ADR-024). See `docs/performance.md` for current numbers
