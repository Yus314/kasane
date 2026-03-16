# Kasane Plugin Development Guide

Kasane plugins are WASM components distributed as single `.wasm` files.
Place one in `~/.local/share/kasane/plugins/` and it loads at startup
— sandboxed, composable, and automatically cached.

Plugins describe *what* to display. The framework handles rendering,
layout, and cache invalidation.

For API details, see [plugin-api.md](./plugin-api.md). For composition semantics, see [semantics.md](./semantics.md).

## What Plugins Can Do

| Mechanism | Examples |
|---|---|
| `contribute_to()` | Line numbers, git markers, status bar widgets |
| `annotate_line()` | Cursor line highlight, indent guides |
| `contribute_overlay_v2()` | Color picker, tooltips, diagnostic popups |
| `transform_element()` | Status bar customization, menu layout changes |
| `handle_key()` + `handle_mouse()` | Interactive pickers, dialogs |

> Native plugins can also use `Surface` for sidebars and dedicated panels. See [Appendix A](#appendix-a-alternative-native-plugin).

## Quick Start

### Project Setup

```toml
# Cargo.toml
[package]
name = "sel-badge"
version = "0.1.0"
edition = "2024"

[lib]
crate-type = ["cdylib"]

[dependencies]
kasane-plugin-sdk = "0.1"
wit-bindgen = "0.41"
```

You also need the `wasm32-wasip2` target:

```bash
rustup target add wasm32-wasip2
```

### Full Example: sel-badge

This plugin displays the selection count on the right side of the status bar when multiple cursors are active.

```rust
// examples/wasm/sel-badge/src/lib.rs
kasane_plugin_sdk::generate!();

use std::cell::Cell;

use exports::kasane::plugin::plugin_api::Guest;
use kasane::plugin::element_builder;
use kasane::plugin::host_state;
use kasane::plugin::types::*;
use kasane_plugin_sdk::{dirty, plugin};

thread_local! {
    static CURSOR_COUNT: Cell<u32> = const { Cell::new(0) };
}

struct SelBadgePlugin;

#[plugin]
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

    fn state_hash() -> u64 {
        CURSOR_COUNT.get() as u64
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
}

export!(SelBadgePlugin);
```

**Key points:**

- **`#[plugin]`** fills in default implementations for all `Guest` methods you don't write. Without it, you would need to add `default_*!()` macro stubs for every unused method.
- **`on_state_changed()`** is called when editor state changes. Cache data you need; the `dirty_flags` bitmask tells you what changed.
- **`contribute_to()`** injects an element at a named slot. Use `route_slot_ids!` to match slot regions.
- **`contribute_deps()`** declares which dirty flags affect your contribution (enables caching).
- **`state_hash()`** returns a value that changes when your plugin's output would change. The framework uses this to skip redundant work.

### Build & Deploy

```bash
cargo build --target wasm32-wasip2 --release
cp target/wasm32-wasip2/release/sel_badge.wasm ~/.local/share/kasane/plugins/
```

The plugin loads automatically on the next `kasane` launch.

### Transform Example: prompt-highlight

This plugin demonstrates `transform_element()` — the mechanism for wrapping or
replacing existing UI elements. It highlights the status bar when the editor
enters prompt mode (`:`, `/`, etc.).

```rust
// examples/wasm/prompt-highlight/src/lib.rs
kasane_plugin_sdk::generate!();

use std::cell::Cell;

use exports::kasane::plugin::plugin_api::Guest;
use kasane::plugin::element_builder;
use kasane::plugin::host_state;
use kasane::plugin::types::*;
use kasane_plugin_sdk::{dirty, plugin};

const MODE_BUFFER: u8 = 0;
const MODE_PROMPT: u8 = 1;

thread_local! {
    static CURSOR_MODE: Cell<u8> = const { Cell::new(MODE_BUFFER) };
}

struct PromptHighlightPlugin;

#[plugin]
impl Guest for PromptHighlightPlugin {
    fn get_id() -> String {
        "prompt_highlight".to_string()
    }

    fn on_state_changed(dirty_flags: u16) -> Vec<Command> {
        if dirty_flags & dirty::STATUS != 0 {
            CURSOR_MODE.set(host_state::get_cursor_mode());
        }
        vec![]
    }

    fn state_hash() -> u64 {
        CURSOR_MODE.get() as u64
    }

    fn transform_element(
        target: TransformTarget,
        element: ElementHandle,
        _ctx: TransformContext,
    ) -> ElementHandle {
        if !matches!(target, TransformTarget::StatusBarT) {
            return element; // passthrough for non-status targets
        }
        if CURSOR_MODE.get() != MODE_PROMPT {
            return element; // passthrough in buffer mode
        }
        // Wrap status bar in highlighted container
        let face = Face {
            fg: Color::Named(NamedColor::Black),
            bg: Color::Named(NamedColor::Yellow),
            underline: Color::DefaultColor,
            attributes: 0,
        };
        let padding = Edges { top: 0, right: 0, bottom: 0, left: 0 };
        element_builder::create_container_styled(element, None, false, padding, face, None)
    }

    fn transform_deps(target: TransformTarget) -> u16 {
        match target {
            TransformTarget::StatusBarT => dirty::STATUS,
            _ => 0,
        }
    }
}

export!(PromptHighlightPlugin);
```

**Key points:**

- **`transform_element()`** receives an opaque `ElementHandle` for the target element. Return it unchanged for passthrough, or wrap it with `create_container_styled()`.
- **`TransformTarget`** selects which UI component to transform (e.g., `StatusBarT`, `Buffer`, `MenuT`). Ignore targets your plugin doesn't handle.
- **`transform_deps()`** declares per-target dirty flag dependencies for caching.
- **`transform_priority()`** (defaulted to `0` by `#[plugin]`) controls ordering in the transform chain. Higher priority = applied first (inner).

## Testing

Unit tests can be written using `PluginRegistry` directly.

```rust
#[test]
fn my_plugin_contributes_gutter() {
    let mut registry = PluginRegistry::new();
    registry.register(MyPlugin);  // Plugin trait (state-externalized)

    let state = AppState::default();
    let _ = registry.init_all(&state);

    let contributions = registry.collect_contributions(&SlotId::BUFFER_LEFT, &state);
    assert_eq!(contributions.len(), 1);
}
```

For WASM integration tests, see `tools/wasm-test/` and `kasane-wasm/src/tests/`.

## Registration and Distribution

### Registration Order

Kasane registers plugins in the following order:

1. Example WASM (embedded in the binary)
2. FS-discovered WASM (`~/.local/share/kasane/plugins/*.wasm`)
3. Native plugins registered via `kasane::run(|registry| { ... })`

An FS-discovered WASM plugin with the same ID can override an example plugin.

### Distribution Methods

- WASM: Place `.wasm` files in `~/.local/share/kasane/plugins/`
- Native: Distribute as a custom binary using `kasane::run()`

### Control via config.toml

```toml
[plugins]
enabled = ["cursor_line", "color_preview"]
disabled = ["some_plugin"]

# Per-plugin WASI capability denial
[plugins.deny_capabilities]
untrusted_plugin = ["filesystem", "environment"]
```

### WASI Capabilities

WASM plugins can declare required WASI capabilities via `requested_capabilities()`.
The host configures a WASI context per plugin based on the declarations.

| Capability | Effect | Default |
|---|---|---|
| `Capability::Filesystem` | Preopens `data/` (plugin-specific, read/write) and `.` (CWD, read-only) | Disabled |
| `Capability::Environment` | Inherits host environment variables | Disabled |
| `Capability::MonotonicClock` | Access to a monotonic clock | Enabled |
| `Capability::Process` | Spawn external processes | Disabled |

```rust
fn requested_capabilities() -> Vec<Capability> {
    vec![Capability::Filesystem]
}
```

Capabilities are granted upon declaration. Users can deny them via `deny_capabilities` in `config.toml`.

Constraint: WASI capabilities are available from `on_init()` onward. They are not available during component initialization (`_initialize`).

## Example Plugin List

| Plugin | Path | Main Features |
|---|---|---|
| cursor-line | `examples/wasm/cursor-line/` | `annotate_line()`, `state_hash()` |
| sel-badge | `examples/wasm/sel-badge/` | `contribute_to()` (`STATUS_RIGHT`) |
| prompt-highlight | `examples/wasm/prompt-highlight/` | `transform_element()` (`StatusBarT`), `transform_deps()` |
| color-preview | `examples/wasm/color-preview/` | `annotate_line()`, `contribute_overlay_v2()`, `handle_mouse()` |
| fuzzy-finder | `examples/wasm/fuzzy-finder/` | `contribute_overlay_v2()`, `handle_key()`, `Command::SpawnProcess` |
| line-numbers (native) | `examples/line-numbers/` | Direct `PluginBackend` trait, `contribute_to()`, `kasane::run()` |

## Appendix A: Alternative: Native Plugin {#appendix-a-alternative-native-plugin}

For use cases that require full `&AppState` access, or features not yet available via WASM (such as `Surface` or `PaintHook`), you can write a native plugin. Native plugins are distributed as custom binaries.

The `Plugin` trait (state-externalized) is the recommended native API. The framework owns the state; all methods are pure functions receiving state as a parameter and returning new state + effects.

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

**Key differences from WASM:**

| Aspect | WASM | Native (`Plugin`) |
|---|---|---|
| Safety | Sandbox isolation | Same process as host |
| Distribution | `.wasm` file placement | Custom binary |
| State | Manual (`thread_local!` + `state_hash()`) | Automatic (`PartialEq` comparison) |
| Registration | Auto-discovered or embedded | `registry.register(..)` in `kasane::run()` |

## Appendix B: PluginBackend (Internal) {#appendix-b-pluginbackend-internal}

`PluginBackend` is the internal mutable-state plugin model (`&mut self`). It provides access to all extension points including `Surface`, `PaintHook`, and pane lifecycle. Use this only when `Plugin` cannot express what you need.

```rust
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

Register via `registry.register_backend(Box::new(..))`. See the `PluginBackend` trait in `kasane-core/src/plugin/traits.rs` for the full method list.

## Appendix C: WASM vs Native Comparison {#appendix-c-wasm-vs-native-comparison}

| Aspect | WASM | Native (`Plugin`, recommended) | Native (`PluginBackend`, internal) |
|---|---|---|---|
| Safety | Sandbox isolation | Same process as host | Same process as host |
| Performance | WASM boundary crossing cost | Direct function calls | Direct function calls |
| API access | `host-state` + `element-builder` | Direct `&AppState` + `&State` | Direct `&AppState` reference |
| Distribution | `.wasm` file placement | Custom binary | Custom binary |
| Developer experience | `#[plugin]` macro + `wit-bindgen` | Derive `Clone + PartialEq + Debug + Default` on state | Implement `PluginBackend` directly |
| `Surface` / `PaintHook` | Not supported ([details](./wasm-constraints.md)) | Not supported (use `PluginBackend`) | Supported |
| State model | Mutable (guest linear memory) | Externalized (framework-owned, pure functions) | Mutable (`&mut self`) |
| Cache invalidation | Manual `state_hash()` | Automatic (`PartialEq` comparison) | Manual `state_hash()` |
| Inter-plugin communication | `Vec<u8>` | `Box<dyn Any>` | `Box<dyn Any>` |

> Earlier WIT versions (v0.3) used `contribute()`, `contribute_line()`, and `contribute_overlay()`. These are superseded by the current API. Legacy stubs are generated automatically by `#[plugin]`.

## Related Documents

- [plugin-api.md](./plugin-api.md) — API reference
- [semantics.md](./semantics.md) — Composition order and correctness conditions
- [repo-layout.md](./repo-layout.md) — Code locations
- [index.md](./index.md) — Entry point for all docs
