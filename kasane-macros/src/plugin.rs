//! Code generation for `#[kasane_plugin]` — produces `PluginBackend` implementations.

use proc_macro2::{Span, TokenStream};
use quote::{format_ident, quote};
use syn::{Attribute, Error, Expr, ExprPath, Ident, Item, ItemMod, Lit, parse2};

/// Parsed information extracted from the module.
struct PluginDef {
    mod_ident: Ident,
    has_state: bool,
    has_event: bool,
    has_update: bool,
    has_on_init_effects: bool,
    has_on_init: bool,
    has_on_active_session_ready_effects: bool,
    has_on_active_session_ready: bool,
    has_on_shutdown: bool,
    has_on_state_changed_effects: bool,
    has_on_state_changed: bool,
    has_update_effects: bool,
    has_observe_key: bool,
    has_observe_mouse: bool,
    has_handle_key: bool,
    has_handle_mouse: bool,
    has_annotate_line: bool,
    has_transform_menu_item: bool,
    transforms: Vec<TransformBinding>,
}

struct TransformBinding {
    target_path: ExprPath,
    priority: Option<i16>,
    fn_name: Ident,
}

pub fn expand_kasane_plugin(input: TokenStream) -> syn::Result<TokenStream> {
    let module: ItemMod = parse2(input)?;

    let Some((_, ref items)) = module.content else {
        return Err(Error::new_spanned(
            &module,
            "#[kasane_plugin] requires an inline module (mod name { ... })",
        ));
    };

    let mut def = PluginDef {
        mod_ident: module.ident.clone(),
        has_state: false,
        has_event: false,
        has_update: false,
        has_on_init_effects: false,
        has_on_init: false,
        has_on_active_session_ready_effects: false,
        has_on_active_session_ready: false,
        has_on_shutdown: false,
        has_on_state_changed_effects: false,
        has_on_state_changed: false,
        has_update_effects: false,
        has_observe_key: false,
        has_observe_mouse: false,
        has_handle_key: false,
        has_handle_mouse: false,
        has_annotate_line: false,
        has_transform_menu_item: false,
        transforms: Vec::new(),
    };

    for item in items {
        match item {
            Item::Struct(s) => {
                if has_attr(&s.attrs, "state") {
                    def.has_state = true;
                }
            }
            Item::Enum(e) => {
                if has_attr(&e.attrs, "event") {
                    def.has_event = true;
                }
            }
            Item::Fn(f) => {
                match f.sig.ident.to_string().as_str() {
                    "update" => def.has_update = true,
                    "on_init_effects" => def.has_on_init_effects = true,
                    "on_init" => def.has_on_init = true,
                    "on_active_session_ready_effects" => {
                        def.has_on_active_session_ready_effects = true
                    }
                    "on_active_session_ready" => def.has_on_active_session_ready = true,
                    "on_shutdown" => def.has_on_shutdown = true,
                    "on_state_changed_effects" => def.has_on_state_changed_effects = true,
                    "on_state_changed" => def.has_on_state_changed = true,
                    "update_effects" => def.has_update_effects = true,
                    "observe_key" => def.has_observe_key = true,
                    "observe_mouse" => def.has_observe_mouse = true,
                    "handle_key" => def.has_handle_key = true,
                    "handle_mouse" => def.has_handle_mouse = true,
                    "annotate_line" => def.has_annotate_line = true,
                    "transform_menu_item" => def.has_transform_menu_item = true,
                    _ => {}
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

    let generated = generate_plugin_struct(&def, &module)?;
    Ok(generated)
}

fn has_attr(attrs: &[Attribute], name: &str) -> bool {
    attrs.iter().any(|a| a.path().is_ident(name))
}

/// Extract `#[transform(TransformTarget::StatusBar, priority = 50)]`
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

/// Generates the state field definition and its initializer for the plugin struct.
///
/// Returns `(field_definition, field_initializer)` — both empty if the plugin has no state.
fn gen_state_field(def: &PluginDef) -> (TokenStream, TokenStream) {
    let mod_ident = &def.mod_ident;
    if def.has_state {
        (
            quote! { pub state: #mod_ident::State, },
            quote! { state: #mod_ident::State::default(), },
        )
    } else {
        (quote! {}, quote! {})
    }
}

/// Generates the typed `PluginBackend::update_effects()` implementation.
///
/// Returns an empty TokenStream if the plugin has no update function or event type.
fn gen_update_impl(def: &PluginDef, struct_name: &Ident) -> TokenStream {
    let mod_ident = &def.mod_ident;
    let _ = struct_name; // available for future use (e.g., error messages)
    let mut tokens = TokenStream::new();

    if def.has_update_effects {
        tokens.extend(quote! {
            fn update_effects(
                &mut self,
                _msg: &mut dyn ::std::any::Any,
                _state: &kasane_core::plugin::AppView<'_>,
            ) -> kasane_core::plugin::RuntimeEffects {
                #mod_ident::update_effects(&mut self.state, _msg, _state)
            }
        });
    }

    tokens
}

/// Generates typed lifecycle hook implementations.
fn gen_lifecycle_impl(def: &PluginDef) -> TokenStream {
    let mod_ident = &def.mod_ident;
    let mut tokens = TokenStream::new();

    if def.has_on_init_effects {
        tokens.extend(quote! {
            fn on_init_effects(&mut self, _state: &kasane_core::plugin::AppView<'_>) -> kasane_core::plugin::BootstrapEffects {
                #mod_ident::on_init_effects(&mut self.state, _state)
            }
        });
    }

    if def.has_on_active_session_ready_effects {
        tokens.extend(quote! {
            fn on_active_session_ready_effects(
                &mut self,
                _state: &kasane_core::plugin::AppView<'_>,
            ) -> kasane_core::plugin::SessionReadyEffects {
                #mod_ident::on_active_session_ready_effects(&mut self.state, _state)
            }
        });
    }

    if def.has_on_shutdown {
        tokens.extend(quote! {
            fn on_shutdown(&mut self) {
                #mod_ident::on_shutdown(&mut self.state)
            }
        });
    }

    if def.has_on_state_changed_effects {
        tokens.extend(quote! {
            fn on_state_changed_effects(
                &mut self,
                _state: &kasane_core::plugin::AppView<'_>,
                _dirty: kasane_core::state::DirtyFlags,
            ) -> kasane_core::plugin::RuntimeEffects {
                #mod_ident::on_state_changed_effects(&mut self.state, _state, _dirty)
            }
        });
    }

    tokens
}

/// Generates input hook implementations (observe_key, observe_mouse, handle_key, handle_mouse).
fn gen_input_impl(def: &PluginDef) -> TokenStream {
    let mod_ident = &def.mod_ident;
    let mut tokens = TokenStream::new();

    if def.has_observe_key {
        tokens.extend(quote! {
            fn observe_key(&mut self, _key: &kasane_core::input::KeyEvent, _state: &kasane_core::plugin::AppView<'_>) {
                #mod_ident::observe_key(&mut self.state, _key, _state)
            }
        });
    }

    if def.has_observe_mouse {
        tokens.extend(quote! {
            fn observe_mouse(&mut self, _event: &kasane_core::input::MouseEvent, _state: &kasane_core::plugin::AppView<'_>) {
                #mod_ident::observe_mouse(&mut self.state, _event, _state)
            }
        });
    }

    if def.has_handle_key {
        tokens.extend(quote! {
            fn handle_key(&mut self, _key: &kasane_core::input::KeyEvent, _state: &kasane_core::plugin::AppView<'_>) -> Option<Vec<kasane_core::plugin::Command>> {
                #mod_ident::handle_key(&mut self.state, _key, _state)
            }
        });
    }

    if def.has_handle_mouse {
        tokens.extend(quote! {
            fn handle_mouse(&mut self, _event: &kasane_core::input::MouseEvent, _id: kasane_core::element::InteractiveId, _state: &kasane_core::plugin::AppView<'_>) -> Option<Vec<kasane_core::plugin::Command>> {
                #mod_ident::handle_mouse(&mut self.state, _event, _id, _state)
            }
        });
    }

    tokens
}

/// Generates transform_menu_item implementation.
fn gen_transform_menu_item_impl(def: &PluginDef) -> TokenStream {
    let mod_ident = &def.mod_ident;
    if def.has_transform_menu_item {
        quote! {
            fn transform_menu_item(
                &self,
                _item: &[kasane_core::protocol::Atom],
                _index: usize,
                _selected: bool,
                _state: &kasane_core::plugin::AppView<'_>,
            ) -> Option<Vec<kasane_core::protocol::Atom>> {
                #mod_ident::transform_menu_item(&self.state, _item, _index, _selected, _state)
            }
        }
    } else {
        quote! {}
    }
}

/// Generates the `Plugin::state_hash()` implementation (L1 caching).
///
/// Only generated when the plugin has a `#[state]` struct.
fn gen_state_hash_impl(def: &PluginDef) -> TokenStream {
    if def.has_state {
        quote! {
            fn state_hash(&self) -> u64 {
                use ::std::hash::{Hash, Hasher};
                let mut hasher = ::std::collections::hash_map::DefaultHasher::new();
                self.state.hash(&mut hasher);
                hasher.finish()
            }
        }
    } else {
        quote! {}
    }
}

/// Generates the `Plugin::transform()` and `Plugin::transform_priority()` trait method
/// implementations for the new Transform API.
fn gen_transform_impl(def: &PluginDef) -> TokenStream {
    let mod_ident = &def.mod_ident;
    if def.transforms.is_empty() {
        return quote! {};
    }

    let transform_arms: Vec<_> = def
        .transforms
        .iter()
        .map(|tb| {
            let target_path = &tb.target_path;
            let fn_name = &tb.fn_name;
            quote! {
                kasane_core::plugin::#target_path => #mod_ident::#fn_name(&self.state, _subject, _state),
            }
        })
        .collect();

    let transform_fn = quote! {
        fn transform(
            &self,
            _target: &kasane_core::plugin::TransformTarget,
            _subject: kasane_core::plugin::TransformSubject,
            _state: &kasane_core::plugin::AppView<'_>,
            _ctx: &kasane_core::plugin::TransformContext,
        ) -> kasane_core::plugin::TransformSubject {
            match _target {
                #(#transform_arms)*
                _ => _subject,
            }
        }
    };

    // transform_priority() — use the max priority among transforms
    let max_priority = def
        .transforms
        .iter()
        .filter_map(|t| t.priority)
        .max()
        .unwrap_or(0);
    let priority_fn = if max_priority != 0 {
        let lit = syn::LitInt::new(&max_priority.to_string(), Span::call_site());
        quote! {
            fn transform_priority(&self) -> i16 {
                #lit
            }
        }
    } else {
        quote! {}
    };

    quote! {
        #transform_fn
        #priority_fn
    }
}

/// Generates the `Plugin::annotate_line_with_ctx()` implementation.
///
/// Detected by function name `annotate_line` in the module.
fn gen_annotate_line_impl(def: &PluginDef) -> TokenStream {
    let mod_ident = &def.mod_ident;
    if def.has_annotate_line {
        quote! {
            fn annotate_line_with_ctx(
                &self,
                _line: usize,
                _state: &kasane_core::plugin::AppView<'_>,
                _ctx: &kasane_core::plugin::AnnotateContext,
            ) -> Option<kasane_core::plugin::LineAnnotation> {
                #mod_ident::annotate_line(&self.state, _line, _state, _ctx)
            }
        }
    } else {
        quote! {}
    }
}

/// Generates the `Plugin::capabilities()` method with accurate flags.
fn gen_capabilities_impl(def: &PluginDef) -> TokenStream {
    let mut caps = Vec::new();

    if def.has_annotate_line {
        caps.push(quote! { kasane_core::plugin::PluginCapabilities::ANNOTATOR });
    }
    if !def.transforms.is_empty() {
        caps.push(quote! { kasane_core::plugin::PluginCapabilities::TRANSFORMER });
    }
    if def.has_transform_menu_item {
        caps.push(quote! { kasane_core::plugin::PluginCapabilities::MENU_TRANSFORM });
    }
    if def.has_handle_key || def.has_handle_mouse {
        caps.push(quote! { kasane_core::plugin::PluginCapabilities::INPUT_HANDLER });
    }

    // Only generate if we have specific caps (not all)
    if caps.is_empty() {
        // No capabilities detected — use default (which excludes new API flags)
        return quote! {};
    }

    quote! {
        fn capabilities(&self) -> kasane_core::plugin::PluginCapabilities {
            #(#caps)|*
        }
    }
}

fn generate_plugin_struct(def: &PluginDef, module: &ItemMod) -> syn::Result<TokenStream> {
    let mod_ident = &def.mod_ident;
    let struct_name = format_ident!("{}Plugin", to_pascal_case(&mod_ident.to_string()));

    let cleaned_module = strip_custom_attrs(module);

    let (state_field, state_init) = gen_state_field(def);
    let id_str = mod_ident.to_string();

    let update_impl = gen_update_impl(def, &struct_name);
    let lifecycle_impl = gen_lifecycle_impl(def);
    let input_impl = gen_input_impl(def);
    let transform_impl = gen_transform_impl(def);
    let annotate_line_impl = gen_annotate_line_impl(def);
    let transform_menu_item_impl = gen_transform_menu_item_impl(def);
    let state_hash_impl = gen_state_hash_impl(def);
    let capabilities_impl = gen_capabilities_impl(def);

    Ok(quote! {
        #cleaned_module

        pub struct #struct_name {
            #state_field
        }

        impl #struct_name {
            pub fn new() -> Self {
                Self {
                    #state_init
                }
            }
        }

        impl kasane_core::plugin::PluginBackend for #struct_name {
            fn id(&self) -> kasane_core::plugin::PluginId {
                kasane_core::plugin::PluginId(#id_str.to_string())
            }

            #capabilities_impl
            #lifecycle_impl
            #input_impl
            #update_impl
            #state_hash_impl
            #transform_impl
            #annotate_line_impl
            #transform_menu_item_impl
        }
    })
}

/// Strip our custom attributes (#[state], #[event], #[transform(...)],
/// #[keybind(...)]) from module items so they don't cause compiler errors.
fn strip_custom_attrs(module: &ItemMod) -> TokenStream {
    let vis = &module.vis;
    let ident = &module.ident;
    let attrs = &module.attrs;

    let items = if let Some((_, items)) = &module.content {
        items.iter().map(strip_item_attrs).collect::<Vec<_>>()
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

const CUSTOM_ATTRS: &[&str] = &[
    "state",
    "event",
    "transform",
    "keybind",
    "lifecycle",
    "input",
];

fn is_custom_attr(attr: &Attribute) -> bool {
    CUSTOM_ATTRS.iter().any(|name| attr.path().is_ident(name))
}

fn filter_attrs(attrs: &[Attribute]) -> Vec<&Attribute> {
    attrs.iter().filter(|a| !is_custom_attr(a)).collect()
}

fn strip_item_attrs(item: &Item) -> TokenStream {
    match item {
        Item::Struct(s) => {
            let kept: Vec<_> = filter_attrs(&s.attrs);
            let vis = &s.vis;
            let ident = &s.ident;
            let generics = &s.generics;
            let fields = &s.fields;
            let semi = &s.semi_token;
            let semi_tok = semi.map(|_| quote! { ; }).unwrap_or_default();

            // Add #[derive(Hash)] for #[state] structs
            let is_state = s.attrs.iter().any(|a| a.path().is_ident("state"));
            let extra_derive = if is_state {
                quote! { #[derive(Hash)] }
            } else {
                quote! {}
            };

            quote! {
                #(#kept)*
                #extra_derive
                #vis struct #ident #generics #fields #semi_tok
            }
        }
        Item::Enum(e) => {
            let kept: Vec<_> = filter_attrs(&e.attrs);
            let vis = &e.vis;
            let ident = &e.ident;
            let generics = &e.generics;
            let variants = &e.variants;
            quote! {
                #(#kept)*
                #vis enum #ident #generics {
                    #variants
                }
            }
        }
        Item::Fn(f) => {
            let kept: Vec<_> = filter_attrs(&f.attrs);
            let vis = &f.vis;
            let sig = &f.sig;
            let block = &f.block;
            quote! {
                #(#kept)*
                #vis #sig #block
            }
        }
        // Pass through other items unchanged
        other => quote! { #other },
    }
}
