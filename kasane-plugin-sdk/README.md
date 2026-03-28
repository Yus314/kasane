# kasane-plugin-sdk

SDK for writing [Kasane](https://github.com/Yus314/kasane) WASM plugins.

Kasane is an alternative frontend for [Kakoune](https://kakoune.org/).
Plugins are WASM components (`wasm32-wasip2`) — sandboxed, composable,
and hot-loadable.

## Quick Start

```bash
kasane plugin new my-plugin --template hello
cd my-plugin && kasane plugin build
```

This generates a minimal plugin (`src/lib.rs`):

```rust
kasane_plugin_sdk::define_plugin! {
    id: "my_plugin",
    slots {
        STATUS_RIGHT => plain(" Hello! "),
    },
}
```

`define_plugin!` combines WIT bindings, state, `#[plugin]`, and `export!()` into one macro.
SDK helpers (`plain()`, `colored()`, `is_ctrl()`, `status_badge()`, `redraw()`, `hex()`, etc.) are auto-imported.

For full control, use the explicit pattern: `generate!()` + `#[plugin]` + `export!()`.
See the [Plugin Development Guide](https://github.com/Yus314/kasane/blob/master/docs/plugin-development.md) for details.

## Compatibility

| SDK version | Minimum host version | WIT ABI |
|---|---|---|
| 0.3.x | kasane >= 0.3.0 | `kasane:plugin@0.22.0` |
| 0.2.x | kasane >= 0.2.0 | `kasane:plugin@0.14.0` |

Plugins built with SDK 0.3.x are not compatible with kasane 0.2.x due to WIT
interface breaking changes.

## MSRV

Rust 1.85+

## License

MIT OR Apache-2.0
