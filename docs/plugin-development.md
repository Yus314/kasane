# Kasane Plugin Development Guide: Declarative UI

This document is the quickstart guide for writing Kasane plugins.
For API details, see [plugin-api.md](./plugin-api.md). For composition order and correctness conditions, see [semantics.md](./semantics.md).

## 1. Introduction

### 1.1 Target Audience and Development Paths

There are two development paths for Kasane plugins.

| | WASM (recommended) | Native |
|---|---|---|
| Safety | Runs inside a sandbox | Same address space as the host process |
| Distribution | Place `.wasm` files in `plugins/` | Distribute as a custom binary |
| API | Via WIT (`host-state` + `element-builder`) | Direct `&AppState` reference |
| Dependencies | `kasane-plugin-sdk` + `wit-bindgen` | `kasane` + `kasane-core` |

The WASM path is recommended for first-time plugin development. The native path is suited for cases that require full access to `&AppState` or need to use escape hatches that do not yet have WASM parity. With native, you can both directly implement the `Plugin` trait (state-externalized, recommended) and use proc macro assistance. For advanced use cases requiring `Surface`, `PaintHook`, or pane lifecycle, implement the `PluginBackend` trait instead.

### 1.2 How to Read This Guide

1. First, run the WASM example in `## 2. Quick Start` as-is
2. Then look up the extension point you want to use in [plugin-api.md](./plugin-api.md)
3. Only read [semantics.md](./semantics.md) when changing the semantics of `transform()` / `stable()` / cache

> Note: Kasane is moving toward treating `display transformation` and `display unit` as first-class concepts, but the dedicated APIs are not yet complete. The current shared APIs are being incrementally validated through combinations of `contribute_to()`, `annotate_line_with_ctx()`, `contribute_overlay_with_ctx()`, and `transform()`. `Surface` and `PaintHook` are native escape hatches, and will be redesigned toward WASM parity in the long term.

### 1.3 Design Philosophy

- Plugins describe "what to display"; the framework decides "how to render it"
- Extensions offer progressive levels of freedom through `contribute_to()`, `annotate_line_with_ctx()`, `transform()`, etc.
- Bold restructuring of the display is permitted as a future direction, but fabricating protocol truth is not allowed
- Kasane is a UI foundation specifically for Kakoune; becoming a general-purpose UI framework is a non-goal

### 1.4 What Plugins Can Achieve

The following shows examples of what each mechanism can achieve.

| Mechanism | Achievable Examples |
|---|---|
| `contribute_to()` | Line numbers, selection cursor count badge, Git diff markers, breadcrumbs |
| `annotate_line_with_ctx()` | Cursor line highlight, indent guides, changed line markers |
| `contribute_overlay_with_ctx()` | Color picker, tooltips, diagnostic popups |
| `transform()` | Status bar customization, menu layout changes |
| `handle_key()` + `handle_mouse()` | Interactive UI (pickers, dialogs) |
| `Surface` (currently native only) | Sidebars, file trees, dedicated panels |

Filesystem access is available through WASI capability declaration (`Capability::Filesystem`). External process execution (e.g., fuzzy finders) requires declaring `Capability::Process`, spawning a process with `Command::SpawnProcess`, and receiving stdout/stderr/exit via `Plugin::on_io_event()` (Phase P-2). For details, see [plugin-api.md §0](./plugin-api.md#0-scope-of-the-plugin-api).

`Command::Session(SessionCommand::Spawn { .. })` / `Close { .. }` allows adding or terminating Kakoune sessions managed by the host runtime. Setting `activate: true` makes the new session immediately active, and all subsequent Kakoune events, surface events, and command execution operate on that session. In V1, Kakoune events from inactive sessions are still reflected in an off-screen snapshot, but only the active session is rendered; automatic surface generation for inactive sessions is not yet implemented.

## 2. Quick Start

### 2.1 WASM Plugin (Recommended)

The following is the complete source of a `sel-badge` plugin that displays the selection cursor count on the right side of the status bar.

```rust
// examples/wasm/sel-badge/src/lib.rs
kasane_plugin_sdk::generate!("../../../kasane-plugin-sdk/wit");

use std::cell::Cell;

use exports::kasane::plugin::plugin_api::Guest;
use kasane::plugin::element_builder;
use kasane::plugin::host_state;
use kasane::plugin::types::*;
use kasane_plugin_sdk::{dirty, slot};

thread_local! {
    static CURSOR_COUNT: Cell<u32> = const { Cell::new(0) };
}

struct SelBadgePlugin;

impl Guest for SelBadgePlugin {
    fn get_id() -> String {
        "sel_badge".to_string()
    }

    fn on_state_changed(dirty_flags: u16) -> Vec<Command> {
        if dirty_flags & dirty::BUFFER != 0 {
            CURSOR_COUNT.set(host_state::get_cursor_count());
        }
        vec![]
    }

    fn contribute_to(region: SlotId, _ctx: ContributeContext) -> Option<Contribution> {
        kasane_plugin_sdk::route_slot_ids!(region, {
            STATUS_RIGHT => {
                let count = CURSOR_COUNT.get();
                if count > 1 {
                    let text = format!(" {} sel ", count);
                    let face = Face {
                        fg: Color::DefaultColor,
                        bg: Color::DefaultColor,
                        underline: Color::DefaultColor,
                        attributes: 0,
                    };
                    let el = element_builder::create_text(&text, face);
                    Some(Contribution {
                        element: el,
                        priority: 0,
                        size_hint: ContribSizeHint::Auto,
                    })
                } else {
                    None
                }
            },
        })
    }

    fn contribute_deps(region: SlotId) -> u16 {
        kasane_plugin_sdk::route_slot_id_deps!(region, {
            STATUS_RIGHT => dirty::BUFFER,
        })
    }

    fn state_hash() -> u64 {
        CURSOR_COUNT.get() as u64
    }

    // Legacy WIT stubs (still required by the interface)
    kasane_plugin_sdk::default_contribute!();
    kasane_plugin_sdk::default_line!();
    kasane_plugin_sdk::default_overlay!();
    kasane_plugin_sdk::default_decorate!();
    kasane_plugin_sdk::default_replace!();
    kasane_plugin_sdk::default_decorator_priority!();
    kasane_plugin_sdk::default_named_slot!();

    // Shared API defaults
    kasane_plugin_sdk::default_init!();
    kasane_plugin_sdk::default_shutdown!();
    kasane_plugin_sdk::default_input!();
    kasane_plugin_sdk::default_surfaces!();
    kasane_plugin_sdk::default_render_surface!();
    kasane_plugin_sdk::default_handle_surface_event!();
    kasane_plugin_sdk::default_handle_surface_state_changed!();
    kasane_plugin_sdk::default_menu_transform!();
    kasane_plugin_sdk::default_update!();
    kasane_plugin_sdk::default_cursor_style!();
    kasane_plugin_sdk::default_transform!();
    kasane_plugin_sdk::default_transform_priority!();
    kasane_plugin_sdk::default_annotate!();
    kasane_plugin_sdk::default_overlay_v2!();
    kasane_plugin_sdk::default_transform_deps!();
    kasane_plugin_sdk::default_annotate_deps!();
    kasane_plugin_sdk::default_capabilities!();
}

export!(SelBadgePlugin);
```

Commands returned by `handle_surface_event(...)` and `handle_surface_state_changed(...)` are passed to the host side with the surface owner plugin as the source. Capability checks for deferred commands such as `SpawnProcess` are also performed against this owner plugin, so the same permission model as regular plugin commands applies in hosted surface handlers.

**Project Setup:**

```toml
# Cargo.toml
[package]
name = "sel-badge"
version = "0.1.0"
edition = "2024"

[lib]
crate-type = ["cdylib"]

[dependencies]
kasane-plugin-sdk = { path = "../../kasane-plugin-sdk" }
wit-bindgen = "0.41"
```

**Build & Deploy:**

```bash
cargo build --target wasm32-wasip2 --release
cp target/wasm32-wasip2/release/sel_badge.wasm ~/.local/share/kasane/plugins/
```

### 2.2 Native Plugin (PluginBackend)

> **Note:** For most native plugins, the `Plugin` trait (state-externalized, section 2.3) is recommended. Use `PluginBackend` only when you need `Surface`, `PaintHook`, pane lifecycle, or other advanced framework integration.

```rust
// examples/line-numbers/src/main.rs
use kasane::kasane_core::plugin_prelude::*;

struct LineNumbersPlugin;

impl PluginBackend for LineNumbersPlugin {
    fn id(&self) -> PluginId {
        PluginId("line_numbers".into())
    }

    fn capabilities(&self) -> PluginCapabilities {
        PluginCapabilities::CONTRIBUTOR
    }

    fn contribute_to(
        &self,
        region: &SlotId,
        state: &AppState,
        _ctx: &ContributeContext,
    ) -> Option<Contribution> {
        if region != &SlotId::BUFFER_LEFT {
            return None;
        }

        let total = state.lines.len();
        let width = total.to_string().len().max(2);

        let children: Vec<_> = (0..total)
            .map(|i| {
                let num = format!("{:>w$} ", i + 1, w = width);
                FlexChild::fixed(Element::text(
                    num,
                    Face {
                        fg: Color::Named(NamedColor::Cyan),
                        ..Face::default()
                    },
                ))
            })
            .collect();

        Some(Contribution {
            element: Element::column(children),
            priority: 0,
            size_hint: ContribSizeHint::Auto,
        })
    }

    fn contribute_deps(&self, _region: &SlotId) -> DirtyFlags {
        DirtyFlags::BUFFER_CONTENT
    }
}

fn main() {
    kasane::run(|registry| {
        registry.register_backend(Box::new(LineNumbersPlugin));
    });
}
```

```toml
# Cargo.toml
[dependencies]
kasane = { path = "../kasane" }
kasane-core = { path = "../kasane-core" }
```

Directly implement the `PluginBackend` trait and register the plugin with `kasane::run()` to distribute as a custom binary. Use `PluginCapabilities` to declare which features are used. The `#[kasane_plugin]` macro is convenient for supported hooks, but direct implementation is required for some features where hook parity is not yet complete.

### 2.3 Native Plugin (Recommended)

`Plugin` is the recommended model for native plugins. The framework owns the state; all methods are pure functions receiving state as a parameter and returning new state + effects.

```rust
use kasane::kasane_core::plugin_prelude::*;

#[derive(Clone, Debug, PartialEq, Default)]
struct CursorLineState {
    active_line: i32,
}

struct CursorLinePlugin;

impl Plugin for CursorLinePlugin {
    type State = CursorLineState;

    fn id(&self) -> PluginId {
        PluginId("cursor_line".into())
    }

    fn capabilities(&self) -> PluginCapabilities {
        PluginCapabilities::ANNOTATOR
    }

    fn on_state_changed(
        &self,
        state: &Self::State,
        app: &AppState,
        dirty: DirtyFlags,
    ) -> (Self::State, Vec<Command>) {
        if dirty.intersects(DirtyFlags::BUFFER) {
            (CursorLineState { active_line: app.cursor_pos.line }, vec![])
        } else {
            (state.clone(), vec![])
        }
    }

    fn annotate_line_with_ctx(
        &self,
        state: &Self::State,
        line: usize,
        _app: &AppState,
        _ctx: &AnnotateContext,
    ) -> Option<LineAnnotation> {
        if line as i32 == state.active_line {
            Some(LineAnnotation {
                left_gutter: None,
                right_gutter: None,
                background: Some(BackgroundLayer {
                    face: Face { bg: Color::Named(NamedColor::Blue), ..Face::default() },
                    z_order: 0,
                    blend: BlendMode::Opaque,
                }),
                priority: 0,
            })
        } else {
            None
        }
    }

    fn annotate_deps(&self) -> DirtyFlags {
        DirtyFlags::BUFFER
    }
}

fn main() {
    kasane::run(|registry| {
        registry.register(CursorLinePlugin);
    });
}
```

**Key differences from `PluginBackend`:**

| Aspect | `PluginBackend` (internal) | `Plugin` (recommended) |
|---|---|---|
| State mutations | `&mut self` methods | `(&self, &State) → (State, effects)` |
| State hash | Manual `state_hash()` | Automatic (framework compares via `PartialEq`) |
| Registration | `registry.register_backend(Box::new(..))` | `registry.register(..)` |
| State type | Implicit (fields on impl struct) | Explicit `type State` (must derive `Clone + PartialEq + Debug + Default`) |

## 3. Further Reading

| Purpose | Document to Read |
|---|---|
| Understand the differences between `contribute_to`, `transform`, `annotate_line_with_ctx`, and `contribute_overlay_with_ctx` | [plugin-api.md](./plugin-api.md) |
| Learn about the future direction of `display transformation` / `display unit` | [plugin-api.md](./plugin-api.md), [semantics.md](./semantics.md) |
| Look up how to create an `Element` | [plugin-api.md](./plugin-api.md) |
| Check `host-state`, input, and `Command` | [plugin-api.md](./plugin-api.md) |
| Use `state_hash()`, `contribute_deps()`, or `PaintHook` | [plugin-api.md](./plugin-api.md) |
| Use `Surface`, `Workspace`, or custom slots | [plugin-api.md](./plugin-api.md) |
| Check composition order, `stable()`, and observational equivalence | [semantics.md](./semantics.md) |
| Learn about dominant performance costs and measurement results | [performance.md](./performance.md) |

## 4. Registration and Distribution

### 4.1 Registration Order

Kasane registers plugins in the following order:

1. Example WASM (embedded in the binary)
2. FS-discovered WASM (`~/.local/share/kasane/plugins/*.wasm`)
3. Native plugins registered via `kasane::run(|registry| { ... })`

An FS-discovered WASM plugin with the same ID can override an example plugin.

### 4.2 Distribution Methods

- WASM: Place `.wasm` files in `~/.local/share/kasane/plugins/`
- Native: Distribute as a custom binary using `kasane::run()`

### 4.3 Control via config.toml

```toml
[plugins]
disabled = ["color_preview"]

# Per-plugin WASI capability denial
[plugins.deny_capabilities]
untrusted_plugin = ["filesystem", "environment"]
```

### 4.4 WASI Capabilities

WASM plugins can declare required WASI capabilities via `requested_capabilities()`.
The host configures a WASI context per plugin based on the declarations.

Available capabilities:

| Capability | Effect | Default |
|---|---|---|
| `Capability::Filesystem` | Preopens `data/` (plugin-specific data directory, read/write) and `.` (CWD, read-only) | Disabled |
| `Capability::Environment` | Inherits host environment variables | Disabled |
| `Capability::MonotonicClock` | Access to a monotonic clock (enabled by default, but declaration enables auditing) | Enabled |

```rust
// Example of a plugin that needs filesystem access
fn requested_capabilities() -> Vec<Capability> {
    vec![Capability::Filesystem]
}
```

Capabilities are granted upon declaration. Users can deny them via `deny_capabilities` in `config.toml`.

Constraint: WASI capabilities are available from `on_init()` onward. They are not available during component initialization (`_initialize`).

### 4.5 Testing

Unit tests can be written using `PluginRegistry` directly.

```rust
#[test]
fn my_plugin_contributes_gutter() {
    let mut registry = PluginRegistry::new();
    registry.register(MyPlugin);  // Plugin trait (state-externalized)
    // or: registry.register_backend(Box::new(MyBackendPlugin));  // PluginBackend trait

    let state = AppState::default();
    let _ = registry.init_all(&state);

    let contributions = registry.collect_contributions(&SlotId::BUFFER_LEFT, &state);
    assert_eq!(contributions.len(), 1);
}
```

## 5. Example Plugin List

| Plugin | Path | Lines | Main Features |
|---|---|---|---|
| cursor-line (WASM) | `examples/wasm/cursor-line/` | 73 lines | `annotate_line_with_ctx()`, `state_hash()` |
| sel-badge (WASM) | `examples/wasm/sel-badge/` | 111 lines | `contribute_to()` (`STATUS_RIGHT`) |
| line-numbers (WASM) | `examples/wasm/line-numbers/` | 92 lines | `contribute_to()` (`BUFFER_LEFT`) |
| color-preview (WASM) | `examples/wasm/color-preview/` | 641 lines | `annotate_line_with_ctx()`, `contribute_overlay_with_ctx()`, `handle_mouse()` |
| fuzzy-finder (WASM) | `examples/wasm/fuzzy-finder/` | 620 lines | `contribute_overlay_with_ctx()`, `handle_key()`, `Command::SpawnProcess` |
| line-numbers (native) | `examples/line-numbers/` | 57 lines | Direct `PluginBackend` trait implementation, `contribute_to()`, `kasane::run()` |
| cursor-line-pure (test) | `kasane-core/src/plugin/pure.rs` | test double | `Plugin` implementation (state-externalized), `annotate_line_with_ctx()`, automatic state tracking |
| color-preview-pure (test) | `kasane-core/src/plugin/pure.rs` | test double | `Plugin` with complex `HashMap` state |

## 6. Appendix: WASM vs Native Comparison

| Aspect | WASM | Native (`Plugin`, recommended) | Native (`PluginBackend`, internal) |
|---|---|---|---|
| Safety | Sandbox isolation | Same process as host | Same process as host |
| Performance | WASM boundary crossing cost | Direct function calls | Direct function calls |
| API access | `host-state` + `element-builder` | Direct `&AppState` + `&State` | Direct `&AppState` reference |
| Distribution | `.wasm` file placement | Custom binary | Custom binary |
| Developer experience | SDK macros + `wit-bindgen` | Derive `Clone + PartialEq + Debug + Default` on state | `#[kasane::plugin]` macro |
| `Surface` / `PaintHook` | Not supported | Not supported (use `PluginBackend`) | Supported |
| State model | Mutable (guest linear memory) | Externalized (framework-owned, pure functions) | Mutable (`&mut self`) |
| Cache invalidation | Manual `state_hash()` | Automatic (`PartialEq` comparison) | Manual `state_hash()` |
| Salsa compatibility | Not directly | Future path to Salsa memoization | Not directly |
| Inter-plugin communication | `Vec<u8>` | `Box<dyn Any>` | `Box<dyn Any>` |

## 7. Related Documents

- [plugin-api.md](./plugin-api.md) — API details
- [semantics.md](./semantics.md) — Composition order and correctness conditions
- [repo-layout.md](./repo-layout.md) — Code locations
- [index.md](./index.md) — Entry point for all docs
