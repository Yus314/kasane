# Plugin testing

Kasane ships a mock-host harness so plugin authors can unit-test their plugin
logic — including `Effects` production and `host_state::*` interactions —
without compiling to `wasm32-wasip2` and without driving a real Kakoune
instance.

The harness lives in the `kasane-plugin-sdk-test` crate. It is re-exported as
`kasane_plugin_sdk::test::*` for convenience; both paths reach the same
implementation.

## Enabling the harness

Add a `test-harness` feature to your plugin crate that forwards to the SDK:

```toml
# Cargo.toml
[features]
test-harness = ["kasane-plugin-sdk/test-harness"]

[lib]
crate-type = ["cdylib", "rlib"]  # rlib needed so tests can link
```

The macros (`define_plugin!`, `generate!`) emit a cfg-switched `host_state`
module. When `feature = "test-harness"` is active, `host_state::*` calls
route into the harness's thread-local mock state instead of WASM imports.
Build for `wasm32-wasip2` as usual without the feature — production builds
are unchanged.

## Quick example

```rust
// my-plugin/src/lib.rs
kasane_plugin_sdk::define_plugin! {
    manifest: "kasane-plugin.toml",

    state {
        #[bind(host_state::get_cursor_line(), on: dirty::BUFFER)]
        active_line: i32 = -1,
    },

    display() {
        if state.active_line < 0 { return vec![]; }
        vec![style_line(state.active_line as u32, style_bg(rgb(40, 40, 50)))]
    },
}

#[cfg(all(test, feature = "test-harness"))]
mod tests {
    use kasane_plugin_sdk::test::TestHarness;
    use crate::exports::kasane::plugin::plugin_api::Guest;
    use crate::DisplayDirective;

    #[test]
    fn highlights_cursor_line() {
        let mut h = TestHarness::new();
        h.set_cursor_line(7);

        let _ = crate::__KasanePlugin::on_state_changed_effects(crate::dirty::ALL);
        let out = crate::__KasanePlugin::display();

        assert_eq!(out.len(), 1);
        let DisplayDirective::StyleLine(d) = &out[0] else { panic!() };
        assert_eq!(d.line, 7);
    }
}
```

Run with:

```bash
cargo test --features test-harness --lib
```

## What the harness provides

### Mock host state

`TestHarness` exposes setters mirroring every `host_state::*` query supported
by the cfg-switched binding shim. Categories:

- **Cursor**: `set_cursor_line`, `set_cursor_col`, `set_cursor_count`,
  `set_cursor_mode`, `set_secondary_cursors`.
- **Buffer**: `set_lines` (text), `set_line_atoms` (styled), `set_line_count`,
  `set_buffer_file_path`.
- **Screen**: `set_screen_size`, `set_focused`, `set_dragging`.
- **Status**: `set_status_prompt`, `set_status_content`, `set_status_line`,
  `set_status_mode_line`, `set_status_default_style`.
- **Menu**: `set_menu` (items + selection), `set_menu_anchor`, `set_menu_mode`,
  `set_menu_style`, `set_menu_selected_style`, `clear_menu`.
- **Info**: `set_info`, `clear_info`.
- **Theme**: `set_theme_style`, `set_dark_background`, `set_default_style`,
  `set_padding_style`.
- **Settings**: `set_setting_bool`, `set_setting_integer`, `set_setting_float`,
  `set_setting_string`.
- **Sessions**: `set_session_count`, `set_active_session_key`, `set_sessions`.
- **Syntax**: `set_syntax_generation`, `set_fold_ranges`, `set_indent_level`,
  `set_scopes_at`.

See [`MockHostState`](https://docs.rs/kasane-plugin-sdk-test/latest/kasane_plugin_sdk_test/struct.MockHostState.html)
for the full field list.

### Effects observation

The harness keeps a per-thread `CommandLog`. Tests that observe `Effects`
returned from plugin handlers can push `CommandRecord`s into this log and
later drain them:

```rust
use kasane_plugin_sdk::test::{CommandRecord, TestHarness};

let mut h = TestHarness::new();
// ... invoke plugin handler that produces Effects ...
// Push observed commands manually (or use a future Effects-translation helper):
h.push_command(CommandRecord::eval("write"));

let commands = h.drain_commands();
assert!(commands.iter().any(|c| c.kind == "EvalCommand"));
```

### Element arena & logs

`TestHarness::arena()` returns a clone of the element arena populated by
`element_builder::*` calls; `MockElementArena::find(needle)` looks up handles
by debug-string substring. `TestHarness::drain_logs()` returns captured
`host_log::log_message` entries.

## Threading model

Mock state is thread-local. Tests using the harness on the same thread must
not run in parallel. Either:

```bash
cargo test --features test-harness --lib -- --test-threads=1
```

or add a serialization crate (e.g. `serial_test`) and annotate each test with
`#[serial]`.

## What the harness does **not** cover

- **Selection-algebra / history** — the WIT `selection-set` and `history`
  imports are not modeled. Tests that drive these paths must extract the
  selection-using logic into a pure function.
- **Named colors** — `MockBrush` covers `Default` and `Rgb` only. Tests
  depending on `named-color` brushes should construct the equivalent RGB.
- **Subprocess / HTTP / Workspace effects** — these `Command` variants can
  still be produced by your plugin code, but the harness does not execute
  them. Use `drain_commands` + assertions to verify they were issued.

If your plugin depends on an unmodeled path, the recommended pattern is to
extract the dependent logic into a plain Rust function that takes structured
inputs, and test that function directly without the harness.

## Where the harness lives

| Crate | Purpose |
|---|---|
| `kasane-plugin-sdk-test` | The harness implementation (mock state, `TestHarness`, mock host modules). |
| `kasane-plugin-sdk` (with `test-harness` feature) | Pulls the harness in and re-exports it as `kasane_plugin_sdk::test`. |
| `kasane-plugin-sdk-macros` | Emits the cfg-switched `host_state` shim that routes calls into the harness. |
