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
| `transform()` | Status bar customization, menu layout changes, overlay repositioning |
| `display_directives()` | Code folding, line hiding, virtual text insertion |
| `handle_key()` + `handle_mouse()` | Interactive pickers, dialogs |
| `handle_default_scroll()` | Wheel policy, smooth scrolling |
| `Command::EditBuffer` | Structured buffer edits (insert, replace, delete) |
| `Command::InjectInput` | Programmatic key injection |

> Native plugins can also use `Surface` for sidebars and dedicated panels. See [Appendix A](#appendix-a-alternative-native-plugin).

## Quick Start

### Hello World (3 lines)

```bash
kasane plugin new my-hello --template hello
cd my-hello && kasane plugin build
```

This creates a minimal plugin:

```rust
kasane_plugin_sdk::define_plugin! {
    id: "my_hello",
    slots {
        STATUS_RIGHT => plain(" Hello from my_hello! "),
    },
}
```

The simple slot form `SLOT => expr` auto-wraps the expression in `auto_contribution()`. For full control, use the closure form `SLOT(deps) => |ctx| { ... }`.

### Progressive Learning Path

| Level | Template | What you learn | Key concepts |
|---|---|---|---|
| 1 | `hello` | Minimal plugin, slot contribution | `define_plugin!`, `plain()`, simple slot form |
| 2 | `contribution` | State, dirty flags, state caching | `state {}`, `#[bind]`, `dirty::BUFFER` |
| 3 | `annotation` | Per-line decoration | `annotate()` |
| 4 | `overlay` | Interactive UI, key handling | `handle_key()`, `overlay()`, `redraw()` |
| 5 | `process` | External processes, I/O events | `capabilities`, `on_io_event_effects()`, `is_ctrl_shift()` |

### Project Setup

```bash
kasane plugin new my-plugin                              # Default (contribution template)
kasane plugin new my-plugin --template hello              # Minimal hello world
kasane plugin new my-highlighter --template annotation    # Line annotation template
kasane plugin new my-transform --template transform       # Element transform template
kasane plugin new my-overlay --template overlay            # Interactive overlay template
kasane plugin new my-runner --template process             # Process launcher template
```

This generates a ready-to-build project with `Cargo.toml`, `src/lib.rs`, and `.gitignore`. You also need the `wasm32-wasip2` target:

```bash
rustup target add wasm32-wasip2
# or: kasane plugin doctor --fix
```

<details>
<summary>Manual setup (without <code>kasane plugin new</code>)</summary>

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

</details>

### Full Example: sel-badge

This plugin displays the selection count on the right side of the status bar when multiple cursors are active.

```rust
kasane_plugin_sdk::define_plugin! {
    id: "sel_badge",

    state {
        #[bind(host_state::get_cursor_count(), on: dirty::BUFFER)]
        cursor_count: u32 = 0,
    },

    slots {
        STATUS_RIGHT(dirty::BUFFER) => |_ctx| {
            status_badge(state.cursor_count > 1, &format!(" {} sel ", state.cursor_count))
        },
    },
}
```

**Key points:**

- **`define_plugin!`** combines `generate!()`, state declaration, `#[plugin]`, and `export!()` into a single macro. All sections are optional except `id`.
- **`#[bind(expr, on: flags)]`** on state fields auto-generates sync code in `on_state_changed_effects()`. The expression is evaluated when the specified dirty flags are set.
- **`slots {}`** declares slot contributions with dependency tracking. Inside `slots` closures, `state.field` is available directly (read-only).
- **`status_badge()`** is a helper that returns `Some(auto_contribution(plain(label)))` when the condition is true.
- The `dirty` and `modifiers` modules are auto-imported by `define_plugin!`.
- In mutable contexts (`handle_key`, `overlay`, `on_io_event`, etc.), `bump_generation()` is called automatically when the state guard drops.

### SDK Helpers Reference

Common helpers like `plain()`, `colored()`, `is_ctrl()`, `status_badge()`, `hex()`, `redraw()`, and `send_command()` are available in all plugin code (emitted by `generate!()` / `define_plugin!`). For the full list including face/color construction, overlay layout, key escaping, and attribute constants, see [plugin-api.md §4.5](./plugin-api.md#45-sdk-helpers).

### Plugin Profiles

| Profile | Sections | Template | Example |
|---|---|---|---|
| Status widget | `state` (`#[bind]`), `slots` | `contribution` | sel-badge |
| Line annotator | `state` (`#[bind]`), `annotate` | `annotation` | cursor-line |
| Element transformer | `state` (`#[bind]`), `transform` | `transform` | prompt-highlight |
| Display transform | `state`, `on_state_changed_effects`, `display_directives` | — | virtual-text-demo (native) |
| Interactive overlay | `state`, `handle_key`, `overlay` | `overlay` | session-ui |
| Process launcher | Above + `on_io_event_effects`, `capabilities` | `process` | fuzzy-finder |
| Scroll policy | `handle_default_scroll` | — | smooth-scroll |

Available `define_plugin!` sections: `id`, `state` (with optional `#[bind]`), `on_init_effects`, `on_active_session_ready_effects`, `on_state_changed_effects`, `update_effects`, `slots`, `annotate`, `transform`, `transform_priority`, `overlay`, `handle_key`, `handle_mouse`, `handle_default_scroll`, `capabilities`, `on_io_event_effects`.

### Build & Deploy

```bash
kasane plugin build              # Build for wasm32-wasip2 (release)
kasane plugin install            # Build, validate, and install to plugins directory
kasane plugin dev [path]         # Build (debug) and watch for changes (hot-reload)
kasane plugin dev --release      # Same, but release builds
```

`kasane plugin install` copies the `.wasm` to `~/.local/share/kasane/plugins/` (or the path configured in `config.toml`). The plugin loads automatically on the next `kasane` launch.

`kasane plugin dev` does the same as `install`, then watches `src/` and `Cargo.toml` for changes and automatically rebuilds and reinstalls. By default it uses debug builds for faster iteration; add `--release` for optimized builds. A running Kasane instance picks up the updated plugin via the `.reload` sentinel file without restart.

WASM plugin ABI note: current Kasane releases expect
`kasane:plugin@0.22.0`. Rebuild and reinstall any plugin that was built
against an older version; older binaries will not load.

To see installed plugins or diagnose environment issues:

```bash
kasane plugin list               # List installed plugins
kasane plugin doctor             # Check toolchain, SDK version, and plugin health
kasane plugin doctor --fix       # Auto-fix missing target and plugins directory
```

<details>
<summary>Manual build &amp; deploy (without <code>kasane plugin</code>)</summary>

```bash
cargo build --target wasm32-wasip2 --release
cp target/wasm32-wasip2/release/sel_badge.wasm ~/.local/share/kasane/plugins/
```

</details>

### Transform Example: prompt-highlight

This plugin demonstrates `transform` — the mechanism for wrapping or
replacing existing UI elements. It highlights the status bar when the editor
enters prompt mode (`:`, `/`, etc.).

```rust
kasane_plugin_sdk::define_plugin! {
    id: "prompt_highlight",

    state {
        #[bind(host_state::get_cursor_mode(), on: dirty::STATUS)]
        cursor_mode: u8 = 0,
    },

    transform(target, subject, _ctx) {
        if !matches!(target, TransformTarget::StatusBar) {
            return subject;
        }
        if state.cursor_mode != 1 {
            return subject;
        }
        match subject {
            TransformSubject::Element(element) => {
                TransformSubject::Element(
                    container(element)
                        .style(face(named(NamedColor::Black), named(NamedColor::Yellow)))
                        .build(),
                )
            }
            other => other,
        }
    },

    transform_priority: 0,
}
```

**Key points:**

- **`transform(target, subject, ctx)`** receives a `TransformSubject` — either `Element(Element)` for non-overlay targets or `Overlay(Overlay)` for overlay targets. Return it unchanged for passthrough, or pattern-match and wrap.
- **`TransformTarget`** selects which UI component to transform (e.g., `StatusBar`, `Buffer`, `Menu`). Ignore targets your plugin doesn't handle.
- **`transform_priority`** (default `0`) controls ordering in the transform chain. Higher priority = applied first (inner).

### Scroll Policy Example: smooth-scroll

This plugin demonstrates `handle_default_scroll()` — a policy hook that runs
after core has classified the event as a default buffer scroll candidate, but
before fallback scroll behavior is applied.

```rust
kasane_plugin_sdk::generate!();

use kasane_plugin_sdk::plugin;

struct SmoothScrollPlugin;

#[plugin]
impl Guest for SmoothScrollPlugin {
    fn get_id() -> String {
        "smooth_scroll".to_string()
    }

    fn state_hash() -> u64 {
        0
    }

    fn handle_default_scroll(candidate: DefaultScrollCandidate) -> Option<ScrollPolicyResult> {
        let enabled = host_state::get_config_string("smooth-scroll.enabled")
            .or_else(|| host_state::get_config_string("smooth_scroll"))
            .and_then(|raw| raw.parse::<bool>().ok())
            .unwrap_or(false);

        if !enabled {
            return None;
        }

        Some(ScrollPolicyResult::Plan(ScrollPlan {
            total_amount: candidate.resolved.amount,
            line: candidate.resolved.line,
            column: candidate.resolved.column,
            frame_interval_ms: 16,
            curve: ScrollCurve::Linear,
            accumulation: ScrollAccumulationMode::Add,
        }))
    }
}

export!(SmoothScrollPlugin);
```

**Key points:**

- **`handle_default_scroll(candidate)`** only runs for default buffer scroll candidates. It does not override info popups, drag-scroll routing, or other core-owned scroll paths.
- **Return `ScrollPolicyResult::Plan(...)`** to let the host runtime execute time-based scrolling. The plugin does not tick frames itself.
- **Return `None`** to let the next scroll-policy plugin decide. For exact `None` / `Pass` / `Suppress` / `Immediate` semantics, see [`plugin-api.md`](./plugin-api.md#34-input-handling).
- The source example lives at [`examples/wasm/smooth-scroll/`](../examples/wasm/smooth-scroll/). It is not part of the bundled WASM plugin set unless you build and install it yourself.

## Testing

Unit tests can be written using `PluginRuntime` directly.

```rust
#[test]
fn my_plugin_contributes_gutter() {
    let mut registry = PluginRuntime::new();
    registry.register(MyPlugin);  // Plugin trait (state-externalized)

    let state = AppState::default();
    let view = AppView::new(&state);
    let _ = registry.init_all(&view);

    let ctx = ContributeContext::new(&view, None);
    let contributions = registry.collect_contributions(&SlotId::BUFFER_LEFT, &view, &ctx);
    assert_eq!(contributions.len(), 1);
}
```

For WASM integration tests, see `tools/wasm-test/` and `kasane-wasm/src/tests/`.

## Debugging

### Viewing plugin load results

```bash
KASANE_LOG=info kasane file.txt
```

Plugin loading results (success, failure, skip) are logged at `info` level.
For detailed WASM instantiation errors, use `debug`.

### Common issues

| Symptom | Cause | Fix |
|---|---|---|
| `plugin X failed to load: ABI mismatch` | Plugin built against an older WIT version | Rebuild with the current `kasane-plugin-sdk` |
| `plugin X skipped: disabled` | Plugin ID is in `[plugins].disabled` | Remove from the disabled list in config.toml |
| Plugin loads but contributes nothing | `state_hash()` returns a constant, or dirty flags don't cover the relevant state | Verify `#[bind]` flags or manual `state_hash()` implementation |
| `wasm trap: unreachable` in logs | Guest code panicked | Run `KASANE_LOG=debug` and check the backtrace |

### Inspecting plugin state at runtime

`kasane plugin list` shows loaded plugins and their IDs.
`kasane plugin doctor` checks toolchain, SDK version, and plugin health.

## Registration and Distribution

### Registration Order

Kasane registers plugins in the following order:

1. Example WASM (embedded in the binary)
2. FS-discovered WASM (`~/.local/share/kasane/plugins/*.wasm`)
3. Native plugins supplied via `kasane::run_with_factories(...)` or a custom `PluginProvider`

An FS-discovered WASM plugin with the same ID can override an example plugin.

### Distribution Methods

- WASM: Place `.wasm` files in `~/.local/share/kasane/plugins/`
- Native: Distribute as a custom binary using `kasane::run_with_factories(...)` or `kasane::run(provider)`

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

Constraint: WASI capabilities are available from `on_init_effects()` onward. They are not available during component initialization (`_initialize`).

## Session-Aware Plugins

Plugins can observe and control sessions using the Tier 8 host-state API and `SessionCommand`. For type definitions and semantics, see [plugin-api.md §3.5.3](./plugin-api.md#353-session-observability). The `session-ui` example (`examples/wasm/session-ui/src/lib.rs`) demonstrates the full pattern including overlay UI, keybinding, and session switching.

## Example Plugins

For the full list of bundled and source example plugins, see [using-plugins.md](./using-plugins.md#bundled-wasm-plugins).

## Appendix A: Alternative: Native Plugin {#appendix-a-alternative-native-plugin}

For use cases that require features not yet available via WASM (such as `Surface` or `PaintHook`), you can write a native plugin. Native plugins are distributed as custom binaries.

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

    fn view_deps(&self) -> DirtyFlags {
        DirtyFlags::BUFFER
    }

    fn on_state_changed_effects(
        &self,
        state: &Self::State,
        app: &AppView<'_>,
        dirty: DirtyFlags,
    ) -> (Self::State, RuntimeEffects) {
        if dirty.intersects(DirtyFlags::BUFFER) {
            (
                CursorLineState {
                    active_line: app.cursor_line(),
                },
                RuntimeEffects::default(),
            )
        } else {
            (state.clone(), RuntimeEffects::default())
        }
    }

    fn annotate_line_with_ctx(
        &self,
        state: &Self::State,
        line: usize,
        _app: &AppView<'_>,
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
                inline: None,
            })
        } else {
            None
        }
    }
}

fn main() {
    kasane::run_with_factories([host_plugin("cursor_line", || {
        PluginBridge::new(CursorLinePlugin)
    })]);
}
```

For a comparison of WASM vs Native plugin models, see [Appendix C](#appendix-c-wasm-vs-native-comparison) or [wasm-constraints.md Quick Reference](./wasm-constraints.md#quick-reference).

## Appendix B: PluginBackend (Internal) {#appendix-b-pluginbackend-internal}

`PluginBackend` is the internal mutable-state plugin model (`&mut self`). It provides access to all extension points including `Surface`, `PaintHook`, and workspace observation. Use this only when `Plugin` cannot express what you need.

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
        app: &AppView<'_>,
        _ctx: &ContributeContext,
    ) -> Option<Contribution> {
        if region != &SlotId::BUFFER_LEFT {
            return None;
        }

        let total = app.line_count();
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
}

fn main() {
    kasane::run_with_factories([host_plugin("line_numbers", || LineNumbersPlugin)]);
}
```

Register `PluginBackend` implementors via `host_plugin("id", || PluginType)` and `kasane::run_with_factories(...)`. See the `PluginBackend` trait in `kasane-core/src/plugin/traits.rs` for the full method list.

## Appendix C: WASM vs Native Comparison {#appendix-c-wasm-vs-native-comparison}

For the comprehensive WASM vs Native feature comparison table, see [wasm-constraints.md Quick Reference](./wasm-constraints.md#quick-reference). For choosing a plugin model (WASM, Native `Plugin`, Native `PluginBackend`), see [plugin-api.md §1.2.2](./plugin-api.md#122-choosing-a-plugin-model).

## Appendix D: Explicit WASM Pattern {#appendix-d-explicit-wasm-pattern}

The `define_plugin!` macro is recommended for most WASM plugins. For full control over state management, you can use the explicit `generate!()` + `#[plugin]` + `export!()` pattern:

```rust
kasane_plugin_sdk::generate!();

use std::cell::Cell;

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

    fn on_state_changed_effects(dirty_flags: u16) -> RuntimeEffects {
        if dirty_flags & dirty::BUFFER != 0 {
            CURSOR_COUNT.set(host_state::get_cursor_count());
        }
        RuntimeEffects::default()
    }

    fn state_hash() -> u64 {
        CURSOR_COUNT.get() as u64
    }

    kasane_plugin_sdk::slots! {
        STATUS_RIGHT(dirty::BUFFER) => |_ctx| {
            let count = CURSOR_COUNT.get();
            (count > 1).then(|| {
                auto_contribution(text(&format!(" {} sel ", count), default_face()))
            })
        },
    }
}

export!(SelBadgePlugin);
```

Key differences from `define_plugin!`:
- You manage state manually (e.g., `thread_local!` + `Cell`/`RefCell`)
- You implement `state_hash()` explicitly
- You get direct control over struct naming and imports
- You can use `#[plugin]` on an existing `impl Guest` block

## Related Documents

- [plugin-api.md](./plugin-api.md) — API reference
- [semantics.md](./semantics.md) — Composition order and correctness conditions
- [index.md](./index.md) — Entry point for all docs
