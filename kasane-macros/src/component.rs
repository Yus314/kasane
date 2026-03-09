use proc_macro2::TokenStream;
use syn::parse::{Parse, ParseStream};
use syn::punctuated::Punctuated;
use syn::{Error, FnArg, Ident, ItemFn, Pat, ReturnType, Token, Type, parenthesized};

/// Known DirtyFlags flag names.
const KNOWN_FLAGS: &[&str] = &[
    "BUFFER",
    "STATUS",
    "MENU_STRUCTURE",
    "MENU_SELECTION",
    "MENU",
    "INFO",
    "OPTIONS",
    "ALL",
];

/// Parsed `deps(FLAG1, FLAG2, ...)` attribute content.
struct DepsAttr {
    flags: Vec<Ident>,
}

impl Parse for DepsAttr {
    fn parse(input: ParseStream) -> syn::Result<Self> {
        if input.is_empty() {
            return Ok(DepsAttr { flags: vec![] });
        }

        let keyword: Ident = input.parse()?;
        if keyword != "deps" {
            return Err(Error::new_spanned(
                &keyword,
                format!("expected `deps(...)`, found `{keyword}`"),
            ));
        }

        let content;
        parenthesized!(content in input);
        let flags: Punctuated<Ident, Token![,]> =
            content.parse_terminated(Ident::parse, Token![,])?;

        Ok(DepsAttr {
            flags: flags.into_iter().collect(),
        })
    }
}

pub fn expand_kasane_component(attr: TokenStream, input: TokenStream) -> syn::Result<TokenStream> {
    let func: ItemFn = syn::parse2(input.clone())?;

    // Parse and validate deps() if present
    let deps: DepsAttr = syn::parse2(attr)?;
    for flag in &deps.flags {
        let name = flag.to_string();
        if !KNOWN_FLAGS.contains(&name.as_str()) {
            return Err(Error::new_spanned(
                flag,
                format!(
                    "unknown DirtyFlags variant `{name}`. Expected one of: {}",
                    KNOWN_FLAGS.join(", ")
                ),
            ));
        }
    }

    // Must have a return type
    if matches!(func.sig.output, ReturnType::Default) {
        return Err(Error::new_spanned(
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
            return Err(Error::new_spanned(
                &pat_type.ty,
                format!("#[kasane_component] functions must be pure: `{name}` cannot be &mut"),
            ));
        }
    }

    // Pass through unchanged (deps is validated documentation only)
    Ok(input)
}
