use proc_macro2::{Span, TokenStream};
use quote::{format_ident, quote};
use syn::{Attribute, Error, Expr, ExprPath, Ident, Item, ItemMod, Lit, parse2};

/// Parsed information extracted from the module.
struct PluginDef {
    mod_ident: Ident,
    has_state: bool,
    has_event: bool,
    has_update: bool,
    slots: Vec<SlotBinding>,
    decorators: Vec<DecoratorBinding>,
    replacements: Vec<ReplacementBinding>,
}

struct SlotBinding {
    slot_path: ExprPath,
    fn_name: Ident,
}

struct DecoratorBinding {
    target_path: ExprPath,
    priority: Option<u32>,
    fn_name: Ident,
}

struct ReplacementBinding {
    target_path: ExprPath,
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
        slots: Vec::new(),
        decorators: Vec::new(),
        replacements: Vec::new(),
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
                if f.sig.ident == "update" {
                    def.has_update = true;
                }
                // Check for #[slot(Slot::*)]
                if let Some(slot_path) = extract_single_path_attr(&f.attrs, "slot")? {
                    def.slots.push(SlotBinding {
                        slot_path,
                        fn_name: f.sig.ident.clone(),
                    });
                }
                // Check for #[decorate(DecorateTarget::*, priority = N)]
                if let Some((target_path, priority)) = extract_decorate_attr(&f.attrs)? {
                    def.decorators.push(DecoratorBinding {
                        target_path,
                        priority,
                        fn_name: f.sig.ident.clone(),
                    });
                }
                // Check for #[replace(ReplaceTarget::*)]
                if let Some(target_path) = extract_single_path_attr(&f.attrs, "replace")? {
                    def.replacements.push(ReplacementBinding {
                        target_path,
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

/// Extract a path like `Slot::BufferLeft` from `#[slot(Slot::BufferLeft)]`.
fn extract_single_path_attr(attrs: &[Attribute], attr_name: &str) -> syn::Result<Option<ExprPath>> {
    for attr in attrs {
        if attr.path().is_ident(attr_name) {
            let expr: Expr = attr.parse_args()?;
            if let Expr::Path(p) = expr {
                return Ok(Some(p));
            }
            return Err(Error::new_spanned(
                attr,
                format!("expected a path in #[{attr_name}(...)]"),
            ));
        }
    }
    Ok(None)
}

/// Extract `#[decorate(DecorateTarget::Buffer, priority = 10)]`
fn extract_decorate_attr(attrs: &[Attribute]) -> syn::Result<Option<(ExprPath, Option<u32>)>> {
    for attr in attrs {
        if attr.path().is_ident("decorate") {
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
                    "#[decorate(...)] requires a DecorateTarget path",
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

fn generate_plugin_struct(def: &PluginDef, module: &ItemMod) -> syn::Result<TokenStream> {
    let mod_ident = &def.mod_ident;
    let struct_name = format_ident!("{}Plugin", to_pascal_case(&mod_ident.to_string()));

    // Strip our custom attributes from items to produce clean module output
    let cleaned_module = strip_custom_attrs(module);

    // State field and constructor
    let (state_field, state_init) = if def.has_state {
        (
            quote! { pub state: #mod_ident::State, },
            quote! { state: #mod_ident::State::default(), },
        )
    } else {
        (quote! {}, quote! {})
    };

    // Plugin::id()
    let id_str = mod_ident.to_string();

    // Plugin::update()
    let update_impl = if def.has_update && def.has_event {
        quote! {
            fn update(&mut self, msg: Box<dyn ::std::any::Any>, state: &kasane_core::state::AppState) -> Vec<kasane_core::plugin::Command> {
                if let Ok(msg) = msg.downcast::<#mod_ident::Msg>() {
                    #mod_ident::update(&mut self.state, *msg, state)
                } else {
                    vec![]
                }
            }
        }
    } else {
        quote! {}
    };

    // Plugin::contribute() — dispatch to slot functions
    let contribute_impl = if def.slots.is_empty() {
        quote! {}
    } else {
        let slot_arms: Vec<_> = def
            .slots
            .iter()
            .map(|sb| {
                let slot_path = &sb.slot_path;
                let fn_name = &sb.fn_name;
                quote! {
                    kasane_core::plugin::#slot_path => #mod_ident::#fn_name(&self.state, _state),
                }
            })
            .collect();
        quote! {
            fn contribute(&self, _slot: kasane_core::plugin::Slot, _state: &kasane_core::state::AppState) -> Option<kasane_core::element::Element> {
                match _slot {
                    #(#slot_arms)*
                    _ => None,
                }
            }
        }
    };

    // Plugin::decorate() — dispatch to decorator functions
    let decorate_impl = if def.decorators.is_empty() {
        quote! {}
    } else {
        let decorate_arms: Vec<_> = def
            .decorators
            .iter()
            .map(|db| {
                let target_path = &db.target_path;
                let fn_name = &db.fn_name;
                quote! {
                    kasane_core::plugin::#target_path => #mod_ident::#fn_name(&self.state, _element, _state),
                }
            })
            .collect();
        quote! {
            fn decorate(&self, _target: kasane_core::plugin::DecorateTarget, _element: kasane_core::element::Element, _state: &kasane_core::state::AppState) -> kasane_core::element::Element {
                match _target {
                    #(#decorate_arms)*
                    _ => _element,
                }
            }
        }
    };

    // Plugin::decorator_priority()
    let priority_impl = {
        // Use the max priority among decorators, or 0
        let max_priority = def
            .decorators
            .iter()
            .filter_map(|d| d.priority)
            .max()
            .unwrap_or(0);
        if max_priority > 0 {
            let lit = syn::LitInt::new(&max_priority.to_string(), Span::call_site());
            quote! {
                fn decorator_priority(&self) -> u32 {
                    #lit
                }
            }
        } else {
            quote! {}
        }
    };

    // Plugin::replace() — dispatch to replacement functions
    let replace_impl = if def.replacements.is_empty() {
        quote! {}
    } else {
        let replace_arms: Vec<_> = def
            .replacements
            .iter()
            .map(|rb| {
                let target_path = &rb.target_path;
                let fn_name = &rb.fn_name;
                quote! {
                    kasane_core::plugin::#target_path => #mod_ident::#fn_name(&self.state, _state),
                }
            })
            .collect();
        quote! {
            fn replace(&self, _target: kasane_core::plugin::ReplaceTarget, _state: &kasane_core::state::AppState) -> Option<kasane_core::element::Element> {
                match _target {
                    #(#replace_arms)*
                    _ => None,
                }
            }
        }
    };

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

        impl kasane_core::plugin::Plugin for #struct_name {
            fn id(&self) -> kasane_core::plugin::PluginId {
                kasane_core::plugin::PluginId(#id_str.to_string())
            }

            #update_impl
            #contribute_impl
            #decorate_impl
            #priority_impl
            #replace_impl
        }
    })
}

/// Strip our custom attributes (#[state], #[event], #[slot(...)], #[decorate(...)],
/// #[replace(...)], #[keybind(...)]) from module items so they don't cause compiler errors.
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

const CUSTOM_ATTRS: &[&str] = &["state", "event", "slot", "decorate", "replace", "keybind"];

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
            quote! {
                #(#kept)*
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
