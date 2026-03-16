//! Proc macros for the Kasane WASM plugin SDK.
//!
//! Provides `#[kasane_wasm_plugin]` to auto-fill default method stubs
//! in a `Guest` trait implementation, so plugin authors only need to
//! implement the methods they actually use.

use proc_macro::TokenStream;
use quote::quote;
use syn::{ImplItem, ItemImpl, parse_macro_input};

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
        "slots" => vec!["contribute_to".into(), "contribute_deps".into()],
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
                    panic!(
                        "kasane_wasm_plugin: failed to parse default for `{}`: {}",
                        $name, e
                    )
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
        quote! { fn state_hash() -> u64 { __kasane_auto_state_hash() } }
    );

    add_default!(
        "slot_deps",
        // dirty::ALL = 0x17F (excludes PLUGIN_STATE)
        quote! { fn slot_deps(_slot: u8) -> u16 { 0x17F } }
    );

    add_default!(
        "contribute_deps",
        // dirty::ALL = 0x17F (excludes PLUGIN_STATE) — safe default
        quote! { fn contribute_deps(_region: SlotId) -> u16 { 0x17F } }
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
///
/// In addition to the WIT bindings, this macro emits:
/// - Auto `use` statements for common WIT types (`Guest`, `types::*`, etc.)
/// - Face/Color helper functions (`default_face()`, `face_bg()`, `rgb()`, etc.)
/// - Overlay positioning helpers (`centered_overlay()`)
#[proc_macro]
pub fn kasane_generate(input: TokenStream) -> TokenStream {
    let wit_bindings = if input.is_empty() {
        let wit_content = include_str!("../wit/plugin.wit");
        quote! {
            wit_bindgen::generate!({
                world: "kasane-plugin",
                inline: #wit_content,
            });
        }
    } else {
        let path = parse_macro_input!(input as syn::LitStr);
        quote! {
            wit_bindgen::generate!({
                world: "kasane-plugin",
                path: #path,
            });
        }
    };

    let sdk_helpers = generate_sdk_helpers();

    quote! {
        #wit_bindings
        #sdk_helpers
    }
    .into()
}

/// Generate SDK helper code emitted alongside WIT bindings.
///
/// This code lives in the user's crate so it can reference WIT-generated types
/// (Face, Color, RgbColor, etc.) which are not accessible from the SDK crate.
fn generate_sdk_helpers() -> proc_macro2::TokenStream {
    quote! {
        /// SDK-generated prelude and helper functions.
        ///
        /// Items are re-exported via glob import so that explicit user imports
        /// shadow them without conflict (standard Rust prelude pattern).
        #[doc(hidden)]
        #[allow(dead_code)]
        mod __kasane_sdk {
            pub use super::exports::kasane::plugin::plugin_api::Guest;
            pub use super::kasane::plugin::host_state;
            pub use super::kasane::plugin::element_builder;
            pub use super::kasane::plugin::types::*;

            use super::kasane::plugin::types::*;

            /// Create a default face (all colors default, no attributes).
            pub fn default_face() -> Face {
                Face {
                    fg: Color::DefaultColor,
                    bg: Color::DefaultColor,
                    underline: Color::DefaultColor,
                    attributes: 0,
                }
            }

            /// Create a face with only the foreground color set.
            pub fn face_fg(color: Color) -> Face {
                Face {
                    fg: color,
                    bg: Color::DefaultColor,
                    underline: Color::DefaultColor,
                    attributes: 0,
                }
            }

            /// Create a face with only the background color set.
            pub fn face_bg(color: Color) -> Face {
                Face {
                    fg: Color::DefaultColor,
                    bg: color,
                    underline: Color::DefaultColor,
                    attributes: 0,
                }
            }

            /// Create a face with foreground and background colors.
            pub fn face(fg: Color, bg: Color) -> Face {
                Face {
                    fg,
                    bg,
                    underline: Color::DefaultColor,
                    attributes: 0,
                }
            }

            /// Create a face with all fields specified.
            pub fn face_full(fg: Color, bg: Color, underline: Color, attrs: u16) -> Face {
                Face {
                    fg,
                    bg,
                    underline,
                    attributes: attrs,
                }
            }

            /// Create an RGB color.
            pub fn rgb(r: u8, g: u8, b: u8) -> Color {
                Color::Rgb(RgbColor { r, g, b })
            }

            /// Create a named color.
            pub fn named(n: NamedColor) -> Color {
                Color::Named(n)
            }

            // ----- Element builder shorthand wrappers -----

            /// Create a text element.
            pub fn text(content: &str, f: Face) -> ElementHandle {
                super::kasane::plugin::element_builder::create_text(content, f)
            }

            /// Create a styled-line element from atoms.
            pub fn styled_line(atoms: &[Atom]) -> ElementHandle {
                super::kasane::plugin::element_builder::create_styled_line(atoms)
            }

            /// Create a vertical column of children.
            pub fn column(children: &[ElementHandle]) -> ElementHandle {
                super::kasane::plugin::element_builder::create_column(children)
            }

            /// Create a horizontal row of children.
            pub fn row(children: &[ElementHandle]) -> ElementHandle {
                super::kasane::plugin::element_builder::create_row(children)
            }

            /// Create an interactive wrapper element.
            pub fn interactive(child: ElementHandle, id: InteractiveId) -> ElementHandle {
                super::kasane::plugin::element_builder::create_interactive(child, id)
            }

            // ----- LineAnnotation shortcuts -----

            /// Create a background-only line annotation.
            pub fn bg_annotation(f: Face) -> LineAnnotation {
                LineAnnotation {
                    left_gutter: None,
                    right_gutter: None,
                    background: Some(BackgroundLayer {
                        face: f,
                        z_order: 0,
                        blend_opaque: true,
                    }),
                    priority: 0,
                }
            }

            /// Create a left-gutter line annotation.
            pub fn gutter_annotation(el: ElementHandle, priority: i16) -> LineAnnotation {
                LineAnnotation {
                    left_gutter: Some(el),
                    right_gutter: None,
                    background: None,
                    priority,
                }
            }

            // ----- Contribution shortcut -----

            /// Create a contribution with auto size hint and priority 0.
            pub fn auto_contribution(element: ElementHandle) -> Contribution {
                Contribution {
                    element,
                    priority: 0,
                    size_hint: ContribSizeHint::Auto,
                }
            }

            // ----- Edges shortcuts -----

            /// Create edges with explicit values.
            pub fn edges(top: u16, right: u16, bottom: u16, left: u16) -> Edges {
                Edges { top, right, bottom, left }
            }

            /// Create edges with horizontal padding only.
            pub fn padding_h(lr: u16) -> Edges {
                Edges { top: 0, right: lr, bottom: 0, left: lr }
            }

            // ----- Container builder -----

            /// Builder for container elements.
            pub struct ContainerBuilder {
                child: ElementHandle,
                border: Option<BorderLineStyle>,
                shadow: bool,
                padding: Edges,
                style: Face,
                title: Option<Vec<Atom>>,
            }

            impl ContainerBuilder {
                pub fn new(child: ElementHandle) -> Self {
                    Self {
                        child,
                        border: None,
                        shadow: false,
                        padding: Edges { top: 0, right: 0, bottom: 0, left: 0 },
                        style: Face {
                            fg: Color::DefaultColor,
                            bg: Color::DefaultColor,
                            underline: Color::DefaultColor,
                            attributes: 0,
                        },
                        title: None,
                    }
                }

                pub fn border(mut self, style: BorderLineStyle) -> Self {
                    self.border = Some(style);
                    self
                }

                pub fn shadow(mut self) -> Self {
                    self.shadow = true;
                    self
                }

                pub fn padding(mut self, edges: Edges) -> Self {
                    self.padding = edges;
                    self
                }

                pub fn style(mut self, face: Face) -> Self {
                    self.style = face;
                    self
                }

                pub fn title_text(mut self, text: &str) -> Self {
                    self.title = Some(vec![Atom {
                        face: Face {
                            fg: Color::DefaultColor,
                            bg: Color::DefaultColor,
                            underline: Color::DefaultColor,
                            attributes: 0,
                        },
                        contents: text.to_string(),
                    }]);
                    self
                }

                pub fn title(mut self, atoms: &[Atom]) -> Self {
                    self.title = Some(atoms.to_vec());
                    self
                }

                pub fn build(self) -> ElementHandle {
                    super::kasane::plugin::element_builder::create_container_styled(
                        self.child,
                        self.border,
                        self.shadow,
                        self.padding,
                        self.style,
                        self.title.as_deref(),
                    )
                }
            }

            /// Start building a container element.
            pub fn container(child: ElementHandle) -> ContainerBuilder {
                ContainerBuilder::new(child)
            }

            /// Compute a centered overlay `AbsoluteAnchor` given screen dimensions,
            /// desired size as percentages, and minimum dimensions.
            pub fn centered_overlay(
                screen_cols: u16,
                screen_rows: u16,
                w_pct: u32,
                h_pct: u32,
                min_w: u16,
                min_h: u16,
            ) -> AbsoluteAnchor {
                let w = (screen_cols as u32 * w_pct / 100)
                    .max(min_w as u32)
                    .min(screen_cols as u32) as u16;
                let h = (screen_rows as u32 * h_pct / 100)
                    .max(min_h as u32)
                    .min(screen_rows as u32) as u16;
                let x = (screen_cols.saturating_sub(w)) / 2;
                let y = (screen_rows.saturating_sub(h)) / 2;
                AbsoluteAnchor { x, y, w, h }
            }
        }

        #[allow(unused_imports)]
        use __kasane_sdk::*;
    }
}
