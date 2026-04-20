//! Application state: `AppState`, `apply()`, `update()`, dirty generation tracking.

mod apply;
pub mod config_state;
pub mod derived;
pub mod inference;
pub mod inference_state;
mod info;
mod menu;
pub mod observed;
pub mod policy;
pub mod runtime_state;
pub mod session_state;
pub(crate) mod setting_registry;
pub mod shadow_cursor;
pub mod snapshot;
#[cfg(test)]
#[allow(clippy::field_reassign_with_default)]
mod tests;
pub mod truth;
mod update;

use bitflags::bitflags;

use crate::config::{Config, StatusPosition};
use crate::plugin::PluginId;
use crate::plugin::setting::SettingValue;
use crate::protocol::CursorMode;
use crate::render::theme::Theme;

pub use config_state::ConfigState;
pub use inference::Inference;
pub use inference_state::InferenceState;
pub use info::{InfoIdentity, InfoState};
pub use menu::{ItemSplit, MenuColumns, MenuParams, MenuState, split_single_item};
pub use observed::ObservedState;
pub use policy::Policy;
pub use runtime_state::RuntimeState;
pub use session_state::SessionState;
pub use truth::Truth;
pub use update::{Msg, UpdateResult, update, update_in_place};

bitflags! {
    /// Tracks which parts of `AppState` changed during a frame.
    ///
    /// ## Roles
    ///
    /// 1. **Salsa sync hints** — `sync_inputs_from_state()` checks flags to decide which
    ///    Salsa inputs need updating.
    /// 2. **Plugin contribution gating** — `prepare_plugin_cache()` compares plugin state
    ///    hashes to gate re-collection of plugin contributions.
    /// 3. **Selective grid clear** — `BUFFER_CONTENT` triggers line-level `mark_region_dirty`.
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub struct DirtyFlags: u16 {
        /// Buffer content changed (lines, faces, widget_columns, etc.).
        const BUFFER_CONTENT  = 1 << 0;
        const STATUS          = 1 << 1;
        const MENU_STRUCTURE  = 1 << 2;
        const MENU_SELECTION  = 1 << 3;
        const INFO            = 1 << 4;
        const OPTIONS         = 1 << 5;
        /// Cursor position/mode changed (cursor_pos, cursor_mode, secondary_cursors).
        const BUFFER_CURSOR   = 1 << 6;
        /// Plugin internal state changed (for Plugin state externalization).
        const PLUGIN_STATE    = 1 << 7;
        /// Session list or active session changed.
        const SESSION         = 1 << 8;
        /// Plugin settings changed (typed per-plugin configuration).
        const SETTINGS        = 1 << 9;

        /// Composite: any buffer-related change.
        const BUFFER = Self::BUFFER_CONTENT.bits() | Self::BUFFER_CURSOR.bits();
        const MENU = Self::MENU_STRUCTURE.bits() | Self::MENU_SELECTION.bits();
        const ALL  = Self::BUFFER.bits() | Self::STATUS.bits()
                   | Self::MENU.bits() | Self::INFO.bits() | Self::OPTIONS.bits()
                   | Self::PLUGIN_STATE.bits() | Self::SESSION.bits()
                   | Self::SETTINGS.bits();
    }
}

/// Drag state for mouse selection tracking.
#[derive(Debug, Clone, PartialEq, Default)]
pub enum DragState {
    #[default]
    None,
    Active {
        button: crate::input::MouseButton,
        start_line: u32,
        start_column: u32,
    },
}

/// The central application state, decomposed into epistemic sub-structs.
///
/// The world model `W = (T, I, Π, S)` from ADR-030 is structurally enforced:
/// - `observed` — Truth (`T`): protocol-observed fields
/// - `inference` — Inference (`I`): derived + heuristic fields
/// - `config` — Policy (`Π`): user-controlled configuration
/// - `session` — Session (`S`): session metadata
/// - `runtime` — ephemeral runtime state (outside the world model)
///
/// `cursor_cache` is kept as an independent field to avoid split-borrow
/// conflicts in `detect_cursors_incremental()`.
#[derive(Debug, Clone, Default)]
pub struct AppState {
    pub observed: ObservedState,
    pub inference: InferenceState,
    pub config: ConfigState,
    pub session: SessionState,
    pub runtime: RuntimeState,
    pub(crate) cursor_cache: derived::CursorCache,
}

// ---------------------------------------------------------------------------
// DirtyTracked-equivalent constants (manually composed from sub-structs)
// ---------------------------------------------------------------------------
// These constants maintain the same interface as the former `#[derive(DirtyTracked)]`
// so that structural witness tests and Salsa projection coverage tests continue to work.
impl AppState {
    /// Field → DirtyFlags mapping.
    pub const FIELD_DIRTY_MAP: &[(&str, &[&str])] = &[
        // observed
        ("lines", &["BUFFER_CONTENT"]),
        ("default_face", &["BUFFER_CONTENT"]),
        ("padding_face", &["BUFFER_CONTENT"]),
        ("cursor_pos", &["BUFFER_CURSOR"]),
        ("status_prompt", &["STATUS"]),
        ("status_content", &["STATUS"]),
        ("status_content_cursor_pos", &["STATUS"]),
        ("status_mode_line", &["STATUS"]),
        ("status_default_face", &["STATUS"]),
        ("status_style", &["STATUS"]),
        ("widget_columns", &["BUFFER_CONTENT"]),
        ("menu", &["MENU_STRUCTURE", "MENU_SELECTION"]),
        ("infos", &["INFO"]),
        ("ui_options", &["OPTIONS"]),
        // inference
        ("lines_dirty", &["BUFFER_CONTENT"]),
        ("cursor_mode", &["BUFFER_CURSOR"]),
        ("status_line", &["STATUS"]),
        ("editor_mode", &["STATUS"]),
        ("color_context", &["BUFFER_CONTENT"]),
        ("cursor_count", &["BUFFER_CURSOR"]),
        ("secondary_cursors", &["BUFFER_CURSOR"]),
        ("selections", &["BUFFER_CONTENT"]),
        // config
        ("shadow_enabled", &["OPTIONS"]),
        ("padding_char", &["OPTIONS"]),
        ("menu_max_height", &["OPTIONS"]),
        ("menu_position", &["OPTIONS"]),
        ("search_dropdown", &["OPTIONS"]),
        ("status_at_top", &["OPTIONS"]),
        ("scrollbar_thumb", &["MENU_STRUCTURE"]),
        ("scrollbar_track", &["MENU_STRUCTURE"]),
        ("assistant_art", &["OPTIONS"]),
        ("plugin_config", &["OPTIONS"]),
        ("plugin_settings", &["SETTINGS"]),
        ("secondary_blend_ratio", &["BUFFER_CONTENT"]),
        ("theme", &["OPTIONS"]),
        // session
        ("session_descriptors", &["SESSION"]),
        ("active_session_key", &["SESSION"]),
    ];

    /// Fields that are free reads (no DirtyFlag needed).
    pub const FREE_READ_FIELDS: &[&str] = &[
        "focused",
        "drag",
        "cols",
        "rows",
        "hit_map",
        "cursor_cache",
        "display_scroll_offset",
        "display_map",
        "display_unit_map",
        "available_projections",
        "shadow_cursor",
        "fold_toggle_state",
        "projection_policy",
    ];

    /// Field → epistemological category.
    pub const FIELD_EPISTEMIC_MAP: &[(&str, &str)] = &[
        // observed
        ("lines", "observed"),
        ("default_face", "observed"),
        ("padding_face", "observed"),
        ("cursor_pos", "observed"),
        ("status_prompt", "observed"),
        ("status_content", "observed"),
        ("status_content_cursor_pos", "observed"),
        ("status_mode_line", "observed"),
        ("status_default_face", "observed"),
        ("status_style", "observed"),
        ("widget_columns", "observed"),
        ("menu", "observed"),
        ("infos", "observed"),
        ("ui_options", "observed"),
        // derived
        ("lines_dirty", "derived"),
        ("cursor_mode", "derived"),
        ("status_line", "derived"),
        ("editor_mode", "derived"),
        ("color_context", "derived"),
        // heuristic
        ("cursor_count", "heuristic"),
        ("secondary_cursors", "heuristic"),
        ("selections", "heuristic"),
        // config
        ("shadow_enabled", "config"),
        ("padding_char", "config"),
        ("menu_max_height", "config"),
        ("menu_position", "config"),
        ("search_dropdown", "config"),
        ("status_at_top", "config"),
        ("scrollbar_thumb", "config"),
        ("scrollbar_track", "config"),
        ("assistant_art", "config"),
        ("plugin_config", "config"),
        ("plugin_settings", "config"),
        ("secondary_blend_ratio", "config"),
        ("theme", "config"),
        ("fold_toggle_state", "config"),
        ("projection_policy", "config"),
        // session
        ("session_descriptors", "session"),
        ("active_session_key", "session"),
        // runtime
        ("focused", "runtime"),
        ("drag", "runtime"),
        ("cols", "runtime"),
        ("rows", "runtime"),
        ("hit_map", "runtime"),
        ("cursor_cache", "runtime"),
        ("display_scroll_offset", "runtime"),
        ("display_map", "runtime"),
        ("display_unit_map", "runtime"),
        ("available_projections", "runtime"),
        ("shadow_cursor", "runtime"),
    ];

    /// Heuristic fields: `(field, rule, severity)`.
    pub const HEURISTIC_FIELDS: &[(&str, &str, &str)] = &[
        ("cursor_count", "I-1", "degraded"),
        ("secondary_cursors", "I-1", "degraded"),
        ("selections", "I-7", "degraded"),
    ];

    /// Derived fields: `(field, source_description)`.
    pub const DERIVED_FIELDS: &[(&str, &str)] = &[
        ("lines_dirty", "line equality diff (R-3)"),
        ("cursor_mode", "content_cursor_pos sign (I-3)"),
        ("status_line", "prompt + content concatenation"),
        ("editor_mode", "cursor_mode + mode_line (I-2)"),
        ("color_context", "default_face luminance analysis"),
    ];

    /// Fields grouped by epistemological category.
    pub const FIELDS_BY_CATEGORY: &[(&str, &[&str])] = &[
        (
            "observed",
            &[
                "lines",
                "default_face",
                "padding_face",
                "cursor_pos",
                "status_prompt",
                "status_content",
                "status_content_cursor_pos",
                "status_mode_line",
                "status_default_face",
                "status_style",
                "widget_columns",
                "menu",
                "infos",
                "ui_options",
            ],
        ),
        (
            "derived",
            &[
                "lines_dirty",
                "cursor_mode",
                "status_line",
                "editor_mode",
                "color_context",
            ],
        ),
        (
            "heuristic",
            &["cursor_count", "secondary_cursors", "selections"],
        ),
        (
            "config",
            &[
                "shadow_enabled",
                "padding_char",
                "menu_max_height",
                "menu_position",
                "search_dropdown",
                "status_at_top",
                "scrollbar_thumb",
                "scrollbar_track",
                "assistant_art",
                "plugin_config",
                "plugin_settings",
                "secondary_blend_ratio",
                "theme",
                "fold_toggle_state",
                "projection_policy",
            ],
        ),
        ("session", &["session_descriptors", "active_session_key"]),
        (
            "runtime",
            &[
                "focused",
                "drag",
                "cols",
                "rows",
                "hit_map",
                "cursor_cache",
                "display_scroll_offset",
                "display_map",
                "display_unit_map",
                "available_projections",
                "shadow_cursor",
            ],
        ),
    ];

    /// Fields that declared `salsa_opt_out`.
    pub const SALSA_OPT_OUTS: &[(&str, &str)] = &[
        (
            "lines_dirty",
            "consumed by paint.rs selective grid clear; not needed in Salsa projection",
        ),
        (
            "editor_mode",
            "consumed directly by view/widget; not surfaced in Salsa projection",
        ),
        (
            "color_context",
            "consumed directly by theme application; not surfaced in Salsa projection",
        ),
        (
            "selections",
            "consumed directly by paint.rs/view; not surfaced in Salsa projection",
        ),
        (
            "padding_char",
            "consumed directly by paint.rs padding renderer; not surfaced in Salsa projection",
        ),
        (
            "menu_max_height",
            "consumed directly by menu layout; not surfaced in Salsa projection",
        ),
        (
            "plugin_config",
            "consumed by plugins via AppView/registry; not surfaced in Salsa projection",
        ),
        (
            "plugin_settings",
            "consumed by plugins via AppView/registry; not surfaced in Salsa projection",
        ),
        (
            "theme",
            "consumed directly by paint.rs/render; not surfaced in Salsa projection",
        ),
        (
            "fold_toggle_state",
            "consumed by DisplayMap construction; not surfaced in Salsa projection",
        ),
        (
            "projection_policy",
            "consumed by DisplayMap construction; not surfaced in Salsa projection",
        ),
    ];
}

impl AppState {
    /// Available height (rows minus status bar).
    pub fn available_height(&self) -> u16 {
        self.runtime.rows.saturating_sub(1)
    }

    /// Range of visible line indices in the buffer.
    pub fn visible_line_range(&self) -> std::ops::Range<usize> {
        0..self.observed.lines.len()
    }

    /// Number of buffer lines currently loaded.
    pub fn buffer_line_count(&self) -> usize {
        self.observed.lines.len()
    }

    /// Whether a completion menu is currently shown.
    pub fn has_menu(&self) -> bool {
        self.observed.menu.is_some()
    }

    /// Whether any info popups are currently shown.
    pub fn has_info(&self) -> bool {
        !self.observed.infos.is_empty()
    }

    /// Whether the cursor is in prompt mode.
    pub fn is_prompt_mode(&self) -> bool {
        self.inference.cursor_mode == CursorMode::Prompt
    }

    /// Apply configuration from `Config` to the config sub-struct.
    pub fn apply_config(&mut self, config: &Config) {
        self.config.shadow_enabled = config.ui.shadow;
        self.config.padding_char = config.ui.padding_char.clone();
        self.config.menu_max_height = config.menu.max_height;
        self.config.menu_position = config.menu.position;
        self.config.search_dropdown = config.search.dropdown;
        self.config.status_at_top = config.ui.status_position == StatusPosition::Top;
        self.config.theme = Theme::from_config(&config.theme);
        self.config
            .theme
            .apply_color_context(&self.inference.color_context);
    }

    /// Reset session-owned UI state while preserving frontend configuration and dimensions.
    pub fn reset_for_session_switch(&mut self) {
        self.observed = ObservedState::default();
        self.inference = InferenceState::default();
        self.cursor_cache = derived::CursorCache::default();
        self.runtime.drag = DragState::None;
        self.runtime.shadow_cursor = None;
        self.config.fold_toggle_state = crate::display::FoldToggleState::default();
        self.config.projection_policy = crate::display::ProjectionPolicyState::default();
        // session, config (except fold_toggle_state/projection_policy), and runtime (except drag/shadow_cursor)
        // are intentionally preserved
    }

    /// Notify that a frame has been rendered; clears consumed ephemeral state.
    ///
    /// Cross-crate code (TUI/GUI backends) calls this instead of directly
    /// accessing `inference.lines_dirty` (which is `pub(crate)`).
    pub fn on_frame_rendered(&mut self) {
        self.inference.lines_dirty.clear();
    }
}

/// Apply a SetConfig command to AppState.
///
/// Known core keys are dispatched through the [`setting_registry`]; unknown
/// keys are stored in `plugin_config` for plugin-defined configuration, or
/// `unknown_options` for unknown non-dotted keys from the config path.
pub fn apply_set_config(state: &mut AppState, dirty: &mut DirtyFlags, key: &str, value: &str) {
    // Try the core setting registry first
    if let Some(d) = setting_registry::REGISTRY.apply(state, key, value) {
        *dirty |= d;
        return;
    }

    // Unknown key: route to plugin config or ui_options
    if key.contains('.') {
        // Plugin-namespaced keys (e.g. "color-preview.opacity")
        state
            .config
            .plugin_config
            .insert(key.to_string(), value.to_string());
    } else {
        state
            .observed
            .ui_options
            .insert(key.to_string(), value.to_string());
    }
    *dirty |= DirtyFlags::OPTIONS;
}

/// Apply a SetSetting command to AppState.
///
/// Stores the typed value in `plugin_settings` under the plugin's namespace.
pub fn apply_set_setting(
    state: &mut AppState,
    dirty: &mut DirtyFlags,
    plugin_id: &PluginId,
    key: &str,
    value: SettingValue,
) {
    state
        .config
        .plugin_settings
        .entry(plugin_id.clone())
        .or_default()
        .insert(key.to_string(), value);
    *dirty |= DirtyFlags::SETTINGS;
}
