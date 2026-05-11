# kasane-plugin-sdk-test

Mock-host harness for unit-testing Kasane WASM plugins without a live wasmtime
+ Kakoune stack.

This crate is consumed implicitly by enabling the `test-harness` feature on
`kasane-plugin-sdk`. The macros emitted by `define_plugin!` / `generate!`
then route `host_state::*` calls into this crate's thread-local mock state.

See `docs/plugin-testing.md` in the Kasane repository for the usage guide.

## Stability

The crate version tracks `kasane-plugin-sdk` 1:1. The mock API surface is
considered stable within a minor version; adding new setters is non-breaking
but changing existing signatures is.
