//! Read-only projection of `AppState` onto `#[epistemic(config)]` fields.
//!
//! `Policy<'a>` is the Level 2 (policy) counterpart to
//! [`Truth<'a>`](super::truth::Truth) and
//! [`Inference<'a>`](super::inference::Inference) under ADR-030. It realises
//! the projection
//!
//! ```text
//! π : AppState → PolicyFacts
//! π(s) = extract_config(s)
//! ```
//!
//! formalised in `docs/semantics.md` §2.5 (World Model) as the `Π` component
//! of `W = (T, I, Π, S)`. Policy covers *user-controlled* configuration:
//! visual options, scrollbar glyphs, menu behaviour, plugin config, theme,
//! and fold toggle state. It excludes session metadata (`S`) and ephemeral
//! runtime state, both of which live outside the world model's projection
//! contract.
//!
//! # Invariants
//!
//! - Every accessor returns a field carrying `#[epistemic(config)]` on
//!   `AppState` (structurally witnessed by `state/tests/policy.rs`).
//! - `Policy<'a>` is `Copy`.
//! - Construction requires `&AppState`; no accessor returns an `&mut`
//!   reference. Writing through `Policy` is a compile error.

use std::collections::HashMap;

use crate::config::MenuPosition;
use crate::display::FoldToggleState;
use crate::plugin::PluginId;
use crate::plugin::setting::SettingValue;
use crate::render::theme::Theme;
use crate::state::{AppState, ConfigState};

/// Read-only projection of `AppState` onto its config (policy) fields.
///
/// See module-level documentation for the enforcement contract.
#[derive(Clone, Copy)]
pub struct Policy<'a> {
    inner: &'a ConfigState,
}

impl<'a> Policy<'a> {
    /// Create a new `Policy` projection over the given config state.
    #[inline]
    pub fn new(inner: &'a ConfigState) -> Self {
        Self { inner }
    }

    // =========================================================================
    // Display policy
    // =========================================================================

    /// Config: whether to render drop shadows on overlays.
    #[inline]
    pub fn shadow_enabled(&self) -> bool {
        self.inner.shadow_enabled
    }

    /// Config: padding character used beyond end-of-buffer lines.
    #[inline]
    pub fn padding_char(&self) -> &'a str {
        &self.inner.padding_char
    }

    /// Config: blend ratio applied to secondary cursor rendering.
    #[inline]
    pub fn secondary_blend_ratio(&self) -> f32 {
        self.inner.secondary_blend_ratio
    }

    /// Config: colour theme.
    #[inline]
    pub fn theme(&self) -> &'a Theme {
        &self.inner.theme
    }

    // =========================================================================
    // Status / menu policy
    // =========================================================================

    /// Config: whether the status bar is drawn at the top of the screen.
    #[inline]
    pub fn status_at_top(&self) -> bool {
        self.inner.status_at_top
    }

    /// Config: maximum menu height in rows.
    #[inline]
    pub fn menu_max_height(&self) -> u16 {
        self.inner.menu_max_height
    }

    /// Config: preferred menu placement policy.
    #[inline]
    pub fn menu_position(&self) -> MenuPosition {
        self.inner.menu_position
    }

    /// Config: whether the search UI renders as a dropdown.
    #[inline]
    pub fn search_dropdown(&self) -> bool {
        self.inner.search_dropdown
    }

    /// Config: scrollbar thumb glyph.
    #[inline]
    pub fn scrollbar_thumb(&self) -> &'a str {
        &self.inner.scrollbar_thumb
    }

    /// Config: scrollbar track glyph.
    #[inline]
    pub fn scrollbar_track(&self) -> &'a str {
        &self.inner.scrollbar_track
    }

    /// Config: optional ASCII assistant art lines.
    #[inline]
    pub fn assistant_art(&self) -> Option<&'a [String]> {
        self.inner.assistant_art.as_deref()
    }

    // =========================================================================
    // Plugin policy
    // =========================================================================

    /// Config: plugin-namespaced key/value configuration from `SetConfig`.
    #[inline]
    pub fn plugin_config(&self) -> &'a HashMap<String, String> {
        &self.inner.plugin_config
    }

    /// Config: typed per-plugin settings, schema-validated from manifests.
    #[inline]
    pub fn plugin_settings(&self) -> &'a HashMap<PluginId, HashMap<String, SettingValue>> {
        &self.inner.plugin_settings
    }

    // =========================================================================
    // Display transform policy
    // =========================================================================

    /// Config: fold toggle state — which fold ranges are currently expanded.
    #[inline]
    pub fn fold_toggle_state(&self) -> &'a FoldToggleState {
        &self.inner.fold_toggle_state
    }

    // =========================================================================
    // Structural witness
    // =========================================================================

    /// Names of every accessor on `Policy`, in the order they are defined.
    ///
    /// Used by `state/tests/policy.rs` to witness — structurally — that the
    /// accessor set matches the `#[epistemic(config)]` field set of
    /// `AppState`.
    pub const POLICY_ACCESSOR_NAMES: &'static [&'static str] = &[
        "shadow_enabled",
        "padding_char",
        "secondary_blend_ratio",
        "theme",
        "status_at_top",
        "menu_max_height",
        "menu_position",
        "search_dropdown",
        "scrollbar_thumb",
        "scrollbar_track",
        "assistant_art",
        "plugin_config",
        "plugin_settings",
        "fold_toggle_state",
    ];
}

impl AppState {
    /// Read-only projection onto config (policy) fields.
    ///
    /// See [`Policy`] for the enforcement contract.
    #[inline]
    pub fn policy(&self) -> Policy<'_> {
        Policy::new(&self.config)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn policy_is_copy() {
        fn assert_copy<T: Copy>() {}
        assert_copy::<Policy<'_>>();
    }

    #[test]
    fn construction_roundtrips_scalars() {
        let mut state = AppState::default();
        state.config.shadow_enabled = false;
        state.config.menu_max_height = 42;
        state.config.secondary_blend_ratio = 0.25;
        let policy = state.policy();
        assert!(!policy.shadow_enabled());
        assert_eq!(policy.menu_max_height(), 42);
        assert!((policy.secondary_blend_ratio() - 0.25).abs() < f32::EPSILON);
    }

    #[test]
    fn construction_roundtrips_strings() {
        let mut state = AppState::default();
        state.config.padding_char = "·".to_string();
        state.config.scrollbar_thumb = "X".to_string();
        let policy = state.policy();
        assert_eq!(policy.padding_char(), "·");
        assert_eq!(policy.scrollbar_thumb(), "X");
    }

    #[test]
    fn accessor_names_nonempty_and_unique() {
        let names = Policy::POLICY_ACCESSOR_NAMES;
        assert!(!names.is_empty());
        let mut sorted = names.to_vec();
        sorted.sort_unstable();
        sorted.dedup();
        assert_eq!(sorted.len(), names.len(), "accessor names must be unique");
    }
}
