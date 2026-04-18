//! Core-maintained fold toggle state.
//!
//! Tracks which fold ranges are currently expanded (toggled open). Consulted
//! during `collect_display_map()` to filter out expanded folds before building
//! the `DisplayMap`.

use std::ops::Range;

use super::DisplayDirective;

/// Tracks fold ranges that have been expanded via user interaction.
///
/// When a fold range is expanded, the corresponding `Fold` directive is
/// filtered out during `DisplayMap` construction, causing the folded lines
/// to appear as individual display lines.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct FoldToggleState {
    expanded: Vec<Range<usize>>,
}

impl FoldToggleState {
    /// Create an empty fold toggle state (const-compatible).
    pub const fn empty() -> Self {
        Self {
            expanded: Vec::new(),
        }
    }

    /// Toggle a fold range: expand if collapsed, collapse if expanded.
    pub fn toggle(&mut self, range: &Range<usize>) {
        if let Some(pos) = self.expanded.iter().position(|r| r == range) {
            self.expanded.swap_remove(pos);
        } else {
            self.expanded.push(range.clone());
        }
    }

    /// Whether the given range is currently expanded.
    pub fn is_expanded(&self, range: &Range<usize>) -> bool {
        self.expanded.iter().any(|r| r == range)
    }

    /// Remove fold directives whose ranges are currently expanded.
    ///
    /// Non-fold directives (Hide) are preserved.
    pub fn filter_directives(&self, directives: &mut Vec<DisplayDirective>) {
        if self.expanded.is_empty() {
            return;
        }
        directives.retain(|d| {
            if let DisplayDirective::Fold { range, .. } = d {
                !self.is_expanded(range)
            } else {
                true
            }
        });
    }

    /// Clear all toggle state.
    pub fn clear(&mut self) {
        self.expanded.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::protocol::{Atom, Face};

    #[test]
    fn toggle_expands_fold() {
        let mut state = FoldToggleState::default();
        state.toggle(&(2..5));
        assert!(state.is_expanded(&(2..5)));
    }

    #[test]
    fn toggle_twice_collapses() {
        let mut state = FoldToggleState::default();
        state.toggle(&(2..5));
        state.toggle(&(2..5));
        assert!(!state.is_expanded(&(2..5)));
    }

    #[test]
    fn filter_removes_expanded_fold() {
        let mut state = FoldToggleState::default();
        state.toggle(&(2..5));

        let mut directives = vec![
            DisplayDirective::Fold {
                range: 2..5,
                summary: vec![Atom {
                    face: Face::default(),
                    contents: "fold".into(),
                }],
            },
            DisplayDirective::Hide { range: 6..8 },
        ];
        state.filter_directives(&mut directives);

        assert_eq!(directives.len(), 1);
        assert!(matches!(directives[0], DisplayDirective::Hide { .. }));
    }

    #[test]
    fn filter_preserves_non_expanded() {
        let mut state = FoldToggleState::default();
        state.toggle(&(10..15)); // different range

        let mut directives = vec![DisplayDirective::Fold {
            range: 2..5,
            summary: vec![Atom {
                face: Face::default(),
                contents: "fold".into(),
            }],
        }];
        state.filter_directives(&mut directives);

        assert_eq!(directives.len(), 1);
    }

    #[test]
    fn filter_preserves_non_fold_directives() {
        let mut state = FoldToggleState::default();
        state.toggle(&(0..10)); // expand everything

        let mut directives = vec![DisplayDirective::Hide { range: 3..5 }];
        state.filter_directives(&mut directives);

        assert_eq!(directives.len(), 1);
    }

    #[test]
    fn clear_resets_all() {
        let mut state = FoldToggleState::default();
        state.toggle(&(2..5));
        state.toggle(&(10..15));
        state.clear();
        assert!(!state.is_expanded(&(2..5)));
        assert!(!state.is_expanded(&(10..15)));
    }
}
