//! Syntax-aware zoom strategies for Semantic Zoom levels 1–4.
//!
//! Uses `SyntaxProvider` methods (declarations, fold_ranges, scopes_at,
//! signature_summary) to generate display directives with AST awareness.

use crate::display::DisplayDirective;
use crate::plugin::app_view::AppView;
use crate::protocol::{Atom, Style, WireFace};
use crate::syntax::SyntaxProvider;

use super::{SemanticZoomState, ZoomLevel};

/// Compute display directives for the given zoom level using syntax analysis.
pub fn syntax_directives(state: &SemanticZoomState, app: &AppView<'_>) -> Vec<DisplayDirective> {
    let Some(sp) = app.syntax_provider() else {
        return vec![];
    };
    match state.level {
        ZoomLevel::ANNOTATED => annotated(sp.as_ref(), app),
        ZoomLevel::COMPRESSED => compressed(sp.as_ref()),
        ZoomLevel::OUTLINE => outline(sp.as_ref()),
        ZoomLevel::SKELETON => skeleton(sp.as_ref()),
        _ => vec![],
    }
}

fn plain_atom(text: &str) -> Atom {
    Atom::plain(text)
}

// =============================================================================
// Level 1: Annotated — scope/type hints via StyleInline
// =============================================================================

fn annotated(sp: &dyn SyntaxProvider, app: &AppView<'_>) -> Vec<DisplayDirective> {
    // Add scope hints as virtual text at the end of declaration lines.
    let declarations = sp.declarations();
    let mut directives = Vec::new();

    for decl in &declarations {
        if decl.name_line < app.line_count() {
            let hint_text = format!("  // {}", decl.kind);
            directives.push(DisplayDirective::VirtualText {
                line: decl.name_line,
                position: crate::display::VirtualTextPosition::EndOfLine,
                content: vec![Atom::with_style(
                    hint_text,
                    Style::from_face(&WireFace {
                        fg: crate::protocol::Color::Named(crate::protocol::NamedColor::Cyan),
                        ..WireFace::default()
                    }),
                )],
                priority: -50,
            });
        }
    }

    directives
}

// =============================================================================
// Level 2: Compressed — fold all foldable regions from syntax
// =============================================================================

fn compressed(sp: &dyn SyntaxProvider) -> Vec<DisplayDirective> {
    let fold_ranges = sp.fold_ranges();
    let mut directives = Vec::new();

    // Deduplicate and sort ranges, resolve overlaps by keeping the outermost.
    let mut ranges: Vec<(usize, usize)> =
        fold_ranges.into_iter().map(|r| (r.start, r.end)).collect();
    ranges.sort_by_key(|r| (r.0, std::cmp::Reverse(r.1)));

    let mut last_end = 0;
    for (start, end) in ranges {
        if start >= last_end && end > start + 1 {
            let count = end - start - 1;
            directives.push(DisplayDirective::Fold {
                range: start..end,
                summary: vec![plain_atom(&format!("  ... ({count} lines)"))],
            });
            last_end = end;
        }
    }

    directives
}

// =============================================================================
// Level 3: Outline — show declarations, hide/fold bodies
// =============================================================================

fn outline(sp: &dyn SyntaxProvider) -> Vec<DisplayDirective> {
    let declarations = sp.declarations();
    if declarations.is_empty() {
        return vec![];
    }

    let mut directives = Vec::new();

    for decl in &declarations {
        // Fold the body of each declaration.
        if let Some(ref body) = decl.body_lines
            && body.end > body.start + 1
        {
            let count = body.end - body.start;
            let summary = sp
                .signature_summary(decl.name_line)
                .map(|s| vec![plain_atom(&format!("  {s} ... ({count} lines)"))])
                .unwrap_or_else(|| vec![plain_atom(&format!("  ... ({count} lines)"))]);
            directives.push(DisplayDirective::Fold {
                range: body.start..body.end,
                summary,
            });
        }
    }

    // Sort by range start to ensure deterministic ordering.
    directives.sort_by_key(|d| match d {
        DisplayDirective::Fold { range, .. } => range.start,
        DisplayDirective::Hide { range } => range.start,
        _ => 0,
    });

    // Remove overlapping ranges (keep first/outermost).
    let mut filtered = Vec::new();
    let mut last_end = 0;
    for d in directives {
        let start = match &d {
            DisplayDirective::Fold { range, .. } => range.start,
            DisplayDirective::Hide { range } => range.start,
            _ => 0,
        };
        if start >= last_end {
            let end = match &d {
                DisplayDirective::Fold { range, .. } => range.end,
                DisplayDirective::Hide { range } => range.end,
                _ => 0,
            };
            last_end = end;
            filtered.push(d);
        }
    }

    filtered
}

// =============================================================================
// Level 4: Skeleton — show only signatures, hide bodies
// =============================================================================

fn skeleton(sp: &dyn SyntaxProvider) -> Vec<DisplayDirective> {
    let declarations = sp.declarations();
    if declarations.is_empty() {
        return vec![];
    }

    let mut directives = Vec::new();

    for decl in &declarations {
        // Hide the body entirely; add signature summary as virtual text.
        if let Some(ref body) = decl.body_lines
            && body.end > body.start
        {
            directives.push(DisplayDirective::Hide {
                range: body.start..body.end,
            });
        }

        // Add signature summary as virtual text.
        if let Some(summary) = sp.signature_summary(decl.name_line)
            && decl.body_lines.is_some()
        {
            directives.push(DisplayDirective::VirtualText {
                line: decl.name_line,
                position: crate::display::VirtualTextPosition::EndOfLine,
                content: vec![plain_atom(&format!("  // {summary}"))],
                priority: -50,
            });
        }
    }

    // Sort and deduplicate spatial directives.
    let (spatial, non_spatial): (Vec<_>, Vec<_>) =
        directives.into_iter().partition(|d| d.is_spatial());

    let mut sorted_spatial = spatial;
    sorted_spatial.sort_by_key(|d| match d {
        DisplayDirective::Fold { range, .. } => range.start,
        DisplayDirective::Hide { range } => range.start,
        _ => 0,
    });

    // Remove overlapping spatial ranges.
    let mut filtered = Vec::new();
    let mut last_end = 0;
    for d in sorted_spatial {
        let start = match &d {
            DisplayDirective::Fold { range, .. } => range.start,
            DisplayDirective::Hide { range } => range.start,
            _ => 0,
        };
        if start >= last_end {
            let end = match &d {
                DisplayDirective::Fold { range, .. } => range.end,
                DisplayDirective::Hide { range } => range.end,
                _ => 0,
            };
            last_end = end;
            filtered.push(d);
        }
    }

    // Re-add non-spatial directives.
    filtered.extend(non_spatial);

    filtered
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::syntax::{Declaration, DeclarationKind, NullSyntaxProvider};
    use std::ops::Range;
    use std::sync::atomic::{AtomicU64, Ordering};

    /// A mock syntax provider for testing.
    struct MockSyntaxProvider {
        generation: AtomicU64,
        fold_ranges: Vec<Range<usize>>,
        declarations: Vec<Declaration>,
    }

    impl MockSyntaxProvider {
        fn new(fold_ranges: Vec<Range<usize>>, declarations: Vec<Declaration>) -> Self {
            Self {
                generation: AtomicU64::new(1),
                fold_ranges,
                declarations,
            }
        }
    }

    impl SyntaxProvider for MockSyntaxProvider {
        fn generation(&self) -> u64 {
            self.generation.load(Ordering::Acquire)
        }
        fn fold_ranges(&self) -> Vec<Range<usize>> {
            self.fold_ranges.clone()
        }
        fn scopes_at(&self, _line: usize, _byte_offset: usize) -> Vec<String> {
            vec![]
        }
        fn nodes_in_range(
            &self,
            _range: Range<usize>,
            _kind: Option<&str>,
        ) -> Vec<crate::syntax::SyntaxNode> {
            vec![]
        }
        fn indent_level(&self, _line: usize) -> u32 {
            0
        }
        fn declarations(&self) -> Vec<Declaration> {
            self.declarations.clone()
        }
        fn signature_summary(&self, line: usize) -> Option<String> {
            self.declarations
                .iter()
                .find(|d| d.name_line == line)
                .map(|d| format!("{} {}", d.kind, d.name))
        }
    }

    fn make_decl(
        kind: DeclarationKind,
        name: &str,
        name_line: usize,
        sig: Range<usize>,
        body: Option<Range<usize>>,
    ) -> Declaration {
        Declaration {
            kind,
            name: name.to_string(),
            name_line,
            signature_lines: sig,
            body_lines: body,
            depth: 0,
        }
    }

    #[test]
    fn compressed_uses_fold_ranges() {
        let sp = MockSyntaxProvider::new(vec![2..8, 10..15], vec![]);
        let directives = compressed(&sp);
        assert_eq!(directives.len(), 2);
        match &directives[0] {
            DisplayDirective::Fold { range, .. } => assert_eq!(range.clone(), 2..8),
            other => panic!("expected Fold, got {other:?}"),
        }
    }

    #[test]
    fn outline_folds_bodies() {
        let decls = vec![
            make_decl(DeclarationKind::Function, "foo", 0, 0..1, Some(1..5)),
            make_decl(DeclarationKind::Function, "bar", 6, 6..7, Some(7..10)),
        ];
        let sp = MockSyntaxProvider::new(vec![], decls);
        let directives = outline(&sp);
        assert!(!directives.is_empty());
        // All spatial directives should be Fold
        for d in &directives {
            if d.is_spatial() {
                assert!(matches!(d, DisplayDirective::Fold { .. }));
            }
        }
    }

    #[test]
    fn skeleton_hides_bodies() {
        let decls = vec![make_decl(
            DeclarationKind::Function,
            "foo",
            0,
            0..1,
            Some(1..5),
        )];
        let sp = MockSyntaxProvider::new(vec![], decls);
        let directives = skeleton(&sp);
        let spatial: Vec<_> = directives.iter().filter(|d| d.is_spatial()).collect();
        assert!(!spatial.is_empty());
        for d in spatial {
            assert!(matches!(d, DisplayDirective::Hide { .. }));
        }
    }

    #[test]
    fn no_directives_for_null_provider() {
        let sp = NullSyntaxProvider;
        assert!(compressed(&sp).is_empty());
        assert!(outline(&sp).is_empty());
        assert!(skeleton(&sp).is_empty());
    }

    #[test]
    fn no_overlapping_spatial_ranges() {
        let decls = vec![
            make_decl(DeclarationKind::Function, "foo", 0, 0..1, Some(1..10)),
            make_decl(DeclarationKind::Function, "inner", 3, 3..4, Some(4..8)),
            make_decl(DeclarationKind::Function, "bar", 12, 12..13, Some(13..20)),
        ];
        let sp = MockSyntaxProvider::new(vec![1..10, 4..8, 13..20], decls);

        for level in [
            ZoomLevel::COMPRESSED,
            ZoomLevel::OUTLINE,
            ZoomLevel::SKELETON,
        ] {
            let state = SemanticZoomState { level };
            let app_state = crate::state::AppState::default();
            let view = AppView::new(&app_state);
            // Call the strategy directly with the mock
            let directives = match level {
                ZoomLevel::COMPRESSED => compressed(&sp),
                ZoomLevel::OUTLINE => outline(&sp),
                ZoomLevel::SKELETON => skeleton(&sp),
                _ => vec![],
            };
            let _ = (state, view); // suppress unused warnings

            let mut ranges: Vec<(usize, usize)> = Vec::new();
            for d in &directives {
                let r = match d {
                    DisplayDirective::Fold { range, .. } => (range.start, range.end),
                    DisplayDirective::Hide { range } => (range.start, range.end),
                    _ => continue,
                };
                for prev in &ranges {
                    assert!(
                        r.1 <= prev.0 || r.0 >= prev.1,
                        "level {level}: overlapping ranges {r:?} and {prev:?}"
                    );
                }
                ranges.push(r);
            }
        }
    }
}
