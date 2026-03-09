mod component;
mod plugin;

use proc_macro::TokenStream;

/// Derive a `Plugin` impl from a module definition.
///
/// Place `#[kasane_plugin]` on a `mod` block containing:
/// - `#[state] struct State { ... }` — plugin state type
/// - `#[event] enum Msg { ... }` — message type
/// - `fn update(state: &mut State, msg: Msg, core: &AppState) -> Vec<Command>` — state update
/// - `#[slot(Slot::*)] fn view(state: &State, core: &AppState) -> Option<Element>` — slot contribution
/// - `#[decorate(DecorateTarget::*, priority = N)] fn decorate(...)` — decorator
/// - `#[replace(ReplaceTarget::*)] fn replace(...)` — replacement
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
