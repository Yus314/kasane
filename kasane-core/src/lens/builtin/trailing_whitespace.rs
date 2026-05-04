//! Trailing-whitespace lens: highlight whitespace at the end of
//! lines so accidental trailing spaces / tabs are visible.
//!
//! ## Usage
//!
//! ```ignore
//! use std::sync::Arc;
//! use kasane_core::lens::builtin::TrailingWhitespaceLens;
//! use kasane_core::protocol::WireFace;
//!
//! let lens = TrailingWhitespaceLens::new(WireFace::default());
//! let id = lens.id();
//! state.lens_registry.register(Arc::new(lens));
//! state.lens_registry.enable(&id);
//! ```
//!
//! ## What counts as trailing whitespace
//!
//! Bytes are flagged as whitespace via `char::is_whitespace` —
//! covers ASCII space / tab plus the Unicode whitespace set
//! (NBSP, ideographic space, etc.). The flagged range is the
//! longest suffix consisting entirely of whitespace characters.
//!
//! Lines that are entirely whitespace produce a single
//! `StyleInline` covering the whole line; lines with no trailing
//! whitespace produce no directive for that line.
//!
//! ## Per-frame cost
//!
//! O(line-content bytes) over all visible lines — concatenates
//! atom contents per line then walks the resulting string in
//! reverse to find the last non-whitespace byte. Lens output is
//! recomputed every frame (no caching layer in the MVP); a
//! future Salsa cache key `(file_id, line, lens_stack)` would
//! make this incremental.

use crate::display::DisplayDirective;
use crate::lens::{CacheStrategy, Lens, LensId};
use crate::plugin::{AppView, PluginId};
use crate::protocol::WireFace;

/// Highlights trailing whitespace runs on each line.
#[derive(Debug, Clone)]
pub struct TrailingWhitespaceLens {
    style: WireFace,
    name: String,
}

impl TrailingWhitespaceLens {
    /// Construct with the default lens name (`trailing-whitespace`)
    /// and the supplied highlight style. A typical choice is a
    /// faint red background so the marker is noticeable but not
    /// distracting.
    pub fn new(style: WireFace) -> Self {
        Self {
            style,
            name: "trailing-whitespace".into(),
        }
    }

    /// Override the lens name (default `trailing-whitespace`).
    /// Useful when a single embedder wants to register multiple
    /// trailing-whitespace lenses (e.g. distinct styles per
    /// language).
    pub fn with_name(mut self, name: impl Into<String>) -> Self {
        self.name = name.into();
        self
    }
}

impl Lens for TrailingWhitespaceLens {
    fn id(&self) -> LensId {
        LensId::new(PluginId(super::BUILTIN_PLUGIN_ID.into()), self.name.clone())
    }

    fn label(&self) -> String {
        "Trailing whitespace".into()
    }

    fn cache_strategy(&self) -> CacheStrategy {
        // Output depends on line text only — no cursor / selection
        // / syntax reads.
        CacheStrategy::PerBuffer
    }

    fn display(&self, view: &AppView<'_>) -> Vec<DisplayDirective> {
        let lines = view.lines();
        let mut out = Vec::new();
        for (line_idx, atoms) in lines.iter().enumerate() {
            let text: String = atoms.iter().map(|a| a.contents.as_str()).collect();
            let Some(byte_range) = trailing_whitespace_byte_range(&text) else {
                continue;
            };
            out.push(DisplayDirective::StyleInline {
                line: line_idx,
                byte_range,
                face: self.style,
            });
        }
        out
    }
}

/// Compute the byte range of the trailing whitespace run on a
/// line, or `None` if the line has no trailing whitespace.
///
/// The range is `[start, line.len())` where `start` is the byte
/// offset just after the last non-whitespace character. For a
/// line that's entirely whitespace, `start == 0` (the whole line
/// is the trailing run).
fn trailing_whitespace_byte_range(line: &str) -> Option<std::ops::Range<usize>> {
    if line.is_empty() {
        return None;
    }
    // Walk char_indices in reverse and find the last
    // non-whitespace character; the trailing run starts after it.
    let trailing_start = line
        .char_indices()
        .rev()
        .find(|(_, c)| !c.is_whitespace())
        .map(|(idx, c)| idx + c.len_utf8())
        .unwrap_or(0);
    if trailing_start == line.len() {
        // No whitespace at the end.
        None
    } else {
        Some(trailing_start..line.len())
    }
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

    fn run(lines: Vec<Line>) -> Vec<DisplayDirective> {
        let mut state = AppState::default();
        state.observed.lines = StdArc::new(lines);
        let lens = TrailingWhitespaceLens::new(WireFace::default());
        let view = AppView::new(&state);
        lens.display(&view)
    }

    // -----------------------------------------------------------------
    // Range computation — pure helper
    // -----------------------------------------------------------------

    #[test]
    fn empty_line_yields_no_range() {
        assert_eq!(trailing_whitespace_byte_range(""), None);
    }

    #[test]
    fn line_without_trailing_whitespace_yields_no_range() {
        assert_eq!(trailing_whitespace_byte_range("hello"), None);
        assert_eq!(trailing_whitespace_byte_range("a b c"), None);
    }

    #[test]
    fn single_trailing_space_yields_one_byte_range() {
        assert_eq!(trailing_whitespace_byte_range("hello "), Some(5..6));
    }

    #[test]
    fn multiple_trailing_spaces_yield_full_run() {
        assert_eq!(trailing_whitespace_byte_range("hi   "), Some(2..5));
    }

    #[test]
    fn trailing_tab_is_flagged() {
        assert_eq!(trailing_whitespace_byte_range("hi\t"), Some(2..3));
    }

    #[test]
    fn mixed_trailing_whitespace_is_flagged() {
        assert_eq!(trailing_whitespace_byte_range("hi \t \t"), Some(2..6));
    }

    #[test]
    fn line_of_only_whitespace_yields_full_line() {
        assert_eq!(trailing_whitespace_byte_range("    "), Some(0..4));
        assert_eq!(trailing_whitespace_byte_range("\t\t"), Some(0..2));
    }

    #[test]
    fn cjk_text_with_trailing_space_uses_byte_offsets() {
        // 日本語 = 9 bytes (3 chars × 3 bytes each); trailing " " = 1 byte
        assert_eq!(trailing_whitespace_byte_range("日本語 "), Some(9..10));
    }

    #[test]
    fn unicode_whitespace_nbsp_is_flagged() {
        // NBSP = U+00A0 (2 bytes UTF-8)
        assert_eq!(trailing_whitespace_byte_range("hi\u{00A0}"), Some(2..4));
    }

    #[test]
    fn interior_whitespace_does_not_count_as_trailing() {
        // Space in the middle but not at the end.
        assert_eq!(trailing_whitespace_byte_range("a b"), None);
        // Space in the middle AND at the end — only the trailing run flagged.
        assert_eq!(trailing_whitespace_byte_range("a b "), Some(3..4));
    }

    // -----------------------------------------------------------------
    // Lens display() integration
    // -----------------------------------------------------------------

    #[test]
    fn empty_buffer_produces_no_directives() {
        let dirs = run(vec![]);
        assert!(dirs.is_empty());
    }

    #[test]
    fn buffer_with_clean_lines_produces_no_directives() {
        let dirs = run(vec![line_of("hello"), line_of("world")]);
        assert!(dirs.is_empty());
    }

    #[test]
    fn line_with_trailing_space_produces_style_inline() {
        let dirs = run(vec![line_of("hello   ")]);
        assert_eq!(dirs.len(), 1);
        match &dirs[0] {
            DisplayDirective::StyleInline {
                line, byte_range, ..
            } => {
                assert_eq!(*line, 0);
                assert_eq!(byte_range.clone(), 5..8);
            }
            other => panic!("expected StyleInline, got {other:?}"),
        }
    }

    #[test]
    fn mixed_buffer_flags_only_dirty_lines() {
        let dirs = run(vec![
            line_of("clean"),
            line_of("dirty   "),
            line_of("also clean"),
            line_of("\t\t"),
        ]);
        assert_eq!(dirs.len(), 2, "only lines 1 and 3 are flagged");
        let lines: Vec<usize> = dirs
            .iter()
            .map(|d| match d {
                DisplayDirective::StyleInline { line, .. } => *line,
                _ => unreachable!(),
            })
            .collect();
        assert_eq!(lines, vec![1, 3]);
    }

    #[test]
    fn multi_atom_line_concatenates_correctly() {
        // Two atoms; trailing whitespace lives entirely in the
        // second atom. The byte range is computed against the
        // concatenated line text.
        let line: Line = vec![Atom::plain("hello"), Atom::plain("   ")];
        let dirs = run(vec![line]);
        assert_eq!(dirs.len(), 1);
        match &dirs[0] {
            DisplayDirective::StyleInline { byte_range, .. } => {
                assert_eq!(byte_range.clone(), 5..8);
            }
            _ => unreachable!(),
        }
    }

    // -----------------------------------------------------------------
    // Lens trait surface
    // -----------------------------------------------------------------

    #[test]
    fn id_uses_builtin_plugin_namespace() {
        let lens = TrailingWhitespaceLens::new(WireFace::default());
        let id = lens.id();
        assert_eq!(id.plugin.0, "kasane.builtin");
        assert_eq!(id.name, "trailing-whitespace");
    }

    #[test]
    fn with_name_overrides_lens_name() {
        let lens = TrailingWhitespaceLens::new(WireFace::default()).with_name("ws-rust");
        assert_eq!(lens.id().name, "ws-rust");
    }

    #[test]
    fn label_is_human_readable() {
        let lens = TrailingWhitespaceLens::new(WireFace::default());
        assert_eq!(lens.label(), "Trailing whitespace");
    }

    #[test]
    fn priority_defaults_to_zero() {
        let lens = TrailingWhitespaceLens::new(WireFace::default());
        assert_eq!(lens.priority(), 0);
    }
}
