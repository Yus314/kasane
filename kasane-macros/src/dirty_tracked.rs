//! `#[derive(DirtyTracked)]` — compile-time enforcement of field → DirtyFlags mapping
//! and epistemological classification.
//!
//! Every field in the annotated struct must carry exactly one of:
//! - `#[dirty(FLAG1, FLAG2, ...)]` — this field requires the listed DirtyFlags
//! - `#[dirty(free)]` — this field is a free read (no DirtyFlag needed)
//!
//! Every field must also carry exactly one `#[epistemic(...)]` annotation classifying
//! it as `observed`, `derived`, `heuristic`, `config`, `session`, or `runtime`.
//!
//! The derive generates public constants:
//! - `FIELD_DIRTY_MAP: &[(&str, &[&str])]` — field→flags for tracked fields
//! - `FREE_READ_FIELDS: &[&str]` — field names marked as free reads
//! - `FIELD_EPISTEMIC_MAP: &[(&str, &str)]` — field→category
//! - `HEURISTIC_FIELDS: &[(&str, &str, &str)]` — (field, rule, severity) for heuristics
//! - `DERIVED_FIELDS: &[(&str, &str)]` — (field, source) for derived fields
//! - `FIELDS_BY_CATEGORY: &[(&str, &[&str])]` — category→fields
//! - `SALSA_OPT_OUTS: &[(&str, &str)]` — fields that declare `salsa_opt_out = "reason"`
//!
//! Any category accepts an optional `salsa_opt_out = "reason"` key, used by
//! `kasane-core/tests/salsa_projection_coverage_level2.rs` to justify
//! derived/heuristic/config fields that are not surfaced in the Salsa input
//! layer. See ADR-030 Level 2.

use proc_macro2::TokenStream;
use quote::quote;
use syn::{Data, DeriveInput, Error, Fields, Ident, Meta};

/// Parsed dirty annotation for one field.
enum DirtyAnnotation {
    /// `#[dirty(FLAG1, FLAG2, ...)]`
    Flags(Vec<String>),
    /// `#[dirty(free)]`
    Free,
}

/// Known epistemological categories.
const KNOWN_CATEGORIES: &[&str] = &[
    "observed",
    "derived",
    "heuristic",
    "config",
    "session",
    "runtime",
];

/// Known severity levels for heuristic fields.
const KNOWN_SEVERITIES: &[&str] = &["catastrophic", "degraded", "cosmetic"];

/// Parsed epistemic annotation for one field.
struct EpistemicAnnotation {
    category: EpistemicCategory,
    /// Optional justification for why this field is not surfaced in the
    /// Salsa input layer. Consumed by
    /// `kasane-core/tests/salsa_projection_coverage_level2.rs`. See ADR-030
    /// Level 2 for the enforcement contract.
    salsa_opt_out: Option<String>,
}

/// Parsed epistemic category body (without the optional `salsa_opt_out`).
enum EpistemicCategory {
    Observed,
    Derived { source: Option<String> },
    Heuristic { rule: String, severity: String },
    Config,
    Session,
    Runtime,
}

impl EpistemicCategory {
    fn name(&self) -> &'static str {
        match self {
            EpistemicCategory::Observed => "observed",
            EpistemicCategory::Derived { .. } => "derived",
            EpistemicCategory::Heuristic { .. } => "heuristic",
            EpistemicCategory::Config => "config",
            EpistemicCategory::Session => "session",
            EpistemicCategory::Runtime => "runtime",
        }
    }
}

pub fn expand_dirty_tracked(input: TokenStream) -> syn::Result<TokenStream> {
    let input: DeriveInput = syn::parse2(input)?;

    let Data::Struct(data_struct) = &input.data else {
        return Err(Error::new_spanned(
            &input.ident,
            "DirtyTracked can only be derived on structs",
        ));
    };

    let Fields::Named(fields) = &data_struct.fields else {
        return Err(Error::new_spanned(
            &input.ident,
            "DirtyTracked requires named fields",
        ));
    };

    let known_flags: &[&str] = &[
        "BUFFER_CONTENT",
        "BUFFER_CURSOR",
        "STATUS",
        "MENU_STRUCTURE",
        "MENU_SELECTION",
        "INFO",
        "OPTIONS",
        "SESSION",
        "SETTINGS",
    ];

    let mut field_entries = Vec::new(); // (field_name, &[flag_names])
    let mut free_fields = Vec::new(); // field_name
    let mut epistemic_entries = Vec::new(); // (field_name, EpistemicAnnotation)

    for field in &fields.named {
        let field_name = field
            .ident
            .as_ref()
            .ok_or_else(|| Error::new_spanned(field, "expected named field"))?;

        // Parse dirty annotation
        let annotation = parse_dirty_attr(field_name, &field.attrs)?;
        let Some(annotation) = annotation else {
            return Err(Error::new_spanned(
                field_name,
                format!(
                    "field `{}` is missing a `#[dirty(...)]` annotation. \
                     Add `#[dirty(FLAG)]` or `#[dirty(free)]`",
                    field_name
                ),
            ));
        };

        match annotation {
            DirtyAnnotation::Flags(flags) => {
                // Validate flag names
                for flag in &flags {
                    if !known_flags.contains(&flag.as_str()) {
                        return Err(Error::new_spanned(
                            field_name,
                            format!(
                                "unknown DirtyFlags variant `{flag}` in #[dirty(...)]. \
                                 Expected one of: {}",
                                known_flags.join(", ")
                            ),
                        ));
                    }
                }
                field_entries.push((field_name.to_string(), flags));
            }
            DirtyAnnotation::Free => {
                free_fields.push(field_name.to_string());
            }
        }

        // Parse epistemic annotation
        let epistemic = parse_epistemic_attr(field_name, &field.attrs)?;
        let Some(epistemic) = epistemic else {
            return Err(Error::new_spanned(
                field_name,
                format!(
                    "field `{}` is missing an `#[epistemic(...)]` annotation. \
                     Add e.g. `#[epistemic(observed)]` or `#[epistemic(config)]`",
                    field_name
                ),
            ));
        };

        epistemic_entries.push((field_name.to_string(), epistemic));
    }

    // Generate the FIELD_DIRTY_MAP constant
    let map_entries = field_entries.iter().map(|(name, flags)| {
        let flag_strs = flags.iter().map(|f| quote! { #f });
        quote! { (#name, &[#(#flag_strs),*] as &[&str]) }
    });

    let free_entries = free_fields.iter().map(|name| {
        quote! { #name }
    });

    // Generate FIELD_EPISTEMIC_MAP
    let epistemic_map_entries = epistemic_entries.iter().map(|(name, ann)| {
        let cat_str = ann.category.name();
        quote! { (#name, #cat_str) }
    });

    // Generate HEURISTIC_FIELDS
    let heuristic_entries = epistemic_entries.iter().filter_map(|(name, ann)| {
        if let EpistemicCategory::Heuristic { rule, severity } = &ann.category {
            Some(quote! { (#name, #rule, #severity) })
        } else {
            None
        }
    });

    // Generate DERIVED_FIELDS
    let derived_entries = epistemic_entries.iter().filter_map(|(name, ann)| {
        if let EpistemicCategory::Derived { source } = &ann.category {
            let source_str = source.as_deref().unwrap_or("");
            Some(quote! { (#name, #source_str) })
        } else {
            None
        }
    });

    // Generate SALSA_OPT_OUTS — `(field, reason)` for every field that declared
    // `#[epistemic(..., salsa_opt_out = "...")]`.
    let salsa_opt_out_entries = epistemic_entries.iter().filter_map(|(name, ann)| {
        ann.salsa_opt_out
            .as_ref()
            .map(|reason| quote! { (#name, #reason) })
    });

    // Generate FIELDS_BY_CATEGORY — collect fields per category
    let fields_by_cat = {
        let mut by_cat: Vec<(&str, Vec<&str>)> =
            KNOWN_CATEGORIES.iter().map(|&c| (c, Vec::new())).collect();

        for (name, ann) in &epistemic_entries {
            let cat_name = ann.category.name();
            for (c, fields) in &mut by_cat {
                if *c == cat_name {
                    fields.push(name.as_str());
                    break;
                }
            }
        }

        by_cat
    };

    let fields_by_cat_entries = fields_by_cat.iter().map(|(cat, fields)| {
        quote! { (#cat, &[#(#fields),*] as &[&str]) }
    });

    let struct_name = &input.ident;

    // We generate an impl block with associated constants
    let expanded = quote! {
        impl #struct_name {
            /// Field → DirtyFlags mapping, generated by `#[derive(DirtyTracked)]`.
            ///
            /// Each entry is `(field_name, &[flag_names])`. Fields marked
            /// `#[dirty(free)]` are excluded (they appear in `FREE_READ_FIELDS`).
            pub const FIELD_DIRTY_MAP: &[(&str, &[&str])] = &[
                #(#map_entries),*
            ];

            /// Fields that are free reads (no DirtyFlag needed).
            pub const FREE_READ_FIELDS: &[&str] = &[
                #(#free_entries),*
            ];

            /// Field → epistemological category, generated by `#[derive(DirtyTracked)]`.
            pub const FIELD_EPISTEMIC_MAP: &[(&str, &str)] = &[
                #(#epistemic_map_entries),*
            ];

            /// Heuristic fields: `(field, rule, severity)`.
            pub const HEURISTIC_FIELDS: &[(&str, &str, &str)] = &[
                #(#heuristic_entries),*
            ];

            /// Derived fields: `(field, source_description)`.
            pub const DERIVED_FIELDS: &[(&str, &str)] = &[
                #(#derived_entries),*
            ];

            /// Fields grouped by epistemological category.
            pub const FIELDS_BY_CATEGORY: &[(&str, &[&str])] = &[
                #(#fields_by_cat_entries),*
            ];

            /// Fields that declared `#[epistemic(..., salsa_opt_out = "reason")]`.
            ///
            /// Consumed by `kasane-core/tests/salsa_projection_coverage_level2.rs`
            /// to witness that every derived/heuristic/config field is either
            /// surfaced in the Salsa input layer or explicitly exempted with a
            /// documented reason. See ADR-030 Level 2.
            pub const SALSA_OPT_OUTS: &[(&str, &str)] = &[
                #(#salsa_opt_out_entries),*
            ];
        }
    };

    Ok(expanded)
}

/// Parse the `#[dirty(...)]` attribute from a field's attributes.
fn parse_dirty_attr(
    field_name: &Ident,
    attrs: &[syn::Attribute],
) -> syn::Result<Option<DirtyAnnotation>> {
    let dirty_attrs: Vec<_> = attrs
        .iter()
        .filter(|a| a.path().is_ident("dirty"))
        .collect();

    if dirty_attrs.is_empty() {
        return Ok(None);
    }

    if dirty_attrs.len() > 1 {
        return Err(Error::new_spanned(
            field_name,
            "multiple #[dirty(...)] annotations on the same field",
        ));
    }

    let attr = dirty_attrs[0];

    let Meta::List(meta_list) = &attr.meta else {
        return Err(Error::new_spanned(
            attr,
            "expected #[dirty(FLAG, ...)] or #[dirty(free)]",
        ));
    };

    let tokens: Vec<proc_macro2::TokenTree> = meta_list.tokens.clone().into_iter().collect();

    if tokens.is_empty() {
        return Err(Error::new_spanned(
            attr,
            "empty #[dirty()]: specify flag names or `free`",
        ));
    }

    // Check for `free`
    if tokens.len() == 1
        && let proc_macro2::TokenTree::Ident(ref ident) = tokens[0]
        && ident == "free"
    {
        return Ok(Some(DirtyAnnotation::Free));
    }

    // Parse comma-separated flag identifiers
    let mut flags = Vec::new();
    let mut expect_ident = true;
    for token in &tokens {
        match token {
            proc_macro2::TokenTree::Ident(ident) if expect_ident => {
                if ident == "free" {
                    return Err(Error::new_spanned(
                        ident,
                        "`free` cannot be combined with flag names",
                    ));
                }
                flags.push(ident.to_string());
                expect_ident = false;
            }
            proc_macro2::TokenTree::Punct(p) if !expect_ident && p.as_char() == ',' => {
                expect_ident = true;
            }
            other => {
                return Err(Error::new_spanned(
                    other,
                    "expected flag name or comma in #[dirty(...)]",
                ));
            }
        }
    }

    if flags.is_empty() {
        return Err(Error::new_spanned(
            attr,
            "#[dirty()] must contain at least one flag name or `free`",
        ));
    }

    Ok(Some(DirtyAnnotation::Flags(flags)))
}

/// Parse the `#[epistemic(...)]` attribute from a field's attributes.
fn parse_epistemic_attr(
    field_name: &Ident,
    attrs: &[syn::Attribute],
) -> syn::Result<Option<EpistemicAnnotation>> {
    let epistemic_attrs: Vec<_> = attrs
        .iter()
        .filter(|a| a.path().is_ident("epistemic"))
        .collect();

    if epistemic_attrs.is_empty() {
        return Ok(None);
    }

    if epistemic_attrs.len() > 1 {
        return Err(Error::new_spanned(
            field_name,
            "multiple #[epistemic(...)] annotations on the same field",
        ));
    }

    let attr = epistemic_attrs[0];

    let Meta::List(meta_list) = &attr.meta else {
        return Err(Error::new_spanned(
            attr,
            "expected #[epistemic(category, ...)]",
        ));
    };

    let tokens: Vec<proc_macro2::TokenTree> = meta_list.tokens.clone().into_iter().collect();

    if tokens.is_empty() {
        return Err(Error::new_spanned(
            attr,
            "empty #[epistemic()]: specify a category",
        ));
    }

    // First token must be the category ident
    let proc_macro2::TokenTree::Ident(ref category_ident) = tokens[0] else {
        return Err(Error::new_spanned(
            &tokens[0],
            "expected category name in #[epistemic(...)]",
        ));
    };

    let category_str = category_ident.to_string();
    if !KNOWN_CATEGORIES.contains(&category_str.as_str()) {
        return Err(Error::new_spanned(
            category_ident,
            format!(
                "unknown epistemic category `{category_str}`. \
                 Expected one of: {}",
                KNOWN_CATEGORIES.join(", ")
            ),
        ));
    }

    // Parse remaining tokens as key-value pairs: `, key = "value"`
    let kvs = parse_key_value_pairs(&tokens[1..])?;

    // `salsa_opt_out` is a universal optional key accepted on every category.
    // It declares that the field is intentionally not surfaced in the Salsa
    // input layer. See ADR-030 Level 2.
    let salsa_opt_out = kvs
        .iter()
        .find(|(k, _)| k == "salsa_opt_out")
        .map(|(_, v)| v.clone());

    let category = match category_str.as_str() {
        "observed" | "config" | "session" | "runtime" => {
            // Reject unknown keys
            for (k, _) in &kvs {
                if k != "salsa_opt_out" {
                    return Err(Error::new_spanned(
                        attr,
                        format!(
                            "unknown key `{k}` in #[epistemic({category_str}, ...)]. \
                             Allowed: salsa_opt_out"
                        ),
                    ));
                }
            }
            match category_str.as_str() {
                "observed" => EpistemicCategory::Observed,
                "config" => EpistemicCategory::Config,
                "session" => EpistemicCategory::Session,
                "runtime" => EpistemicCategory::Runtime,
                _ => unreachable!(),
            }
        }
        "derived" => {
            let source = kvs
                .iter()
                .find(|(k, _)| k == "source")
                .map(|(_, v)| v.clone());
            // Reject unknown keys
            for (k, _) in &kvs {
                if k != "source" && k != "salsa_opt_out" {
                    return Err(Error::new_spanned(
                        attr,
                        format!(
                            "unknown key `{k}` in #[epistemic(derived, ...)]. \
                             Allowed: source, salsa_opt_out"
                        ),
                    ));
                }
            }
            EpistemicCategory::Derived { source }
        }
        "heuristic" => {
            let rule = kvs
                .iter()
                .find(|(k, _)| k == "rule")
                .map(|(_, v)| v.clone());
            let severity = kvs
                .iter()
                .find(|(k, _)| k == "severity")
                .map(|(_, v)| v.clone());
            // Reject unknown keys
            for (k, _) in &kvs {
                if k != "rule" && k != "severity" && k != "salsa_opt_out" {
                    return Err(Error::new_spanned(
                        attr,
                        format!(
                            "unknown key `{k}` in #[epistemic(heuristic, ...)]. \
                             Allowed: rule, severity, salsa_opt_out"
                        ),
                    ));
                }
            }
            let Some(rule) = rule else {
                return Err(Error::new_spanned(
                    attr,
                    format!(
                        "field `{field_name}`: #[epistemic(heuristic)] requires `rule = \"...\"`"
                    ),
                ));
            };
            let Some(severity) = severity else {
                return Err(Error::new_spanned(
                    attr,
                    format!(
                        "field `{field_name}`: #[epistemic(heuristic)] requires `severity = \"...\"`"
                    ),
                ));
            };
            if !KNOWN_SEVERITIES.contains(&severity.as_str()) {
                return Err(Error::new_spanned(
                    attr,
                    format!(
                        "unknown severity `{severity}` in #[epistemic(heuristic, ...)]. \
                         Expected one of: {}",
                        KNOWN_SEVERITIES.join(", ")
                    ),
                ));
            }
            EpistemicCategory::Heuristic { rule, severity }
        }
        _ => unreachable!(),
    };

    Ok(Some(EpistemicAnnotation {
        category,
        salsa_opt_out,
    }))
}

/// Parse `, key = "value"` pairs from a token slice.
/// Returns `Vec<(key, value)>`.
fn parse_key_value_pairs(tokens: &[proc_macro2::TokenTree]) -> syn::Result<Vec<(String, String)>> {
    let mut result = Vec::new();
    let mut i = 0;

    while i < tokens.len() {
        // Expect comma
        match &tokens[i] {
            proc_macro2::TokenTree::Punct(p) if p.as_char() == ',' => {
                i += 1;
            }
            other => {
                return Err(Error::new_spanned(
                    other,
                    "expected `,` before key-value pair",
                ));
            }
        }

        if i >= tokens.len() {
            break; // trailing comma is ok
        }

        // Expect key ident
        let proc_macro2::TokenTree::Ident(ref key) = tokens[i] else {
            return Err(Error::new_spanned(&tokens[i], "expected key identifier"));
        };
        let key_str = key.to_string();
        i += 1;

        // Expect `=`
        if i >= tokens.len() {
            return Err(Error::new_spanned(
                key,
                format!("expected `= \"...\"` after `{key_str}`"),
            ));
        }
        match &tokens[i] {
            proc_macro2::TokenTree::Punct(p) if p.as_char() == '=' => {
                i += 1;
            }
            other => {
                return Err(Error::new_spanned(
                    other,
                    format!("expected `=` after `{key_str}`"),
                ));
            }
        }

        // Expect string literal
        if i >= tokens.len() {
            return Err(Error::new_spanned(
                key,
                format!("expected string literal after `{key_str} =`"),
            ));
        }
        let proc_macro2::TokenTree::Literal(ref lit) = tokens[i] else {
            return Err(Error::new_spanned(&tokens[i], "expected string literal"));
        };
        // Parse the literal — it should be a string like `"I-1"`
        let lit_str = lit.to_string();
        let value = if lit_str.starts_with('"') && lit_str.ends_with('"') && lit_str.len() >= 2 {
            lit_str[1..lit_str.len() - 1].to_string()
        } else {
            return Err(Error::new_spanned(
                lit,
                "expected a string literal (e.g. \"...\") as value",
            ));
        };
        i += 1;

        result.push((key_str, value));
    }

    Ok(result)
}
