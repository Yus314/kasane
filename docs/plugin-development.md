# Kasane Plugin Development Guide

Kasane plugins are WASM components packaged as single `.kpk` artifacts.
Build one with `kasane plugin build`, install it with `kasane plugin install`,
and Kasane loads it from the plugins directory at startup.

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
| `define_projection()` | Named structural/additive display strategies (e.g. Semantic Zoom) |
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
    manifest: "kasane-plugin.toml",
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

This generates a ready-to-build project with `Cargo.toml`, `kasane-plugin.toml`, `src/lib.rs`, and `.gitignore`. You also need the `wasm32-wasip2` target:

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
kasane-plugin-sdk = "0.5.0"
wit-bindgen = "0.53"
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

Common helpers like `plain()`, `colored()`, `is_ctrl()`, `status_badge()`, `hex()`, `redraw()`, `send_command()`, and `paste_clipboard()` are available in all plugin code (emitted by `generate!()` / `define_plugin!`). `paste_clipboard()` specifically requests insertion of the host system clipboard contents; committed text input and bracketed paste payloads already flow through the text-input pipeline without using this command. For the full list including face/color construction, overlay layout, key escaping, and attribute constants, see [plugin-api.md §4.4](./plugin-api.md#44-sdk-helpers).

### Plugin Manifest

Every plugin project ships with a `kasane-plugin.toml` manifest file. The build step embeds that manifest into the generated `.kpk` package, and the host reads its static metadata **before** compiling or instantiating WASM — the plugin never participates in its own permission decisions.

```toml
[plugin]
id = "fuzzy_finder"
abi_version = "0.25.0"

[capabilities]
wasi = ["process"]

[authorities]
host = ["pty-process"]

[handlers]
flags = ["overlay", "input-handler", "io-handler", "contributor"]
transform_targets = ["kasane.buffer", "kasane.menu"]
publish_topics = ["cursor.line"]
subscribe_topics = ["theme.changed"]
extensions_defined = ["myplugin.status-items"]
extensions_consumed = ["other.ext"]

[view]
deps = ["buffer-content", "buffer-cursor", "menu-structure", "menu-selection"]
```

| Section | Required | Default | Purpose |
|---|---|---|---|
| `plugin.id` | Yes | — | Plugin identifier |
| `plugin.abi_version` | Yes | — | WIT package version the plugin targets |
| `capabilities.wasi` | No | `[]` | WASI capabilities for sandbox construction |
| `authorities.host` | No | `[]` | Host authorities for privileged effects |
| `handlers.flags` | No | `[]` (→ all) | Handler capability bitmask (empty = all-set) |
| `handlers.transform_targets` | No | `[]` | Transform target names for interference detection |
| `handlers.publish_topics` | No | `[]` | Pub/sub topics this plugin publishes |
| `handlers.subscribe_topics` | No | `[]` | Pub/sub topics this plugin subscribes to |
| `handlers.extensions_defined` | No | `[]` | Extension points defined by this plugin |
| `handlers.extensions_consumed` | No | `[]` | Extension points consumed by this plugin |
| `view.deps` | No | `[]` (→ ALL) | Dirty-flag subscription (empty = all flags) |
| `settings.<key>.type` | No | — | Setting type: `"bool"`, `"integer"`, `"float"`, `"string"` |
| `settings.<key>.default` | No | — | Default value (must match type) |
| `settings.<key>.description` | No | — | Human-readable description |

Example with settings:

```toml
[settings.enabled]
type = "bool"
default = false
description = "Enable smooth scrolling animation"
```

In `define_plugin!`, use the `manifest:` section instead of `id:`, `capabilities:`, and `authorities:`:

```rust
kasane_plugin_sdk::define_plugin! {
    manifest: "kasane-plugin.toml",

    state { /* ... */ },
    // ...
}
```

The macro reads the TOML at compile time and generates `get_id()`, `requested_capabilities()`, `requested_authorities()`, and `view_deps()` from its contents. `manifest:` is mutually exclusive with `id:`, `capabilities:`, and `authorities:`.

At build time, Kasane packages the manifest and compiled WASM component into a single `.kpk` artifact. The plugins directory contains `.kpk` files, not loose `.wasm` binaries.

### Plugin Profiles

| Profile | Sections | Template | Example |
|---|---|---|---|
| Status widget | `state` (`#[bind]`), `slots` | `contribution` | sel-badge |
| Line annotator | `state` (`#[bind]`), `annotate` | `annotation` | cursor-line |
| Element transformer | `state` (`#[bind]`), `transform` | `transform` | prompt-highlight |
| Display transform | `state`, `on_state_changed_effects`, `display_directives` | — | virtual-text-demo (native) |
| Structural projection | `state`, `define_projection`, `on_key_map` | — | semantic-zoom (builtin) |
| Interactive overlay | `state`, `handle_key`, `overlay` | `overlay` | session-ui |
| Process launcher | Above + `on_io_event_effects`, `capabilities` | `process` | fuzzy-finder |
| Scroll policy | `handle_default_scroll` | — | smooth-scroll |

Available `define_plugin!` sections: `manifest` or `id`, `state` (with optional `#[bind]`), `settings`, `on_init_effects`, `on_active_session_ready_effects`, `on_state_changed_effects`, `update_effects`, `slots`, `annotate`, `transform`, `transform_patch`, `transform_priority`, `overlay`, `handle_key`, `handle_mouse`, `handle_default_scroll`, `capabilities`, `authorities`, `on_io_event_effects`.

### Build & Deploy

```bash
kasane plugin build              # Build a .kpk package (release)
kasane plugin install            # Build or verify a .kpk package, then activate it
kasane plugin dev [path]         # Build (debug), install, and watch for changes
kasane plugin dev --release      # Same, but release builds
```

`kasane plugin install` installs a `.kpk` package into the package store under `~/.local/share/kasane/plugins/` (or the path configured in `kasane.kdl`) and updates `plugins.lock`.

`kasane plugin dev` does the same as `install`, then watches `src/`, `Cargo.toml`, and `kasane-plugin.toml` for changes and automatically rebuilds and reinstalls. By default it uses debug builds for faster iteration; add `--release` for optimized builds. A running Kasane instance picks up the updated plugin via the `.reload` sentinel file without restart.

WASM plugin ABI note: current Kasane releases expect
`kasane:plugin@1.0.0`. Rebuild and reinstall any plugin that was built
against an older version; older binaries will not load.

### Migrating to ABI 0.25.0

If you are upgrading a plugin from the previous ABI, the required changes are:

1. Update the SDK crate to `kasane-plugin-sdk = "0.5"` and set
   `abi_version = "0.25.0"` in `kasane-plugin.toml`.
2. Rename clipboard-paste commands from `Command::Paste` to
   `Command::PasteClipboard`. If you use SDK helpers, prefer
   `paste_clipboard()` instead of constructing the command directly.
3. Rebuild and reinstall the `.wasm`. Existing artifacts built against
   the previous ABI will be rejected by current Kasane releases.

No code change is required for committed text input or bracketed paste
payloads. Those go through the text-input pipeline and do not use
`Command::PasteClipboard`.

To see installed plugins or diagnose environment issues:

```bash
kasane plugin list               # List installed plugins
kasane plugin gc                 # Remove unreferenced package artifacts from the store
kasane plugin rollback           # Restore the previous active plugin set
kasane plugin doctor             # Check toolchain, SDK version, and plugin health
kasane plugin doctor --fix       # Auto-fix missing target and plugins directory
```

<details>
<summary>Manual build &amp; deploy</summary>

```bash
cargo build --target wasm32-wasip2 --release
kasane plugin build
kasane plugin install target/kasane/sel-badge-0.1.0.kpk
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
        if *target != TransformTarget::STATUS_BAR {
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
- **`transform_patch(target, ctx)`** is a declarative alternative that returns `Vec<ElementPatchOp>` instead of imperatively transforming the subject. Pure patches are Salsa-memoizable. See [plugin-cookbook.md](./plugin-cookbook.md#declarative-transform-wasm) for an example.

### Scroll Policy Example: smooth-scroll

This plugin demonstrates `handle_default_scroll()` — a policy hook that runs
after core has classified the event as a default buffer scroll candidate, but
before fallback scroll behavior is applied. It also shows the `settings {}` block
for typed configuration.

```rust
kasane_plugin_sdk::define_plugin! {
    manifest: "kasane-plugin.toml",

    settings {
        enabled: bool = false,
    }

    handle_default_scroll(candidate) {
        if !__setting_enabled() {
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
    },
}
```

The `settings {}` block generates `__setting_enabled() -> bool` which calls
`host_state::get_setting_bool("enabled")` with the manifest default as fallback.
When `manifest:` is present, the macro validates at compile time that each setting
exists in the manifest's `[settings.*]` and types match.

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
| `plugin X skipped: disabled` | Plugin ID is in `plugins { disabled }` | Remove from the disabled list in `kasane.kdl` |
| Plugin loads but contributes nothing | `state_hash()` returns a constant, or dirty flags don't cover the relevant state | Verify `#[bind]` flags or manual `state_hash()` implementation |
| `wasm trap: unreachable` in logs | Guest code panicked | Run `KASANE_LOG=debug` and check the backtrace |

### Inspecting plugin state at runtime

`kasane plugin list` shows loaded plugins and their IDs.
`kasane plugin doctor` checks toolchain, SDK version, and plugin health.

## Registration and Distribution

### Registration Order

Kasane registers plugins in the following order:

1. Example WASM (embedded in the binary)
2. FS-discovered packages (`~/.local/share/kasane/plugins/*.kpk`)
3. Native plugins supplied via `kasane::run_with_factories(...)` or a custom `PluginProvider`

An FS-discovered WASM plugin with the same ID can override an example plugin.

### Distribution Methods

- WASM: Place `.kpk` files in `~/.local/share/kasane/plugins/`
- Native: Distribute as a custom binary using `kasane::run_with_factories(...)` or `kasane::run(provider)`

### Control via kasane.kdl

```kdl
plugins {
    enabled "cursor_line" "color_preview"
    disabled "some_plugin"

    // Per-plugin WASI capability denial
    deny_capabilities {
        untrusted_plugin "filesystem" "environment"
    }
}
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

Capabilities are granted upon declaration. Users can deny them via `deny_capabilities` in `kasane.kdl`.

Constraint: WASI capabilities are available from `on_init_effects()` onward. They are not available during component initialization (`_initialize`).

## Session-Aware Plugins

Plugins can observe and control sessions using the Tier 8 host-state API and `SessionCommand`. For type definitions and semantics, see [plugin-api.md §3.5.3](./plugin-api.md#353-session-observability). The `session-ui` example (`examples/wasm/session-ui/src/lib.rs`) demonstrates the full pattern including overlay UI, keybinding, and session switching.

## Example Plugins

For the full list of bundled and source example plugins, see [using-plugins.md](./using-plugins.md#bundled-wasm-plugins).

## Appendix A: Alternative: Native Plugin {#appendix-a-alternative-native-plugin}

For use cases that require features not yet available via WASM (such as `Surface`), you can write a native plugin. Native plugins are distributed as custom binaries.

The `Plugin` trait uses `HandlerRegistry`-based registration: 2 methods + 1 associated type (`id()`, `State` type, `register()`). The framework owns the state; handlers are pure functions. Capabilities are auto-inferred from which handlers are registered.

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

    fn register(&self, r: &mut HandlerRegistry<CursorLineState>) {
        r.declare_interests(DirtyFlags::BUFFER);
        r.on_state_changed(|state, app, dirty| {
            if dirty.intersects(DirtyFlags::BUFFER) {
                (
                    CursorLineState {
                        active_line: app.cursor_line(),
                    },
                    Effects::default(),
                )
            } else {
                (state.clone(), Effects::default())
            }
        });
        r.on_annotate_background(|state, line, _app, _ctx| {
            if line as i32 == state.active_line {
                Some(BackgroundLayer {
                    face: Face { bg: Color::Named(NamedColor::Blue), ..Face::default() },
                    z_order: 0,
                    blend: BlendMode::Opaque,
                })
            } else {
                None
            }
        });
    }
}

fn main() {
    kasane::run_with_factories([host_plugin("cursor_line", || {
        PluginBridge::new(CursorLinePlugin)
    })]);
}
```

For a comparison of WASM vs Native plugin models, see [Appendix C](#appendix-c-wasm-vs-native-comparison) or [plugin-api.md §8](./plugin-api.md#8-wasm-plugin-constraints).

## Appendix B: PluginBackend (Internal) {#appendix-b-pluginbackend-internal}

`PluginBackend` is the internal mutable-state plugin model (`&mut self`). It provides access to all extension points including `Surface` and workspace observation. Use this only when `Plugin` cannot express what you need.

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

For the WASM vs Native feature gap table and runtime constraints, see [plugin-api.md §8](./plugin-api.md#8-wasm-plugin-constraints). For choosing a plugin model (WASM, Native `Plugin`, Native `PluginBackend`), see [plugin-api.md §1.2.2](./plugin-api.md#122-choosing-a-plugin-model).

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

    fn on_state_changed_effects(dirty_flags: u16) -> Effects {
        if dirty_flags & dirty::BUFFER != 0 {
            CURSOR_COUNT.set(host_state::get_cursor_count());
        }
        Effects::default()
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
