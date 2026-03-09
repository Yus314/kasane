use std::collections::HashSet;

use proc_macro2::TokenStream;
use syn::parse::{Parse, ParseStream};
use syn::punctuated::Punctuated;
use syn::{Error, FnArg, Ident, ItemFn, Pat, ReturnType, Token, Type, parenthesized};

use crate::analysis::*;

/// Parsed `deps(FLAG1, FLAG2, ...), allow(field1, field2, ...)` attribute content.
struct ComponentAttr {
    flags: Vec<Ident>,
    allowed_fields: Vec<Ident>,
    has_deps: bool,
}

impl Parse for ComponentAttr {
    fn parse(input: ParseStream) -> syn::Result<Self> {
        if input.is_empty() {
            return Ok(ComponentAttr {
                flags: vec![],
                allowed_fields: vec![],
                has_deps: false,
            });
        }

        // Parse deps(...)
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

        let mut allowed_fields = Vec::new();

        // Parse optional allow(...)
        if input.peek(Token![,]) {
            input.parse::<Token![,]>()?;
            if !input.is_empty() {
                let allow_keyword: Ident = input.parse()?;
                if allow_keyword != "allow" {
                    return Err(Error::new_spanned(
                        &allow_keyword,
                        format!("expected `allow(...)`, found `{allow_keyword}`"),
                    ));
                }
                let allow_content;
                parenthesized!(allow_content in input);
                let fields: Punctuated<Ident, Token![,]> =
                    allow_content.parse_terminated(Ident::parse, Token![,])?;
                allowed_fields = fields.into_iter().collect();
            }
        }

        Ok(ComponentAttr {
            flags: flags.into_iter().collect(),
            allowed_fields,
            has_deps: true,
        })
    }
}

pub fn expand_kasane_component(attr: TokenStream, input: TokenStream) -> syn::Result<TokenStream> {
    let func: ItemFn = syn::parse2(input.clone())?;

    // Parse deps() and allow()
    let comp_attr: ComponentAttr = syn::parse2(attr)?;

    // Validate flag names
    for flag in &comp_attr.flags {
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

    // Field-access analysis: only when deps() is present
    if comp_attr.has_deps {
        // Validate allow() field names
        let known = all_known_fields();
        // Also accept free-read fields in allow() (cols, rows, etc. — though pointless, not wrong)
        for field in &comp_attr.allowed_fields {
            let field_name = field.to_string();
            if !known.contains(field_name.as_str()) {
                // Check if it's a known free-read field
                let free_reads = [
                    "cols",
                    "rows",
                    "focused",
                    "drag",
                    "smooth_scroll",
                    "scroll_animation",
                ];
                if !free_reads.contains(&field_name.as_str()) {
                    return Err(Error::new_spanned(
                        field,
                        format!(
                            "unknown AppState field `{field_name}` in allow(). \
                             Known fields: {}",
                            known.iter().copied().collect::<Vec<_>>().join(", ")
                        ),
                    ));
                }
            }
        }

        if let Some(state_ident) = find_appstate_param(&func) {
            let mut visitor = StateFieldVisitor {
                state_ident,
                accessed_fields: HashSet::new(),
            };
            syn::visit::Visit::visit_item_fn(&mut visitor, &func);

            let covered_flags = expand_flags(&comp_attr.flags);
            let allowed: HashSet<String> = comp_attr
                .allowed_fields
                .iter()
                .map(|i| i.to_string())
                .collect();

            // Check each accessed field
            for field in &visitor.accessed_fields {
                if allowed.contains(field) {
                    continue;
                }
                if let Some(required_flags) = flags_for_field(field) {
                    for &req_flag in required_flags {
                        if !covered_flags.contains(req_flag) {
                            return Err(Error::new_spanned(
                                &func.sig.ident,
                                format!(
                                    "component reads `state.{field}` which requires DirtyFlags::{req_flag}, \
                                     but deps() only declares [{}]. \
                                     Add `{req_flag}` to deps() or `{field}` to allow()",
                                    comp_attr
                                        .flags
                                        .iter()
                                        .map(|f| f.to_string())
                                        .collect::<Vec<_>>()
                                        .join(", ")
                                ),
                            ));
                        }
                    }
                }
                // Field not in FIELD_FLAG_MAP → free read, skip
            }
        }
        // No AppState parameter → no field access analysis needed
    }

    // Pass through unchanged.
    // DEPS constants (e.g., BUILD_BASE_DEPS) are defined manually in view/mod.rs
    // rather than being macro-generated, because the macro cannot reliably determine
    // the crate path for DirtyFlags (crate::state vs kasane_core::state) across
    // different invocation contexts.
    Ok(input)
}
