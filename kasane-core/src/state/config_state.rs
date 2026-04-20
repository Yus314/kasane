//! Config (policy) state sub-struct.
//!
//! Contains user-controlled configuration: visual options, scrollbar glyphs,
//! menu behaviour, plugin config, theme, and fold toggle state.
//! This is the `Π` component of the world model `W = (T, I, Π, S)`.

use std::collections::HashMap;

use crate::config::MenuPosition;
use crate::display::FoldToggleState;
use crate::display::ProjectionPolicyState;
use crate::plugin::PluginId;
use crate::plugin::setting::SettingValue;
use crate::render::theme::Theme;

/// User-controlled policy/configuration state.
///
/// Every field here carries `#[epistemic(config)]` semantics.
#[derive(Debug, Clone, PartialEq)]
pub struct ConfigState {
    pub shadow_enabled: bool,
    pub padding_char: String,
    pub menu_max_height: u16,
    pub menu_position: MenuPosition,
    pub search_dropdown: bool,
    pub status_at_top: bool,
    pub scrollbar_thumb: String,
    pub scrollbar_track: String,
    pub assistant_art: Option<Vec<String>>,
    pub plugin_config: HashMap<String, String>,
    /// Typed per-plugin settings (schema-validated, from manifest + config.toml).
    pub plugin_settings: HashMap<PluginId, HashMap<String, SettingValue>>,
    pub secondary_blend_ratio: f32,
    /// Vertical split divider character (default: "│").
    pub divider_vertical: String,
    /// Horizontal split divider character (default: "─").
    pub divider_horizontal: String,
    /// Scroll edge margin: selection scroll stops this many rows from the edge (default: 2).
    pub scroll_edge_margin: u16,
    /// Info popup scroll step in lines per scroll event (default: 3).
    pub info_scroll_step: u16,
    /// Replacement string for newline characters in the grid (default: " ").
    pub newline_display: String,
    /// String appended when content is truncated to fit (default: "…").
    pub truncation_char: String,
    pub theme: Theme,
    /// Core fold toggle state: tracks which fold ranges are currently expanded.
    pub fold_toggle_state: FoldToggleState,
    /// Projection mode policy: which projections are active and per-projection fold state.
    pub projection_policy: ProjectionPolicyState,
}

impl Default for ConfigState {
    fn default() -> Self {
        Self {
            shadow_enabled: true,
            padding_char: "~".to_string(),
            menu_max_height: 10,
            menu_position: MenuPosition::Auto,
            search_dropdown: false,
            status_at_top: false,
            scrollbar_thumb: "\u{2588}".to_string(), // █
            scrollbar_track: "\u{2591}".to_string(), // ░
            assistant_art: None,
            plugin_config: HashMap::new(),
            plugin_settings: HashMap::new(),
            secondary_blend_ratio: 0.4,
            divider_vertical: "\u{2502}".to_string(),   // │
            divider_horizontal: "\u{2500}".to_string(), // ─
            scroll_edge_margin: 2,
            info_scroll_step: 3,
            newline_display: " ".to_string(),
            truncation_char: "\u{2026}".to_string(), // …
            theme: Theme::default_theme(),
            fold_toggle_state: FoldToggleState::default(),
            projection_policy: ProjectionPolicyState::default(),
        }
    }
}
