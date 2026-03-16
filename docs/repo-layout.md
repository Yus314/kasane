# Repository Layout Guide

This document serves as a reference for the Kasane workspace structure and the responsibilities of each major directory.
For system boundaries and semantics, see [architecture.md](./architecture.md) and [semantics.md](./semantics.md).

## 1. Workspace Overview

```text
kasane/
├── flake.nix
├── flake.lock
├── .envrc
├── rust-toolchain.toml
├── Cargo.toml
├── kasane-core/
├── kasane-tui/
├── kasane-macros/
├── kasane-gui/
├── kasane/
├── kasane-wasm/
├── kasane-plugin-sdk/
├── kasane-plugin-sdk-macros/
├── kasane-wasm-bench/
├── examples/
│   ├── line-numbers/        # Native plugin example
│   └── wasm/                # WASM plugin examples
│       ├── cursor-line/
│       ├── color-preview/
│       ├── sel-badge/
│       ├── fuzzy-finder/
│       ├── prompt-highlight/
│       └── session-ui/
└── tools/
    └── wasm-test/           # WASM integration test binary
```

## 2. Crate Responsibilities

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

## 3. Source Tree Guide

### 3.1 `kasane-core/src/`

```text
kasane-core/src/
├── lib.rs
├── element.rs
├── plugin/
│   ├── mod.rs
│   ├── pure.rs
│   ├── traits.rs
│   ├── registry.rs
│   ├── context.rs
│   ├── command.rs
│   ├── io.rs
│   └── tests/
├── input/
│   ├── mod.rs
│   └── builtin.rs
├── config.rs
├── io.rs
├── perf.rs
├── pane.rs
├── workspace.rs
├── plugin_prelude.rs
├── test_support.rs
├── surface/
│   ├── mod.rs
│   ├── buffer.rs
│   ├── menu.rs
│   ├── status.rs
│   └── info.rs
├── bin/
│   └── alloc_budget.rs
├── protocol/
│   ├── mod.rs
│   ├── color.rs
│   ├── message.rs
│   ├── parse.rs
│   └── tests.rs
├── test_utils.rs
├── state/
│   ├── mod.rs
│   ├── apply.rs
│   ├── update.rs
│   ├── derived.rs
│   ├── snapshot.rs
│   ├── info.rs
│   ├── menu.rs
│   └── tests/
├── layout/
│   ├── mod.rs
│   ├── flex.rs
│   ├── grid.rs
│   ├── position.rs
│   ├── info.rs
│   ├── hit_test.rs
│   ├── text.rs
│   └── word_wrap.rs
└── render/
    ├── mod.rs
    ├── grid.rs
    ├── paint.rs
    ├── patch.rs
    ├── cursor.rs
    ├── pipeline.rs
    ├── cache.rs
    ├── scene/
    │   ├── mod.rs
    │   └── cache.rs
    ├── theme.rs
    ├── markup.rs
    ├── test_helpers/
    │   ├── mod.rs
    │   └── info.rs
    ├── tests/
    │   ├── mod.rs
    │   ├── pipeline.rs
    │   ├── view_cache.rs
    │   ├── scene_cache.rs
    │   └── cursor.rs
    ├── menu.rs
    └── view/
        ├── mod.rs
        ├── info.rs
        ├── menu.rs
        └── tests.rs
```

Key responsibilities:

| Path | Contents |
|---|---|
| `element.rs` | The core `Element` type for declarative UI |
| `plugin/` | `Plugin` trait, `PluginBackend` trait, registry, context, command, I/O |
| `state/` | `AppState`, `apply()`, `update()`, dirty generation |
| `layout/` | measure/place, overlay positioning, hit test |
| `render/` | View construction, paint, cache, pipeline, scene |
| `surface/` | Surface abstraction and core surface implementations |
| `workspace.rs` | Surface placement and split structure |
| `protocol/` | JSON-RPC parser and message types |
| `input/` | Conversion from frontend input to Kakoune input |

### 3.2 `kasane-tui/src/`

```text
kasane-tui/src/
├── lib.rs
├── backend.rs
└── input.rs
```

| Path | Contents |
|---|---|
| `backend.rs` | TUI implementation of `RenderBackend` |
| `input.rs` | crossterm event conversion |

### 3.3 `kasane-gui/src/`

```text
kasane-gui/src/
├── lib.rs
├── app.rs
├── backend.rs
├── input.rs
├── animation.rs
├── colors.rs
├── gpu/
│   ├── mod.rs
│   ├── cell_renderer.rs
│   ├── scene_renderer.rs
│   ├── metrics.rs
│   ├── bg_pipeline.rs
│   ├── border_pipeline.rs
│   ├── bg.wgsl
│   └── rounded_rect.wgsl
└── cpu/
    └── mod.rs
```

| Path | Contents |
|---|---|
| `app.rs` | winit application loop |
| `backend.rs` | GUI backend implementation |
| `animation.rs` | Animations such as smooth scroll |
| `gpu/` | GPU renderer core |

### 3.4 `kasane-macros/src/`

```text
kasane-macros/src/
├── lib.rs
├── plugin.rs
├── component.rs
└── analysis.rs
```

| Path | Contents |
|---|---|
| `plugin.rs` | Code generation for `#[kasane_plugin]` |
| `component.rs` | `#[kasane_component]`, deps, allow, validation |
| `analysis.rs` | Shared AST analysis code |

### 3.5 `kasane/src/`

```text
kasane/src/
├── lib.rs
├── main.rs
├── cli.rs
├── process.rs
├── process_manager.rs
└── plugin_cmd/
    ├── mod.rs
    ├── new.rs
    ├── build.rs
    ├── install.rs
    ├── list.rs
    ├── doctor.rs
    ├── dev.rs
    └── templates.rs
```

| Path | Contents |
|---|---|
| `lib.rs` | `kasane::run()` |
| `main.rs` | Default binary |
| `cli.rs` | CLI arguments and `PluginSubcommand` parser |
| `process.rs` | Kakoune child process management |
| `plugin_cmd/` | `kasane plugin` subcommand handlers (new, build, install, list, doctor, dev) and embedded templates |

### 3.6 `kasane-wasm/`

```text
kasane-wasm/
├── src/
│   ├── lib.rs
│   ├── adapter.rs
│   ├── host.rs
│   ├── convert.rs
│   └── tests.rs
├── bundled/
│   ├── cursor-line.wasm
│   ├── color-preview.wasm
│   ├── sel-badge.wasm
│   ├── fuzzy-finder.wasm
│   └── line-numbers.wasm
├── fixtures/
│   └── *.wasm              # Pre-built .wasm for tests
└── guests/
    └── surface-probe/       # Test-only WASM guest
```

| Path | Contents |
|---|---|
| `src/adapter.rs` | WASM adapter for the `PluginBackend` trait |
| `src/host.rs` | Guest-to-host calls |
| `bundled/` | Pre-built .wasm embedded in binary via `include_bytes!` |
| `fixtures/` | Pre-built .wasm for tests |
| `guests/` | Test-only WASM guests (not user-facing examples) |

### 3.7 Auxiliary Crates

| Path | Contents |
|---|---|
| `kasane-plugin-sdk/src/lib.rs` | WIT bindings, constants, guest helper macros |
| `kasane-wasm-bench/src/lib.rs` | WASM bench harness |
| `kasane-wasm-bench/guests/` | Benchmark guest plugins |

## 4. Where to Make Changes

| Desired change | Primary locations |
|---|---|
| Changes to `AppState` or dirty flags | `kasane-core/src/state/` |
| Changes to plugin composition or registry | `kasane-core/src/plugin/` |
| Adding or modifying `Element` types | `kasane-core/src/element.rs` |
| Changes to layout algorithms | `kasane-core/src/layout/` |
| Changes to the TUI rendering pipeline | `kasane-core/src/render/` and `kasane-tui/src/backend.rs` |
| Changes to GUI scene/pipeline | `kasane-core/src/render/scene/` and `kasane-gui/src/gpu/` |
| Proc macro deps validation | `kasane-macros/src/component.rs` and `analysis.rs` |
| Changes to plugin WIT / host API | `kasane-wasm/wit/plugin.wit`, `kasane-wasm/src/host.rs`, `kasane-plugin-sdk/src/lib.rs` |
| Changes to CLI or startup paths | `kasane/src/cli.rs`, `kasane/src/process.rs`, `kasane/src/lib.rs` |
| Changes to `kasane plugin` subcommand or templates | `kasane/src/plugin_cmd/` |
| Changes to example plugins | `examples/wasm/`, `examples/line-numbers/` |

## 5. Related Documents

- [architecture.md](./architecture.md): System boundaries and runtime architecture
- [semantics.md](./semantics.md): State, rendering, invalidation, and correctness conditions
- [plugin-api.md](./plugin-api.md): Plugin API reference for plugin authors
