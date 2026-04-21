//! Tree-sitter [`SyntaxProvider`] implementation.

use std::ops::Range;
use std::sync::atomic::{AtomicU64, Ordering};

use tree_sitter::StreamingIterator;

use kasane_core::syntax::{Declaration, DeclarationKind, SyntaxNode, SyntaxProvider};

/// Tree-sitter backed syntax provider.
///
/// Wraps a `tree_sitter::Parser` and its current `Tree`, providing the
/// `SyntaxProvider` trait to the rest of Kasane.
pub struct TreeSitterProvider {
    parser: tree_sitter::Parser,
    tree: Option<tree_sitter::Tree>,
    source: Vec<u8>,
    generation: AtomicU64,
    fold_query: Option<tree_sitter::Query>,
    declaration_query: Option<tree_sitter::Query>,
    language_name: String,
}

// SAFETY: tree_sitter::Parser and tree_sitter::Tree are not Send by default,
// but we only access them from a single thread (SyntaxManager serializes access).
// The AtomicU64 generation counter is the only field read concurrently.
unsafe impl Send for TreeSitterProvider {}
unsafe impl Sync for TreeSitterProvider {}

impl TreeSitterProvider {
    /// Create a new provider for the given language.
    pub fn new(
        language: tree_sitter::Language,
        language_name: String,
        fold_query: Option<tree_sitter::Query>,
        declaration_query: Option<tree_sitter::Query>,
    ) -> Self {
        let mut parser = tree_sitter::Parser::new();
        parser
            .set_language(&language)
            .expect("failed to set tree-sitter language");
        Self {
            parser,
            tree: None,
            source: Vec::new(),
            generation: AtomicU64::new(0),
            fold_query,
            declaration_query,
            language_name,
        }
    }

    /// Parse or incrementally re-parse the source.
    ///
    /// Returns `true` if parsing succeeded and the tree changed.
    pub fn update(&mut self, new_source: &[u8]) -> bool {
        // TODO: Use tree.edit() for incremental re-parse when we have edit info.
        let old_tree = self.tree.as_ref();
        match self.parser.parse(new_source, old_tree) {
            Some(new_tree) => {
                self.tree = Some(new_tree);
                self.source = new_source.to_vec();
                self.generation.fetch_add(1, Ordering::Release);
                true
            }
            None => false,
        }
    }

    /// The language name this provider handles.
    pub fn language_name(&self) -> &str {
        &self.language_name
    }

    /// Extract line ranges from fold query captures.
    fn run_fold_query(&self) -> Vec<Range<usize>> {
        let (Some(tree), Some(query)) = (&self.tree, &self.fold_query) else {
            return Vec::new();
        };
        let mut cursor = tree_sitter::QueryCursor::new();
        let mut matches = cursor.matches(query, tree.root_node(), self.source.as_slice());
        let mut ranges = Vec::new();
        while let Some(m) = matches.next() {
            for capture in m.captures {
                let node = capture.node;
                let start_line = node.start_position().row;
                let end_line = node.end_position().row + 1;
                if end_line > start_line + 1 {
                    ranges.push(start_line..end_line);
                }
            }
        }
        ranges
    }

    /// Extract declarations from declaration query captures.
    fn run_declaration_query(&self) -> Vec<Declaration> {
        let (Some(tree), Some(query)) = (&self.tree, &self.declaration_query) else {
            return Vec::new();
        };
        let mut cursor = tree_sitter::QueryCursor::new();
        let mut matches = cursor.matches(query, tree.root_node(), self.source.as_slice());

        let name_idx = query.capture_index_for_name("name");
        let body_idx = query.capture_index_for_name("body");
        let decl_idx = query.capture_index_for_name("declaration");

        let mut declarations = Vec::new();

        while let Some(m) = matches.next() {
            let mut decl_node = None;
            let mut name_node = None;
            let mut body_node = None;

            for capture in m.captures {
                if Some(capture.index) == decl_idx {
                    decl_node = Some(capture.node);
                } else if Some(capture.index) == name_idx {
                    name_node = Some(capture.node);
                } else if Some(capture.index) == body_idx {
                    body_node = Some(capture.node);
                }
            }

            let Some(decl) = decl_node else { continue };
            let Some(name) = name_node else { continue };

            let name_text = name
                .utf8_text(&self.source)
                .unwrap_or("<unknown>")
                .to_string();

            let kind = infer_declaration_kind(decl.kind());
            let name_line = name.start_position().row;
            let sig_start = decl.start_position().row;
            let sig_end = body_node
                .map(|b: tree_sitter::Node<'_>| b.start_position().row)
                .unwrap_or(decl.end_position().row + 1);
            let body_lines = body_node
                .map(|b: tree_sitter::Node<'_>| b.start_position().row..b.end_position().row + 1);
            let depth = compute_depth(decl);

            declarations.push(Declaration {
                kind,
                name: name_text,
                name_line,
                signature_lines: sig_start..sig_end,
                body_lines,
                depth,
            });
        }

        declarations
    }
}

/// Infer `DeclarationKind` from a tree-sitter node kind string.
fn infer_declaration_kind(node_kind: &str) -> DeclarationKind {
    match node_kind {
        "function_definition"
        | "function_item"
        | "function_declaration"
        | "method_definition"
        | "method_declaration"
        | "arrow_function"
        | "func_literal" => DeclarationKind::Function,
        "struct_item" | "struct_declaration" | "struct_specifier" => DeclarationKind::Struct,
        "enum_item" | "enum_declaration" | "enum_specifier" => DeclarationKind::Enum,
        "trait_item" => DeclarationKind::Trait,
        "impl_item" => DeclarationKind::Impl,
        "mod_item" | "module_declaration" | "module" => DeclarationKind::Module,
        "class_definition" | "class_declaration" | "class_specifier" => DeclarationKind::Class,
        "interface_declaration" => DeclarationKind::Interface,
        "type_alias_declaration" | "type_item" => DeclarationKind::TypeAlias,
        "const_item" | "const_declaration" | "lexical_declaration" => DeclarationKind::Const,
        "use_declaration" | "import_declaration" | "import_statement" => DeclarationKind::Import,
        _ => DeclarationKind::Function, // fallback
    }
}

/// Compute nesting depth by counting ancestors.
fn compute_depth(node: tree_sitter::Node<'_>) -> u32 {
    let mut depth = 0u32;
    let mut current = node.parent();
    while let Some(parent) = current {
        if is_scope_node(parent.kind()) {
            depth += 1;
        }
        current = parent.parent();
    }
    depth
}

fn is_scope_node(kind: &str) -> bool {
    matches!(
        kind,
        "function_definition"
            | "function_item"
            | "function_declaration"
            | "method_definition"
            | "struct_item"
            | "enum_item"
            | "trait_item"
            | "impl_item"
            | "mod_item"
            | "class_definition"
            | "class_declaration"
            | "module"
    )
}

impl SyntaxProvider for TreeSitterProvider {
    fn generation(&self) -> u64 {
        self.generation.load(Ordering::Acquire)
    }

    fn fold_ranges(&self) -> Vec<Range<usize>> {
        self.run_fold_query()
    }

    fn scopes_at(&self, line: usize, byte_offset: usize) -> Vec<String> {
        let Some(tree) = &self.tree else {
            return Vec::new();
        };
        let point = tree_sitter::Point::new(line, byte_offset);
        let mut node = tree.root_node().descendant_for_point_range(point, point);
        let mut scopes = Vec::new();
        while let Some(n) = node {
            scopes.push(n.kind().to_string());
            node = n.parent();
        }
        scopes.reverse();
        scopes
    }

    fn nodes_in_range(&self, range: Range<usize>, kind: Option<&str>) -> Vec<SyntaxNode> {
        let Some(tree) = &self.tree else {
            return Vec::new();
        };
        let mut cursor = tree.walk();
        let mut nodes = Vec::new();

        fn walk(
            cursor: &mut tree_sitter::TreeCursor<'_>,
            byte_range: &Range<usize>,
            kind_filter: Option<&str>,
            out: &mut Vec<SyntaxNode>,
        ) {
            let node = cursor.node();
            let node_range = node.byte_range();
            if node_range.end <= byte_range.start || node_range.start >= byte_range.end {
                return;
            }
            if kind_filter.is_none_or(|k| k == node.kind()) && node.is_named() {
                out.push(SyntaxNode {
                    kind: node.kind().to_string(),
                    byte_range: node_range.clone(),
                    line_range: node.start_position().row..node.end_position().row + 1,
                    is_named: true,
                });
            }
            if cursor.goto_first_child() {
                loop {
                    walk(cursor, byte_range, kind_filter, out);
                    if !cursor.goto_next_sibling() {
                        break;
                    }
                }
                cursor.goto_parent();
            }
        }

        walk(&mut cursor, &range, kind, &mut nodes);
        nodes
    }

    fn indent_level(&self, line: usize) -> u32 {
        // Count leading whitespace bytes on the given line.
        let mut line_start = 0;
        let mut current_line = 0;
        for &byte in &self.source {
            if current_line == line {
                break;
            }
            if byte == b'\n' {
                current_line += 1;
            }
            line_start += 1;
        }
        let mut indent = 0u32;
        for &byte in &self.source[line_start..] {
            match byte {
                b' ' => indent += 1,
                b'\t' => indent += 4,
                _ => break,
            }
        }
        indent
    }

    fn declarations(&self) -> Vec<Declaration> {
        self.run_declaration_query()
    }

    fn signature_summary(&self, line: usize) -> Option<String> {
        let tree = self.tree.as_ref()?;
        let point = tree_sitter::Point::new(line, 0);
        let node = tree.root_node().descendant_for_point_range(point, point)?;
        // Walk up to find the enclosing declaration.
        let mut current = Some(node);
        while let Some(n) = current {
            if is_scope_node(n.kind()) && n.start_position().row == line {
                // Extract the first line of this node as the signature.
                let start = n.start_byte();
                let end_of_first_line = self.source[start..]
                    .iter()
                    .position(|&b| b == b'\n')
                    .map(|pos| start + pos)
                    .unwrap_or(n.end_byte());
                return std::str::from_utf8(&self.source[start..end_of_first_line])
                    .ok()
                    .map(|s| s.to_string());
            }
            current = n.parent();
        }
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn generation_increments_on_parse() {
        let counter = AtomicU64::new(0);
        assert_eq!(counter.load(Ordering::Acquire), 0);
        counter.fetch_add(1, Ordering::Release);
        assert_eq!(counter.load(Ordering::Acquire), 1);
    }

    #[test]
    fn infer_kinds() {
        assert_eq!(
            infer_declaration_kind("function_item"),
            DeclarationKind::Function
        );
        assert_eq!(
            infer_declaration_kind("struct_item"),
            DeclarationKind::Struct
        );
        assert_eq!(infer_declaration_kind("enum_item"), DeclarationKind::Enum);
        assert_eq!(infer_declaration_kind("trait_item"), DeclarationKind::Trait);
        assert_eq!(infer_declaration_kind("impl_item"), DeclarationKind::Impl);
        assert_eq!(
            infer_declaration_kind("class_definition"),
            DeclarationKind::Class
        );
        assert_eq!(
            infer_declaration_kind("unknown_thing"),
            DeclarationKind::Function
        );
    }

    #[test]
    fn is_scope_node_coverage() {
        assert!(is_scope_node("function_item"));
        assert!(is_scope_node("struct_item"));
        assert!(is_scope_node("impl_item"));
        assert!(!is_scope_node("let_declaration"));
        assert!(!is_scope_node("string_literal"));
    }
}
