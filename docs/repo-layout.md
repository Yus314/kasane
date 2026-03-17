# Repository Layout Guide

This document serves as a reference for the Kasane workspace structure and the responsibilities of each major directory.
For system boundaries and semantics, see [architecture.md](./architecture.md) and [semantics.md](./semantics.md).

## 1. Crate Responsibilities

| crate | Role |
|---|---|
| `kasane-core` | Protocol, state management, layout, abstract rendering, plugin infrastructure |
| `kasane-tui` | crossterm-based TUI backend |
| `kasane-gui` | winit + wgpu + glyphon-based GUI backend |
| `kasane-macros` | Proc macros such as `#[kasane::plugin]` and `#[kasane::component]` |
| `kasane` | Main binary, CLI, process management, backend selection, `kasane plugin` subcommand |
| `kasane-wasm` | WASM plugin runtime, WIT host adapter |
| `kasane-plugin-sdk` | SDK for WASM guests |
| `kasane-plugin-sdk-macros` | Proc macros for WASM SDK (`define_plugin!`) |
| `kasane-wasm-bench` | WASM benchmark harness |

## 2. Source Guide

### 2.1 `kasane-core/src/`

| Path | Contents |
|---|---|
| `element.rs` | The core `Element` type for declarative UI |
| `plugin/` | `Plugin` trait, `PluginBackend` trait, registry, context, command, I/O |
| `state/` | `AppState`, `apply()`, `update()`, dirty generation |
| `layout/` | measure/place, overlay positioning, hit test |
| `render/` | View construction, paint, cache, pipeline, scene |
| `display/` | `DisplayMap` — O(1) bidirectional mapping between buffer lines and display lines |
| `surface/` | Surface abstraction and core surface implementations |
| `workspace/` | Surface placement and split structure |
| `session.rs` | `SessionManager`, session state store, session lifecycle |
| `event_loop.rs` | Backend-agnostic deferred command handling shared by TUI and GUI |
| `protocol/` | JSON-RPC parser and message types |
| `input/` | Conversion from frontend input to Kakoune input |

### 2.2 `kasane-tui/src/`

| Path | Contents |
|---|---|
| `backend.rs` | TUI implementation of `RenderBackend` |
| `input.rs` | crossterm event conversion |
| `event_handler.rs` | TUI event loop |
| `sgr.rs` | SGR escape sequence generation for crossterm |

### 2.3 `kasane-gui/src/`

| Path | Contents |
|---|---|
| `app.rs` | winit application loop |
| `backend.rs` | GUI backend implementation |
| `animation.rs` | Animations such as smooth scroll |
| `gpu/` | GPU renderer core |

### 2.4 `kasane-macros/src/`

| Path | Contents |
|---|---|
| `plugin.rs` | Code generation for `#[kasane_plugin]` |
| `component.rs` | `#[kasane_component]`, deps, allow, validation |
| `analysis.rs` | Shared AST analysis code |

### 2.5 `kasane/src/`

| Path | Contents |
|---|---|
| `lib.rs` | `kasane::run()` |
| `main.rs` | Default binary |
| `cli.rs` | CLI arguments and `PluginSubcommand` parser |
| `process.rs` | Kakoune child process management |
| `plugin_cmd/` | `kasane plugin` subcommand handlers (new, build, install, list, doctor, dev) and embedded templates |

### 2.6 `kasane-wasm/`

| Path | Contents |
|---|---|
| `src/adapter.rs` | WASM adapter for the `PluginBackend` trait |
| `src/host.rs` | Guest-to-host calls |
| `src/capability.rs` | WASI capability resolution per plugin |
| `bundled/` | Pre-built .wasm embedded in binary via `include_bytes!` |
| `fixtures/` | Pre-built .wasm for tests |
| `guests/` | Test-only WASM guests (not user-facing examples) |

### 2.7 Auxiliary Crates

| Path | Contents |
|---|---|
| `kasane-plugin-sdk/src/lib.rs` | WIT bindings, constants, guest helper macros |
| `kasane-wasm-bench/src/lib.rs` | WASM bench harness |
| `kasane-wasm-bench/guests/` | Benchmark guest plugins |

## 3. Where to Make Changes

| Desired change | Primary locations |
|---|---|
| Changes to `AppState` or dirty flags | `kasane-core/src/state/` |
| Changes to plugin composition or registry | `kasane-core/src/plugin/` |
| Adding or modifying `Element` types | `kasane-core/src/element.rs` |
| Changes to layout algorithms | `kasane-core/src/layout/` |
| Changes to display transformation / `DisplayMap` | `kasane-core/src/display/` |
| Changes to the TUI rendering pipeline | `kasane-core/src/render/` and `kasane-tui/src/backend.rs` |
| Changes to GUI scene/pipeline | `kasane-core/src/render/scene/` and `kasane-gui/src/gpu/` |
| Proc macro deps validation | `kasane-macros/src/component.rs` and `analysis.rs` |
| Changes to plugin WIT / host API | `kasane-wasm/wit/plugin.wit`, `kasane-wasm/src/host.rs`, `kasane-plugin-sdk/src/lib.rs` |
| Changes to CLI or startup paths | `kasane/src/cli.rs`, `kasane/src/process.rs`, `kasane/src/lib.rs` |
| Changes to `kasane plugin` subcommand or templates | `kasane/src/plugin_cmd/` |
| Changes to session management | `kasane-core/src/session.rs` |
| Changes to example plugins | `examples/wasm/`, `examples/line-numbers/` |

## 4. Related Documents

- [architecture.md](./architecture.md): System boundaries and runtime architecture
- [semantics.md](./semantics.md): State, rendering, invalidation, and correctness conditions
- [plugin-api.md](./plugin-api.md): Plugin API reference for plugin authors
