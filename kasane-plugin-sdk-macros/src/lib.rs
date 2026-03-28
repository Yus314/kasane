//! Proc macros for the Kasane WASM plugin SDK.
//!
//! Provides `#[kasane_wasm_plugin]` to auto-fill default method stubs
//! in a `Guest` trait implementation, so plugin authors only need to
//! implement the methods they actually use.
//!
//! Also provides `define_plugin!` for a single-macro plugin definition
//! that combines `generate!()`, `state!`, `#[plugin]`, and `export!()`.

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
///     fn on_state_changed_effects(dirty_flags: u16) -> RuntimeEffects {
///         let _ = dirty_flags;
///         RuntimeEffects::default()
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

    // Validate that all user-written methods are known Guest methods.
    let known = known_guest_methods();
    let mut errors = Vec::new();
    for item in &impl_block.items {
        if let ImplItem::Fn(method) = item {
            let name = method.sig.ident.to_string();
            if !known.contains(name.as_str()) {
                let suggestions = suggest_similar(&name, &known);
                let msg = if suggestions.is_empty() {
                    format!("unknown Guest method `{name}`")
                } else {
                    format!("unknown Guest method `{name}`. Did you mean {suggestions}?")
                };
                errors.push(syn::Error::new(method.sig.ident.span(), msg));
            }
        }
    }
    if !errors.is_empty() {
        let combined = errors
            .into_iter()
            .reduce(|mut a, b| {
                a.combine(b);
                a
            })
            .unwrap();
        return combined.into_compile_error().into();
    }

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
        "default_init" => vec!["on_init_effects".into()],
        "default_active_session_ready" => vec!["on_active_session_ready_effects".into()],
        "default_shutdown" => vec!["on_shutdown".into()],
        "default_state_changed" => vec!["on_state_changed_effects".into()],
        "default_lifecycle" => vec![
            "on_init_effects".into(),
            "on_active_session_ready_effects".into(),
            "on_shutdown".into(),
            "on_state_changed_effects".into(),
        ],
        "default_typed_lifecycle" => vec![
            "on_init_effects".into(),
            "on_active_session_ready_effects".into(),
            "on_state_changed_effects".into(),
            "on_shutdown".into(),
        ],
        "default_typed_init" => vec!["on_init_effects".into()],
        "default_typed_active_session_ready" => vec!["on_active_session_ready_effects".into()],
        "default_typed_state_changed" => vec!["on_state_changed_effects".into()],
        "default_cache" => vec!["state_hash".into()],
        "default_input" => vec![
            "handle_mouse".into(),
            "handle_key".into(),
            "handle_key_middleware".into(),
            "handle_default_scroll".into(),
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
        "default_line" => vec!["contribute_line".into()],
        "default_overlay" => vec!["contribute_overlay".into()],
        "default_overlay_v2" => vec!["contribute_overlay_v2".into()],
        "default_annotate" => vec!["annotate_line".into()],
        "default_display_directives" => vec!["display_directives".into()],
        "default_workspace_changed" => vec!["on_workspace_changed".into()],
        "default_named_slot" => vec!["contribute_named".into()],
        "default_transform" => vec!["transform".into()],
        "default_transform_priority" => vec!["transform_priority".into()],
        "default_menu_transform" => vec!["transform_menu_item".into()],
        "default_replace" => vec!["replace".into()],
        "default_decorate" => vec!["decorate".into()],
        "default_decorator_priority" => vec!["decorator_priority".into()],
        "default_cursor_style" => vec!["cursor_style_override".into()],
        "default_update" => vec!["update_effects".into()],
        "default_typed_runtime" => vec!["update_effects".into(), "on_io_event_effects".into()],
        "default_typed_update" => vec!["update_effects".into()],
        "default_typed_io_event" => vec!["on_io_event_effects".into()],
        "default_capabilities" => vec!["requested_capabilities".into()],
        "default_authorities" => vec!["requested_authorities".into()],
        "default_io_event" => vec!["on_io_event_effects".into()],
        "slots" => vec!["contribute_to".into()],
        _ => vec![],
    }
}

/// The complete set of valid Guest trait method names.
fn known_guest_methods() -> std::collections::HashSet<&'static str> {
    [
        "get_id",
        "on_init_effects",
        "on_active_session_ready_effects",
        "on_shutdown",
        "on_state_changed_effects",
        "on_workspace_changed",
        "surfaces",
        "render_surface",
        "handle_surface_event",
        "handle_surface_state_changed",
        "contribute",
        "contribute_named",
        "contribute_to",
        "contribute_line",
        "contribute_overlay",
        "contribute_overlay_v2",
        "annotate_line",
        "display_directives",
        "replace",
        "decorate",
        "decorator_priority",
        "transform",
        "transform_priority",
        "transform_menu_item",
        "handle_mouse",
        "handle_key",
        "handle_key_middleware",
        "handle_default_scroll",
        "observe_key",
        "observe_mouse",
        "state_hash",
        "cursor_style_override",
        "decorate_cells",
        "update_effects",
        "requested_capabilities",
        "requested_authorities",
        "on_io_event_effects",
        "view_deps",
        "register_capabilities",
    ]
    .into_iter()
    .collect()
}

/// Suggest similar method names using edit distance (Levenshtein).
fn suggest_similar(input: &str, known: &std::collections::HashSet<&str>) -> String {
    let mut candidates: Vec<(usize, &str)> = known
        .iter()
        .map(|k| (edit_distance(input, k), *k))
        .filter(|(d, _)| *d <= 4)
        .collect();
    candidates.sort_by_key(|(d, _)| *d);
    candidates
        .iter()
        .take(3)
        .map(|(_, k)| format!("`{k}`"))
        .collect::<Vec<_>>()
        .join(", ")
}

/// Simple Levenshtein distance.
fn edit_distance(a: &str, b: &str) -> usize {
    let a: Vec<char> = a.chars().collect();
    let b: Vec<char> = b.chars().collect();
    let (m, n) = (a.len(), b.len());
    let mut dp = vec![vec![0usize; n + 1]; m + 1];
    for i in 0..=m {
        dp[i][0] = i;
    }
    for j in 0..=n {
        dp[0][j] = j;
    }
    for i in 1..=m {
        for j in 1..=n {
            let cost = if a[i - 1] == b[j - 1] { 0 } else { 1 };
            dp[i][j] = (dp[i - 1][j] + 1)
                .min(dp[i][j - 1] + 1)
                .min(dp[i - 1][j - 1] + cost);
        }
    }
    dp[m][n]
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
        "on_init_effects",
        quote! { fn on_init_effects() -> BootstrapEffects { BootstrapEffects::default() } }
    );

    add_default!(
        "on_active_session_ready_effects",
        quote! {
            fn on_active_session_ready_effects() -> SessionReadyEffects {
                SessionReadyEffects::default()
            }
        }
    );

    add_default!(
        "on_shutdown",
        quote! { fn on_shutdown() -> Vec<Command> { vec![] } }
    );

    add_default!(
        "on_state_changed_effects",
        quote! {
            fn on_state_changed_effects(_dirty_flags: u16) -> RuntimeEffects {
                RuntimeEffects::default()
            }
        }
    );

    add_default!(
        "on_workspace_changed",
        quote! {
            fn on_workspace_changed(_snapshot: WorkspaceSnapshot) {}
        }
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

    add_default!(
        "display_directives",
        quote! {
            fn display_directives() -> Vec<DisplayDirective> {
                vec![]
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
        "transform",
        quote! {
            fn transform(
                _target: TransformTarget,
                subject: TransformSubject,
                _ctx: TransformContext,
            ) -> TransformSubject {
                subject
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
        "handle_key_middleware",
        quote! {
            fn handle_key_middleware(event: KeyEvent) -> KeyHandleResult {
                match Self::handle_key(event) {
                    Some(commands) => KeyHandleResult::Consumed(commands),
                    None => KeyHandleResult::Passthrough,
                }
            }
        }
    );

    add_default!(
        "handle_default_scroll",
        quote! {
            fn handle_default_scroll(
                _candidate: DefaultScrollCandidate
            ) -> Option<ScrollPolicyResult> {
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

    // --- Cursor ---

    add_default!(
        "cursor_style_override",
        quote! { fn cursor_style_override() -> Option<u8> { None } }
    );

    add_default!(
        "decorate_cells",
        quote! { fn decorate_cells() -> Vec<CellDecoration> { Vec::new() } }
    );

    // --- Inter-plugin messaging ---

    add_default!(
        "update_effects",
        quote! { fn update_effects(_payload: Vec<u8>) -> RuntimeEffects { RuntimeEffects::default() } }
    );

    // --- WASI capabilities ---

    add_default!(
        "requested_capabilities",
        quote! { fn requested_capabilities() -> Vec<Capability> { vec![] } }
    );

    add_default!(
        "requested_authorities",
        quote! { fn requested_authorities() -> Vec<PluginAuthority> { vec![] } }
    );

    // --- I/O events ---

    add_default!(
        "on_io_event_effects",
        quote! { fn on_io_event_effects(_event: IoEvent) -> RuntimeEffects { RuntimeEffects::default() } }
    );

    // --- View dependency declaration ---

    add_default!(
        "view_deps",
        quote! { fn view_deps() -> u16 { 0x17F } } // ALL
    );

    // --- Handler capability declaration (v0.23.0) ---
    // Auto-infer PluginCapabilities bitmask from which methods are implemented.
    // Bit layout matches kasane-core PluginCapabilities bitflags.
    if !existing.contains("register_capabilities") {
        let mut caps: u32 = 0;
        // OVERLAY = 1 << 2
        if existing.contains("contribute_overlay") || existing.contains("contribute_overlay_v2") {
            caps |= 1 << 2;
        }
        // MENU_TRANSFORM = 1 << 5
        if existing.contains("transform_menu_item") {
            caps |= 1 << 5;
        }
        // CURSOR_STYLE = 1 << 6
        if existing.contains("cursor_style_override") {
            caps |= 1 << 6;
        }
        // INPUT_HANDLER = 1 << 7
        if existing.contains("handle_key")
            || existing.contains("handle_key_middleware")
            || existing.contains("handle_mouse")
        {
            caps |= 1 << 7;
        }
        // SURFACE_PROVIDER = 1 << 11
        if existing.contains("surfaces") {
            caps |= 1 << 11;
        }
        // WORKSPACE_OBSERVER = 1 << 12
        if existing.contains("on_workspace_changed") {
            caps |= 1 << 12;
        }
        // CONTRIBUTOR = 1 << 14
        if existing.contains("contribute")
            || existing.contains("contribute_to")
            || existing.contains("contribute_named")
        {
            caps |= 1 << 14;
        }
        // TRANSFORMER = 1 << 15
        if existing.contains("transform")
            || existing.contains("replace")
            || existing.contains("decorate")
        {
            caps |= 1 << 15;
        }
        // ANNOTATOR = 1 << 16
        if existing.contains("annotate_line") || existing.contains("contribute_line") {
            caps |= 1 << 16;
        }
        // IO_HANDLER = 1 << 17
        if existing.contains("on_io_event_effects") {
            caps |= 1 << 17;
        }
        // DISPLAY_TRANSFORM = 1 << 18
        if existing.contains("display_directives") {
            caps |= 1 << 18;
        }
        // SCROLL_POLICY = 1 << 19
        if existing.contains("handle_default_scroll") {
            caps |= 1 << 19;
        }
        // CELL_DECORATION = 1 << 20
        if existing.contains("decorate_cells") {
            caps |= 1 << 20;
        }

        let caps_literal = caps;
        defaults.push(
            syn::parse2(quote! {
                fn register_capabilities() -> u32 { #caps_literal }
            })
            .expect("register_capabilities default"),
        );
    }

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

            // Re-export host-log for direct use
            pub use super::kasane::plugin::host_log;

            use super::kasane::plugin::types::*;

            impl ::core::default::Default for BootstrapEffects {
                fn default() -> Self {
                    Self { redraw: 0 }
                }
            }

            impl ::core::default::Default for SessionReadyEffects {
                fn default() -> Self {
                    Self {
                        redraw: 0,
                        commands: vec![],
                        scroll_plans: vec![],
                    }
                }
            }

            impl ::core::default::Default for RuntimeEffects {
                fn default() -> Self {
                    Self {
                        redraw: 0,
                        commands: vec![],
                        scroll_plans: vec![],
                    }
                }
            }

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

            /// Create an image element from a file path. Size is in cells.
            pub fn image_file(path: &str, width: u16, height: u16) -> ElementHandle {
                let source = super::kasane::plugin::types::ImageSource::FilePath(path.to_string());
                super::kasane::plugin::element_builder::create_image(
                    &source,
                    width,
                    height,
                    super::kasane::plugin::types::ImageFit::Contain,
                    1.0,
                )
            }

            /// Create an image element from inline RGBA data. Size is in cells.
            pub fn image_rgba(
                data: &[u8],
                img_width: u32,
                img_height: u32,
                cell_width: u16,
                cell_height: u16,
            ) -> ElementHandle {
                let source = super::kasane::plugin::types::ImageSource::RgbaData(
                    super::kasane::plugin::types::RgbaImage {
                        data: data.to_vec(),
                        width: img_width,
                        height: img_height,
                    },
                );
                super::kasane::plugin::element_builder::create_image(
                    &source,
                    cell_width,
                    cell_height,
                    super::kasane::plugin::types::ImageFit::Contain,
                    1.0,
                )
            }

            /// Create an image element from inline SVG data. Size is in cells.
            pub fn image_svg(svg_data: &[u8], width: u16, height: u16) -> ElementHandle {
                let source = super::kasane::plugin::types::ImageSource::SvgData(
                    svg_data.to_vec(),
                );
                super::kasane::plugin::element_builder::create_image(
                    &source,
                    width,
                    height,
                    super::kasane::plugin::types::ImageFit::Contain,
                    1.0,
                )
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
                    inline: None,
                    virtual_text: vec![],
                }
            }

            /// Create a left-gutter line annotation.
            pub fn gutter_annotation(el: ElementHandle, priority: i16) -> LineAnnotation {
                LineAnnotation {
                    left_gutter: Some(el),
                    right_gutter: None,
                    background: None,
                    priority,
                    inline: None,
                    virtual_text: vec![],
                }
            }

            /// Create an EOL virtual text annotation.
            pub fn eol_annotation(atoms: Vec<Atom>, priority: i16) -> LineAnnotation {
                LineAnnotation {
                    left_gutter: None,
                    right_gutter: None,
                    background: None,
                    priority: 0,
                    inline: None,
                    virtual_text: vec![VirtualTextItem { atoms, priority }],
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

            // ----- Convenience shortcuts -----

            /// Create a text element with the default face.
            pub fn plain(s: &str) -> ElementHandle {
                text(s, default_face())
            }

            /// Create a text element with a foreground color.
            pub fn colored(s: &str, fg: Color) -> ElementHandle {
                text(s, face_fg(fg))
            }

            /// Check if a key event is Ctrl+key (no Alt/Shift).
            pub fn is_ctrl(event: &KeyEvent, key: &str) -> bool {
                matches!(event.key, KeyCode::Character(ref c) if c == key)
                    && event.modifiers == 0x01 // CTRL only
            }

            /// Check if a key event is Alt+key (no Ctrl/Shift).
            pub fn is_alt(event: &KeyEvent, key: &str) -> bool {
                matches!(event.key, KeyCode::Character(ref c) if c == key)
                    && event.modifiers == 0x02 // ALT only
            }

            /// Check if a key event is Ctrl+Shift+key (no Alt).
            pub fn is_ctrl_shift(event: &KeyEvent, key: &str) -> bool {
                matches!(event.key, KeyCode::Character(ref c) if c == key)
                    && event.modifiers == (0x01 | 0x04) // CTRL + SHIFT
            }

            /// Conditional status bar badge: returns a contribution if `condition` is true.
            pub fn status_badge(condition: bool, label: &str) -> Option<Contribution> {
                condition.then(|| auto_contribution(plain(label)))
            }

            /// Parse a hex color string (`"#rrggbb"` or `"#rgb"`) into a `Color`.
            /// Returns `Color::DefaultColor` on invalid input.
            pub fn hex(s: &str) -> Color {
                let s = s.strip_prefix('#').unwrap_or(s);
                match s.len() {
                    6 => {
                        let Ok(r) = u8::from_str_radix(&s[0..2], 16) else { return Color::DefaultColor };
                        let Ok(g) = u8::from_str_radix(&s[2..4], 16) else { return Color::DefaultColor };
                        let Ok(b) = u8::from_str_radix(&s[4..6], 16) else { return Color::DefaultColor };
                        Color::Rgb(RgbColor { r, g, b })
                    }
                    3 => {
                        let Ok(r) = u8::from_str_radix(&s[0..1], 16) else { return Color::DefaultColor };
                        let Ok(g) = u8::from_str_radix(&s[1..2], 16) else { return Color::DefaultColor };
                        let Ok(b) = u8::from_str_radix(&s[2..3], 16) else { return Color::DefaultColor };
                        Color::Rgb(RgbColor { r: r * 17, g: g * 17, b: b * 17 })
                    }
                    _ => Color::DefaultColor,
                }
            }

            // ----- Theme helpers -----

            /// Query the host theme for a face associated with a token name.
            /// Returns `None` if the token is not in the theme.
            pub fn get_theme_face(token: &str) -> Option<Face> {
                super::kasane::plugin::host_state::get_theme_face(token)
            }

            /// Whether the current editor background is dark.
            pub fn is_dark_background() -> bool {
                super::kasane::plugin::host_state::is_dark_background()
            }

            /// Query the host theme for a face, falling back to `fallback` if not found.
            pub fn theme_face_or(token: &str, fallback: Face) -> Face {
                get_theme_face(token).unwrap_or(fallback)
            }

            // ----- Command shortcuts -----

            /// Request a full redraw (all dirty flags).
            pub fn redraw() -> Vec<Command> {
                vec![Command::RequestRedraw(0x17F)]
            }

            /// Request a redraw with specific dirty flags.
            pub fn redraw_flags(flags: u16) -> Vec<Command> {
                vec![Command::RequestRedraw(flags)]
            }

            /// Build a `Command::SendKeys` that runs a Kakoune command.
            pub fn send_command(cmd: &str) -> Command {
                Command::SendKeys(kasane_plugin_sdk::keys::command(cmd))
            }

            /// Build a single-atom InsertAfter display directive.
            pub fn plain_insert_after(after: u32, text: &str, face: Face) -> DisplayDirective {
                DisplayDirective::InsertAfter(InsertAfterDirective {
                    after,
                    content: vec![Atom { face, contents: text.to_string() }],
                })
            }

            /// Build a single-atom InsertBefore display directive.
            pub fn plain_insert_before(before: u32, text: &str, face: Face) -> DisplayDirective {
                DisplayDirective::InsertBefore(InsertBeforeDirective {
                    before,
                    content: vec![Atom { face, contents: text.to_string() }],
                })
            }

            /// Build a single-atom Fold display directive.
            pub fn plain_fold(range_start: u32, range_end: u32, text: &str, face: Face) -> DisplayDirective {
                DisplayDirective::Fold(FoldDirective {
                    range_start,
                    range_end,
                    summary: vec![Atom { face, contents: text.to_string() }],
                })
            }

            /// Build a dynamic hosted surface registration command.
            pub fn register_surface(
                surface_key: &str,
                size_hint: SurfaceSizeHint,
                declared_slots: Vec<DeclaredSlot>,
                placement: SurfacePlacement,
            ) -> Command {
                Command::RegisterSurface(DynamicSurfaceConfig {
                    surface_key: surface_key.to_string(),
                    size_hint,
                    declared_slots,
                    placement,
                })
            }

            /// Build a dynamic hosted surface unregistration command.
            pub fn unregister_surface(surface_key: &str) -> Command {
                Command::UnregisterSurface(surface_key.to_string())
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

            /// Centered overlay sized to fit content rows (auto height).
            pub fn content_fit_overlay(
                screen_cols: u16,
                screen_rows: u16,
                w_pct: u32,
                min_w: u16,
                content_rows: u16,
                chrome_rows: u16,
            ) -> AbsoluteAnchor {
                let w = (screen_cols as u32 * w_pct / 100)
                    .max(min_w as u32)
                    .min(screen_cols as u32) as u16;
                let h = (content_rows + chrome_rows).min(screen_rows);
                let x = (screen_cols.saturating_sub(w)) / 2;
                let y = (screen_rows.saturating_sub(h)) / 2;
                AbsoluteAnchor { x, y, w, h }
            }

            // --- Logging helpers ---

            pub fn log_debug(msg: &str) {
                host_log::log_message(host_log::LogLevel::Debug, msg);
            }

            pub fn log_info(msg: &str) {
                host_log::log_message(host_log::LogLevel::Info, msg);
            }

            pub fn log_warn(msg: &str) {
                host_log::log_message(host_log::LogLevel::Warn, msg);
            }

            pub fn log_error(msg: &str) {
                host_log::log_message(host_log::LogLevel::Error, msg);
            }

            // --- RuntimeEffects shortcuts ---

            /// RuntimeEffects with commands only (no redraw flag, no scroll).
            pub fn effects(commands: Vec<Command>) -> RuntimeEffects {
                RuntimeEffects { redraw: 0, commands, scroll_plans: vec![] }
            }

            /// RuntimeEffects with commands + trailing RequestRedraw(ALL).
            pub fn effects_redraw(mut commands: Vec<Command>) -> RuntimeEffects {
                commands.push(Command::RequestRedraw(0x17F));
                RuntimeEffects { redraw: 0, commands, scroll_plans: vec![] }
            }

            /// RuntimeEffects with only RequestRedraw(ALL).
            pub fn just_redraw() -> RuntimeEffects {
                RuntimeEffects {
                    redraw: 0,
                    commands: vec![Command::RequestRedraw(0x17F)],
                    scroll_plans: vec![],
                }
            }

            // --- Key handler shortcuts ---

            /// Consume key with RequestRedraw(ALL).
            pub fn consumed_redraw() -> Option<Vec<Command>> {
                Some(vec![Command::RequestRedraw(0x17F)])
            }

            /// Consume key with no side effects.
            pub fn consumed() -> Option<Vec<Command>> {
                Some(vec![])
            }

            /// Navigate up in a list and redraw.
            pub fn nav_up(selected: &mut usize) -> Option<Vec<Command>> {
                if *selected > 0 { *selected -= 1; }
                consumed_redraw()
            }

            /// Navigate down in a list and redraw.
            pub fn nav_down(selected: &mut usize, len: usize) -> Option<Vec<Command>> {
                if len > 0 && *selected < len - 1 { *selected += 1; }
                consumed_redraw()
            }

            // --- WIT → SDK event conversion ---

            /// Convert WIT ProcessEventKind to SDK IoEventKind.
            pub fn to_io_event_kind(kind: &ProcessEventKind) -> ::kasane_plugin_sdk::process::IoEventKind<'_> {
                match kind {
                    ProcessEventKind::Stdout(d) => ::kasane_plugin_sdk::process::IoEventKind::Stdout(d),
                    ProcessEventKind::Stderr(d) => ::kasane_plugin_sdk::process::IoEventKind::Stderr(d),
                    ProcessEventKind::Exited(c) => ::kasane_plugin_sdk::process::IoEventKind::Exited(*c),
                    ProcessEventKind::SpawnFailed(e) => ::kasane_plugin_sdk::process::IoEventKind::SpawnFailed(e),
                }
            }
        }

        #[allow(unused_imports)]
        use __kasane_sdk::*;
    }
}

// ============================================================================
// define_plugin! proc macro
// ============================================================================

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
/// - `on_init_effects() { ... }` → `fn on_init_effects() -> BootstrapEffects`
/// - `on_active_session_ready_effects() { ... }` → `fn on_active_session_ready_effects() -> SessionReadyEffects`
/// - `on_state_changed_effects(dirty) { ... }` → `fn on_state_changed_effects() -> RuntimeEffects`
/// - `slots { SLOT => expr, ... }` — simple form (auto-wraps in `auto_contribution()`)
/// - `slots { SLOT(deps) => |ctx| { ... }, ... }` — full form with state access via `state.field`
/// - `on_workspace_changed(snapshot) { ... }` → `fn on_workspace_changed()`
/// - `annotate(line, ctx) { ... }` → `fn annotate_line()`
/// - `display_directives() { ... }` → `fn display_directives() -> Vec<DisplayDirective>`
/// - `transform(target, subject, ctx) { ... }` → `fn transform()`
/// - `transform_priority: expr` → `fn transform_priority()`
/// - `overlay(ctx) { ... }` → `fn contribute_overlay_v2()`
/// - `handle_key(event) { ... }` → `fn handle_key()`
/// - `handle_key_middleware(event) { ... }` → `fn handle_key_middleware()`
/// - `handle_mouse(event, id) { ... }` → `fn handle_mouse()`
/// - `capabilities: [Cap1, Cap2]` → `fn requested_capabilities()`
/// - `authorities: [Auth1, Auth2]` → `fn requested_authorities()`
/// - `update_effects(payload) { ... }` → `fn update_effects() -> RuntimeEffects`
/// - `on_io_event_effects(event) { ... }` → `fn on_io_event_effects() -> RuntimeEffects`
#[proc_macro]
pub fn kasane_define_plugin(input: TokenStream) -> TokenStream {
    let input2: proc_macro2::TokenStream = input.into();

    // We parse at the token stream level rather than using syn's full parser
    // because the input has a custom DSL syntax, not standard Rust.
    let result = define_plugin_impl(input2);
    match result {
        Ok(tokens) => tokens.into(),
        Err(e) => e.into_compile_error().into(),
    }
}

fn define_plugin_impl(input: proc_macro2::TokenStream) -> syn::Result<proc_macro2::TokenStream> {
    let def: PluginDef = syn::parse2(input)?;

    // 1. generate!()
    let wit_content = include_str!("../wit/plugin.wit");
    let wit_bindings = quote! {
        wit_bindgen::generate!({
            world: "kasane-plugin",
            inline: #wit_content,
        });
    };
    let sdk_helpers = generate_sdk_helpers();

    // 2. State definition (if present)
    let state_tokens = if let Some(ref state_def) = def.state {
        let fields: Vec<_> = state_def
            .fields
            .iter()
            .map(|f| {
                let name = &f.name;
                let ty = &f.ty;
                quote! { #name: #ty }
            })
            .collect();
        let defaults: Vec<_> = state_def
            .fields
            .iter()
            .map(|f| {
                let name = &f.name;
                let default = &f.default;
                quote! { #name: #default }
            })
            .collect();
        quote! {
            #[derive(Debug)]
            struct __KasanePluginState {
                #( #fields, )*
                generation: u64,
            }

            impl Default for __KasanePluginState {
                fn default() -> Self {
                    Self {
                        #( #defaults, )*
                        generation: 0,
                    }
                }
            }

            impl __KasanePluginState {
                fn bump_generation(&mut self) {
                    self.generation = self.generation.wrapping_add(1);
                }
            }

            ::std::thread_local! {
                static STATE: ::std::cell::RefCell<__KasanePluginState> =
                    ::std::cell::RefCell::new(<__KasanePluginState>::default());
            }

            /// RAII guard that auto-bumps generation on drop if state was mutated
            /// but bump_generation() was not called manually.
            struct __KasaneStateMutGuard<'a> {
                inner: ::std::cell::RefMut<'a, __KasanePluginState>,
                old_generation: u64,
                mutated: bool,
            }

            impl ::std::ops::Deref for __KasaneStateMutGuard<'_> {
                type Target = __KasanePluginState;
                fn deref(&self) -> &Self::Target { &self.inner }
            }

            impl ::std::ops::DerefMut for __KasaneStateMutGuard<'_> {
                fn deref_mut(&mut self) -> &mut Self::Target {
                    self.mutated = true;
                    &mut self.inner
                }
            }

            impl Drop for __KasaneStateMutGuard<'_> {
                fn drop(&mut self) {
                    if self.mutated && self.inner.generation == self.old_generation {
                        self.inner.generation = self.inner.generation.wrapping_add(1);
                    }
                }
            }

            #[doc(hidden)]
            #[allow(dead_code)]
            fn __kasane_auto_state_hash() -> u64 {
                STATE.with(|s| s.borrow().generation)
            }
        }
    } else {
        // No state: provide a minimal state_hash
        quote! {
            #[doc(hidden)]
            #[allow(dead_code)]
            fn __kasane_auto_state_hash() -> u64 { 0 }
        }
    };

    // 3. Build Guest methods
    let id_str = &def.id;
    let get_id = quote! {
        fn get_id() -> String {
            #id_str.to_string()
        }
    };

    let has_state = def.state.is_some();

    // Helper: wrap body with STATE.with + StateMutGuard if state is present (mutable access)
    let wrap_state = |body: &proc_macro2::TokenStream| -> proc_macro2::TokenStream {
        if has_state {
            quote! {
                STATE.with(|__s| {
                    let __old_gen = __s.borrow().generation;
                    let mut state = __KasaneStateMutGuard {
                        inner: __s.borrow_mut(),
                        old_generation: __old_gen,
                        mutated: false,
                    };
                    #body
                })
            }
        } else {
            body.clone()
        }
    };

    let wrap_state_shared = |body: &proc_macro2::TokenStream| -> proc_macro2::TokenStream {
        if has_state {
            quote! {
                STATE.with(|__s| {
                    let state = __s.borrow();
                    #body
                })
            }
        } else {
            body.clone()
        }
    };

    let on_init_method = if let Some(ref body) = def.on_init_effects {
        let wrapped = wrap_state(body);
        quote! { fn on_init_effects() -> BootstrapEffects { #wrapped } }
    } else {
        quote! {}
    };

    let on_active_session_ready_method = if let Some(ref body) = def.on_active_session_ready_effects
    {
        let wrapped = wrap_state(body);
        quote! {
            fn on_active_session_ready_effects() -> SessionReadyEffects { #wrapped }
        }
    } else {
        quote! {}
    };

    // Determine the dirty-flags parameter name early so auto-bindings can reference it.
    let has_osc = def.on_state_changed_effects.is_some();
    let osc_param_name = def
        .on_state_changed_effects
        .as_ref()
        .map(|osc| osc.param.clone())
        .unwrap_or_else(|| syn::Ident::new("__flags", proc_macro2::Span::call_site()));

    // Generate auto-binding code from #[bind] attributes
    let auto_bindings: Vec<proc_macro2::TokenStream> = if let Some(ref state_def) = def.state {
        state_def
            .fields
            .iter()
            .filter_map(|f| {
                f.bind.as_ref().map(|b| {
                    let name = &f.name;
                    let expr = &b.expr;
                    let flag = &b.dirty_flag;
                    let pname = &osc_param_name;
                    quote! {
                        if #pname & #flag != 0 {
                            state.#name = #expr;
                        }
                    }
                })
            })
            .collect()
    } else {
        vec![]
    };

    let has_bindings = !auto_bindings.is_empty();

    let on_state_changed_method = if has_osc || has_bindings {
        let param_name = &osc_param_name;

        let sync_body = def
            .on_state_changed_effects
            .as_ref()
            .map(|osc| osc.body.clone())
            .unwrap_or_else(|| quote! { RuntimeEffects::default() });

        let wrapped = if has_state {
            quote! {
                STATE.with(|__s| {
                    let __old_gen = __s.borrow().generation;
                    let mut state = __KasaneStateMutGuard {
                        inner: __s.borrow_mut(),
                        old_generation: __old_gen,
                        mutated: false,
                    };
                    #( #auto_bindings )*
                    { #sync_body }
                })
            }
        } else {
            quote! {
                { #sync_body }
            }
        };
        quote! {
            fn on_state_changed_effects(#param_name: u16) -> RuntimeEffects {
                #wrapped
            }
        }
    } else {
        quote! {}
    };

    let on_workspace_changed_method = if let Some(ref workspace_changed) = def.on_workspace_changed
    {
        let snapshot_param = &workspace_changed.param;
        let body = &workspace_changed.body;
        let wrapped = wrap_state(body);
        quote! {
            fn on_workspace_changed(#snapshot_param: WorkspaceSnapshot) {
                #wrapped
            }
        }
    } else {
        quote! {}
    };

    let slots_method = if let Some(ref slots) = def.slots {
        let slot_arms: Vec<_> = slots
            .iter()
            .map(|entry| {
                let pattern = slot_name_to_pattern(&entry.name);
                let body = &entry.body;

                let wrapped_body = if entry.has_closure {
                    // Full form: body returns Option<Contribution>
                    let ctx_param = entry.ctx_param.as_ref().unwrap();
                    if has_state {
                        quote! {
                            STATE.with(|__s| {
                                let state = __s.borrow();
                                let #ctx_param = &__ctx;
                                #body
                            })
                        }
                    } else {
                        quote! { let #ctx_param = &__ctx; #body }
                    }
                } else {
                    // Simple form: body is an ElementHandle expression, auto-wrap
                    if has_state {
                        quote! {
                            STATE.with(|__s| {
                                let state = __s.borrow();
                                Some(auto_contribution(#body))
                            })
                        }
                    } else {
                        quote! { Some(auto_contribution(#body)) }
                    }
                };

                quote! { #pattern => { #wrapped_body } }
            })
            .collect();

        quote! {
            fn contribute_to(__region: SlotId, __ctx: ContributeContext) -> Option<Contribution> {
                match &__region {
                    #( #slot_arms, )*
                    _ => None,
                }
            }
        }
    } else {
        quote! {}
    };

    let annotate_method = if let Some(ref ann) = def.annotate {
        let line_param = &ann.line_param;
        let ctx_param = &ann.ctx_param;
        let body = &ann.body;
        let wrapped = wrap_state(body);
        quote! {
            fn annotate_line(#line_param: u32, #ctx_param: AnnotateContext) -> Option<LineAnnotation> {
                #wrapped
            }
        }
    } else {
        quote! {}
    };

    let display_directives_method = if let Some(ref body) = def.display_directives {
        let wrapped = wrap_state_shared(body);
        quote! {
            fn display_directives() -> Vec<DisplayDirective> {
                #wrapped
            }
        }
    } else {
        quote! {}
    };

    let transform_method = if let Some(ref tr) = def.transform {
        let target_param = &tr.target_param;
        let element_param = &tr.element_param;
        let ctx_param = &tr.ctx_param;
        let body = &tr.body;
        let wrapped = wrap_state(body);
        quote! {
            fn transform(
                #target_param: TransformTarget,
                #element_param: TransformSubject,
                #ctx_param: TransformContext,
            ) -> TransformSubject {
                #wrapped
            }
        }
    } else {
        quote! {}
    };

    let transform_priority_method = if let Some(ref tp) = def.transform_priority {
        quote! { fn transform_priority() -> i16 { #tp } }
    } else {
        quote! {}
    };

    let overlay_method = if let Some(ref ov) = def.overlay {
        let ctx_param = &ov.param;
        let body = &ov.body;
        let wrapped = wrap_state(body);
        quote! {
            fn contribute_overlay_v2(#ctx_param: OverlayContext) -> Option<OverlayContribution> {
                #wrapped
            }
        }
    } else {
        quote! {}
    };

    let handle_key_method = if let Some(ref hk) = def.handle_key {
        let event_param = &hk.param;
        let body = &hk.body;
        let wrapped = wrap_state(body);
        quote! {
            fn handle_key(#event_param: KeyEvent) -> Option<Vec<Command>> {
                #wrapped
            }
        }
    } else {
        quote! {}
    };

    let handle_key_middleware_method = if let Some(ref hk) = def.handle_key_middleware {
        let event_param = &hk.param;
        let body = &hk.body;
        let wrapped = wrap_state(body);
        quote! {
            fn handle_key_middleware(#event_param: KeyEvent) -> KeyHandleResult {
                #wrapped
            }
        }
    } else {
        quote! {}
    };

    let handle_mouse_method = if let Some(ref hm) = def.handle_mouse {
        let event_param = &hm.event_param;
        let id_param = &hm.id_param;
        let body = &hm.body;
        let wrapped = wrap_state(body);
        quote! {
            fn handle_mouse(#event_param: MouseEvent, #id_param: InteractiveId) -> Option<Vec<Command>> {
                #wrapped
            }
        }
    } else {
        quote! {}
    };

    let handle_default_scroll_method = if let Some(ref hs) = def.handle_default_scroll {
        let candidate_param = &hs.param;
        let body = &hs.body;
        let wrapped = wrap_state(body);
        quote! {
            fn handle_default_scroll(
                #candidate_param: DefaultScrollCandidate
            ) -> Option<ScrollPolicyResult> {
                #wrapped
            }
        }
    } else {
        quote! {}
    };

    let capabilities_method = if let Some(ref caps) = def.capabilities {
        quote! {
            fn requested_capabilities() -> Vec<Capability> {
                vec![ #caps ]
            }
        }
    } else {
        quote! {}
    };

    let authorities_method = if let Some(ref authorities) = def.authorities {
        quote! {
            fn requested_authorities() -> Vec<PluginAuthority> {
                vec![ #authorities ]
            }
        }
    } else {
        quote! {}
    };

    let update_effects_method = if let Some(ref upd) = def.update_effects {
        let payload_param = &upd.param;
        let body = &upd.body;
        let wrapped = wrap_state(body);
        quote! {
            fn update_effects(#payload_param: Vec<u8>) -> RuntimeEffects {
                #wrapped
            }
        }
    } else {
        quote! {}
    };

    let on_io_event_method = if let Some(ref io) = def.on_io_event_effects {
        let event_param = &io.param;
        let body = &io.body;
        let wrapped = wrap_state(body);
        quote! {
            fn on_io_event_effects(#event_param: IoEvent) -> RuntimeEffects {
                #wrapped
            }
        }
    } else {
        quote! {}
    };

    // Generate view_deps method.
    // Priority: explicit view_deps > auto-infer from #[bind] flags > default (ALL).
    let view_deps_method = if let Some(ref vd) = def.view_deps {
        quote! { fn view_deps() -> u16 { #vd } }
    } else if let Some(ref state_def) = def.state {
        // Auto-infer from #[bind(expr, on: flag)] declarations
        let bind_flags: Vec<&proc_macro2::TokenStream> = state_def
            .fields
            .iter()
            .filter_map(|f| f.bind.as_ref().map(|b| &b.dirty_flag))
            .collect();
        if !bind_flags.is_empty() && !has_osc {
            // Only infer when there's no custom on_state_changed_effects
            // (custom handler may observe flags not declared in #[bind])
            quote! { fn view_deps() -> u16 { #( #bind_flags )|* } }
        } else {
            quote! {} // Fall through to default stub (ALL)
        }
    } else {
        quote! {} // No state, no view_deps — use default
    };

    // Combine everything
    Ok(quote! {
        #wit_bindings
        #sdk_helpers

        #[allow(unused_imports)]
        use kasane_plugin_sdk::{dirty, modifiers, keys, attributes};

        #state_tokens

        struct __KasanePlugin;

        #[kasane_plugin_sdk::plugin]
        impl Guest for __KasanePlugin {
            #get_id
            #on_init_method
            #on_active_session_ready_method
            #on_state_changed_method
            #on_workspace_changed_method
            #update_effects_method
            #slots_method
            #annotate_method
            #display_directives_method
            #transform_method
            #transform_priority_method
            #overlay_method
            #handle_key_method
            #handle_key_middleware_method
            #handle_mouse_method
            #handle_default_scroll_method
            #capabilities_method
            #authorities_method
            #on_io_event_method
            #view_deps_method
        }

        export!(__KasanePlugin);
    })
}

// --- define_plugin! DSL parser ---

struct PluginDef {
    id: syn::LitStr,
    state: Option<StateDef>,
    on_init_effects: Option<proc_macro2::TokenStream>,
    on_active_session_ready_effects: Option<proc_macro2::TokenStream>,
    on_state_changed_effects: Option<OnStateChanged>,
    on_workspace_changed: Option<ParamBodyDef>,
    update_effects: Option<ParamBodyDef>,
    slots: Option<Vec<SlotEntry>>,
    annotate: Option<AnnotateDef>,
    display_directives: Option<proc_macro2::TokenStream>,
    transform: Option<TransformDef>,
    transform_priority: Option<proc_macro2::TokenStream>,
    overlay: Option<ParamBodyDef>,
    handle_key: Option<ParamBodyDef>,
    handle_key_middleware: Option<ParamBodyDef>,
    handle_mouse: Option<HandleMouseDef>,
    handle_default_scroll: Option<ParamBodyDef>,
    capabilities: Option<proc_macro2::TokenStream>,
    authorities: Option<proc_macro2::TokenStream>,
    on_io_event_effects: Option<ParamBodyDef>,
    view_deps: Option<proc_macro2::TokenStream>,
}

struct StateDef {
    fields: Vec<StateField>,
}

struct StateField {
    name: syn::Ident,
    ty: syn::Type,
    default: syn::Expr,
    bind: Option<BindDef>,
}

struct BindDef {
    expr: proc_macro2::TokenStream,
    dirty_flag: proc_macro2::TokenStream,
}

enum SlotName {
    WellKnown(syn::Ident),
    Named(syn::LitStr),
}

struct SlotEntry {
    name: SlotName,
    has_closure: bool,
    ctx_param: Option<syn::Ident>,
    body: proc_macro2::TokenStream,
}

struct OnStateChanged {
    param: syn::Ident,
    body: proc_macro2::TokenStream,
}

struct AnnotateDef {
    line_param: syn::Ident,
    ctx_param: syn::Ident,
    body: proc_macro2::TokenStream,
}

struct TransformDef {
    target_param: syn::Ident,
    element_param: syn::Ident,
    ctx_param: syn::Ident,
    body: proc_macro2::TokenStream,
}

struct ParamBodyDef {
    param: syn::Ident,
    body: proc_macro2::TokenStream,
}

struct HandleMouseDef {
    event_param: syn::Ident,
    id_param: syn::Ident,
    body: proc_macro2::TokenStream,
}

impl syn::parse::Parse for PluginDef {
    fn parse(input: syn::parse::ParseStream) -> syn::Result<Self> {
        let mut def = PluginDef {
            id: syn::LitStr::new("", proc_macro2::Span::call_site()),
            state: None,
            on_init_effects: None,
            on_active_session_ready_effects: None,
            on_state_changed_effects: None,
            on_workspace_changed: None,
            update_effects: None,
            slots: None,
            annotate: None,
            display_directives: None,
            transform: None,
            transform_priority: None,
            overlay: None,
            handle_key: None,
            handle_key_middleware: None,
            handle_mouse: None,
            handle_default_scroll: None,
            capabilities: None,
            authorities: None,
            on_io_event_effects: None,
            view_deps: None,
        };

        let mut has_id = false;

        while !input.is_empty() {
            let ident: syn::Ident = input.parse()?;
            let section = ident.to_string();

            match section.as_str() {
                "id" => {
                    input.parse::<syn::Token![:]>()?;
                    def.id = input.parse()?;
                    has_id = true;
                }
                "state" => {
                    let content;
                    syn::braced!(content in input);
                    let mut fields = Vec::new();
                    while !content.is_empty() {
                        // Parse optional #[bind(expr, on: flag)] attribute
                        let bind = if content.peek(syn::Token![#]) {
                            content.parse::<syn::Token![#]>()?;
                            let attr_content;
                            syn::bracketed!(attr_content in content);
                            let attr_name: syn::Ident = attr_content.parse()?;
                            if attr_name != "bind" {
                                return Err(syn::Error::new(
                                    attr_name.span(),
                                    "only #[bind(...)] is supported on state fields",
                                ));
                            }
                            let bind_args;
                            syn::parenthesized!(bind_args in attr_content);
                            // Parse: expr, on: flag_expr
                            let expr = parse_bind_expr(&bind_args)?;
                            bind_args.parse::<syn::Token![,]>()?;
                            let on_kw: syn::Ident = bind_args.parse()?;
                            if on_kw != "on" {
                                return Err(syn::Error::new(
                                    on_kw.span(),
                                    "expected `on:` in #[bind(expr, on: flags)]",
                                ));
                            }
                            bind_args.parse::<syn::Token![:]>()?;
                            let dirty_flag: proc_macro2::TokenStream = bind_args.parse()?;
                            Some(BindDef { expr, dirty_flag })
                        } else {
                            None
                        };

                        let name: syn::Ident = content.parse()?;
                        content.parse::<syn::Token![:]>()?;
                        let ty: syn::Type = content.parse()?;
                        content.parse::<syn::Token![=]>()?;
                        let default: syn::Expr = content.parse()?;
                        if !content.is_empty() {
                            content.parse::<syn::Token![,]>()?;
                        }
                        fields.push(StateField {
                            name,
                            ty,
                            default,
                            bind,
                        });
                    }
                    def.state = Some(StateDef { fields });
                }
                "on_init_effects" => {
                    let params;
                    syn::parenthesized!(params in input);
                    let _ = params;
                    let body;
                    syn::braced!(body in input);
                    def.on_init_effects = Some(body.parse()?);
                }
                "on_active_session_ready_effects" => {
                    let params;
                    syn::parenthesized!(params in input);
                    let _ = params;
                    let body;
                    syn::braced!(body in input);
                    def.on_active_session_ready_effects = Some(body.parse()?);
                }
                "on_state_changed_effects" => {
                    let params;
                    syn::parenthesized!(params in input);
                    let param: syn::Ident = params.parse()?;
                    let body;
                    syn::braced!(body in input);
                    def.on_state_changed_effects = Some(OnStateChanged {
                        param,
                        body: body.parse()?,
                    });
                }
                "on_workspace_changed" => {
                    let params;
                    syn::parenthesized!(params in input);
                    let param: syn::Ident = params.parse()?;
                    let body;
                    syn::braced!(body in input);
                    def.on_workspace_changed = Some(ParamBodyDef {
                        param,
                        body: body.parse()?,
                    });
                }
                "update_effects" => {
                    let params;
                    syn::parenthesized!(params in input);
                    let param: syn::Ident = params.parse()?;
                    let body;
                    syn::braced!(body in input);
                    def.update_effects = Some(ParamBodyDef {
                        param,
                        body: body.parse()?,
                    });
                }
                "slots" => {
                    let body;
                    syn::braced!(body in input);
                    let entries = parse_slot_entries(&body)?;
                    def.slots = Some(entries);
                }
                "annotate" => {
                    let params;
                    syn::parenthesized!(params in input);
                    let line_param: syn::Ident = params.parse()?;
                    params.parse::<syn::Token![,]>()?;
                    let ctx_param: syn::Ident = params.parse()?;
                    let body;
                    syn::braced!(body in input);
                    def.annotate = Some(AnnotateDef {
                        line_param,
                        ctx_param,
                        body: body.parse()?,
                    });
                }
                "display_directives" => {
                    let params;
                    syn::parenthesized!(params in input);
                    let _ = params;
                    let body;
                    syn::braced!(body in input);
                    def.display_directives = Some(body.parse()?);
                }
                "transform" => {
                    let params;
                    syn::parenthesized!(params in input);
                    let target_param: syn::Ident = params.parse()?;
                    params.parse::<syn::Token![,]>()?;
                    let element_param: syn::Ident = params.parse()?;
                    params.parse::<syn::Token![,]>()?;
                    let ctx_param: syn::Ident = params.parse()?;
                    let body;
                    syn::braced!(body in input);
                    def.transform = Some(TransformDef {
                        target_param,
                        element_param,
                        ctx_param,
                        body: body.parse()?,
                    });
                }
                "transform_priority" => {
                    input.parse::<syn::Token![:]>()?;
                    def.transform_priority = Some(parse_until_comma_or_end(input)?);
                }
                "overlay" => {
                    let params;
                    syn::parenthesized!(params in input);
                    let param: syn::Ident = params.parse()?;
                    let body;
                    syn::braced!(body in input);
                    def.overlay = Some(ParamBodyDef {
                        param,
                        body: body.parse()?,
                    });
                }
                "handle_key" => {
                    let params;
                    syn::parenthesized!(params in input);
                    let param: syn::Ident = params.parse()?;
                    let body;
                    syn::braced!(body in input);
                    def.handle_key = Some(ParamBodyDef {
                        param,
                        body: body.parse()?,
                    });
                }
                "handle_key_middleware" => {
                    let params;
                    syn::parenthesized!(params in input);
                    let param: syn::Ident = params.parse()?;
                    let body;
                    syn::braced!(body in input);
                    def.handle_key_middleware = Some(ParamBodyDef {
                        param,
                        body: body.parse()?,
                    });
                }
                "handle_mouse" => {
                    let params;
                    syn::parenthesized!(params in input);
                    let event_param: syn::Ident = params.parse()?;
                    params.parse::<syn::Token![,]>()?;
                    let id_param: syn::Ident = params.parse()?;
                    let body;
                    syn::braced!(body in input);
                    def.handle_mouse = Some(HandleMouseDef {
                        event_param,
                        id_param,
                        body: body.parse()?,
                    });
                }
                "handle_default_scroll" => {
                    let params;
                    syn::parenthesized!(params in input);
                    let param: syn::Ident = params.parse()?;
                    let body;
                    syn::braced!(body in input);
                    def.handle_default_scroll = Some(ParamBodyDef {
                        param,
                        body: body.parse()?,
                    });
                }
                "capabilities" => {
                    input.parse::<syn::Token![:]>()?;
                    let content;
                    syn::bracketed!(content in input);
                    def.capabilities = Some(content.parse()?);
                }
                "authorities" => {
                    input.parse::<syn::Token![:]>()?;
                    let content;
                    syn::bracketed!(content in input);
                    def.authorities = Some(content.parse()?);
                }
                "on_io_event_effects" => {
                    let params;
                    syn::parenthesized!(params in input);
                    let param: syn::Ident = params.parse()?;
                    let body;
                    syn::braced!(body in input);
                    def.on_io_event_effects = Some(ParamBodyDef {
                        param,
                        body: body.parse()?,
                    });
                }
                "view_deps" => {
                    input.parse::<syn::Token![:]>()?;
                    def.view_deps = Some(input.parse()?);
                }
                "on_init" => {
                    return Err(syn::Error::new(
                        ident.span(),
                        "define_plugin! `on_init` was removed; use `on_init_effects()`",
                    ));
                }
                "on_active_session_ready" => {
                    return Err(syn::Error::new(
                        ident.span(),
                        "define_plugin! `on_active_session_ready` was removed; use `on_active_session_ready_effects()`",
                    ));
                }
                "on_state_changed" => {
                    return Err(syn::Error::new(
                        ident.span(),
                        "define_plugin! `on_state_changed` was removed; use `on_state_changed_effects(...)`",
                    ));
                }
                "on_state_changed_commands" => {
                    return Err(syn::Error::new(
                        ident.span(),
                        "define_plugin! `on_state_changed_commands` was removed; return `RuntimeEffects` from `on_state_changed_effects(...)`",
                    ));
                }
                "update" => {
                    return Err(syn::Error::new(
                        ident.span(),
                        "define_plugin! `update` was removed; use `update_effects(...)`",
                    ));
                }
                "on_io_event" => {
                    return Err(syn::Error::new(
                        ident.span(),
                        "define_plugin! `on_io_event` was removed; use `on_io_event_effects(...)`",
                    ));
                }
                other => {
                    return Err(syn::Error::new(
                        ident.span(),
                        format!("unknown define_plugin section: `{other}`"),
                    ));
                }
            }

            // Consume optional trailing comma between sections
            if !input.is_empty() {
                let _ = input.parse::<syn::Token![,]>();
            }
        }

        if !has_id {
            return Err(syn::Error::new(
                proc_macro2::Span::call_site(),
                "define_plugin! requires an `id: \"...\"` section",
            ));
        }

        Ok(def)
    }
}

/// Parse tokens until a comma or end of input, consuming the comma if present.
fn parse_until_comma_or_end(
    input: syn::parse::ParseStream,
) -> syn::Result<proc_macro2::TokenStream> {
    let mut tokens = Vec::new();
    while !input.is_empty() && !input.peek(syn::Token![,]) {
        let tt: proc_macro2::TokenTree = input.parse()?;
        tokens.push(tt);
    }
    Ok(tokens.into_iter().collect())
}

/// Parse the expression part of `#[bind(expr, on: flag)]` — everything before `, on:`.
fn parse_bind_expr(input: syn::parse::ParseStream) -> syn::Result<proc_macro2::TokenStream> {
    let mut tokens = Vec::new();
    // Collect tokens until we see `, on` (comma followed by `on` ident)
    loop {
        if input.is_empty() {
            return Err(input.error("expected `, on: flags` in #[bind(expr, on: flags)]"));
        }
        // Peek ahead: if next is `,` and then `on`, stop
        if input.peek(syn::Token![,]) {
            let fork = input.fork();
            let _ = fork.parse::<syn::Token![,]>();
            if fork.peek(syn::Ident) {
                let ident: syn::Ident = fork.parse()?;
                if ident == "on" {
                    break;
                }
            }
        }
        let tt: proc_macro2::TokenTree = input.parse()?;
        tokens.push(tt);
    }
    Ok(tokens.into_iter().collect())
}

/// Parse slot entries from the `slots { ... }` block.
fn parse_slot_entries(input: syn::parse::ParseStream) -> syn::Result<Vec<SlotEntry>> {
    let mut entries = Vec::new();
    while !input.is_empty() {
        // 1. Slot name: IDENT or named("...")
        let name = {
            let ident: syn::Ident = input.parse()?;
            if ident == "named" {
                let args;
                syn::parenthesized!(args in input);
                let lit: syn::LitStr = args.parse()?;
                SlotName::Named(lit)
            } else {
                SlotName::WellKnown(ident)
            }
        };

        // 2. Optional (deps) — if next token is `(`, consume and ignore (deps removed)
        if input.peek(syn::token::Paren) {
            let args;
            syn::parenthesized!(args in input);
            let _: proc_macro2::TokenStream = args.parse()?;
        }

        // 3. `=>`
        input.parse::<syn::Token![=>]>()?;

        // 4. Closure `|ctx| { body }` or simple expression
        if input.peek(syn::Token![|]) {
            // Full closure form
            input.parse::<syn::Token![|]>()?;
            let ctx_param: syn::Ident = input.parse()?;
            input.parse::<syn::Token![|]>()?;
            let body;
            syn::braced!(body in input);
            let body_tokens: proc_macro2::TokenStream = body.parse()?;
            entries.push(SlotEntry {
                name,
                has_closure: true,
                ctx_param: Some(ctx_param),
                body: body_tokens,
            });
        } else {
            // Simple expression form — read until `,` or end
            let mut tokens = Vec::new();
            while !input.is_empty() && !input.peek(syn::Token![,]) {
                let tt: proc_macro2::TokenTree = input.parse()?;
                tokens.push(tt);
            }
            let body_tokens: proc_macro2::TokenStream = tokens.into_iter().collect();
            entries.push(SlotEntry {
                name,
                has_closure: false,
                ctx_param: None,
                body: body_tokens,
            });
        }

        // 5. Trailing comma
        if !input.is_empty() {
            let _ = input.parse::<syn::Token![,]>();
        }
    }
    Ok(entries)
}

/// Convert a SlotName to a match pattern for `SlotId`.
fn slot_name_to_pattern(name: &SlotName) -> proc_macro2::TokenStream {
    match name {
        SlotName::WellKnown(ident) => {
            let variant = match ident.to_string().as_str() {
                "BUFFER_LEFT" => quote! { BufferLeft },
                "BUFFER_RIGHT" => quote! { BufferRight },
                "ABOVE_BUFFER" => quote! { AboveBuffer },
                "BELOW_BUFFER" => quote! { BelowBuffer },
                "ABOVE_STATUS" => quote! { AboveStatus },
                "STATUS_LEFT" => quote! { StatusLeft },
                "STATUS_RIGHT" => quote! { StatusRight },
                "OVERLAY" => quote! { Overlay },
                other => {
                    let msg = format!("unknown well-known slot: `{other}`");
                    return quote! { compile_error!(#msg) };
                }
            };
            quote! { SlotId::WellKnown(WellKnownSlot::#variant) }
        }
        SlotName::Named(lit) => {
            quote! { SlotId::Named(ref __n) if __n == #lit }
        }
    }
}
