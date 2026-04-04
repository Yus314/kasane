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
SDK helpers (`plain()`, `colored()`, `is_ctrl()`, `status_badge()`, `redraw()`, `paste_clipboard()`, `hex()`, etc.) are auto-imported.

Additional SDK modules:
- `kasane_plugin_sdk::channel` — MessagePack serialization helpers (`serialize()`, `deserialize()`) for pub/sub and extension point values
- `pred_has_focus!()`, `pred_not!()`, `pred_and!()`, etc. — predicate builder macros for conditional transform patches

For full control, use the explicit pattern: `generate!()` + `#[plugin]` + `export!()`.
See the [Plugin Development Guide](https://github.com/Yus314/kasane/blob/master/docs/plugin-development.md) for details.

## Compatibility

| SDK version | Minimum host version | WIT ABI |
|---|---|---|
| 0.4.x | kasane >= 0.3.0 | `kasane:plugin@0.25.0` |
| 0.3.x | kasane >= 0.3.0 | `kasane:plugin@0.24.0` |
| 0.2.x | kasane >= 0.2.0 | `kasane:plugin@0.14.0` |

Plugins built with SDK 0.4.x are not compatible with kasane 0.2.x due to WIT
interface breaking changes.

Upgrading from SDK 0.3.x to 0.4.x requires:

1. Updating `kasane-plugin.toml` to `abi_version = "0.25.0"`.
2. Renaming `Command::Paste` to `Command::PasteClipboard`, or using the
   `paste_clipboard()` helper.
3. Rebuilding and reinstalling the generated `.wasm`.

## MSRV

Rust 1.85+

## License

MIT OR Apache-2.0
