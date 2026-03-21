//! Read-only view of application state for plugin methods.
//!
//! Provides method-based access to `AppState` fields, mirroring the
//! WASM `host_state::get_*()` pattern for native plugins.

use std::collections::HashMap;
use std::ops::Range;

use crate::protocol::{Coord, CursorMode, Face, Line};
use crate::session::SessionDescriptor;
use crate::state::{AppState, InfoState, MenuState};

/// Read-only view of application state for plugin methods.
///
/// Wraps `&AppState` with method-based accessors, providing a unified
/// access pattern that mirrors the WASM `host_state::get_*()` API.
/// All accessors are `#[inline]` and zero-cost (compile to direct field reads).
#[non_exhaustive]
pub struct AppView<'a> {
    state: &'a AppState,
}

impl<'a> AppView<'a> {
    /// Create a new `AppView` wrapping the given state.
    #[inline]
    pub fn new(state: &'a AppState) -> Self {
        Self { state }
    }

    /// Access the underlying `AppState` directly.
    ///
    /// **Framework-internal.** Plugin authors should use `AppView` accessors
    /// instead. This method exists for framework layers (e.g. WASM host sync)
    /// that need raw `AppState` field access for serialization.
    #[inline]
    pub fn as_app_state(&self) -> &AppState {
        self.state
    }

    // =========================================================================
    // Tier 0: Core buffer state
    // =========================================================================

    /// Cursor line (0-indexed).
    #[inline]
    pub fn cursor_line(&self) -> i32 {
        self.state.cursor_pos.line
    }

    /// Cursor column (0-indexed).
    #[inline]
    pub fn cursor_col(&self) -> i32 {
        self.state.cursor_pos.column
    }

    /// Cursor position as `Coord`.
    #[inline]
    pub fn cursor_pos(&self) -> Coord {
        self.state.cursor_pos
    }

    /// Buffer lines.
    #[inline]
    pub fn lines(&self) -> &[Line] {
        &self.state.lines
    }

    /// Number of buffer lines.
    #[inline]
    pub fn line_count(&self) -> usize {
        self.state.lines.len()
    }

    /// Whether a specific line is dirty (changed since last frame).
    #[inline]
    pub fn is_line_dirty(&self, line: usize) -> bool {
        self.state.lines_dirty.get(line).copied().unwrap_or(false)
    }

    /// Per-line dirty flags.
    #[inline]
    pub fn lines_dirty(&self) -> &[bool] {
        &self.state.lines_dirty
    }

    /// Terminal columns.
    #[inline]
    pub fn cols(&self) -> u16 {
        self.state.cols
    }

    /// Terminal rows.
    #[inline]
    pub fn rows(&self) -> u16 {
        self.state.rows
    }

    /// Whether the terminal is focused.
    #[inline]
    pub fn focused(&self) -> bool {
        self.state.focused
    }

    /// Cursor mode (Buffer or Prompt).
    #[inline]
    pub fn cursor_mode(&self) -> CursorMode {
        self.state.cursor_mode
    }

    /// Default face from `draw`.
    #[inline]
    pub fn default_face(&self) -> Face {
        self.state.default_face
    }

    /// Padding face from `draw`.
    #[inline]
    pub fn padding_face(&self) -> Face {
        self.state.padding_face
    }

    /// Number of widget columns from `draw`.
    #[inline]
    pub fn widget_columns(&self) -> u16 {
        self.state.widget_columns
    }

    /// Total cursor count (primary + secondary).
    #[inline]
    pub fn cursor_count(&self) -> usize {
        self.state.cursor_count
    }

    /// Secondary cursor positions.
    #[inline]
    pub fn secondary_cursors(&self) -> &[Coord] {
        &self.state.secondary_cursors
    }

    // =========================================================================
    // Tier 1: Status bar
    // =========================================================================

    /// Composed status line (prompt + content).
    #[inline]
    pub fn status_line(&self) -> &Line {
        &self.state.status_line
    }

    /// Status mode line.
    #[inline]
    pub fn status_mode_line(&self) -> &Line {
        &self.state.status_mode_line
    }

    /// Default face for the status bar.
    #[inline]
    pub fn status_default_face(&self) -> Face {
        self.state.status_default_face
    }

    /// Status prompt atoms.
    #[inline]
    pub fn status_prompt(&self) -> &Line {
        &self.state.status_prompt
    }

    /// Status content atoms.
    #[inline]
    pub fn status_content(&self) -> &Line {
        &self.state.status_content
    }

    /// Cursor position within status content.
    #[inline]
    pub fn status_content_cursor_pos(&self) -> i32 {
        self.state.status_content_cursor_pos
    }

    // =========================================================================
    // Tier 2: Menu
    // =========================================================================

    /// Menu state (if a completion menu is shown).
    #[inline]
    pub fn menu(&self) -> Option<&MenuState> {
        self.state.menu.as_ref()
    }

    /// Whether a completion menu is currently shown.
    #[inline]
    pub fn has_menu(&self) -> bool {
        self.state.has_menu()
    }

    // =========================================================================
    // Tier 3: Info
    // =========================================================================

    /// Info popup states.
    #[inline]
    pub fn infos(&self) -> &[InfoState] {
        &self.state.infos
    }

    /// Whether any info popups are shown.
    #[inline]
    pub fn has_info(&self) -> bool {
        self.state.has_info()
    }

    // =========================================================================
    // Tier 4: Config / Options
    // =========================================================================

    /// UI options from `set_ui_options`.
    #[inline]
    pub fn ui_options(&self) -> &HashMap<String, String> {
        &self.state.ui_options
    }

    /// Plugin configuration key-value pairs.
    #[inline]
    pub fn plugin_config(&self) -> &HashMap<String, String> {
        &self.state.plugin_config
    }

    /// Whether shadow is enabled.
    #[inline]
    pub fn shadow_enabled(&self) -> bool {
        self.state.shadow_enabled
    }

    /// Whether the status bar is at the top.
    #[inline]
    pub fn status_at_top(&self) -> bool {
        self.state.status_at_top
    }

    /// Secondary cursor blend ratio.
    #[inline]
    pub fn secondary_blend_ratio(&self) -> f32 {
        self.state.secondary_blend_ratio
    }

    // =========================================================================
    // Tier 5: Session
    // =========================================================================

    /// Session descriptors.
    #[inline]
    pub fn session_descriptors(&self) -> &[SessionDescriptor] {
        &self.state.session_descriptors
    }

    /// Active session key.
    #[inline]
    pub fn active_session_key(&self) -> Option<&str> {
        self.state.active_session_key.as_deref()
    }

    // =========================================================================
    // Derived methods
    // =========================================================================

    /// Available height (rows minus status bar).
    #[inline]
    pub fn available_height(&self) -> u16 {
        self.state.available_height()
    }

    /// Range of visible buffer line indices.
    #[inline]
    pub fn visible_line_range(&self) -> Range<usize> {
        self.state.visible_line_range()
    }

    /// Whether the cursor is in prompt mode.
    #[inline]
    pub fn is_prompt_mode(&self) -> bool {
        self.state.is_prompt_mode()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::AppState;

    #[test]
    fn cursor_accessors() {
        let mut state = AppState::default();
        state.cursor_pos = Coord {
            line: 5,
            column: 10,
        };
        let view = AppView::new(&state);
        assert_eq!(view.cursor_line(), 5);
        assert_eq!(view.cursor_col(), 10);
        assert_eq!(
            view.cursor_pos(),
            Coord {
                line: 5,
                column: 10
            }
        );
    }

    #[test]
    fn buffer_accessors() {
        let mut state = AppState::default();
        state.lines = vec![vec![], vec![], vec![]];
        state.lines_dirty = vec![false, true, false];
        let view = AppView::new(&state);
        assert_eq!(view.lines().len(), 3);
        assert_eq!(view.line_count(), 3);
        assert!(!view.is_line_dirty(0));
        assert!(view.is_line_dirty(1));
        assert!(!view.is_line_dirty(2));
        assert!(!view.is_line_dirty(100)); // out of bounds returns false
    }

    #[test]
    fn geometry_accessors() {
        let mut state = AppState::default();
        state.cols = 120;
        state.rows = 40;
        state.focused = false;
        let view = AppView::new(&state);
        assert_eq!(view.cols(), 120);
        assert_eq!(view.rows(), 40);
        assert!(!view.focused());
        assert_eq!(view.available_height(), 39);
    }

    #[test]
    fn status_accessors() {
        let state = AppState::default();
        let view = AppView::new(&state);
        assert!(view.status_line().is_empty());
        assert!(view.status_mode_line().is_empty());
    }

    #[test]
    fn escape_hatch() {
        let state = AppState::default();
        let view = AppView::new(&state);
        assert_eq!(view.as_app_state().cols, 80);
    }

    #[test]
    fn derived_methods() {
        let mut state = AppState::default();
        state.lines = vec![vec![], vec![]];
        let view = AppView::new(&state);
        assert_eq!(view.visible_line_range(), 0..2);
    }
}
