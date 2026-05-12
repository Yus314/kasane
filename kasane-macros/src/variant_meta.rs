//! `#[derive(VariantMeta)]` — compile-time variant-name reflection.
//!
//! For an enum
//!
//! ```ignore
//! #[derive(VariantMeta)]
//! pub enum DisplayDirective {
//!     #[variant_meta(destructive)]
//!     Hide { range: Range<usize> },
//!     Fold { /* ... */ },
//!     // ...
//! }
//! ```
//!
//! the derive emits an inherent impl with:
//!
//! - `pub fn variant_name(&self) -> &'static str` — exhaustive, no wildcard
//! - `pub const ALL_VARIANT_NAMES: &'static [&'static str]` — sorted ascending
//! - `pub const DESTRUCTIVE_VARIANTS: &'static [&'static str]` —
//!   only the variants tagged `#[variant_meta(destructive)]`, sorted.
//! - `pub const PRESERVING_VARIANTS: &'static [&'static str]` —
//!   the complement: every other variant, sorted.
//!
//! The derive is intentionally minimal. The current consumers
//! (`DisplayDirective`, `Command`, `Element`, `Msg`) all only need
//! variant-name reflection plus the destructive / preserving split that
//! ADR-030 attaches to display directives. Additional group attributes
//! can be added later without breaking existing callers.

use proc_macro2::TokenStream;
use quote::quote;
use syn::{Attribute, Data, DeriveInput, Error, Ident};

pub fn expand_variant_meta(input: TokenStream) -> Result<TokenStream, Error> {
    let input: DeriveInput = syn::parse2(input)?;
    let name = &input.ident;

    let Data::Enum(data) = &input.data else {
        return Err(Error::new_spanned(
            &input,
            "VariantMeta can only be derived for enums",
        ));
    };

    // Collect (ident, is_destructive) for every variant.
    let mut variants: Vec<(Ident, bool)> = Vec::new();
    for v in &data.variants {
        let is_destructive = has_variant_meta_marker(&v.attrs, "destructive")?;
        variants.push((v.ident.clone(), is_destructive));
    }

    // Match arm patterns must cover every variant shape.
    let variant_name_arms = data.variants.iter().map(|v| {
        let v_ident = &v.ident;
        let pat = match &v.fields {
            syn::Fields::Unit => quote! { Self::#v_ident },
            syn::Fields::Named(_) => quote! { Self::#v_ident { .. } },
            syn::Fields::Unnamed(_) => quote! { Self::#v_ident(..) },
        };
        let lit = v_ident.to_string();
        quote! { #pat => #lit }
    });

    let mut all_names: Vec<String> = variants.iter().map(|(i, _)| i.to_string()).collect();
    all_names.sort();
    let all_names_lits = all_names.iter().map(|n| quote! { #n });

    let mut destructive: Vec<String> = variants
        .iter()
        .filter(|(_, d)| *d)
        .map(|(i, _)| i.to_string())
        .collect();
    destructive.sort();
    let destructive_lits = destructive.iter().map(|n| quote! { #n });

    let mut preserving: Vec<String> = variants
        .iter()
        .filter(|(_, d)| !d)
        .map(|(i, _)| i.to_string())
        .collect();
    preserving.sort();
    let preserving_lits = preserving.iter().map(|n| quote! { #n });

    Ok(quote! {
        impl #name {
            /// All variant names of this enum, sorted ascending.
            pub const ALL_VARIANT_NAMES: &'static [&'static str] = &[
                #( #all_names_lits ),*
            ];

            /// Variants tagged `#[variant_meta(destructive)]`, sorted ascending.
            pub const DESTRUCTIVE_VARIANTS: &'static [&'static str] = &[
                #( #destructive_lits ),*
            ];

            /// Variants not tagged destructive (the complement), sorted ascending.
            pub const PRESERVING_VARIANTS: &'static [&'static str] = &[
                #( #preserving_lits ),*
            ];

            /// Static name of this variant. Exhaustive; no wildcard.
            pub fn variant_name(&self) -> &'static str {
                match self {
                    #( #variant_name_arms ),*
                }
            }
        }
    })
}

/// Returns `Ok(true)` if `attrs` contains `#[variant_meta(<flag>)]`.
fn has_variant_meta_marker(attrs: &[Attribute], flag: &str) -> Result<bool, Error> {
    for attr in attrs {
        if !attr.path().is_ident("variant_meta") {
            continue;
        }
        let mut found = false;
        attr.parse_nested_meta(|meta| {
            if meta.path.is_ident(flag) {
                found = true;
            }
            Ok(())
        })?;
        if found {
            return Ok(true);
        }
    }
    Ok(false)
}
