//! Application state: `AppState`, `apply()`, `update()`, dirty generation tracking.

mod apply;
pub mod derived;
mod info;
mod menu;
pub mod snapshot;
#[cfg(test)]
#[allow(clippy::field_reassign_with_default)]
mod tests;
mod update;

use std::collections::HashMap;

use bitflags::bitflags;

use crate::DirtyTracked;
use crate::config::{Config, MenuPosition, StatusPosition};
use crate::display::DisplayMapRef;
use crate::input::MouseButton;
use crate::layout::HitMap;
use crate::protocol::{Coord, CursorMode, Face, Line, StatusStyle};
use crate::render::color_context::ColorContext;
use crate::render::theme::Theme;
use crate::scroll::{
    SMOOTH_SCROLL_CONFIG_KEY, is_smooth_scroll_config_key, set_smooth_scroll_enabled,
};
use crate::session::SessionDescriptor;

pub use info::{InfoIdentity, InfoState};
pub use menu::{ItemSplit, MenuColumns, MenuParams, MenuState, split_single_item};
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

        /// Composite: any buffer-related change.
        const BUFFER = Self::BUFFER_CONTENT.bits() | Self::BUFFER_CURSOR.bits();
        const MENU = Self::MENU_STRUCTURE.bits() | Self::MENU_SELECTION.bits();
        const ALL  = Self::BUFFER.bits() | Self::STATUS.bits()
                   | Self::MENU.bits() | Self::INFO.bits() | Self::OPTIONS.bits()
                   | Self::PLUGIN_STATE.bits() | Self::SESSION.bits();
    }
}

/// Drag state for mouse selection tracking.
#[derive(Debug, Clone, PartialEq, Default)]
pub enum DragState {
    #[default]
    None,
    Active {
        button: MouseButton,
        start_line: u32,
        start_column: u32,
    },
}

/// The central application state, updated by Kakoune JSON-RPC messages via [`apply`](AppState::apply).
///
/// Fields are classified into three epistemological categories:
///
/// - **Observed**: Direct 1:1 mapping from Kakoune JSON-RPC protocol messages. No transformation
///   is applied; the value is stored exactly as received from the upstream `draw`, `draw_status`,
///   `menu_show`, `info_show`, or `set_ui_options` request.
///
/// - **Derived**: Deterministically computed from Observed fields. The derivation logic is
///   straightforward and has stable semantics (e.g., concatenation, comparison).
///
/// - **Heuristic**: Inferred from upstream internal implementation details that are not part of
///   the protocol specification. These fields rely on assumptions about how Kakoune renders
///   certain UI elements (e.g., cursor face attributes) and may break if Kakoune changes its
///   rendering behavior in future versions.
///
/// Every field carries a `#[dirty(...)]` annotation that maps it to DirtyFlags
/// and an `#[epistemic(...)]` annotation classifying its epistemological category.
/// The `DirtyTracked` derive enforces both at compile time: adding a field without
/// either annotation is a compile error.
#[derive(Debug, Clone, DirtyTracked)]
pub struct AppState {
    // -- Protocol State (from Kakoune JSON-RPC) --
    /// Observed: buffer lines from `draw`.
    #[epistemic(observed)]
    #[dirty(BUFFER_CONTENT)]
    pub lines: Vec<Line>,
    /// Observed: default face from `draw`.
    #[epistemic(observed)]
    #[dirty(BUFFER_CONTENT)]
    pub default_face: Face,
    /// Observed: padding face from `draw`.
    #[epistemic(observed)]
    #[dirty(BUFFER_CONTENT)]
    pub padding_face: Face,
    /// Derived: per-line dirty flags computed by diffing old vs new `lines`.
    #[epistemic(derived, source = "line equality diff (R-3)")]
    #[dirty(BUFFER_CONTENT)]
    pub lines_dirty: Vec<bool>,
    /// Derived: inferred from `status_content_cursor_pos >= 0` (Buffer vs Prompt).
    #[epistemic(derived, source = "content_cursor_pos sign (I-3)")]
    #[dirty(BUFFER_CURSOR)]
    pub cursor_mode: CursorMode,
    /// Observed: cursor position from `draw` (`cursor_pos` field).
    #[epistemic(observed)]
    #[dirty(BUFFER_CURSOR)]
    pub cursor_pos: Coord,
    /// Observed: status prompt atoms from `draw_status`.
    #[epistemic(observed)]
    #[dirty(STATUS)]
    pub status_prompt: Line,
    /// Observed: status content atoms from `draw_status`.
    #[epistemic(observed)]
    #[dirty(STATUS)]
    pub status_content: Line,
    /// Observed: cursor position within status content from `draw_status`.
    #[epistemic(observed)]
    #[dirty(STATUS)]
    pub status_content_cursor_pos: i32,
    /// Derived: concatenation of `status_prompt` + `status_content` for rendering.
    #[epistemic(derived, source = "prompt + content concatenation")]
    #[dirty(STATUS)]
    pub status_line: Line,
    /// Observed: mode line atoms from `draw_status`.
    #[epistemic(observed)]
    #[dirty(STATUS)]
    pub status_mode_line: Line,
    /// Observed: default face for the status bar from `draw_status`.
    #[epistemic(observed)]
    #[dirty(STATUS)]
    pub status_default_face: Face,
    /// Observed: status bar context from `draw_status` (PR #5458).
    #[epistemic(observed)]
    #[dirty(STATUS)]
    pub status_style: StatusStyle,
    /// Observed: number of widget columns from `draw`.
    #[epistemic(observed)]
    #[dirty(BUFFER_CONTENT)]
    pub widget_columns: u16,
    /// Observed: completion menu state from `menu_show` / `menu_select` / `menu_hide`.
    #[epistemic(observed)]
    #[dirty(MENU_STRUCTURE, MENU_SELECTION)]
    pub menu: Option<MenuState>,
    /// Observed: info popup state from `info_show` / `info_hide`.
    #[epistemic(observed)]
    #[dirty(INFO)]
    pub infos: Vec<InfoState>,
    /// Observed: UI options from `set_ui_options`.
    #[epistemic(observed)]
    #[dirty(OPTIONS)]
    pub ui_options: HashMap<String, String>,
    /// Heuristic: total cursor count (primary + secondary), detected via FINAL_FG + REVERSE
    /// attribute pattern in `draw` atoms. Not part of the protocol specification.
    #[epistemic(heuristic, rule = "I-1", severity = "degraded")]
    #[dirty(BUFFER_CURSOR)]
    pub cursor_count: usize,
    /// Heuristic: positions of secondary cursors (all cursors except primary).
    /// Extracted from `draw` atoms whose face has FINAL_FG + REVERSE attributes, then
    /// filtered to exclude the primary `cursor_pos`. This relies on Kakoune's internal
    /// rendering of multi-cursor selections and may change in future versions.
    #[epistemic(heuristic, rule = "I-1", severity = "degraded")]
    #[dirty(BUFFER_CURSOR)]
    pub secondary_cursors: Vec<Coord>,
    /// Derived: parsed editor mode from cursor_mode + status_mode_line heuristic (I-2).
    #[epistemic(derived, source = "cursor_mode + mode_line (I-2)")]
    #[dirty(STATUS)]
    pub editor_mode: derived::EditorMode,

    /// Heuristic: detected selection ranges from buffer atoms (I-7).
    #[epistemic(heuristic, rule = "I-7", severity = "degraded")]
    #[dirty(BUFFER_CONTENT)]
    pub selections: Vec<derived::Selection>,

    // -- Frontend Config (from user config / SetConfig commands) --
    #[epistemic(config)]
    #[dirty(OPTIONS)]
    pub shadow_enabled: bool,
    #[epistemic(config)]
    #[dirty(OPTIONS)]
    pub padding_char: String,
    #[epistemic(config)]
    #[dirty(OPTIONS)]
    pub menu_max_height: u16,
    #[epistemic(config)]
    #[dirty(OPTIONS)]
    pub menu_position: MenuPosition,
    #[epistemic(config)]
    #[dirty(OPTIONS)]
    pub search_dropdown: bool,
    #[epistemic(config)]
    #[dirty(OPTIONS)]
    pub status_at_top: bool,
    #[epistemic(config)]
    #[dirty(MENU_STRUCTURE)]
    pub scrollbar_thumb: String,
    #[epistemic(config)]
    #[dirty(MENU_STRUCTURE)]
    pub scrollbar_track: String,
    #[epistemic(config)]
    #[dirty(OPTIONS)]
    pub assistant_art: Option<Vec<String>>,
    #[epistemic(config)]
    #[dirty(OPTIONS)]
    pub plugin_config: HashMap<String, String>,
    #[epistemic(config)]
    #[dirty(BUFFER_CONTENT)]
    pub secondary_blend_ratio: f32,
    #[epistemic(config)]
    #[dirty(OPTIONS)]
    pub theme: Theme,
    #[epistemic(derived, source = "default_face luminance analysis")]
    #[dirty(BUFFER_CONTENT)]
    pub color_context: ColorContext,

    // -- Session metadata (from SessionManager, preserved across session switches) --
    #[epistemic(session)]
    #[dirty(SESSION)]
    pub session_descriptors: Vec<SessionDescriptor>,
    #[epistemic(session)]
    #[dirty(SESSION)]
    pub active_session_key: Option<String>,

    // -- Runtime / Ephemeral (not part of protocol or config) --
    #[epistemic(runtime)]
    #[dirty(free)]
    pub focused: bool,
    #[epistemic(runtime)]
    #[dirty(free)]
    pub drag: DragState,
    #[epistemic(runtime)]
    #[dirty(free)]
    pub cols: u16,
    #[epistemic(runtime)]
    #[dirty(free)]
    pub rows: u16,
    /// Post-render hit map for interactive element mouse routing.
    /// Updated after each frame by `rebuild_hit_map()`.
    #[epistemic(runtime)]
    #[dirty(free)]
    pub hit_map: HitMap,
    /// Cached per-line cursor positions for incremental `detect_cursors`.
    #[epistemic(runtime)]
    #[dirty(free)]
    pub cursor_cache: derived::CursorCache,
    /// Display scroll offset from the last rendered frame.
    /// Used by mouse input to translate screen coordinates to display line coordinates.
    /// Set by the rendering pipeline after each frame; not part of protocol state.
    #[epistemic(runtime)]
    #[dirty(free)]
    pub display_scroll_offset: usize,
    /// Display map from the last rendered frame.
    /// Used by mouse input to translate display-space coordinates to buffer-space.
    /// Set by the rendering pipeline after each frame; not part of protocol state.
    #[epistemic(runtime)]
    #[dirty(free)]
    pub display_map: Option<DisplayMapRef>,
    /// Display unit map from the last rendered frame.
    /// Built from `display_map` when non-identity. Used by input dispatch for
    /// display-unit-aware event routing. `None` when no display transforms are active.
    #[epistemic(runtime)]
    #[dirty(free)]
    pub display_unit_map: Option<crate::display::DisplayUnitMap>,
    /// Core fold toggle state: tracks which fold ranges are currently expanded.
    /// Consulted during DisplayMap construction to filter out expanded folds.
    #[epistemic(runtime)]
    #[dirty(free)]
    pub fold_toggle_state: crate::display::FoldToggleState,
}

impl AppState {
    /// ステータスバー行を除いた利用可能な高さを返す。
    pub fn available_height(&self) -> u16 {
        self.rows.saturating_sub(1)
    }

    /// Range of visible line indices in the buffer.
    pub fn visible_line_range(&self) -> std::ops::Range<usize> {
        0..self.lines.len()
    }

    /// Number of buffer lines currently loaded.
    pub fn buffer_line_count(&self) -> usize {
        self.lines.len()
    }

    /// Whether a completion menu is currently shown.
    pub fn has_menu(&self) -> bool {
        self.menu.is_some()
    }

    /// Whether any info popups are currently shown.
    pub fn has_info(&self) -> bool {
        !self.infos.is_empty()
    }

    /// Whether the cursor is in prompt mode.
    pub fn is_prompt_mode(&self) -> bool {
        self.cursor_mode == CursorMode::Prompt
    }

    /// Config の設定を AppState に適用する。
    pub fn apply_config(&mut self, config: &Config) {
        self.shadow_enabled = config.ui.shadow;
        self.padding_char = config.ui.padding_char.clone();
        self.menu_max_height = config.menu.max_height;
        self.menu_position = config.menu.position;
        self.search_dropdown = config.search.dropdown;
        self.status_at_top = config.ui.status_position == StatusPosition::Top;
        set_smooth_scroll_enabled(&mut self.plugin_config, config.scroll.smooth);
        self.theme = Theme::from_config(&config.theme);
        self.theme.apply_color_context(&self.color_context);
    }

    /// Reset session-owned UI state while preserving frontend configuration and dimensions.
    ///
    /// Uses exhaustive destructure of `Self::default()` so that adding a new field
    /// to `AppState` produces a compile error here, forcing an explicit decision
    /// on whether the field should be reset or preserved across session switches.
    pub fn reset_for_session_switch(&mut self) {
        let d = Self::default();
        let AppState {
            // === RESET: move default values into self ===
            lines,
            default_face,
            padding_face,
            lines_dirty,
            cursor_mode,
            cursor_pos,
            status_prompt,
            status_content,
            status_content_cursor_pos,
            status_line,
            status_mode_line,
            status_default_face,
            status_style,
            widget_columns,
            menu,
            infos,
            ui_options,
            cursor_count,
            secondary_cursors,
            editor_mode,
            selections,
            color_context,
            drag,
            // === PRESERVE: discard defaults, keep current values ===
            cols: _,
            rows: _,
            focused: _,
            hit_map: _,
            cursor_cache,
            shadow_enabled: _,
            padding_char: _,
            menu_max_height: _,
            menu_position: _,
            search_dropdown: _,
            status_at_top: _,
            scrollbar_thumb: _,
            scrollbar_track: _,
            assistant_art: _,
            plugin_config: _,
            secondary_blend_ratio: _,
            theme: _,
            session_descriptors: _,
            active_session_key: _,
            display_scroll_offset: _,
            display_map: _,
            display_unit_map: _,
            fold_toggle_state,
        } = d;

        self.lines = lines;
        self.default_face = default_face;
        self.padding_face = padding_face;
        self.lines_dirty = lines_dirty;
        self.cursor_mode = cursor_mode;
        self.cursor_pos = cursor_pos;
        self.status_prompt = status_prompt;
        self.status_content = status_content;
        self.status_content_cursor_pos = status_content_cursor_pos;
        self.status_line = status_line;
        self.status_mode_line = status_mode_line;
        self.status_default_face = status_default_face;
        self.status_style = status_style;
        self.widget_columns = widget_columns;
        self.menu = menu;
        self.infos = infos;
        self.ui_options = ui_options;
        self.cursor_count = cursor_count;
        self.secondary_cursors = secondary_cursors;
        self.editor_mode = editor_mode;
        self.selections = selections;
        self.color_context = color_context;
        self.drag = drag;
        self.cursor_cache = cursor_cache;
        self.fold_toggle_state = fold_toggle_state;
    }
}

/// Apply a SetConfig command to AppState.
///
/// Known keys are dispatched to their respective fields; unknown keys are
/// stored in `plugin_config` for plugin-defined configuration.
pub fn apply_set_config(state: &mut AppState, dirty: &mut DirtyFlags, key: &str, value: &str) {
    match key {
        "shadow_enabled" => {
            state.shadow_enabled = value == "true";
            *dirty |= DirtyFlags::OPTIONS;
        }
        "padding_char" => {
            state.padding_char = value.to_string();
            *dirty |= DirtyFlags::BUFFER_CONTENT;
        }
        "search_dropdown" => {
            state.search_dropdown = value == "true";
            *dirty |= DirtyFlags::OPTIONS;
        }
        "status_at_top" => {
            state.status_at_top = value == "true";
            *dirty |= DirtyFlags::OPTIONS;
        }
        key if is_smooth_scroll_config_key(key) => {
            set_smooth_scroll_enabled(&mut state.plugin_config, value == "true");
            *dirty |= DirtyFlags::OPTIONS;
        }
        "cursor.secondary_blend" => {
            if let Ok(ratio) = value.parse::<f32>() {
                state.secondary_blend_ratio = ratio.clamp(0.0, 1.0);
                *dirty |= DirtyFlags::BUFFER_CONTENT;
            }
        }
        "scrollbar.thumb" => {
            state.scrollbar_thumb = value.to_string();
            *dirty |= DirtyFlags::MENU_STRUCTURE;
        }
        "scrollbar.track" => {
            state.scrollbar_track = value.to_string();
            *dirty |= DirtyFlags::MENU_STRUCTURE;
        }
        _ => {
            // Unknown keys go to ui_options (for Kakoune ui_options) or plugin_config
            if key == SMOOTH_SCROLL_CONFIG_KEY || key.contains('.') {
                // Plugin-namespaced keys (e.g. "color-preview.opacity")
                state
                    .plugin_config
                    .insert(key.to_string(), value.to_string());
            } else {
                state.ui_options.insert(key.to_string(), value.to_string());
            }
            *dirty |= DirtyFlags::OPTIONS;
        }
    }
}

impl Default for AppState {
    fn default() -> Self {
        AppState {
            lines: Vec::new(),
            default_face: Face::default(),
            padding_face: Face::default(),
            lines_dirty: Vec::new(),
            cursor_mode: CursorMode::Buffer,
            cursor_pos: Coord::default(),
            status_prompt: Vec::new(),
            status_content: Vec::new(),
            status_content_cursor_pos: -1,
            status_line: Vec::new(),
            status_mode_line: Vec::new(),
            status_default_face: Face::default(),
            status_style: StatusStyle::default(),
            widget_columns: 0,
            menu: None,
            infos: Vec::new(),
            ui_options: HashMap::new(),
            focused: true,
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
            cursor_count: 0,
            secondary_cursors: Vec::new(),
            editor_mode: derived::EditorMode::default(),
            selections: Vec::new(),
            session_descriptors: Vec::new(),
            active_session_key: None,
            drag: DragState::None,
            secondary_blend_ratio: 0.4,
            theme: Theme::default_theme(),
            color_context: ColorContext::default(),
            cols: 80,
            rows: 24,
            hit_map: HitMap::new(),
            cursor_cache: derived::CursorCache::default(),
            display_scroll_offset: 0,
            display_map: None,
            display_unit_map: None,
            fold_toggle_state: crate::display::FoldToggleState::default(),
        }
    }
}
