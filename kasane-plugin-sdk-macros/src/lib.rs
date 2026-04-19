//! Proc macros for the Kasane WASM plugin SDK.
//!
//! Provides `#[kasane_wasm_plugin]` to auto-fill default method stubs
//! in a `Guest` trait implementation, so plugin authors only need to
//! implement the methods they actually use.
//!
//! Also provides `define_plugin!` for a single-macro plugin definition
//! that combines `generate!()`, `state!`, `#[plugin]`, and `export!()`.

use proc_macro::TokenStream;

mod defaults;
mod define_plugin;
mod key_map;
mod manifest;
mod sdk_helpers;

/// Attribute macro that fills in default implementations for all
/// unimplemented `Guest` trait methods.
///
/// Place this on your `impl Guest for MyPlugin { ... }` block.
/// Any methods you don't write will be filled with SDK defaults
/// (no-op / pass-through / zero).
///
/// # Example
///
/// ```ignore
/// #[kasane_plugin_sdk::plugin]
/// impl Guest for CursorLinePlugin {
///     fn get_id() -> String { "cursor_line".to_string() }
///
///     fn on_state_changed_effects(dirty_flags: u16) -> RuntimeEffects {
///         let _ = dirty_flags;
///         Effects::default()
///     }
///
///     fn annotate_line(line: u32, _ctx: AnnotateContext) -> Option<LineAnnotation> {
///         let _ = line;
///         None
///     }
///
///     fn state_hash() -> u64 { ACTIVE_LINE.get() as u64 }
/// }
/// ```
///
/// All other typed `Guest` methods (`on_init_effects`,
/// `on_active_session_ready_effects`, `on_shutdown`, `contribute`,
/// `handle_key`, `handle_key_middleware`, etc.) are automatically generated
/// with their default implementations.
#[proc_macro_attribute]
pub fn kasane_wasm_plugin(attr: TokenStream, item: TokenStream) -> TokenStream {
    defaults::kasane_wasm_plugin_impl(attr, item)
}

/// Generate Kasane WIT bindings with embedded WIT content.
///
/// Two forms:
/// - `kasane_plugin_sdk::generate!()` — uses embedded WIT (crates.io consumers)
/// - `kasane_plugin_sdk::generate!("path/to/wit")` — uses file path (monorepo dev)
///
/// In addition to the WIT bindings, this macro emits:
/// - Auto `use` statements for common WIT types (`Guest`, `types::*`, etc.)
/// - Face/Color helper functions (`default_face()`, `face_bg()`, `rgb()`, etc.)
/// - Element builder helpers (`text()`, `column()`, `row()`, `flex_row()`,
///   `flex_column()`, `grid()`, `scrollable()`, `container()`, `empty()`, etc.)
/// - Overlay positioning helpers (`centered_overlay()`)
#[proc_macro]
pub fn kasane_generate(input: TokenStream) -> TokenStream {
    sdk_helpers::kasane_generate_impl(input)
}

/// All-in-one plugin definition macro that combines `generate!()`, `state!`,
/// `#[plugin]`, and `export!()` into a single declaration.
///
/// # Example
///
/// ```ignore
/// kasane_plugin_sdk::define_plugin! {
///     id: "sel_badge",
///
///     state {
///         #[bind(host_state::get_cursor_count(), on: dirty::BUFFER)]
///         cursor_count: u32 = 0,
///     },
///
///     slots {
///         STATUS_RIGHT(dirty::BUFFER) => |_ctx| {
///             status_badge(state.cursor_count > 1, &format!(" {} sel ", state.cursor_count))
///         },
///     },
/// }
/// ```
///
/// ## Supported sections (all optional except `id`):
///
/// - `id: "plugin_id"` — plugin identifier (required)
/// - `state { field: Type = default, ... }` — plugin state with generation counter
///   - Fields support `#[bind(expr, on: flags)]` for auto-sync from host state
/// - `on_init_effects() { ... }` → `fn on_init_effects() -> Effects` (auto-converted)
/// - `on_active_session_ready_effects() { ... }` → `fn on_active_session_ready_effects() -> Effects` (auto-converted)
/// - `on_state_changed_effects(dirty) { ... }` → `fn on_state_changed_effects() -> Effects`
/// - `slots { SLOT => expr, ... }` — simple form (auto-wraps in `auto_contribution()`)
/// - `slots { SLOT(deps) => |ctx| { ... }, ... }` — full form with state access via `state.field`
/// - `on_workspace_changed(snapshot) { ... }` → `fn on_workspace_changed()`
/// - `annotate(line, ctx) { ... }` → `fn annotate_line()`
/// - `display_directives() { ... }` → `fn display_directives() -> Vec<DisplayDirective>`
/// - `display() { ... }` → `fn display() -> Vec<DisplayDirective>` (unified display, all categories)
/// - `transform(target, subject, ctx) { ... }` → `fn transform()`
/// - `transform_priority: expr` → `fn transform_priority()`
/// - `overlay(ctx) { ... }` → `fn contribute_overlay_v2()`
/// - `handle_key(event) { ... }` → `fn handle_key()`
/// - `handle_key_middleware(event) { ... }` → `fn handle_key_middleware()`
/// - `handle_mouse(event, id) { ... }` → `fn handle_mouse()`
/// - `capabilities: [Cap1, Cap2]` → `fn requested_capabilities()`
/// - `authorities: [Auth1, Auth2]` → `fn requested_authorities()`
/// - `update_effects(payload) { ... }` → `fn update_effects() -> Effects`
/// - `on_io_event_effects(event) { ... }` → `fn on_io_event_effects() -> Effects`
/// - `impl { fn method(&self) { ... } ... }` — helper methods on `__KasanePluginState` (requires `state`)
#[proc_macro]
pub fn kasane_define_plugin(input: TokenStream) -> TokenStream {
    let input2: proc_macro2::TokenStream = input.into();

    // We parse at the token stream level rather than using syn's full parser
    // because the input has a custom DSL syntax, not standard Rust.
    let result = define_plugin::define_plugin_impl(input2);
    match result {
        Ok(tokens) => tokens.into(),
        Err(e) => e.into_compile_error().into(),
    }
}
