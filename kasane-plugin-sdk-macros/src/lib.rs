//! Proc macros for the Kasane WASM plugin SDK.
//!
//! Provides `#[kasane_wasm_plugin]` to auto-fill default method stubs
//! in a `Guest` trait implementation, so plugin authors only need to
//! implement the methods they actually use.

use proc_macro::TokenStream;
use quote::quote;
use syn::{parse_macro_input, ImplItem, ItemImpl};

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
///     fn on_state_changed(dirty_flags: u16) -> Vec<Command> {
///         // ... your logic
///         vec![]
///     }
///
///     fn annotate_line(line: u32, _ctx: AnnotateContext) -> Option<LineAnnotation> {
///         // ... your logic
///         None
///     }
///
///     fn annotate_deps() -> u16 { dirty::BUFFER }
///     fn state_hash() -> u64 { ACTIVE_LINE.get() as u64 }
/// }
/// ```
///
/// All other `Guest` methods (`on_init`, `on_shutdown`, `contribute`,
/// `handle_key`, etc.) are automatically generated with their default
/// implementations.
#[proc_macro_attribute]
pub fn kasane_wasm_plugin(_attr: TokenStream, item: TokenStream) -> TokenStream {
    let mut impl_block = parse_macro_input!(item as ItemImpl);

    // Collect names of methods already implemented by the user.
    let existing: std::collections::HashSet<String> = impl_block
        .items
        .iter()
        .filter_map(|item| {
            if let ImplItem::Fn(method) = item {
                Some(method.sig.ident.to_string())
            } else {
                None
            }
        })
        .collect();

    // Also collect names from macro invocations (e.g. kasane_plugin_sdk::default_init!()).
    // Users migrating incrementally may still have some default_*!() calls.
    let macro_provided: std::collections::HashSet<String> = impl_block
        .items
        .iter()
        .filter_map(|item| {
            if let ImplItem::Macro(m) = item {
                let seg = m.mac.path.segments.last()?;
                let name = seg.ident.to_string();
                Some(macro_name_to_methods(&name))
            } else {
                None
            }
        })
        .flatten()
        .collect();

    let all_provided: std::collections::HashSet<String> =
        existing.union(&macro_provided).cloned().collect();

    // Generate defaults for every Guest method not already present.
    let defaults = generate_defaults(&all_provided);

    impl_block
        .items
        .extend(defaults.into_iter().map(ImplItem::Fn));

    TokenStream::from(quote! { #impl_block })
}

/// Maps a `default_*!()` macro name to the Guest method names it provides.
fn macro_name_to_methods(macro_name: &str) -> Vec<String> {
    match macro_name {
        "default_init" => vec!["on_init".into()],
        "default_shutdown" => vec!["on_shutdown".into()],
        "default_state_changed" => vec!["on_state_changed".into()],
        "default_lifecycle" => vec![
            "on_init".into(),
            "on_shutdown".into(),
            "on_state_changed".into(),
        ],
        "default_cache" => vec!["state_hash".into(), "slot_deps".into()],
        "default_input" => vec![
            "handle_mouse".into(),
            "handle_key".into(),
            "observe_key".into(),
            "observe_mouse".into(),
        ],
        "default_surfaces" => vec!["surfaces".into()],
        "default_render_surface" => vec!["render_surface".into()],
        "default_handle_surface_event" => vec!["handle_surface_event".into()],
        "default_handle_surface_state_changed" => {
            vec!["handle_surface_state_changed".into()]
        }
        "default_contribute" => vec!["contribute".into()],
        "default_contribute_to" => vec!["contribute_to".into()],
        "default_contribute_deps" => vec!["contribute_deps".into()],
        "default_line" => vec!["contribute_line".into()],
        "default_overlay" => vec!["contribute_overlay".into()],
        "default_overlay_v2" => vec!["contribute_overlay_v2".into()],
        "default_annotate" => vec!["annotate_line".into()],
        "default_annotate_deps" => vec!["annotate_deps".into()],
        "default_named_slot" => vec!["contribute_named".into()],
        "default_transform" => vec!["transform_element".into()],
        "default_transform_priority" => vec!["transform_priority".into()],
        "default_transform_deps" => vec!["transform_deps".into()],
        "default_menu_transform" => vec!["transform_menu_item".into()],
        "default_replace" => vec!["replace".into()],
        "default_decorate" => vec!["decorate".into()],
        "default_decorator_priority" => vec!["decorator_priority".into()],
        "default_cursor_style" => vec!["cursor_style_override".into()],
        "default_update" => vec!["update".into()],
        "default_capabilities" => vec!["requested_capabilities".into()],
        "default_io_event" => vec!["on_io_event".into()],
        _ => vec![],
    }
}

/// Generate default `ImplItemFn` nodes for all Guest methods not in `existing`.
fn generate_defaults(existing: &std::collections::HashSet<String>) -> Vec<syn::ImplItemFn> {
    let mut defaults = Vec::new();

    macro_rules! add_default {
        ($name:expr, $tokens:expr) => {
            if !existing.contains($name) {
                defaults.push(syn::parse2($tokens).unwrap_or_else(|e| {
                    panic!("kasane_wasm_plugin: failed to parse default for `{}`: {}", $name, e)
                }));
            }
        };
    }

    // --- Lifecycle ---

    add_default!(
        "on_init",
        quote! { fn on_init() -> Vec<Command> { vec![] } }
    );

    add_default!(
        "on_shutdown",
        quote! { fn on_shutdown() -> Vec<Command> { vec![] } }
    );

    add_default!(
        "on_state_changed",
        quote! { fn on_state_changed(_dirty_flags: u16) -> Vec<Command> { vec![] } }
    );

    // --- Surfaces ---

    add_default!(
        "surfaces",
        quote! { fn surfaces() -> Vec<SurfaceDescriptor> { vec![] } }
    );

    add_default!(
        "render_surface",
        quote! {
            fn render_surface(
                _surface_key: String,
                _ctx: SurfaceViewContext,
            ) -> Option<ElementHandle> {
                None
            }
        }
    );

    add_default!(
        "handle_surface_event",
        quote! {
            fn handle_surface_event(
                _surface_key: String,
                _event: SurfaceEvent,
                _ctx: SurfaceEventContext,
            ) -> Vec<Command> {
                vec![]
            }
        }
    );

    add_default!(
        "handle_surface_state_changed",
        quote! {
            fn handle_surface_state_changed(
                _surface_key: String,
                _dirty_flags: u16,
            ) -> Vec<Command> {
                vec![]
            }
        }
    );

    // --- Slot contributions (legacy) ---

    add_default!(
        "contribute",
        quote! { fn contribute(_slot: u8) -> Option<ElementHandle> { None } }
    );

    add_default!(
        "contribute_named",
        quote! { fn contribute_named(_slot_name: String) -> Option<ElementHandle> { None } }
    );

    // --- Slot contributions (current) ---

    add_default!(
        "contribute_to",
        quote! {
            fn contribute_to(_region: SlotId, _ctx: ContributeContext) -> Option<Contribution> {
                None
            }
        }
    );

    // --- Line decoration (legacy) ---

    add_default!(
        "contribute_line",
        quote! { fn contribute_line(_line: u32) -> Option<LineDecoration> { None } }
    );

    // --- Overlay (legacy) ---

    add_default!(
        "contribute_overlay",
        quote! { fn contribute_overlay() -> Option<Overlay> { None } }
    );

    // --- Overlay (current) ---

    add_default!(
        "contribute_overlay_v2",
        quote! {
            fn contribute_overlay_v2(_ctx: OverlayContext) -> Option<OverlayContribution> {
                None
            }
        }
    );

    // --- Line annotation (current) ---

    add_default!(
        "annotate_line",
        quote! {
            fn annotate_line(_line: u32, _ctx: AnnotateContext) -> Option<LineAnnotation> {
                None
            }
        }
    );

    // --- Element transformation (legacy) ---

    add_default!(
        "replace",
        quote! { fn replace(_target: ReplaceTarget) -> Option<ElementHandle> { None } }
    );

    add_default!(
        "decorate",
        quote! {
            fn decorate(_target: DecorateTarget, element: ElementHandle) -> ElementHandle {
                element
            }
        }
    );

    add_default!(
        "decorator_priority",
        quote! { fn decorator_priority() -> u32 { 0 } }
    );

    // --- Element transformation (current) ---

    add_default!(
        "transform_element",
        quote! {
            fn transform_element(
                _target: TransformTarget,
                element: ElementHandle,
                _ctx: TransformContext,
            ) -> ElementHandle {
                element
            }
        }
    );

    add_default!(
        "transform_priority",
        quote! { fn transform_priority() -> i16 { 0 } }
    );

    add_default!(
        "transform_menu_item",
        quote! {
            fn transform_menu_item(
                _item: Vec<Atom>,
                _index: u32,
                _selected: bool,
            ) -> Option<Vec<Atom>> {
                None
            }
        }
    );

    // --- Input handling ---

    add_default!(
        "handle_mouse",
        quote! {
            fn handle_mouse(_event: MouseEvent, _id: InteractiveId) -> Option<Vec<Command>> {
                None
            }
        }
    );

    add_default!(
        "handle_key",
        quote! {
            fn handle_key(_event: KeyEvent) -> Option<Vec<Command>> {
                None
            }
        }
    );

    add_default!(
        "observe_key",
        quote! { fn observe_key(_event: KeyEvent) {} }
    );

    add_default!(
        "observe_mouse",
        quote! { fn observe_mouse(_event: MouseEvent) {} }
    );

    // --- Caching ---

    add_default!(
        "state_hash",
        quote! { fn state_hash() -> u64 { 0 } }
    );

    add_default!(
        "slot_deps",
        // dirty::ALL = 0x17F (excludes PLUGIN_STATE)
        quote! { fn slot_deps(_slot: u8) -> u16 { 0x17F } }
    );

    add_default!(
        "contribute_deps",
        quote! { fn contribute_deps(_region: SlotId) -> u16 { 0 } }
    );

    add_default!(
        "transform_deps",
        // dirty::ALL = 0x17F (excludes PLUGIN_STATE)
        quote! { fn transform_deps(_target: TransformTarget) -> u16 { 0x17F } }
    );

    add_default!(
        "annotate_deps",
        // dirty::ALL = 0x17F (excludes PLUGIN_STATE)
        quote! { fn annotate_deps() -> u16 { 0x17F } }
    );

    // --- Cursor ---

    add_default!(
        "cursor_style_override",
        quote! { fn cursor_style_override() -> Option<u8> { None } }
    );

    // --- Inter-plugin messaging ---

    add_default!(
        "update",
        quote! { fn update(_payload: Vec<u8>) -> Vec<Command> { vec![] } }
    );

    // --- WASI capabilities ---

    add_default!(
        "requested_capabilities",
        quote! { fn requested_capabilities() -> Vec<Capability> { vec![] } }
    );

    // --- I/O events ---

    add_default!(
        "on_io_event",
        quote! { fn on_io_event(_event: IoEvent) -> Vec<Command> { vec![] } }
    );

    // Static assertion: SDK ALL must match our hardcoded default.
    // If this fails, update the three 0x17F literals above.
    #[allow(clippy::eq_op, clippy::assertions_on_constants)]
    const _: () = assert!(
        0x17F
            == ((1 << 0)
                | (1 << 1)
                | (1 << 2)
                | (1 << 3)
                | (1 << 4)
                | (1 << 5)
                | (1 << 6)
                | (1 << 8))
    );

    defaults
}

/// Generate Kasane WIT bindings with embedded WIT content.
///
/// Two forms:
/// - `kasane_plugin_sdk::generate!()` — uses embedded WIT (crates.io consumers)
/// - `kasane_plugin_sdk::generate!("path/to/wit")` — uses file path (monorepo dev)
#[proc_macro]
pub fn kasane_generate(input: TokenStream) -> TokenStream {
    if input.is_empty() {
        let wit_content = include_str!("../wit/plugin.wit");
        quote! {
            wit_bindgen::generate!({
                world: "kasane-plugin",
                inline: #wit_content,
            });
        }
        .into()
    } else {
        let path = parse_macro_input!(input as syn::LitStr);
        quote! {
            wit_bindgen::generate!({
                world: "kasane-plugin",
                path: #path,
            });
        }
        .into()
    }
}
