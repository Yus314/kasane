//! Indent guides lens: highlight every Nth column in a line's
//! leading whitespace so indentation structure is visually
//! traceable.
//!
//! ## Usage
//!
//! ```ignore
//! use std::sync::Arc;
//! use kasane_core::lens::builtin::IndentGuidesLens;
//! use kasane_core::protocol::WireFace;
//!
//! let lens = IndentGuidesLens::new(4, WireFace::default());
//! let id = lens.id();
//! state.lens_registry.register(Arc::new(lens));
//! state.lens_registry.enable(&id);
//! ```
//!
//! ## What gets flagged
//!
//! For each line whose leading whitespace consists entirely of
//! ASCII space characters (`b' '`), the lens emits one
//! `StyleInline` per *indent column* — the byte at offset 0,
//! `indent_width`, `2 * indent_width`, etc., up to the last
//! complete indent level. Each marker covers a single byte
//! (the space character at that column).
//!
//! The visual outcome depends on the supplied style: a contrasting
//! background colour produces a column of coloured cells that
//! reads as a vertical guide; a faint foreground colour with a
//! subtle character substitution would require a different
//! mechanism (the lens MVP doesn't substitute glyphs).
//!
//! ## What gets skipped
//!
//! - **Empty lines**: no leading whitespace, no markers.
//! - **No-indent lines**: leading-whitespace run is zero bytes.
//! - **Tab-indented lines**: any tab in the leading-whitespace
//!   run is fatal — the lens treats the whole leading run as
//!   non-space-indented and emits zero markers for that line.
//!   This avoids the tab-width controversy (different editors
//!   render tabs at 2/4/8 cells); a future `tab-aware` variant
//!   can layer on top once the project picks a tab semantics.
//! - **Partial indent runs** at the end (e.g. 6 spaces with
//!   indent-width 4): the trailing partial level (cols 4..6)
//!   gets no marker; only complete indent levels are flagged.
//!
//! ## Per-frame cost
//!
//! O(visible-line-count × leading-whitespace-bytes) over all
//! visible lines. The leading-whitespace scan stops at the first
//! non-space byte, so deeply-nested code is the worst case;
//! typical source files allocate a couple of bytes per line.
//! Per the Composable Lenses MVP, output is recomputed every
//! frame.

use crate::display::DisplayDirective;
use crate::lens::{Lens, LensId};
use crate::plugin::{AppView, PluginId};
use crate::protocol::WireFace;

/// Highlights indent columns on each line. `indent_width` is the
/// number of space characters per indent level (typical: 2 or 4).
#[derive(Debug, Clone)]
pub struct IndentGuidesLens {
    indent_width: u32,
    style: WireFace,
    name: String,
}

impl IndentGuidesLens {
    /// Construct with the given indent width (spaces per level)
    /// and highlight style. `indent_width == 0` is a degenerate
    /// setting — the lens emits no markers at all in that case
    /// (the "every 0 bytes" loop terminates immediately); the
    /// constructor accepts it without checking.
    pub fn new(indent_width: u32, style: WireFace) -> Self {
        Self {
            indent_width,
            style,
            name: format!("indent-guides-{indent_width}"),
        }
    }

    /// Override the lens name (default
    /// `indent-guides-{indent_width}`). Useful when the embedder
    /// registers multiple instances (e.g. one for source code,
    /// one for nested-list rendering).
    pub fn with_name(mut self, name: impl Into<String>) -> Self {
        self.name = name.into();
        self
    }

    /// The indent width this lens uses.
    pub fn indent_width(&self) -> u32 {
        self.indent_width
    }
}

impl Lens for IndentGuidesLens {
    fn id(&self) -> LensId {
        LensId::new(PluginId(super::BUILTIN_PLUGIN_ID.into()), self.name.clone())
    }

    fn label(&self) -> String {
        format!("Indent guides ({} sp)", self.indent_width)
    }

    fn display(&self, view: &AppView<'_>) -> Vec<DisplayDirective> {
        if self.indent_width == 0 {
            return Vec::new();
        }
        let lines = view.lines();
        let mut out = Vec::new();
        for (line_idx, atoms) in lines.iter().enumerate() {
            let text: String = atoms.iter().map(|a| a.contents.as_str()).collect();
            for col in indent_guide_columns(&text, self.indent_width) {
                out.push(DisplayDirective::StyleInline {
                    line: line_idx,
                    byte_range: col..col + 1,
                    face: self.style,
                });
            }
        }
        out
    }
}

/// Compute the byte offsets of indent-guide columns on a line.
///
/// Returns the byte indices `0, indent_width, 2*indent_width, ...`
/// that fall within the line's all-space leading run. Returns an
/// empty vec for:
/// - lines without space-only leading whitespace (incl. lines
///   whose leading run contains a tab),
/// - lines shorter than `indent_width` (no complete level),
/// - any line when `indent_width == 0`.
fn indent_guide_columns(line: &str, indent_width: u32) -> Vec<usize> {
    if indent_width == 0 {
        return Vec::new();
    }
    let bytes = line.as_bytes();
    // Count leading space-only bytes; abort if a non-space appears
    // before the first non-whitespace boundary OR if the run
    // contains a tab (treat tab-prefixed lines as opt-out per the
    // module docs).
    let mut leading = 0usize;
    while leading < bytes.len() {
        match bytes[leading] {
            b' ' => leading += 1,
            b'\t' => return Vec::new(),
            _ => break,
        }
    }
    if leading == 0 {
        return Vec::new();
    }
    let step = indent_width as usize;
    let mut cols = Vec::new();
    let mut col = 0usize;
    while col < leading {
        cols.push(col);
        col += step;
    }
    cols
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::protocol::{Atom, Line};
    use crate::state::AppState;
    use std::sync::Arc as StdArc;

    fn line_of(text: &str) -> Line {
        vec![Atom::plain(text)]
    }

    fn run(indent_width: u32, lines: Vec<Line>) -> Vec<DisplayDirective> {
        let mut state = AppState::default();
        state.observed.lines = StdArc::new(lines);
        let lens = IndentGuidesLens::new(indent_width, WireFace::default());
        let view = AppView::new(&state);
        lens.display(&view)
    }

    fn marker_lines_and_cols(dirs: &[DisplayDirective]) -> Vec<(usize, usize)> {
        dirs.iter()
            .map(|d| match d {
                DisplayDirective::StyleInline {
                    line, byte_range, ..
                } => (*line, byte_range.start),
                _ => panic!("expected StyleInline"),
            })
            .collect()
    }

    // -----------------------------------------------------------------
    // Column computation — pure helper
    // -----------------------------------------------------------------

    #[test]
    fn empty_line_yields_no_columns() {
        assert_eq!(indent_guide_columns("", 4), Vec::<usize>::new());
    }

    #[test]
    fn line_with_no_leading_space_yields_no_columns() {
        assert_eq!(indent_guide_columns("hello", 4), Vec::<usize>::new());
    }

    #[test]
    fn one_indent_level_yields_one_column_at_zero() {
        // "    code" = 4 spaces + "code"; indent_width 4 →
        // marker at byte 0.
        assert_eq!(indent_guide_columns("    code", 4), vec![0]);
    }

    #[test]
    fn two_indent_levels_yield_two_columns() {
        // 8 spaces + content; indent_width 4 → cols 0, 4.
        assert_eq!(indent_guide_columns("        code", 4), vec![0, 4]);
    }

    #[test]
    fn three_indent_levels_yield_three_columns() {
        // 12 spaces; indent_width 4 → cols 0, 4, 8.
        assert_eq!(indent_guide_columns("            x", 4), vec![0, 4, 8]);
    }

    #[test]
    fn partial_indent_at_end_drops_incomplete_level() {
        // 6 spaces; indent_width 4 → only col 0 fits a complete
        // level (cols 0..4); the partial run (cols 4..6) is
        // dropped.
        assert_eq!(indent_guide_columns("      x", 4), vec![0, 4]);
        // 7 spaces, indent_width 4 → cols 0, 4 (col 8 doesn't
        // fit; only cols where col+1 <= leading qualify).
        assert_eq!(indent_guide_columns("       x", 4), vec![0, 4]);
    }

    #[test]
    fn tab_in_leading_run_aborts_marker_emission() {
        // Tab-prefixed line — opt out entirely.
        assert_eq!(indent_guide_columns("\tcode", 4), Vec::<usize>::new());
        assert_eq!(indent_guide_columns("  \tcode", 4), Vec::<usize>::new());
        assert_eq!(indent_guide_columns("\t  code", 4), Vec::<usize>::new());
    }

    #[test]
    fn whitespace_only_line_flags_complete_levels() {
        // 8 spaces with no content; cols 0, 4 are markers.
        // (The 4..8 range is a complete level — col 4 + 1 = 5
        // ≤ 8 — so col 4 also gets a marker.)
        assert_eq!(indent_guide_columns("        ", 4), vec![0, 4]);
    }

    #[test]
    fn indent_width_two_yields_doubled_column_count() {
        // 8 spaces; indent_width 2 → cols 0, 2, 4, 6.
        assert_eq!(indent_guide_columns("        x", 2), vec![0, 2, 4, 6]);
    }

    #[test]
    fn indent_width_zero_yields_no_columns() {
        assert_eq!(indent_guide_columns("    code", 0), Vec::<usize>::new());
    }

    #[test]
    fn indent_width_larger_than_leading_yields_only_col_zero() {
        // 4 spaces, indent_width 8 → col 0 fits (0..1 ≤ 4),
        // but col 8 doesn't.
        assert_eq!(indent_guide_columns("    x", 8), vec![0]);
    }

    // -----------------------------------------------------------------
    // Lens display() integration
    // -----------------------------------------------------------------

    #[test]
    fn empty_buffer_produces_no_directives() {
        assert!(run(4, vec![]).is_empty());
    }

    #[test]
    fn unindented_buffer_produces_no_directives() {
        let dirs = run(4, vec![line_of("foo"), line_of("bar")]);
        assert!(dirs.is_empty());
    }

    #[test]
    fn single_indented_line_produces_marker_at_col_zero() {
        let dirs = run(4, vec![line_of("    code")]);
        assert_eq!(dirs.len(), 1);
        match &dirs[0] {
            DisplayDirective::StyleInline {
                line, byte_range, ..
            } => {
                assert_eq!(*line, 0);
                assert_eq!(byte_range.clone(), 0..1);
            }
            other => panic!("expected StyleInline, got {other:?}"),
        }
    }

    #[test]
    fn deeply_indented_line_produces_one_marker_per_level() {
        // 12 spaces + content → cols 0, 4, 8 → 3 markers.
        let dirs = run(4, vec![line_of("            content")]);
        assert_eq!(dirs.len(), 3);
        assert_eq!(marker_lines_and_cols(&dirs), vec![(0, 0), (0, 4), (0, 8)],);
    }

    #[test]
    fn mixed_buffer_emits_markers_per_indent_level() {
        let dirs = run(
            4,
            vec![
                line_of("top"),               // line 0: no markers
                line_of("    one"),           // line 1: col 0
                line_of("        two"),       // line 2: cols 0, 4
                line_of("            three"), // line 3: cols 0, 4, 8
                line_of("\t  tab-prefixed"),  // line 4: opted out
            ],
        );
        assert_eq!(
            marker_lines_and_cols(&dirs),
            vec![(1, 0), (2, 0), (2, 4), (3, 0), (3, 4), (3, 8)],
        );
    }

    #[test]
    fn multi_atom_indented_line_concatenates_for_leading_run() {
        // First atom carries the spaces; second carries content.
        let line: Line = vec![Atom::plain("        "), Atom::plain("payload")];
        let dirs = run(4, vec![line]);
        assert_eq!(marker_lines_and_cols(&dirs), vec![(0, 0), (0, 4)],);
    }

    #[test]
    fn tab_only_line_produces_no_markers() {
        let dirs = run(4, vec![line_of("\t\tcontent")]);
        assert!(dirs.is_empty());
    }

    // -----------------------------------------------------------------
    // Lens trait surface
    // -----------------------------------------------------------------

    #[test]
    fn id_uses_builtin_plugin_namespace_with_indent_width_in_name() {
        let lens = IndentGuidesLens::new(4, WireFace::default());
        let id = lens.id();
        assert_eq!(id.plugin.0, "kasane.builtin");
        assert_eq!(id.name, "indent-guides-4");
    }

    #[test]
    fn with_name_overrides_lens_name() {
        let lens = IndentGuidesLens::new(2, WireFace::default()).with_name("yaml-guides");
        assert_eq!(lens.id().name, "yaml-guides");
    }

    #[test]
    fn label_includes_indent_width() {
        let lens = IndentGuidesLens::new(4, WireFace::default());
        assert_eq!(lens.label(), "Indent guides (4 sp)");
    }

    #[test]
    fn indent_width_accessor_returns_construction_value() {
        let lens = IndentGuidesLens::new(8, WireFace::default());
        assert_eq!(lens.indent_width(), 8);
    }

    #[test]
    fn priority_defaults_to_zero() {
        let lens = IndentGuidesLens::new(4, WireFace::default());
        assert_eq!(lens.priority(), 0);
    }

    #[test]
    fn two_indent_lenses_with_different_widths_coexist() {
        let two = IndentGuidesLens::new(2, WireFace::default());
        let four = IndentGuidesLens::new(4, WireFace::default());
        assert_ne!(two.id(), four.id());
    }
}
