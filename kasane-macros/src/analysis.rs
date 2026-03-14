use std::collections::HashSet;

use proc_macro2::{TokenStream, TokenTree};
use syn::visit::Visit;
use syn::{Expr, ExprField, ExprMethodCall, FnArg, Ident, ItemFn, Macro, Member, Pat, Type};

/// Known DirtyFlags flag names.
pub const KNOWN_FLAGS: &[&str] = &[
    "BUFFER",
    "BUFFER_CONTENT",
    "BUFFER_CURSOR",
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
pub const FIELD_FLAG_MAP: &[(&str, &[&str])] = &[
    // BUFFER_CONTENT
    ("lines", &["BUFFER_CONTENT"]),
    ("lines_dirty", &["BUFFER_CONTENT"]),
    ("default_face", &["BUFFER_CONTENT"]),
    ("padding_face", &["BUFFER_CONTENT"]),
    ("widget_columns", &["BUFFER_CONTENT"]),
    // BUFFER_CURSOR
    ("cursor_mode", &["BUFFER_CURSOR"]),
    ("cursor_pos", &["BUFFER_CURSOR"]),
    ("cursor_count", &["BUFFER_CURSOR"]),
    ("secondary_cursors", &["BUFFER_CURSOR"]),
    // STATUS
    ("status_prompt", &["STATUS"]),
    ("status_content", &["STATUS"]),
    ("status_content_cursor_pos", &["STATUS"]),
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
    ("scrollbar_thumb", &["MENU_STRUCTURE"]),
    ("scrollbar_track", &["MENU_STRUCTURE"]),
    ("assistant_art", &["OPTIONS"]),
    ("plugin_config", &["OPTIONS"]),
    ("secondary_blend_ratio", &["BUFFER_CONTENT"]),
    // Free reads (no DirtyFlag — resize triggers ALL externally):
    // cols, rows, focused, drag, smooth_scroll, scroll_animation
];

/// Maps known AppState method names to the fields they read.
pub const METHOD_FIELD_MAP: &[(&str, &[&str])] = &[
    ("available_height", &["rows"]),      // rows → free read
    ("has_menu", &["menu"]),              // menu → MENU_STRUCTURE | MENU_SELECTION
    ("has_info", &["infos"]),             // infos → INFO
    ("is_prompt_mode", &["cursor_mode"]), // cursor_mode → BUFFER
    ("buffer_line_count", &["lines"]),    // lines → BUFFER
    ("visible_line_range", &["lines"]),   // lines → BUFFER
];

/// Visitor that collects field accesses on a named identifier (the state parameter).
pub struct StateFieldVisitor {
    pub state_ident: String,
    pub accessed_fields: HashSet<String>,
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
            // Unknown methods and helper function calls that receive `state`
            // are not tracked. These represent a soundness gap that must be
            // covered by manual `allow()` annotations or by adding methods to
            // METHOD_FIELD_MAP. True fix (inter-procedural analysis) is
            // impossible in proc_macros.
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
    pub fn scan_token_stream(&mut self, tokens: TokenStream) {
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
    pub fn is_state_expr(&self, expr: &Expr) -> bool {
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

/// Find the AppState parameter name (parameter whose type path ends in `AppState`).
pub fn find_appstate_param(func: &ItemFn) -> Option<String> {
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
pub fn type_is_app_state(ty: &Type) -> bool {
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

/// Expand the flags covered by composite flags.
///
/// - `ALL` → all atomic flags
/// - `BUFFER` → `BUFFER_CONTENT` + `BUFFER_CURSOR`
/// - `MENU` → `MENU_STRUCTURE` + `MENU_SELECTION`
pub fn expand_flags(flags: &[Ident]) -> HashSet<String> {
    let mut expanded = HashSet::new();
    for flag in flags {
        expand_single_flag(&flag.to_string(), &mut expanded);
    }
    expanded
}

/// Expand string flag names (same logic as `expand_flags` but from strings).
pub fn expand_flag_strs(flags: &[&str]) -> HashSet<String> {
    let mut expanded = HashSet::new();
    for flag in flags {
        expand_single_flag(flag, &mut expanded);
    }
    expanded
}

fn expand_single_flag(flag: &str, expanded: &mut HashSet<String>) {
    match flag {
        "ALL" => {
            expanded.extend(
                [
                    "BUFFER_CONTENT",
                    "BUFFER_CURSOR",
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
        "BUFFER" => {
            expanded.insert("BUFFER_CONTENT".to_string());
            expanded.insert("BUFFER_CURSOR".to_string());
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

/// Look up the required flags for a field name.
pub fn flags_for_field(field: &str) -> Option<&'static [&'static str]> {
    FIELD_FLAG_MAP
        .iter()
        .find(|(f, _)| *f == field)
        .map(|(_, flags)| *flags)
}

/// All known field names in FIELD_FLAG_MAP.
pub fn all_known_fields() -> HashSet<&'static str> {
    FIELD_FLAG_MAP.iter().map(|(f, _)| *f).collect()
}

/// Look up the field names that a method reads.
#[cfg(test)]
pub fn fields_for_method(method: &str) -> Option<&'static [&'static str]> {
    METHOD_FIELD_MAP
        .iter()
        .find(|(m, _)| *m == method)
        .map(|(_, fields)| *fields)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_flags_for_new_fields() {
        assert_eq!(
            flags_for_field("scrollbar_thumb"),
            Some(["MENU_STRUCTURE"].as_slice())
        );
        assert_eq!(
            flags_for_field("scrollbar_track"),
            Some(["MENU_STRUCTURE"].as_slice())
        );
        assert_eq!(
            flags_for_field("assistant_art"),
            Some(["OPTIONS"].as_slice())
        );
        assert_eq!(
            flags_for_field("plugin_config"),
            Some(["OPTIONS"].as_slice())
        );
        assert_eq!(
            flags_for_field("secondary_blend_ratio"),
            Some(["BUFFER_CONTENT"].as_slice())
        );
    }

    #[test]
    fn test_flags_for_existing_fields() {
        assert_eq!(
            flags_for_field("lines"),
            Some(["BUFFER_CONTENT"].as_slice())
        );
        assert_eq!(
            flags_for_field("status_prompt"),
            Some(["STATUS"].as_slice())
        );
        assert_eq!(
            flags_for_field("menu"),
            Some(["MENU_STRUCTURE", "MENU_SELECTION"].as_slice())
        );
        assert_eq!(flags_for_field("infos"), Some(["INFO"].as_slice()));
        assert_eq!(
            flags_for_field("shadow_enabled"),
            Some(["OPTIONS"].as_slice())
        );
    }

    #[test]
    fn test_flags_for_free_reads() {
        assert_eq!(flags_for_field("cols"), None);
        assert_eq!(flags_for_field("rows"), None);
        assert_eq!(flags_for_field("focused"), None);
        assert_eq!(flags_for_field("drag"), None);
        assert_eq!(flags_for_field("smooth_scroll"), None);
    }

    #[test]
    fn test_method_field_map() {
        assert_eq!(
            fields_for_method("available_height"),
            Some(["rows"].as_slice())
        );
        assert_eq!(fields_for_method("has_menu"), Some(["menu"].as_slice()));
        assert_eq!(fields_for_method("has_info"), Some(["infos"].as_slice()));
        assert_eq!(
            fields_for_method("is_prompt_mode"),
            Some(["cursor_mode"].as_slice())
        );
        assert_eq!(
            fields_for_method("buffer_line_count"),
            Some(["lines"].as_slice())
        );
        assert_eq!(
            fields_for_method("visible_line_range"),
            Some(["lines"].as_slice())
        );
        assert_eq!(fields_for_method("unknown_method"), None);
    }

    #[test]
    fn test_expand_flags() {
        let flags = expand_flag_strs(&["MENU"]);
        assert!(flags.contains("MENU_STRUCTURE"));
        assert!(flags.contains("MENU_SELECTION"));
        assert!(!flags.contains("BUFFER_CONTENT"));

        let flags = expand_flag_strs(&["BUFFER"]);
        assert!(flags.contains("BUFFER_CONTENT"));
        assert!(flags.contains("BUFFER_CURSOR"));

        let flags = expand_flag_strs(&["ALL"]);
        assert!(flags.contains("BUFFER_CONTENT"));
        assert!(flags.contains("BUFFER_CURSOR"));
        assert!(flags.contains("STATUS"));
        assert!(flags.contains("INFO"));
        assert!(flags.contains("OPTIONS"));
        assert!(flags.contains("MENU_STRUCTURE"));
        assert!(flags.contains("MENU_SELECTION"));
    }
}
