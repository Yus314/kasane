//! Read-only projection of `AppState` onto `#[epistemic(observed)]` fields.
//!
//! `Truth<'a>` is the Level 1 enforcement of ADR-030 (observed/policy separation).
//! It realises the projection
//!
//! ```text
//! p : AppState → KakouneProtocolFacts
//! p(s) = extract_observed(s)
//! ```
//!
//! formalised in `docs/semantics.md` §2.5 (World Model) and referenced by
//! requirement P-032 (`docs/requirements.md`). The type deliberately exposes
//! **only** the fields that are in 1:1 correspondence with Kakoune JSON-RPC
//! messages. Derived, heuristic, config, session, and runtime fields are not
//! reachable through this projection.
//!
//! # Invariants
//!
//! - Every accessor returns a field carrying `#[epistemic(observed)]` in
//!   `kasane-core/src/state/mod.rs` (structurally witnessed by the test in
//!   `state/tests/truth.rs`).
//! - `Truth<'a>` is `Copy`, so passing it by value never invalidates the
//!   underlying borrow.
//! - Construction requires `&AppState`; there is no `&mut` variant, and no
//!   accessor returns an `&mut` reference. Attempting to write through
//!   `Truth` is a compile error, witnessed by
//!   `kasane-macros/tests/fail/truth_write_denied.rs`.

use std::collections::HashMap;

use crate::protocol::{Coord, Face, Line, StatusStyle};
use crate::state::{AppState, InfoState, MenuState};

/// Read-only projection of `AppState` onto its observed (protocol-facing)
/// fields.
///
/// See module-level documentation for the enforcement contract.
#[derive(Clone, Copy)]
pub struct Truth<'a> {
    state: &'a AppState,
}

impl<'a> Truth<'a> {
    /// Create a new `Truth` projection over the given state.
    #[inline]
    pub fn new(state: &'a AppState) -> Self {
        Self { state }
    }

    // =========================================================================
    // Buffer content (`draw`)
    // =========================================================================

    /// Observed: buffer lines from `draw`.
    #[inline]
    pub fn lines(&self) -> &'a [Line] {
        &self.state.lines
    }

    /// Observed: default face from `draw`.
    #[inline]
    pub fn default_face(&self) -> Face {
        self.state.default_face
    }

    /// Observed: padding face from `draw`.
    #[inline]
    pub fn padding_face(&self) -> Face {
        self.state.padding_face
    }

    /// Observed: number of widget columns from `draw`.
    #[inline]
    pub fn widget_columns(&self) -> u16 {
        self.state.widget_columns
    }

    /// Observed: cursor position from `draw`.
    #[inline]
    pub fn cursor_pos(&self) -> Coord {
        self.state.cursor_pos
    }

    // =========================================================================
    // Status bar (`draw_status`)
    // =========================================================================

    /// Observed: status prompt atoms from `draw_status`.
    #[inline]
    pub fn status_prompt(&self) -> &'a Line {
        &self.state.status_prompt
    }

    /// Observed: status content atoms from `draw_status`.
    #[inline]
    pub fn status_content(&self) -> &'a Line {
        &self.state.status_content
    }

    /// Observed: cursor position within status content from `draw_status`.
    #[inline]
    pub fn status_content_cursor_pos(&self) -> i32 {
        self.state.status_content_cursor_pos
    }

    /// Observed: mode line atoms from `draw_status`.
    #[inline]
    pub fn status_mode_line(&self) -> &'a Line {
        &self.state.status_mode_line
    }

    /// Observed: default face for the status bar from `draw_status`.
    #[inline]
    pub fn status_default_face(&self) -> Face {
        self.state.status_default_face
    }

    /// Observed: status bar context style from `draw_status`.
    #[inline]
    pub fn status_style(&self) -> StatusStyle {
        self.state.status_style
    }

    // =========================================================================
    // Menu / Info (`menu_show`, `info_show`)
    // =========================================================================

    /// Observed: completion menu state from `menu_show` / `menu_select` / `menu_hide`.
    #[inline]
    pub fn menu(&self) -> Option<&'a MenuState> {
        self.state.menu.as_ref()
    }

    /// Observed: info popup state from `info_show` / `info_hide`.
    #[inline]
    pub fn infos(&self) -> &'a [InfoState] {
        &self.state.infos
    }

    // =========================================================================
    // UI options (`set_ui_options`)
    // =========================================================================

    /// Observed: UI options from `set_ui_options`.
    #[inline]
    pub fn ui_options(&self) -> &'a HashMap<String, String> {
        &self.state.ui_options
    }

    // =========================================================================
    // Structural witness
    // =========================================================================

    /// Names of every accessor on `Truth`, in the order they are defined.
    ///
    /// Used by `state/tests/truth.rs` to witness — structurally — that the
    /// accessor set matches the `#[epistemic(observed)]` field set of
    /// `AppState`. When you add a new observed field to `AppState`, add a
    /// matching accessor here and append its name to this list.
    pub const ACCESSOR_NAMES: &'static [&'static str] = &[
        "lines",
        "default_face",
        "padding_face",
        "widget_columns",
        "cursor_pos",
        "status_prompt",
        "status_content",
        "status_content_cursor_pos",
        "status_mode_line",
        "status_default_face",
        "status_style",
        "menu",
        "infos",
        "ui_options",
    ];
}

impl AppState {
    /// Read-only projection onto observed (protocol-facing) fields.
    ///
    /// See [`Truth`] for the enforcement contract.
    #[inline]
    pub fn truth(&self) -> Truth<'_> {
        Truth::new(self)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn truth_is_copy() {
        fn assert_copy<T: Copy>() {}
        assert_copy::<Truth<'_>>();
    }

    #[test]
    fn construction_roundtrips_cursor() {
        let mut state = AppState::default();
        state.cursor_pos = Coord { line: 7, column: 3 };
        let truth = state.truth();
        assert_eq!(truth.cursor_pos(), Coord { line: 7, column: 3 });
    }

    #[test]
    fn construction_roundtrips_buffer() {
        let mut state = AppState::default();
        state.lines = vec![vec![], vec![], vec![]];
        state.widget_columns = 4;
        let truth = state.truth();
        assert_eq!(truth.lines().len(), 3);
        assert_eq!(truth.widget_columns(), 4);
    }

    #[test]
    fn construction_roundtrips_status() {
        let mut state = AppState::default();
        state.status_content_cursor_pos = 12;
        let truth = state.truth();
        assert_eq!(truth.status_content_cursor_pos(), 12);
        assert!(truth.status_prompt().is_empty());
        assert!(truth.status_content().is_empty());
        assert!(truth.status_mode_line().is_empty());
    }

    #[test]
    fn construction_roundtrips_menu_info() {
        let state = AppState::default();
        let truth = state.truth();
        assert!(truth.menu().is_none());
        assert!(truth.infos().is_empty());
        assert!(truth.ui_options().is_empty());
    }

    #[test]
    fn accessor_names_nonempty_and_unique() {
        let names = Truth::ACCESSOR_NAMES;
        assert!(!names.is_empty());
        let mut sorted = names.to_vec();
        sorted.sort_unstable();
        sorted.dedup();
        assert_eq!(sorted.len(), names.len(), "accessor names must be unique");
    }
}
