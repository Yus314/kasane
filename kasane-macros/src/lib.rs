mod component;
mod dirty_tracked;
mod plugin;

use proc_macro::TokenStream;

/// Derive a `Plugin` impl from a module definition.
///
/// # Legacy mode (default)
///
/// Place `#[kasane_plugin]` on a `mod` block containing:
/// - `#[state] struct State { ... }` — plugin state type
/// - `#[event] enum Msg { ... }` — message type
/// - `fn update(state: &mut State, msg: Msg, core: &AppState) -> Vec<Command>` — state update
/// - `#[transform(TransformTarget::*, priority = N)] fn transform(...)` — element transformer
/// - `fn annotate_line(state: &State, line: usize, core: &AppState, ctx: &AnnotateContext) -> Option<LineAnnotation>` — line annotation
/// - `fn transform_menu_item(...)` — menu item transformer
///
/// Generates a `{PascalCase}Plugin` struct with a `PluginBackend` trait impl.
///
/// # Handler registry mode (`v2`)
///
/// Place `#[kasane_plugin(v2)]` on a `mod` block to generate a `Plugin` impl
/// using the `HandlerRegistry` pattern:
/// - `#[state] struct State { ... }` — `type State = State`
/// - `#[contribute("slot.name")]` functions → `r.on_contribute(...)`
/// - `#[annotate_background]` functions → `r.on_annotate_background(...)`
/// - `#[annotate_gutter(Left, priority)]` functions → `r.on_annotate_gutter(...)`
/// - `#[handle_key]` functions → `r.on_key(...)`
/// - `fn on_state_changed(...)` → `r.on_state_changed(...)`
/// - `#[dirty(FLAGS)]` on `#[state]` struct → `r.declare_interests(FLAGS)`
#[proc_macro_attribute]
pub fn kasane_plugin(attr: TokenStream, input: TokenStream) -> TokenStream {
    let attr_str = attr.to_string();
    if attr_str.trim() == "v2" {
        plugin::expand_kasane_plugin_v2(input.into())
            .unwrap_or_else(|e| e.to_compile_error())
            .into()
    } else {
        plugin::expand_kasane_plugin(input.into())
            .unwrap_or_else(|e| e.to_compile_error())
            .into()
    }
}

/// Validate a pure component function.
///
/// Checks that the function has a return type and no `&mut` parameters.
/// Any attribute arguments are accepted but ignored (for backward compatibility).
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
#[proc_macro_derive(DirtyTracked, attributes(dirty, epistemic))]
pub fn derive_dirty_tracked(input: TokenStream) -> TokenStream {
    dirty_tracked::expand_dirty_tracked(input.into())
        .unwrap_or_else(|e| e.to_compile_error())
        .into()
}
