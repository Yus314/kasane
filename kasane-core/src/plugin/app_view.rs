//! Read-only view of application state for plugin methods.
//!
//! Provides method-based access to `AppState` fields, mirroring the
//! WASM `host_state::get_*()` pattern for native plugins.

use std::collections::HashMap;
use std::ops::Range;
use std::sync::Arc;

use crate::display::{FoldToggleState, ProjectionPolicyState};
use crate::plugin::PluginId;
use crate::plugin::setting::SettingValue;
use crate::protocol::{Coord, CursorMode, Line, StatusStyle};
use crate::session::SessionDescriptor;
use crate::state::{AppState, Inference, InfoState, MenuState, Policy, Truth};
use crate::syntax::SyntaxProvider;

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

    /// Read-only projection onto `#[epistemic(observed)]` fields.
    ///
    /// Plugins that need to distinguish Kakoune protocol facts from derived,
    /// heuristic, or policy-level state should use [`Truth`] rather than the
    /// wider `AppView` accessor set. See `docs/semantics.md` §2.5 and ADR-030
    /// for the enforcement rationale.
    #[inline]
    pub fn truth(&self) -> Truth<'a> {
        Truth::new(&self.state.observed)
    }

    /// Read-only projection onto `#[epistemic(derived)]` + `#[epistemic(heuristic)]`
    /// fields.
    ///
    /// Plugins that consume inferred facts — cursor mode, selections,
    /// color context, editor mode, etc. — should prefer [`Inference`] over
    /// the wider `AppView` accessor set. This is the `I` component of the
    /// world model `W = (T, I, Π, S)` formalised under ADR-030 Level 2.
    #[inline]
    pub fn inference(&self) -> Inference<'a> {
        Inference::new(&self.state.inference)
    }

    /// Read-only projection onto `#[epistemic(config)]` fields.
    ///
    /// Plugins that depend on user-controlled policy — theme, scrollbar
    /// glyphs, menu layout, plugin settings, fold toggle state, etc. —
    /// should prefer [`Policy`] over the wider `AppView` accessor set.
    /// This is the `Π` component of the world model `W = (T, I, Π, S)`
    /// formalised under ADR-030 Level 2.
    #[inline]
    pub fn policy(&self) -> Policy<'a> {
        Policy::new(&self.state.config)
    }

    // =========================================================================
    // Tier 0: Core buffer state
    // =========================================================================

    /// Cursor line (0-indexed).
    #[inline]
    pub fn cursor_line(&self) -> i32 {
        self.state.observed.cursor_pos.line
    }

    /// Cursor column (0-indexed).
    #[inline]
    pub fn cursor_col(&self) -> i32 {
        self.state.observed.cursor_pos.column
    }

    /// Cursor position as `Coord`.
    #[inline]
    pub fn cursor_pos(&self) -> Coord {
        self.state.observed.cursor_pos
    }

    /// Buffer lines.
    #[inline]
    pub fn lines(&self) -> &[Line] {
        &self.state.observed.lines
    }

    /// Number of buffer lines.
    #[inline]
    pub fn line_count(&self) -> usize {
        self.state.observed.lines.len()
    }

    /// Whether a specific line is dirty (changed since last frame).
    #[inline]
    pub fn is_line_dirty(&self, line: usize) -> bool {
        self.state
            .inference
            .lines_dirty
            .get(line)
            .copied()
            .unwrap_or(false)
    }

    /// Per-line dirty flags.
    #[inline]
    pub fn lines_dirty(&self) -> &[bool] {
        &self.state.inference.lines_dirty
    }

    /// Terminal columns.
    #[inline]
    pub fn cols(&self) -> u16 {
        self.state.runtime.cols
    }

    /// Terminal rows.
    #[inline]
    pub fn rows(&self) -> u16 {
        self.state.runtime.rows
    }

    /// Whether the terminal is focused.
    #[inline]
    pub fn focused(&self) -> bool {
        self.state.runtime.focused
    }

    /// Cursor mode (Buffer or Prompt).
    #[inline]
    pub fn cursor_mode(&self) -> CursorMode {
        self.state.inference.cursor_mode
    }

    /// Default style from `draw`.
    #[inline]
    pub fn default_style(&self) -> &crate::protocol::Style {
        &self.state.observed.default_style
    }

    /// Padding style from `draw`.
    #[inline]
    pub fn padding_style(&self) -> &crate::protocol::Style {
        &self.state.observed.padding_style
    }

    /// Number of widget columns from `draw`.
    #[inline]
    pub fn widget_columns(&self) -> u16 {
        self.state.observed.widget_columns
    }

    /// Parsed editor mode (Normal/Insert/Replace/Prompt).
    #[inline]
    pub fn editor_mode(&self) -> crate::state::derived::EditorMode {
        self.state.inference.editor_mode
    }

    /// Total cursor count (primary + secondary).
    #[inline]
    pub fn cursor_count(&self) -> usize {
        self.state.inference.cursor_count
    }

    /// Secondary cursor positions.
    #[inline]
    pub fn secondary_cursors(&self) -> &[Coord] {
        &self.state.inference.secondary_cursors
    }

    /// Detected selection ranges (heuristic I-7).
    #[inline]
    pub fn selections(&self) -> &[crate::state::derived::Selection] {
        &self.state.inference.selections
    }

    /// Primary selection, if any.
    #[inline]
    pub fn primary_selection(&self) -> Option<&crate::state::derived::Selection> {
        self.state
            .inference
            .selections
            .iter()
            .find(|s| s.is_primary)
    }

    // =========================================================================
    // Tier 1: Status bar
    // =========================================================================

    /// Composed status line (prompt + content).
    #[inline]
    pub fn status_line(&self) -> &Line {
        &self.state.inference.status_line
    }

    /// Status mode line.
    #[inline]
    pub fn status_mode_line(&self) -> &Line {
        &self.state.observed.status_mode_line
    }

    /// Default style for the status bar.
    #[inline]
    pub fn status_default_style(&self) -> &crate::protocol::Style {
        &self.state.observed.status_default_style
    }

    /// Status prompt atoms.
    #[inline]
    pub fn status_prompt(&self) -> &Line {
        &self.state.observed.status_prompt
    }

    /// Status content atoms.
    #[inline]
    pub fn status_content(&self) -> &Line {
        &self.state.observed.status_content
    }

    /// Cursor position within status content.
    #[inline]
    pub fn status_content_cursor_pos(&self) -> i32 {
        self.state.observed.status_content_cursor_pos
    }

    /// Status bar context style (command, search, prompt, or status).
    #[inline]
    pub fn status_style(&self) -> StatusStyle {
        self.state.observed.status_style
    }

    // =========================================================================
    // Tier 2: Menu
    // =========================================================================

    /// Menu state (if a completion menu is shown).
    #[inline]
    pub fn menu(&self) -> Option<&MenuState> {
        self.state.observed.menu.as_ref()
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
        &self.state.observed.infos
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
        &self.state.observed.ui_options
    }

    /// Plugin configuration key-value pairs.
    #[inline]
    pub fn plugin_config(&self) -> &HashMap<String, String> {
        &self.state.config.plugin_config
    }

    /// Whether shadow is enabled.
    #[inline]
    pub fn shadow_enabled(&self) -> bool {
        self.state.config.shadow_enabled
    }

    /// Whether the status bar is at the top.
    #[inline]
    pub fn status_at_top(&self) -> bool {
        self.state.config.status_at_top
    }

    /// Secondary cursor blend ratio.
    #[inline]
    pub fn secondary_blend_ratio(&self) -> f32 {
        self.state.config.secondary_blend_ratio
    }

    /// All per-plugin settings.
    #[inline]
    pub fn plugin_settings(&self) -> &HashMap<PluginId, HashMap<String, SettingValue>> {
        &self.state.config.plugin_settings
    }

    /// Look up a single plugin setting by plugin ID and key.
    #[inline]
    pub fn plugin_setting(&self, plugin_id: &PluginId, key: &str) -> Option<&SettingValue> {
        self.state
            .config
            .plugin_settings
            .get(plugin_id)
            .and_then(|m| m.get(key))
    }

    // =========================================================================
    // Tier 5: Session
    // =========================================================================

    /// Session descriptors.
    #[inline]
    pub fn session_descriptors(&self) -> &[SessionDescriptor] {
        &self.state.session.session_descriptors
    }

    /// Active session key.
    #[inline]
    pub fn active_session_key(&self) -> Option<&str> {
        self.state.session.active_session_key.as_deref()
    }

    // =========================================================================
    // Tier 9: Theme / Color context
    // =========================================================================

    /// Look up a theme token style.
    #[inline]
    pub fn theme_style(
        &self,
        token: &crate::element::StyleToken,
    ) -> Option<&crate::protocol::Style> {
        self.state.config.theme.get_style(token)
    }

    /// Whether the background is dark.
    #[inline]
    pub fn is_dark_background(&self) -> bool {
        self.state.inference.color_context.is_dark
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

    // =========================================================================
    // Tier 10: Display transform state
    // =========================================================================

    /// Fold toggle state for display transform filtering.
    #[inline]
    pub fn fold_toggle_state(&self) -> &FoldToggleState {
        &self.state.config.fold_toggle_state
    }

    /// Projection mode policy — active projections and per-projection fold state.
    #[inline]
    pub fn projection_policy(&self) -> &ProjectionPolicyState {
        &self.state.config.projection_policy
    }

    /// Available projection descriptors from all registered plugins.
    #[inline]
    pub fn available_projections(&self) -> &[crate::display::ProjectionDescriptor] {
        &self.state.runtime.available_projections
    }

    // =========================================================================
    // Tier 11: Runtime state
    // =========================================================================

    /// Whether the user is currently performing a mouse drag.
    #[inline]
    pub fn is_dragging(&self) -> bool {
        !matches!(self.state.runtime.drag, crate::state::DragState::None)
    }

    /// Check if a built-in target has been suppressed.
    #[inline]
    pub fn is_builtin_suppressed(&self, target: super::BuiltinTarget) -> bool {
        self.state.runtime.suppressed_builtins.contains(&target)
    }

    /// Plugin diagnostic overlay state.
    #[inline]
    pub fn diagnostic_overlay(&self) -> &super::diagnostics::PluginDiagnosticOverlayState {
        &self.state.runtime.diagnostic_overlay
    }

    /// Syntax analysis provider for the current buffer (e.g., tree-sitter).
    #[inline]
    pub fn syntax_provider(&self) -> Option<&Arc<dyn SyntaxProvider>> {
        self.state.runtime.syntax_provider.as_ref()
    }
}

/// Raw `AppState` access for framework layers (WASM host sync, serialization).
///
/// **Not for plugin authors.** Use [`AppView`] accessors instead.
/// This trait exists so that framework-level code in separate crates
/// (e.g. `kasane-wasm`) can access the underlying state for serialization,
/// without exposing `as_app_state()` as an inherent method on `AppView`.
///
/// Importing this trait is an explicit opt-in to the escape hatch.
/// The trait is sealed — only types inside `kasane-core` can implement it.
pub trait FrameworkAccess: sealed::Sealed {
    /// Access the underlying `AppState` directly.
    fn as_app_state(&self) -> &AppState;
}

mod sealed {
    pub trait Sealed {}
    impl Sealed for super::AppView<'_> {}
}

impl FrameworkAccess for AppView<'_> {
    #[inline]
    fn as_app_state(&self) -> &AppState {
        self.state
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::AppState;

    #[test]
    fn cursor_accessors() {
        let mut state = AppState::default();
        state.observed.cursor_pos = Coord {
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
        state.observed.lines = (vec![vec![], vec![], vec![]]).into();
        state.inference.lines_dirty = vec![false, true, false];
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
        state.runtime.cols = 120;
        state.runtime.rows = 40;
        state.runtime.focused = false;
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
        use super::FrameworkAccess;
        let state = AppState::default();
        let view = AppView::new(&state);
        assert_eq!(view.as_app_state().runtime.cols, 80);
    }

    #[test]
    fn derived_methods() {
        let mut state = AppState::default();
        state.observed.lines = (vec![vec![], vec![]]).into();
        let view = AppView::new(&state);
        assert_eq!(view.visible_line_range(), 0..2);
    }
}
