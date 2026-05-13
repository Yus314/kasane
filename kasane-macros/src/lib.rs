mod component;
mod dirty_tracked;
mod handler_table;
mod plugin;
mod variant_meta;

use proc_macro::TokenStream;

/// Derive a `Plugin` impl from a module definition (HandlerRegistry pattern).
///
/// Place `#[kasane_plugin(v2)]` on a `mod` block to generate a `Plugin` impl:
/// - `#[state] struct State { ... }` — `type State = State`
/// - `#[contribute("slot.name")]` functions → `r.on_contribute(...)`
/// - `#[decorate_background]` functions → `r.on_background(...)`
/// - `#[decorate_gutter(Left, priority)]` functions → `r.on_gutter(...)`
/// - `#[handle_key]` functions → `r.on_key(...)`
/// - `fn on_state_changed(...)` → `r.on_state_changed_tier1(...)`
/// - `#[dirty(FLAGS)]` on `#[state]` struct → `r.declare_interests(FLAGS)`
///
/// The macro requires the `v2` argument; bare `#[kasane_plugin]` is
/// rejected at compile time with a migration hint.
#[proc_macro_attribute]
pub fn kasane_plugin(attr: TokenStream, input: TokenStream) -> TokenStream {
    let attr_str = attr.to_string();
    if attr_str.trim() != "v2" {
        return syn::Error::new(
            proc_macro2::Span::call_site(),
            "#[kasane_plugin] without `(v2)` was removed in Phase β-3.2; \
             use #[kasane_plugin(v2)] or write a manual `impl Plugin`",
        )
        .to_compile_error()
        .into();
    }
    plugin::expand_kasane_plugin_v2(input.into())
        .unwrap_or_else(|e| e.to_compile_error())
        .into()
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

/// Generate a `HandlerTable` + canonical name list from a declarative spec module.
///
/// See `docs/handler-table-dsl.md` for the spec. Invoked as a
/// function-like macro (not an attribute) because the DSL uses custom
/// `handler …;` and `config …;` keywords that are not valid Rust
/// surface syntax. The argument is a `pub mod NAME { … }` block whose
/// body contains the spec entries.
///
/// γ-3.2.1 ships base-shape codegen only (`Lifecycle<E>`, `Observer`,
/// `Dispatcher<C>`, `View<Out>`). Modifiers (`tier1`, `tier2`,
/// `transparent`, `per_slot`, `prioritized`, `unified`, `recovery`,
/// `void`, `default`, `suppresses`, `targets`, `full_fallback`,
/// `stateless`) parse but error at codegen with a γ-3.2.2 hint.
#[proc_macro]
pub fn handler_table(input: TokenStream) -> TokenStream {
    handler_table::expand_handler_table(input.into())
        .unwrap_or_else(|e| e.to_compile_error())
        .into()
}

/// Derive variant-name reflection (`variant_name()`, `ALL_VARIANT_NAMES`,
/// `DESTRUCTIVE_VARIANTS`, `PRESERVING_VARIANTS`) for an enum.
///
/// Tag individual variants with `#[variant_meta(destructive)]` to include
/// them in `DESTRUCTIVE_VARIANTS`; all other variants appear in
/// `PRESERVING_VARIANTS`. See the `variant_meta` module documentation
/// for details.
#[proc_macro_derive(VariantMeta, attributes(variant_meta))]
pub fn derive_variant_meta(input: TokenStream) -> TokenStream {
    variant_meta::expand_variant_meta(input.into())
        .unwrap_or_else(|e| e.to_compile_error())
        .into()
}
