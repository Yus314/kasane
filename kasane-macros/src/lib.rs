mod analysis;
mod component;
mod dirty_tracked;
mod plugin;

use proc_macro::TokenStream;

/// Derive a `Plugin` impl from a module definition.
///
/// Place `#[kasane_plugin]` on a `mod` block containing:
/// - `#[state] struct State { ... }` — plugin state type
/// - `#[event] enum Msg { ... }` — message type
/// - `fn update(state: &mut State, msg: Msg, core: &AppState) -> Vec<Command>` — state update
/// - `#[transform(TransformTarget::*, priority = N)] fn transform(...)` — element transformer
/// - `fn annotate_line(state: &State, line: usize, core: &AppState, ctx: &AnnotateContext) -> Option<LineAnnotation>` — line annotation
/// - `fn transform_menu_item(...)` — menu item transformer
///
/// Generates a `{PascalCase}Plugin` struct with a `Plugin` trait impl.
#[proc_macro_attribute]
pub fn kasane_plugin(_attr: TokenStream, input: TokenStream) -> TokenStream {
    plugin::expand_kasane_plugin(input.into())
        .unwrap_or_else(|e| e.to_compile_error())
        .into()
}

/// Validate a pure component function with optional DirtyFlags dependency annotation.
///
/// Usage:
/// - `#[kasane_component]` — bare validation only
/// - `#[kasane_component(deps(BUFFER, STATUS))]` — validate + document dependencies
///
/// Valid flag names: `BUFFER`, `STATUS`, `MENU_STRUCTURE`, `MENU_SELECTION`, `MENU`,
/// `INFO`, `OPTIONS`, `ALL`.
#[proc_macro_attribute]
pub fn kasane_component(attr: TokenStream, input: TokenStream) -> TokenStream {
    component::expand_kasane_component(attr.into(), input.into())
        .unwrap_or_else(|e| e.to_compile_error())
        .into()
}

/// Derive macro that enforces compile-time field → DirtyFlags mapping.
///
/// Every field must have a `#[dirty(FLAG)]` or `#[dirty(free)]` annotation.
/// Missing annotations produce a compile error.
///
/// Generates `AppState::FIELD_DIRTY_MAP` and `AppState::FREE_READ_FIELDS` constants.
#[proc_macro_derive(DirtyTracked, attributes(dirty))]
pub fn derive_dirty_tracked(input: TokenStream) -> TokenStream {
    dirty_tracked::expand_dirty_tracked(input.into())
        .unwrap_or_else(|e| e.to_compile_error())
        .into()
}
