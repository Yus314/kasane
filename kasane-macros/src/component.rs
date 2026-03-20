//! `#[kasane_component]` macro: purity validation for view component functions.

use proc_macro2::TokenStream;
use syn::{FnArg, ItemFn, Pat, ReturnType, Type};

pub fn expand_kasane_component(_attr: TokenStream, input: TokenStream) -> syn::Result<TokenStream> {
    let func: ItemFn = syn::parse2(input.clone())?;

    // Must have a return type
    if matches!(func.sig.output, ReturnType::Default) {
        return Err(syn::Error::new_spanned(
            &func.sig,
            "#[kasane_component] function must have a return type",
        ));
    }

    // No &mut parameters
    for arg in &func.sig.inputs {
        if let FnArg::Typed(pat_type) = arg
            && let Type::Reference(r) = &*pat_type.ty
            && r.mutability.is_some()
        {
            let name = match &*pat_type.pat {
                Pat::Ident(i) => i.ident.to_string(),
                _ => "parameter".to_string(),
            };
            return Err(syn::Error::new_spanned(
                &pat_type.ty,
                format!("#[kasane_component] functions must be pure: `{name}` cannot be &mut"),
            ));
        }
    }

    // Pass through unchanged
    Ok(input)
}
