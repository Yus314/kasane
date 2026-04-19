//! Syntax analysis abstraction layer.
//!
//! Provides a trait-based API for querying syntax information (scope names,
//! fold ranges, indent levels, AST nodes) from a language-specific provider
//! such as tree-sitter.

use std::ops::Range;

/// A syntax tree node returned by [`SyntaxProvider::nodes_in_range`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SyntaxNode {
    /// The grammar node kind (e.g., `"function_definition"`, `"string_literal"`).
    pub kind: String,
    /// Byte range in the buffer this node spans.
    pub byte_range: Range<usize>,
    /// Line range (start inclusive, end exclusive).
    pub line_range: Range<usize>,
    /// Whether this node is a "named" node in the grammar (as opposed to anonymous
    /// punctuation/keyword nodes).
    pub is_named: bool,
}

/// Trait for providing syntax information to plugins.
///
/// Implementations wrap a concrete parser (e.g., tree-sitter) and expose
/// a language-agnostic query API. The `generation` counter allows consumers
/// to skip re-processing when the tree hasn't changed.
pub trait SyntaxProvider: Send + Sync {
    /// Monotonically increasing generation counter. Incremented whenever the
    /// underlying syntax tree is re-parsed. Plugins can cache results and
    /// invalidate when the generation changes.
    fn generation(&self) -> u64;

    /// Return line ranges that can be folded (e.g., function bodies, blocks).
    /// Each range is `start_line..end_line` (exclusive end).
    fn fold_ranges(&self) -> Vec<Range<usize>>;

    /// Return the stack of scope names at a given position.
    /// Example: `["source.rust", "meta.function", "string.quoted.double"]`.
    fn scopes_at(&self, line: usize, byte_offset: usize) -> Vec<String>;

    /// Return all syntax nodes whose byte range intersects `range`.
    /// If `kind` is `Some`, only nodes of that kind are returned.
    fn nodes_in_range(&self, range: Range<usize>, kind: Option<&str>) -> Vec<SyntaxNode>;

    /// Return the indentation level (in units, not spaces) of a given line.
    fn indent_level(&self, line: usize) -> u32;
}

/// A no-op provider that returns empty results for all queries.
/// Used as a fallback when no real syntax provider is available.
#[derive(Debug, Clone)]
pub struct NullSyntaxProvider;

impl SyntaxProvider for NullSyntaxProvider {
    fn generation(&self) -> u64 {
        0
    }

    fn fold_ranges(&self) -> Vec<Range<usize>> {
        Vec::new()
    }

    fn scopes_at(&self, _line: usize, _byte_offset: usize) -> Vec<String> {
        Vec::new()
    }

    fn nodes_in_range(&self, _range: Range<usize>, _kind: Option<&str>) -> Vec<SyntaxNode> {
        Vec::new()
    }

    fn indent_level(&self, _line: usize) -> u32 {
        0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn null_provider_returns_empty() {
        let provider = NullSyntaxProvider;
        assert_eq!(provider.generation(), 0);
        assert!(provider.fold_ranges().is_empty());
        assert!(provider.scopes_at(0, 0).is_empty());
        assert!(provider.nodes_in_range(0..100, None).is_empty());
        assert_eq!(provider.indent_level(0), 0);
    }

    #[test]
    fn null_provider_with_kind_filter() {
        let provider = NullSyntaxProvider;
        assert!(
            provider
                .nodes_in_range(0..50, Some("function_definition"))
                .is_empty()
        );
    }
}
