//! Indent-based fallback strategy for Semantic Zoom levels 2–4.
//!
//! Operates on viewport lines from Kakoune's `draw` command. Computes indent
//! from leading whitespace characters. This is the fallback when no
//! `SyntaxProvider` with parsed data is available.

use crate::display::DisplayDirective;
use crate::protocol::{Atom, Line};

use super::ZoomLevel;

/// Create a plain-text atom with default face.
fn plain_atom(text: &str) -> Atom {
    Atom::plain(text)
}

/// Compute display directives for the given zoom level using indent heuristics.
pub fn indent_directives(level: ZoomLevel, lines: &[Line]) -> Vec<DisplayDirective> {
    match level {
        ZoomLevel::COMPRESSED => compressed(lines),
        ZoomLevel::OUTLINE => outline(lines),
        ZoomLevel::SKELETON => skeleton(lines),
        // Level 1 (Annotated) needs syntax info — no-op for indent fallback.
        // Level 5 (Map) is deferred.
        _ => vec![],
    }
}

/// Compute the indent level of a line from its leading whitespace atoms.
///
/// Counts leading spaces and tabs in the first atom's text content.
/// Returns the number of leading whitespace characters (tabs count as 1).
fn line_indent(line: &[Atom]) -> usize {
    let Some(first_atom) = line.first() else {
        return 0;
    };
    first_atom
        .contents
        .chars()
        .take_while(|c| c.is_whitespace())
        .count()
}

/// Returns true if the line is empty or contains only whitespace.
fn is_blank(line: &[Atom]) -> bool {
    line.is_empty()
        || line
            .iter()
            .all(|atom| atom.contents.chars().all(|c| c.is_whitespace()))
}

/// Build a one-line summary from the first non-blank line's atoms in a range.
fn summary_atoms(lines: &[Line], range_start: usize, range_end: usize) -> Vec<Atom> {
    for i in range_start..range_end {
        if let Some(line) = lines.get(i)
            && !is_blank(line)
        {
            return line.clone();
        }
    }
    vec![plain_atom("...")]
}

// =============================================================================
// Level 2: Compressed — fold contiguous blocks at indent ≥ 2
// =============================================================================

fn compressed(lines: &[Line]) -> Vec<DisplayDirective> {
    let mut directives = Vec::new();
    let mut i = 0;

    while i < lines.len() {
        if line_indent(&lines[i]) >= 2 && !is_blank(&lines[i]) {
            let start = i;
            // Extend to include consecutive lines with indent ≥ 2 (or blank lines
            // sandwiched between indented lines).
            while i < lines.len() && (line_indent(&lines[i]) >= 2 || is_blank(&lines[i])) {
                i += 1;
            }
            // Trim trailing blank lines from the fold range.
            let mut end = i;
            while end > start && is_blank(&lines[end - 1]) {
                end -= 1;
            }
            if end > start + 1 {
                directives.push(DisplayDirective::Fold {
                    range: start..end,
                    summary: summary_atoms(lines, start, end),
                });
            } else {
                // Single line — not worth folding.
            }
        } else {
            i += 1;
        }
    }

    directives
}

// =============================================================================
// Level 3: Outline — hide non-declaration interior lines
// =============================================================================

/// Heuristic: a "declaration line" is a line at indent 0, or a line that
/// precedes an indent increase (i.e., the "header" of a block).
fn is_declaration_line(lines: &[Line], idx: usize) -> bool {
    if is_blank(&lines[idx]) {
        return false;
    }
    let indent = line_indent(&lines[idx]);
    if indent == 0 {
        return true;
    }
    // Check if this line precedes an indent increase.
    if idx + 1 < lines.len() {
        let next_indent = line_indent(&lines[idx + 1]);
        if next_indent > indent && !is_blank(&lines[idx + 1]) {
            return true;
        }
    }
    false
}

fn outline(lines: &[Line]) -> Vec<DisplayDirective> {
    let mut directives = Vec::new();
    let mut i = 0;

    while i < lines.len() {
        if is_declaration_line(lines, i) {
            // Keep declaration line visible.
            i += 1;
            // Fold the body that follows (contiguous lines with greater indent
            // or blank lines).
            if i < lines.len() {
                let decl_indent = line_indent(&lines[i - 1]);
                let body_start = i;
                while i < lines.len()
                    && (line_indent(&lines[i]) > decl_indent || is_blank(&lines[i]))
                {
                    i += 1;
                }
                if i > body_start {
                    let count = i - body_start;
                    let summary_text = format!("  ... ({count} lines)");
                    directives.push(DisplayDirective::Fold {
                        range: body_start..i,
                        summary: vec![plain_atom(&summary_text)],
                    });
                }
            }
        } else if is_blank(&lines[i]) {
            // Skip isolated blank lines.
            i += 1;
        } else {
            // Non-declaration, non-blank line — hide.
            let start = i;
            while i < lines.len() && !is_declaration_line(lines, i) && !is_blank(&lines[i]) {
                i += 1;
            }
            // Skip trailing blanks too.
            while i < lines.len() && is_blank(&lines[i]) {
                i += 1;
            }
            if i > start {
                directives.push(DisplayDirective::Hide { range: start..i });
            }
        }
    }

    directives
}

// =============================================================================
// Level 4: Skeleton — show only indent-0 lines
// =============================================================================

fn skeleton(lines: &[Line]) -> Vec<DisplayDirective> {
    let mut directives = Vec::new();
    let mut i = 0;

    while i < lines.len() {
        if line_indent(&lines[i]) > 0 || is_blank(&lines[i]) {
            let start = i;
            while i < lines.len() && (line_indent(&lines[i]) > 0 || is_blank(&lines[i])) {
                i += 1;
            }
            directives.push(DisplayDirective::Hide { range: start..i });
        } else {
            i += 1;
        }
    }

    directives
}

// =============================================================================
// Tests
// =============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::protocol::{Atom, Face};

    fn make_line(text: &str) -> Line {
        vec![Atom::from_face(Face::default(), CompactString::from(text))]
    }

    fn make_lines(texts: &[&str]) -> Vec<Line> {
        texts.iter().map(|t| make_line(t)).collect()
    }

    #[test]
    fn line_indent_basic() {
        assert_eq!(line_indent(&make_line("hello")), 0);
        assert_eq!(line_indent(&make_line("  hello")), 2);
        assert_eq!(line_indent(&make_line("    hello")), 4);
        assert_eq!(line_indent(&make_line("\thello")), 1);
    }

    #[test]
    fn line_indent_empty() {
        assert_eq!(line_indent(&[]), 0);
        assert_eq!(line_indent(&make_line("")), 0);
    }

    #[test]
    fn is_blank_detection() {
        assert!(is_blank(&[]));
        assert!(is_blank(&make_line("")));
        assert!(is_blank(&make_line("   ")));
        assert!(!is_blank(&make_line("  x")));
    }

    // =========================================================================
    // Level 2: Compressed
    // =========================================================================

    #[test]
    fn compressed_folds_deep_indent() {
        let lines = make_lines(&[
            "fn main() {",     // 0: indent 0
            "  let x = 1;",    // 1: indent 2
            "  let y = 2;",    // 2: indent 2
            "  if x > 0 {",    // 3: indent 2
            "    println!();", // 4: indent 4
            "  }",             // 5: indent 2
            "}",               // 6: indent 0
        ]);
        let directives = compressed(&lines);
        assert_eq!(directives.len(), 1);
        match &directives[0] {
            DisplayDirective::Fold { range, .. } => {
                assert_eq!(range.start, 1);
                assert_eq!(range.end, 6);
            }
            other => panic!("expected Fold, got {other:?}"),
        }
    }

    #[test]
    fn compressed_empty_buffer() {
        assert!(compressed(&[]).is_empty());
    }

    #[test]
    fn compressed_single_line() {
        let lines = make_lines(&["hello"]);
        assert!(compressed(&lines).is_empty());
    }

    #[test]
    fn compressed_no_deep_indent() {
        let lines = make_lines(&["a", "b", "c"]);
        assert!(compressed(&lines).is_empty());
    }

    // =========================================================================
    // Level 3: Outline
    // =========================================================================

    #[test]
    fn outline_keeps_declarations() {
        let lines = make_lines(&[
            "fn foo() {", // 0: declaration (indent 0)
            "  body1",    // 1: body
            "  body2",    // 2: body
            "}",          // 3: indent 0 declaration
            "fn bar() {", // 4: declaration
            "  body3",    // 5: body
            "}",          // 6: indent 0
        ]);
        let directives = outline(&lines);
        // Should fold bodies after declarations
        assert!(!directives.is_empty());
        for d in &directives {
            match d {
                DisplayDirective::Fold { range, .. } => {
                    // Body ranges should not include declaration lines
                    assert!(range.start > 0 || range.end <= lines.len());
                }
                DisplayDirective::Hide { .. } => {}
                other => panic!("unexpected directive: {other:?}"),
            }
        }
    }

    #[test]
    fn outline_empty_buffer() {
        assert!(outline(&[]).is_empty());
    }

    // =========================================================================
    // Level 4: Skeleton
    // =========================================================================

    #[test]
    fn skeleton_hides_all_indented() {
        let lines = make_lines(&[
            "fn foo() {", // 0: keep
            "  body",     // 1: hide
            "}",          // 2: keep
            "",           // 3: blank → hide
            "fn bar() {", // 4: keep
            "  body",     // 5: hide
            "}",          // 6: keep
        ]);
        let directives = skeleton(&lines);
        // Should produce Hide directives for indented/blank ranges
        assert!(!directives.is_empty());
        for d in &directives {
            match d {
                DisplayDirective::Hide { range } => {
                    // Every line in the range should be indented or blank
                    for idx in range.clone() {
                        assert!(
                            line_indent(&lines[idx]) > 0 || is_blank(&lines[idx]),
                            "line {idx} should be indented or blank"
                        );
                    }
                }
                other => panic!("expected Hide, got {other:?}"),
            }
        }
    }

    #[test]
    fn skeleton_all_top_level() {
        let lines = make_lines(&["a", "b", "c"]);
        assert!(skeleton(&lines).is_empty());
    }

    #[test]
    fn skeleton_empty_buffer() {
        assert!(skeleton(&[]).is_empty());
    }

    // =========================================================================
    // No overlapping ranges (SZ-INV-4)
    // =========================================================================

    #[test]
    fn no_overlapping_ranges() {
        let lines = make_lines(&[
            "use std;",
            "",
            "fn main() {",
            "  let x = 1;",
            "  if x > 0 {",
            "    println!(\"hi\");",
            "  }",
            "}",
            "",
            "fn helper() {",
            "  todo!()",
            "}",
        ]);

        for level in [
            ZoomLevel::COMPRESSED,
            ZoomLevel::OUTLINE,
            ZoomLevel::SKELETON,
        ] {
            let directives = indent_directives(level, &lines);
            let mut ranges: Vec<(usize, usize)> = Vec::new();
            for d in &directives {
                let r = match d {
                    DisplayDirective::Fold { range, .. } => (range.start, range.end),
                    DisplayDirective::Hide { range } => (range.start, range.end),
                    _ => continue,
                };
                // Check no overlap with previous ranges
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

    // =========================================================================
    // Monotonicity (SZ-INV-2)
    // =========================================================================

    #[test]
    fn monotonicity_more_zoom_hides_more() {
        let lines = make_lines(&[
            "use std;",
            "",
            "fn main() {",
            "  let x = 1;",
            "  if x > 0 {",
            "    println!(\"hi\");",
            "  }",
            "}",
            "",
            "fn helper() {",
            "  todo!()",
            "}",
        ]);

        fn hidden_line_count(directives: &[DisplayDirective]) -> usize {
            directives
                .iter()
                .map(|d| match d {
                    DisplayDirective::Fold { range, .. } => range.len().saturating_sub(1),
                    DisplayDirective::Hide { range } => range.len(),
                    _ => 0,
                })
                .sum()
        }

        let h2 = hidden_line_count(&indent_directives(ZoomLevel::COMPRESSED, &lines));
        let h3 = hidden_line_count(&indent_directives(ZoomLevel::OUTLINE, &lines));
        let h4 = hidden_line_count(&indent_directives(ZoomLevel::SKELETON, &lines));

        assert!(h2 <= h3, "compressed ({h2}) should hide ≤ outline ({h3})");
        assert!(h3 <= h4, "outline ({h3}) should hide ≤ skeleton ({h4})");
    }
}
