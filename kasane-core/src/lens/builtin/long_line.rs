//! Long-line lens: highlight characters past a configurable
//! column threshold (typical use: flag code that exceeds a
//! style-guide line-length limit, e.g. 80 / 100 / 120 columns).
//!
//! ## Usage
//!
//! ```ignore
//! use std::sync::Arc;
//! use kasane_core::lens::builtin::LongLineLens;
//! use kasane_core::protocol::WireFace;
//!
//! let lens = LongLineLens::new(80, WireFace::default());
//! let id = lens.id();
//! state.lens_registry.register(Arc::new(lens));
//! state.lens_registry.enable(&id);
//! ```
//!
//! ## What counts as "past the threshold"
//!
//! Characters are counted as **Unicode scalar values**
//! (`str::chars()`), one per code point. ASCII rules are exact:
//! a column-80 lens flags character 81 onward. CJK and
//! double-width characters are counted as one column each (so a
//! line of 50 CJK characters does *not* trip a column-80 lens
//! even though it displays as 100 cells); this matches the
//! common Rust/Go style guide convention of "100 chars" rather
//! than "100 display cells", and keeps the lens cheap (no
//! `unicode-width` dependency). A future lens variant can swap
//! in display-width counting if a use case appears.
//!
//! Combining marks (e.g. `e\u{301}` = `é`) count as separate
//! characters. Source code rarely uses combining marks
//! intentionally; the simplification is documented but not
//! enforced.
//!
//! ## What gets flagged
//!
//! For each line longer than the threshold, the lens emits a
//! single `StyleInline` covering the byte range from the
//! `(threshold + 1)`-th character to the line end. Lines at or
//! under the threshold produce no directive.

use crate::display::DisplayDirective;
use crate::lens::{CacheStrategy, Lens, LensId};
use crate::plugin::{AppView, PluginId};
use crate::protocol::WireFace;

/// Highlights characters past `column` on each line. `column` is
/// 1-indexed in the user-facing sense ("column 80") but the
/// implementation uses 0-indexed character counts internally;
/// `LongLineLens::new(80, ...)` flags character 81 onward.
#[derive(Debug, Clone)]
pub struct LongLineLens {
    threshold_chars: u32,
    style: WireFace,
    name: String,
}

impl LongLineLens {
    /// Construct with the given threshold (1-indexed column) and
    /// highlight style. A typical threshold is 80, 100, or 120
    /// depending on the project style guide.
    ///
    /// `column == 0` is a degenerate setting that flags every
    /// non-empty line in its entirety; the constructor accepts
    /// it without checking — the embedder is responsible for
    /// validation.
    pub fn new(column: u32, style: WireFace) -> Self {
        Self {
            threshold_chars: column,
            style,
            name: format!("long-line-{column}"),
        }
    }

    /// Override the lens name (default `long-line-{column}`).
    /// Useful when registering multiple long-line lenses with
    /// different thresholds (e.g. one at 80 for warning, one at
    /// 120 for error).
    pub fn with_name(mut self, name: impl Into<String>) -> Self {
        self.name = name.into();
        self
    }

    /// The column threshold this lens enforces.
    pub fn threshold(&self) -> u32 {
        self.threshold_chars
    }
}

impl Lens for LongLineLens {
    fn id(&self) -> LensId {
        LensId::new(PluginId(super::BUILTIN_PLUGIN_ID.into()), self.name.clone())
    }

    fn label(&self) -> String {
        format!("Long line (> {})", self.threshold_chars)
    }

    fn cache_strategy(&self) -> CacheStrategy {
        // Output depends on line text only.
        CacheStrategy::PerBuffer
    }

    fn display(&self, view: &AppView<'_>) -> Vec<DisplayDirective> {
        let lines = view.lines();
        let mut out = Vec::new();
        for (line_idx, atoms) in lines.iter().enumerate() {
            let text: String = atoms.iter().map(|a| a.contents.as_str()).collect();
            let Some(byte_range) = past_threshold_byte_range(&text, self.threshold_chars) else {
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

/// Compute the byte range of the run starting at the
/// `(threshold + 1)`-th character and extending to line end, or
/// `None` if the line is shorter than or equal to the threshold.
///
/// Counts characters as Unicode scalar values (one per
/// code point); see module docs for the CJK / display-width
/// caveat.
fn past_threshold_byte_range(line: &str, threshold_chars: u32) -> Option<std::ops::Range<usize>> {
    let mut chars = line.char_indices();
    // Advance threshold characters; if the line runs out, no
    // overflow.
    for _ in 0..threshold_chars {
        chars.next()?;
    }
    // The next char (if any) is the first character past the
    // threshold; flag from its byte offset to line end.
    chars.next().map(|(idx, _)| idx..line.len())
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

    fn run(threshold: u32, lines: Vec<Line>) -> Vec<DisplayDirective> {
        let mut state = AppState::default();
        state.observed.lines = StdArc::new(lines);
        let lens = LongLineLens::new(threshold, WireFace::default());
        let view = AppView::new(&state);
        lens.display(&view)
    }

    // -----------------------------------------------------------------
    // Range computation — pure helper
    // -----------------------------------------------------------------

    #[test]
    fn line_shorter_than_threshold_yields_no_range() {
        assert_eq!(past_threshold_byte_range("hello", 10), None);
        assert_eq!(past_threshold_byte_range("", 0), None);
        assert_eq!(past_threshold_byte_range("", 1), None);
    }

    #[test]
    fn line_exactly_at_threshold_yields_no_range() {
        assert_eq!(past_threshold_byte_range("hello", 5), None);
    }

    #[test]
    fn one_char_past_threshold_flagged() {
        // "hello!" — 6 chars; threshold 5 → flag char index 5
        // (byte offset 5) to end (byte offset 6).
        assert_eq!(past_threshold_byte_range("hello!", 5), Some(5..6));
    }

    #[test]
    fn many_chars_past_threshold_flagged() {
        let line = "a".repeat(100);
        // threshold 80 → flag from byte 80 to byte 100.
        assert_eq!(past_threshold_byte_range(&line, 80), Some(80..100));
    }

    #[test]
    fn cjk_chars_count_one_each_not_two() {
        // 5 CJK chars (15 bytes total), threshold 5: flag nothing
        // (line is exactly at threshold by char count).
        assert_eq!(past_threshold_byte_range("あいうえお", 5), None);
        // 6 CJK chars, threshold 5: flag the 6th onward (byte
        // offset 15 = 5 chars × 3 bytes).
        assert_eq!(past_threshold_byte_range("あいうえおか", 5), Some(15..18));
    }

    #[test]
    fn threshold_zero_flags_entire_non_empty_line() {
        assert_eq!(past_threshold_byte_range("hi", 0), Some(0..2));
        assert_eq!(past_threshold_byte_range("", 0), None);
    }

    #[test]
    fn mixed_ascii_and_cjk_byte_offset_correct() {
        // "abc" + "あい" = 3 ASCII + 2 CJK = 5 chars, 9 bytes.
        // threshold 4 → flag the 5th char ('い'), byte offset
        // 3+3 = 6 to byte 9.
        assert_eq!(past_threshold_byte_range("abcあい", 4), Some(6..9));
    }

    // -----------------------------------------------------------------
    // Lens display() integration
    // -----------------------------------------------------------------

    #[test]
    fn empty_buffer_produces_no_directives() {
        let dirs = run(80, vec![]);
        assert!(dirs.is_empty());
    }

    #[test]
    fn buffer_with_short_lines_produces_no_directives() {
        let dirs = run(80, vec![line_of("hello"), line_of("world")]);
        assert!(dirs.is_empty());
    }

    #[test]
    fn long_line_produces_style_inline_for_overflow() {
        let line = "x".repeat(85);
        let dirs = run(80, vec![line_of(&line)]);
        assert_eq!(dirs.len(), 1);
        match &dirs[0] {
            DisplayDirective::StyleInline {
                line, byte_range, ..
            } => {
                assert_eq!(*line, 0);
                assert_eq!(byte_range.clone(), 80..85);
            }
            other => panic!("expected StyleInline, got {other:?}"),
        }
    }

    #[test]
    fn mixed_buffer_flags_only_overflow_lines() {
        let short = line_of("short");
        let long = line_of(&"x".repeat(90));
        let exact = line_of(&"y".repeat(80));
        let dirs = run(80, vec![short, long, exact]);
        assert_eq!(dirs.len(), 1, "only the long line is flagged");
        match &dirs[0] {
            DisplayDirective::StyleInline { line, .. } => assert_eq!(*line, 1),
            _ => unreachable!(),
        }
    }

    #[test]
    fn multi_atom_long_line_concatenates_for_count() {
        // 50 chars in atom 1 + 50 chars in atom 2 = 100 chars,
        // threshold 80 → flag from char 80 (byte 80) to end
        // (byte 100).
        let line: Line = vec![Atom::plain(&"a".repeat(50)), Atom::plain(&"b".repeat(50))];
        let dirs = run(80, vec![line]);
        assert_eq!(dirs.len(), 1);
        match &dirs[0] {
            DisplayDirective::StyleInline { byte_range, .. } => {
                assert_eq!(byte_range.clone(), 80..100);
            }
            _ => unreachable!(),
        }
    }

    // -----------------------------------------------------------------
    // Lens trait surface
    // -----------------------------------------------------------------

    #[test]
    fn id_uses_builtin_plugin_namespace_with_threshold_in_name() {
        let lens = LongLineLens::new(80, WireFace::default());
        let id = lens.id();
        assert_eq!(id.plugin.0, "kasane.builtin");
        assert_eq!(id.name, "long-line-80");
    }

    #[test]
    fn with_name_overrides_lens_name() {
        let lens = LongLineLens::new(120, WireFace::default()).with_name("style-guide-error");
        assert_eq!(lens.id().name, "style-guide-error");
    }

    #[test]
    fn label_includes_threshold() {
        let lens = LongLineLens::new(100, WireFace::default());
        assert_eq!(lens.label(), "Long line (> 100)");
    }

    #[test]
    fn threshold_accessor_returns_construction_value() {
        let lens = LongLineLens::new(120, WireFace::default());
        assert_eq!(lens.threshold(), 120);
    }

    #[test]
    fn priority_defaults_to_zero() {
        let lens = LongLineLens::new(80, WireFace::default());
        assert_eq!(lens.priority(), 0);
    }

    #[test]
    fn two_long_line_lenses_with_different_thresholds_coexist() {
        // Distinct thresholds → distinct default names → both
        // can register without conflict.
        let warn = LongLineLens::new(80, WireFace::default());
        let err = LongLineLens::new(120, WireFace::default());
        assert_ne!(warn.id(), err.id());
    }
}
