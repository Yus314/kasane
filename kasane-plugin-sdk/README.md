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
        STATUS_RIGHT(0) => |_ctx| {
            Some(auto_contribution(plain(" Hello! ")))
        },
    },
}
```

`define_plugin!` combines WIT bindings, state, `#[plugin]`, and `export!()` into one macro.
SDK helpers (`plain()`, `colored()`, `is_ctrl()`, `status_badge()`, `hex()`, etc.) are auto-imported.

For full control, use the explicit pattern: `generate!()` + `#[plugin]` + `export!()`.
See the [Plugin Development Guide](https://github.com/Yus314/kasane/blob/master/docs/plugin-development.md) for details.

## MSRV

Rust 1.85+

## License

MIT OR Apache-2.0
