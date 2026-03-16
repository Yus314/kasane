# kasane-plugin-sdk

SDK for writing [Kasane](https://github.com/Yus314/kasane) WASM plugins.

Kasane is an alternative frontend for [Kakoune](https://kakoune.org/).
Plugins are WASM components (`wasm32-wasip2`) — sandboxed, composable,
and hot-loadable.

## Quick Start

```toml
# Cargo.toml
[package]
name = "my-plugin"
edition = "2024"

[lib]
crate-type = ["cdylib"]

[dependencies]
kasane-plugin-sdk = "0.1"
wit-bindgen = "0.41"
```

```rust
// src/lib.rs
kasane_plugin_sdk::generate!();

use exports::kasane::plugin::plugin_api::Guest;
use kasane::plugin::types::*;
use kasane_plugin_sdk::plugin;

struct MyPlugin;

#[plugin]
impl Guest for MyPlugin {
    fn get_id() -> String { "my_plugin".into() }
    // ... only implement the methods you need
}

export!(MyPlugin);
```

```bash
rustup target add wasm32-wasip2
cargo build --target wasm32-wasip2 --release
cp target/wasm32-wasip2/release/my_plugin.wasm ~/.local/share/kasane/plugins/
```

## MSRV

Rust 1.85+

## License

MIT OR Apache-2.0
