//! Code generation for `#[kasane_plugin(v2)]` — produces `Plugin` (HandlerRegistry)
//! implementations.

use proc_macro2::TokenStream;
use quote::{format_ident, quote};
use syn::{Attribute, Error, Expr, ExprPath, Ident, Item, ItemMod, Lit, parse2};

/// A `#[transform(TransformTarget::*, priority = N)]` binding extracted
/// from a function declared inside a `#[kasane_plugin(v2)]` module.
///
/// `target_path` is parsed but currently unused — v2 emits
/// `r.on_transform(priority, ...)` which applies to all targets; the
/// per-target dispatch would route through `r.on_transform_for(...)`.
/// Kept parsed for forward compatibility.
struct TransformBinding {
    #[allow(dead_code)]
    target_path: ExprPath,
    priority: Option<i16>,
    fn_name: Ident,
}

fn has_attr(attrs: &[Attribute], name: &str) -> bool {
    attrs.iter().any(|a| a.path().is_ident(name))
}

/// Extract `#[transform(TransformTarget::STATUS_BAR, priority = 50)]`
fn extract_transform_attr(attrs: &[Attribute]) -> syn::Result<Option<(ExprPath, Option<u32>)>> {
    for attr in attrs {
        if attr.path().is_ident("transform") {
            let args: syn::punctuated::Punctuated<Expr, syn::Token![,]> =
                attr.parse_args_with(syn::punctuated::Punctuated::parse_terminated)?;

            let mut target: Option<ExprPath> = None;
            let mut priority: Option<u32> = None;

            for expr in args {
                match &expr {
                    Expr::Path(p) => {
                        target = Some(p.clone());
                    }
                    Expr::Assign(assign) => {
                        if let Expr::Path(left) = &*assign.left
                            && left.path.is_ident("priority")
                            && let Expr::Lit(lit) = &*assign.right
                            && let Lit::Int(int_lit) = &lit.lit
                        {
                            priority = Some(int_lit.base10_parse()?);
                        }
                    }
                    _ => {}
                }
            }

            let Some(target) = target else {
                return Err(Error::new_spanned(
                    attr,
                    "#[transform(...)] requires a TransformTarget path",
                ));
            };

            return Ok(Some((target, priority)));
        }
    }
    Ok(None)
}

fn to_pascal_case(s: &str) -> String {
    s.split('_')
        .map(|word| {
            let mut chars = word.chars();
            match chars.next() {
                Some(c) => c.to_uppercase().to_string() + &chars.as_str().to_lowercase(),
                None => String::new(),
            }
        })
        .collect()
}

// =============================================================================
// V2: Generate `impl Plugin` with `register()` using HandlerRegistry
// =============================================================================

/// Parsed information for v2 plugin generation.
struct PluginDefV2 {
    mod_ident: Ident,
    has_state: bool,
    dirty_flags: Option<TokenStream>,
    on_state_changed: bool,
    on_init: bool,
    on_session_ready: bool,
    on_shutdown: bool,
    on_io_event: bool,
    handle_key: bool,
    observe_key: bool,
    observe_mouse: bool,
    handle_mouse: bool,
    decorate_background: bool,
    decorate_gutter: Vec<GutterBinding>,
    decorate_inline: bool,
    virtual_text: bool,
    contributes: Vec<ContributeBinding>,
    transforms: Vec<TransformBinding>,
    transform_menu_item: bool,
    overlay: bool,
    display_directives: bool,
    default_scroll: bool,
}

struct GutterBinding {
    side: TokenStream,
    priority: i16,
    fn_name: Ident,
}

struct ContributeBinding {
    slot: String,
    fn_name: Ident,
}

pub fn expand_kasane_plugin_v2(input: TokenStream) -> syn::Result<TokenStream> {
    let module: ItemMod = parse2(input)?;

    let Some((_, ref items)) = module.content else {
        return Err(Error::new_spanned(
            &module,
            "#[kasane_plugin(v2)] requires an inline module (mod name { ... })",
        ));
    };

    let mut def = PluginDefV2 {
        mod_ident: module.ident.clone(),
        has_state: false,
        dirty_flags: None,
        on_state_changed: false,
        on_init: false,
        on_session_ready: false,
        on_shutdown: false,
        on_io_event: false,
        handle_key: false,
        observe_key: false,
        observe_mouse: false,
        handle_mouse: false,
        decorate_background: false,
        decorate_gutter: Vec::new(),
        decorate_inline: false,
        virtual_text: false,
        contributes: Vec::new(),
        transforms: Vec::new(),
        transform_menu_item: false,
        overlay: false,
        display_directives: false,
        default_scroll: false,
    };

    for item in items {
        match item {
            Item::Struct(s) => {
                if has_attr(&s.attrs, "state") {
                    def.has_state = true;
                    // Check for #[dirty(FLAGS)] on the struct
                    for attr in &s.attrs {
                        if attr.path().is_ident("dirty") {
                            let flags: Expr = attr.parse_args()?;
                            def.dirty_flags = Some(quote! { #flags });
                        }
                    }
                }
            }
            Item::Fn(f) => {
                let name = f.sig.ident.to_string();
                match name.as_str() {
                    "on_state_changed" => def.on_state_changed = true,
                    "on_init" => def.on_init = true,
                    "on_session_ready" => def.on_session_ready = true,
                    "on_shutdown" => def.on_shutdown = true,
                    "on_io_event" => def.on_io_event = true,
                    "handle_key" => def.handle_key = true,
                    "observe_key" => def.observe_key = true,
                    "observe_mouse" => def.observe_mouse = true,
                    "handle_mouse" => def.handle_mouse = true,
                    "decorate_background" => def.decorate_background = true,
                    "decorate_inline" => def.decorate_inline = true,
                    "virtual_text" => def.virtual_text = true,
                    "transform_menu_item" => def.transform_menu_item = true,
                    "contribute_overlay" => def.overlay = true,
                    "display_directives" => def.display_directives = true,
                    "handle_default_scroll" => def.default_scroll = true,
                    _ => {}
                }

                // Check for #[contribute("slot.name")]
                for attr in &f.attrs {
                    if attr.path().is_ident("contribute") {
                        let slot_str: syn::LitStr = attr.parse_args()?;
                        def.contributes.push(ContributeBinding {
                            slot: slot_str.value(),
                            fn_name: f.sig.ident.clone(),
                        });
                    }
                }

                // Check for #[decorate_gutter(Left, 10)]
                for attr in &f.attrs {
                    if attr.path().is_ident("decorate_gutter") {
                        let args: syn::punctuated::Punctuated<Expr, syn::Token![,]> =
                            attr.parse_args_with(syn::punctuated::Punctuated::parse_terminated)?;
                        let mut side = quote! { kasane_core::plugin::GutterSide::Left };
                        let mut priority: i16 = 0;
                        for (i, expr) in args.iter().enumerate() {
                            if i == 0 {
                                side = quote! { #expr };
                            } else if let Expr::Lit(lit) = expr
                                && let Lit::Int(int_lit) = &lit.lit
                            {
                                priority = int_lit.base10_parse()?;
                            }
                        }
                        def.decorate_gutter.push(GutterBinding {
                            side,
                            priority,
                            fn_name: f.sig.ident.clone(),
                        });
                    }
                }

                // Check for #[transform(TransformTarget::*, priority = N)]
                if let Some((target_path, priority)) = extract_transform_attr(&f.attrs)? {
                    def.transforms.push(TransformBinding {
                        target_path,
                        priority: priority.map(|p| p as i16),
                        fn_name: f.sig.ident.clone(),
                    });
                }
            }
            _ => {}
        }
    }

    generate_v2_plugin(&def, &module)
}

fn generate_v2_plugin(def: &PluginDefV2, module: &ItemMod) -> syn::Result<TokenStream> {
    let mod_ident = &def.mod_ident;
    let struct_name = format_ident!("{}Plugin", to_pascal_case(&mod_ident.to_string()));
    let id_str = mod_ident.to_string();

    let cleaned_module = strip_custom_attrs_v2(module);

    // State type: use module::State if #[state] is present, otherwise ()
    let state_type = if def.has_state {
        quote! { #mod_ident::State }
    } else {
        quote! { () }
    };

    // Build register() body
    let mut register_body = Vec::new();

    // declare_interests
    if let Some(flags) = &def.dirty_flags {
        register_body.push(quote! {
            r.declare_interests(#flags);
        });
    }

    // Lifecycle handlers — emit tier-typed setters (ADR-044 W1-A).
    // User fn return types must match the setter's bound:
    // - on_init / on_session_ready / on_state_changed: (S, KakouneSideEffects)
    // - on_io_event: (S, ProcessCapableEffects)
    if def.on_init {
        register_body.push(quote! {
            r.on_init_tier1(|state, app| #mod_ident::on_init(state, app));
        });
    }
    if def.on_session_ready {
        register_body.push(quote! {
            r.on_session_ready_tier1(|state, app| #mod_ident::on_session_ready(state, app));
        });
    }
    if def.on_state_changed {
        register_body.push(quote! {
            r.on_state_changed_tier1(|state, app, dirty| #mod_ident::on_state_changed(state, app, dirty));
        });
    }
    if def.on_io_event {
        register_body.push(quote! {
            r.on_io_event_tier2(|state, event, app| #mod_ident::on_io_event(state, event, app));
        });
    }
    if def.on_shutdown {
        register_body.push(quote! {
            r.on_shutdown(|state| #mod_ident::on_shutdown(state));
        });
    }

    // Input handlers
    if def.handle_key {
        register_body.push(quote! {
            r.on_key(|state, key, app| #mod_ident::handle_key(state, key, app));
        });
    }
    if def.observe_key {
        register_body.push(quote! {
            r.on_observe_key(|state, key, app| #mod_ident::observe_key(state, key, app));
        });
    }
    if def.observe_mouse {
        register_body.push(quote! {
            r.on_observe_mouse(|state, event, app| #mod_ident::observe_mouse(state, event, app));
        });
    }
    if def.handle_mouse {
        register_body.push(quote! {
            r.on_handle_mouse(|state, event, id, app| #mod_ident::handle_mouse(state, event, id, app));
        });
    }
    if def.default_scroll {
        register_body.push(quote! {
            r.on_default_scroll(|state, candidate, app| #mod_ident::handle_default_scroll(state, candidate, app));
        });
    }

    // Contribute handlers
    for cb in &def.contributes {
        let slot = &cb.slot;
        let fn_name = &cb.fn_name;
        register_body.push(quote! {
            r.on_contribute(
                kasane_core::plugin::SlotId::new(#slot),
                |state, app, ctx| #mod_ident::#fn_name(state, app, ctx),
            );
        });
    }

    // Transform handlers
    for tb in &def.transforms {
        let fn_name = &tb.fn_name;
        let priority = tb.priority.unwrap_or(0);
        register_body.push(quote! {
            r.on_transform(
                #priority,
                |state, subject, app, ctx| #mod_ident::#fn_name(state, subject, app, ctx),
            );
        });
    }

    // Annotation handlers
    for gb in &def.decorate_gutter {
        let side = &gb.side;
        let priority = gb.priority;
        let fn_name = &gb.fn_name;
        register_body.push(quote! {
            r.on_decorate_gutter(#side, #priority, |state, line, app, ctx| #mod_ident::#fn_name(state, line, app, ctx));
        });
    }
    if def.decorate_background {
        register_body.push(quote! {
            r.on_decorate_background(|state, line, app, ctx| #mod_ident::decorate_background(state, line, app, ctx));
        });
    }
    if def.decorate_inline {
        register_body.push(quote! {
            r.on_decorate_inline(|state, line, app, ctx| #mod_ident::decorate_inline(state, line, app, ctx));
        });
    }
    if def.virtual_text {
        register_body.push(quote! {
            r.on_virtual_text(|state, line, app, ctx| #mod_ident::virtual_text(state, line, app, ctx));
        });
    }

    // Overlay
    if def.overlay {
        register_body.push(quote! {
            r.on_overlay(|state, app, ctx| #mod_ident::contribute_overlay(state, app, ctx));
        });
    }

    // Display directives
    if def.display_directives {
        register_body.push(quote! {
            r.on_display(|state, app| #mod_ident::display_directives(state, app));
        });
    }

    // Menu transform
    if def.transform_menu_item {
        register_body.push(quote! {
            r.on_menu_transform(|state, item, index, selected, app| #mod_ident::transform_menu_item(state, item, index, selected, app));
        });
    }

    Ok(quote! {
        #cleaned_module

        pub struct #struct_name;

        impl kasane_core::plugin::Plugin for #struct_name {
            type State = #state_type;

            fn id(&self) -> kasane_core::plugin::PluginId {
                kasane_core::plugin::PluginId(#id_str.to_string())
            }

            // Emits tier-typed setters (on_init_tier1, on_state_changed_tier1,
            // on_io_event_tier2, etc.). User attribute fns must return the
            // matching tier-typed effects (KakouneSideEffects / ProcessCapableEffects).
            fn register(&self, r: &mut kasane_core::plugin::HandlerRegistry<#state_type>) {
                #(#register_body)*
            }
        }
    })
}

/// Strip custom attributes for v2 (includes additional attrs like #[contribute], #[decorate_gutter]).
fn strip_custom_attrs_v2(module: &ItemMod) -> TokenStream {
    let vis = &module.vis;
    let ident = &module.ident;
    let attrs = &module.attrs;

    let items = if let Some((_, items)) = &module.content {
        items.iter().map(strip_item_attrs_v2).collect::<Vec<_>>()
    } else {
        vec![]
    };

    quote! {
        #(#attrs)*
        #vis mod #ident {
            #(#items)*
        }
    }
}

const V2_CUSTOM_ATTRS: &[&str] = &[
    "state",
    "event",
    "transform",
    "keybind",
    "lifecycle",
    "input",
    "dirty",
    "contribute",
    "decorate_gutter",
    "decorate_background",
    "decorate_inline",
    "virtual_text",
    "handle_key",
    "overlay",
];

fn is_v2_custom_attr(attr: &Attribute) -> bool {
    V2_CUSTOM_ATTRS
        .iter()
        .any(|name| attr.path().is_ident(name))
}

fn filter_v2_attrs(attrs: &[Attribute]) -> Vec<&Attribute> {
    attrs.iter().filter(|a| !is_v2_custom_attr(a)).collect()
}

fn strip_item_attrs_v2(item: &Item) -> TokenStream {
    match item {
        Item::Struct(s) => {
            let kept: Vec<_> = filter_v2_attrs(&s.attrs);
            let vis = &s.vis;
            let ident = &s.ident;
            let generics = &s.generics;
            let fields = &s.fields;
            let semi = &s.semi_token;
            let semi_tok = semi.map(|_| quote! { ; }).unwrap_or_default();

            quote! {
                #(#kept)*
                #vis struct #ident #generics #fields #semi_tok
            }
        }
        Item::Fn(f) => {
            let kept: Vec<_> = filter_v2_attrs(&f.attrs);
            let vis = &f.vis;
            let sig = &f.sig;
            let block = &f.block;
            quote! {
                #(#kept)*
                #vis #sig #block
            }
        }
        other => quote! { #other },
    }
}
