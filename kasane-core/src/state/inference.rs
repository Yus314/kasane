//! Read-only projection of `AppState` onto `#[epistemic(derived)]` and
//! `#[epistemic(heuristic)]` fields.
//!
//! `Inference<'a>` is the Level 2 (derived/heuristic) counterpart to
//! [`Truth<'a>`](super::truth::Truth) under ADR-030. It realises the
//! projection
//!
//! ```text
//! i : AppState → InferredFacts
//! i(s) = extract_derived_and_heuristic(s)
//! ```
//!
//! formalised in `docs/semantics.md` §2.5 (World Model) as the `I` component
//! of `W = (T, I, Π, S)`. Axiom A8 (Inference Boundedness) additionally
//! asserts that `i(s)` depends on `s` only through the pair
//! `(truth(s), policy(s))` — i.e. Inference never directly reads session or
//! runtime state.
//!
//! # Invariants
//!
//! - Every accessor returns a field carrying `#[epistemic(derived)]` or
//!   `#[epistemic(heuristic)]` on `AppState` (structurally witnessed by
//!   `state/tests/inference.rs`).
//! - `Inference<'a>` is `Copy`.
//! - Construction requires `&AppState`; no accessor returns an `&mut`
//!   reference. Writing through `Inference` is a compile error.

use crate::protocol::{CursorMode, Line};
use crate::render::color_context::ColorContext;
use crate::state::derived::{EditorMode, Selection};
use crate::state::{AppState, Coord};

/// Read-only projection of `AppState` onto its derived + heuristic fields.
///
/// See module-level documentation for the enforcement contract.
#[derive(Clone, Copy)]
pub struct Inference<'a> {
    state: &'a AppState,
}

impl<'a> Inference<'a> {
    /// Create a new `Inference` projection over the given state.
    #[inline]
    pub fn new(state: &'a AppState) -> Self {
        Self { state }
    }

    // =========================================================================
    // Derived fields
    // =========================================================================

    /// Derived: per-line dirty flags computed by diffing old vs new `lines`
    /// (rule R-3).
    #[inline]
    pub fn lines_dirty(&self) -> &'a [bool] {
        &self.state.lines_dirty
    }

    /// Derived: cursor mode inferred from `status_content_cursor_pos` sign
    /// (rule I-3).
    #[inline]
    pub fn cursor_mode(&self) -> CursorMode {
        self.state.cursor_mode
    }

    /// Derived: concatenation of `status_prompt` + `status_content` used by
    /// the status bar renderer.
    #[inline]
    pub fn status_line(&self) -> &'a Line {
        &self.state.status_line
    }

    /// Derived: parsed editor mode from `cursor_mode` + `status_mode_line`
    /// (rule I-2).
    #[inline]
    pub fn editor_mode(&self) -> EditorMode {
        self.state.editor_mode
    }

    /// Derived: color context (light/dark classification) inferred from
    /// `default_face` luminance.
    #[inline]
    pub fn color_context(&self) -> &'a ColorContext {
        &self.state.color_context
    }

    // =========================================================================
    // Heuristic fields
    // =========================================================================

    /// Heuristic: total cursor count detected from buffer atom attributes
    /// (rule I-1, severity: degraded).
    #[inline]
    pub fn cursor_count(&self) -> usize {
        self.state.cursor_count
    }

    /// Heuristic: positions of secondary cursors, filtered to exclude the
    /// primary cursor (rule I-1, severity: degraded).
    #[inline]
    pub fn secondary_cursors(&self) -> &'a [Coord] {
        &self.state.secondary_cursors
    }

    /// Heuristic: detected selection ranges from buffer atoms (rule I-7,
    /// severity: degraded).
    #[inline]
    pub fn selections(&self) -> &'a [Selection] {
        &self.state.selections
    }

    // =========================================================================
    // Structural witness
    // =========================================================================

    /// Names of every accessor on `Inference`, in the order they are defined.
    ///
    /// Used by `state/tests/inference.rs` to witness — structurally — that
    /// the accessor set matches the union of `#[epistemic(derived)]` and
    /// `#[epistemic(heuristic)]` fields on `AppState`.
    pub const INFERENCE_ACCESSOR_NAMES: &'static [&'static str] = &[
        "lines_dirty",
        "cursor_mode",
        "status_line",
        "editor_mode",
        "color_context",
        "cursor_count",
        "secondary_cursors",
        "selections",
    ];
}

impl AppState {
    /// Read-only projection onto derived + heuristic fields.
    ///
    /// See [`Inference`] for the enforcement contract.
    #[inline]
    pub fn inference(&self) -> Inference<'_> {
        Inference::new(self)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn inference_is_copy() {
        fn assert_copy<T: Copy>() {}
        assert_copy::<Inference<'_>>();
    }

    #[test]
    fn construction_roundtrips() {
        let mut state = AppState::default();
        state.cursor_count = 3;
        state.secondary_cursors = vec![Coord { line: 1, column: 1 }];
        let inference = state.inference();
        assert_eq!(inference.cursor_count(), 3);
        assert_eq!(inference.secondary_cursors().len(), 1);
        assert!(inference.lines_dirty().is_empty());
        assert!(inference.selections().is_empty());
    }

    #[test]
    fn accessor_names_nonempty_and_unique() {
        let names = Inference::INFERENCE_ACCESSOR_NAMES;
        assert!(!names.is_empty());
        let mut sorted = names.to_vec();
        sorted.sort_unstable();
        sorted.dedup();
        assert_eq!(sorted.len(), names.len(), "accessor names must be unique");
    }
}
