use std::collections::HashSet;

use proc_macro2::{TokenStream, TokenTree};
use syn::parse::{Parse, ParseStream};
use syn::punctuated::Punctuated;
use syn::visit::Visit;
use syn::{
    Error, Expr, ExprField, ExprMethodCall, FnArg, Ident, ItemFn, Macro, Member, Pat, ReturnType,
    Token, Type, parenthesized,
};

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

/// Maps AppState field names to the DirtyFlags they belong to.
/// Fields not listed here are "free reads" (geometry, non-rendered state).
const FIELD_FLAG_MAP: &[(&str, &[&str])] = &[
    // BUFFER
    ("lines", &["BUFFER"]),
    ("lines_dirty", &["BUFFER"]),
    ("default_face", &["BUFFER"]),
    ("padding_face", &["BUFFER"]),
    ("cursor_mode", &["BUFFER"]),
    ("cursor_pos", &["BUFFER"]),
    ("cursor_count", &["BUFFER"]),
    // STATUS
    ("status_line", &["STATUS"]),
    ("status_mode_line", &["STATUS"]),
    ("status_default_face", &["STATUS"]),
    // MENU
    ("menu", &["MENU_STRUCTURE", "MENU_SELECTION"]),
    // INFO
    ("infos", &["INFO"]),
    // OPTIONS
    ("ui_options", &["OPTIONS"]),
    ("shadow_enabled", &["OPTIONS"]),
    ("padding_char", &["OPTIONS"]),
    ("menu_max_height", &["OPTIONS"]),
    ("menu_position", &["OPTIONS"]),
    ("search_dropdown", &["OPTIONS"]),
    ("status_at_top", &["OPTIONS"]),
    // Free reads (no DirtyFlag — resize triggers ALL externally):
    // cols, rows, focused, drag, smooth_scroll, scroll_animation
];

/// Maps known AppState method names to the fields they read.
const METHOD_FIELD_MAP: &[(&str, &[&str])] = &[
    ("available_height", &["rows"]), // rows → free read
];

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

/// Visitor that collects field accesses on a named identifier (the state parameter).
struct StateFieldVisitor {
    state_ident: String,
    accessed_fields: HashSet<String>,
}

impl<'ast> Visit<'ast> for StateFieldVisitor {
    fn visit_expr_field(&mut self, node: &'ast ExprField) {
        if self.is_state_expr(&node.base)
            && let Member::Named(ref ident) = node.member
        {
            self.accessed_fields.insert(ident.to_string());
        }
        syn::visit::visit_expr_field(self, node);
    }

    fn visit_expr_method_call(&mut self, node: &'ast ExprMethodCall) {
        // Check if receiver is our state ident
        if self.is_state_expr(&node.receiver) {
            let method_name = node.method.to_string();
            // Look up in METHOD_FIELD_MAP
            if let Some(fields) = METHOD_FIELD_MAP
                .iter()
                .find(|(m, _)| *m == method_name)
                .map(|(_, fields)| *fields)
            {
                for field in fields {
                    self.accessed_fields.insert((*field).to_string());
                }
            }
            // Unknown methods are silently ignored
        }
        // Continue visiting child nodes
        syn::visit::visit_expr_method_call(self, node);
    }

    fn visit_macro(&mut self, node: &'ast Macro) {
        // Scan macro token streams for `state.field` patterns
        // (e.g., format!("...", state.cursor_count))
        self.scan_token_stream(node.tokens.clone());
        syn::visit::visit_macro(self, node);
    }
}

impl StateFieldVisitor {
    /// Scan a token stream for `state.field` patterns (for macro bodies).
    fn scan_token_stream(&mut self, tokens: TokenStream) {
        let tokens: Vec<TokenTree> = tokens.into_iter().collect();
        let mut i = 0;
        while i < tokens.len() {
            // Look for pattern: Ident(state) Punct(.) Ident(field)
            if let TokenTree::Ident(ref ident) = tokens[i]
                && ident == &self.state_ident
                && i + 2 < tokens.len()
                && let TokenTree::Punct(ref punct) = tokens[i + 1]
                && punct.as_char() == '.'
                && let TokenTree::Ident(ref field) = tokens[i + 2]
            {
                self.accessed_fields.insert(field.to_string());
                i += 3;
                continue;
            }
            // Recurse into groups (parentheses, brackets, braces)
            if let TokenTree::Group(ref group) = tokens[i] {
                self.scan_token_stream(group.stream());
            }
            i += 1;
        }
    }

    /// Check if an expression is our state identifier (handles `state`, `*state`, etc.)
    fn is_state_expr(&self, expr: &Expr) -> bool {
        match expr {
            Expr::Path(ep) => {
                ep.path.segments.len() == 1 && ep.path.segments[0].ident == self.state_ident
            }
            Expr::Unary(eu) => self.is_state_expr(&eu.expr),
            Expr::Paren(ep) => self.is_state_expr(&ep.expr),
            _ => false,
        }
    }
}

/// Find the state parameter name (parameter whose type path ends in `AppState`).
fn find_state_param(func: &ItemFn) -> Option<String> {
    for arg in &func.sig.inputs {
        if let FnArg::Typed(pat_type) = arg
            && type_is_app_state(&pat_type.ty)
            && let Pat::Ident(pi) = &*pat_type.pat
        {
            return Some(pi.ident.to_string());
        }
    }
    None
}

/// Check if a type path ends in `AppState` (handles `&AppState`, `&crate::state::AppState`, etc.)
fn type_is_app_state(ty: &Type) -> bool {
    match ty {
        Type::Reference(r) => type_is_app_state(&r.elem),
        Type::Path(tp) => tp
            .path
            .segments
            .last()
            .is_some_and(|s| s.ident == "AppState"),
        _ => false,
    }
}

/// Expand the flags covered by composite flags (MENU → MENU_STRUCTURE + MENU_SELECTION, ALL → everything).
fn expand_flags(flags: &[Ident]) -> HashSet<String> {
    let mut expanded = HashSet::new();
    for flag in flags {
        let name = flag.to_string();
        match name.as_str() {
            "ALL" => {
                expanded.extend(
                    [
                        "BUFFER",
                        "STATUS",
                        "MENU_STRUCTURE",
                        "MENU_SELECTION",
                        "INFO",
                        "OPTIONS",
                    ]
                    .iter()
                    .map(|s| s.to_string()),
                );
            }
            "MENU" => {
                expanded.insert("MENU_STRUCTURE".to_string());
                expanded.insert("MENU_SELECTION".to_string());
            }
            other => {
                expanded.insert(other.to_string());
            }
        }
    }
    expanded
}

/// Look up the required flags for a field name.
fn flags_for_field(field: &str) -> Option<&'static [&'static str]> {
    FIELD_FLAG_MAP
        .iter()
        .find(|(f, _)| *f == field)
        .map(|(_, flags)| *flags)
}

/// All known field names in FIELD_FLAG_MAP.
fn all_known_fields() -> HashSet<&'static str> {
    FIELD_FLAG_MAP.iter().map(|(f, _)| *f).collect()
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

        if let Some(state_ident) = find_state_param(&func) {
            let mut visitor = StateFieldVisitor {
                state_ident,
                accessed_fields: HashSet::new(),
            };
            visitor.visit_item_fn(&func);

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
