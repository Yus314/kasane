mod apply;
mod info;
mod menu;
#[cfg(test)]
#[allow(clippy::field_reassign_with_default)]
mod tests;
mod update;

use std::collections::HashMap;

use bitflags::bitflags;

use crate::config::{Config, MenuPosition, StatusPosition};
use crate::input::MouseButton;
use crate::protocol::{Coord, CursorMode, Face, KasaneRequest, Line};

pub use info::{InfoIdentity, InfoState};
pub use menu::{ItemSplit, MenuColumns, MenuParams, MenuState, split_single_item};
pub use update::{Msg, update};

bitflags! {
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

        /// Composite: any buffer-related change.
        const BUFFER = Self::BUFFER_CONTENT.bits() | Self::BUFFER_CURSOR.bits();
        const MENU = Self::MENU_STRUCTURE.bits() | Self::MENU_SELECTION.bits();
        const ALL  = Self::BUFFER.bits() | Self::STATUS.bits()
                   | Self::MENU.bits() | Self::INFO.bits() | Self::OPTIONS.bits();
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

/// State for smooth scroll animation.
#[derive(Debug, Clone, Default)]
pub struct ScrollAnimation {
    /// Remaining scroll amount (positive=down, negative=up).
    pub remaining: i32,
    /// Scroll amount per frame.
    pub step: i32,
    /// Mouse coordinates that initiated the scroll.
    pub line: u32,
    pub column: u32,
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
#[derive(Debug, Clone)]
pub struct AppState {
    // -- Protocol State (from Kakoune JSON-RPC) --
    /// Observed: buffer lines from `draw`.
    pub lines: Vec<Line>,
    /// Observed: default face from `draw`.
    pub default_face: Face,
    /// Observed: padding face from `draw`.
    pub padding_face: Face,
    /// Derived: per-line dirty flags computed by diffing old vs new `lines`.
    pub lines_dirty: Vec<bool>,
    /// Derived: inferred from `status_content_cursor_pos >= 0` (Buffer vs Prompt).
    pub cursor_mode: CursorMode,
    /// Observed: cursor position from `draw` (`cursor_pos` field).
    pub cursor_pos: Coord,
    /// Observed: status prompt atoms from `draw_status`.
    pub status_prompt: Line,
    /// Observed: status content atoms from `draw_status`.
    pub status_content: Line,
    /// Observed: cursor position within status content from `draw_status`.
    pub status_content_cursor_pos: i32,
    /// Derived: concatenation of `status_prompt` + `status_content` for rendering.
    pub status_line: Line,
    /// Observed: mode line atoms from `draw_status`.
    pub status_mode_line: Line,
    /// Observed: default face for the status bar from `draw_status`.
    pub status_default_face: Face,
    /// Observed: number of widget columns from `draw`.
    pub widget_columns: u16,
    /// Observed: completion menu state from `menu_show` / `menu_select` / `menu_hide`.
    pub menu: Option<MenuState>,
    /// Observed: info popup state from `info_show` / `info_hide`.
    pub infos: Vec<InfoState>,
    /// Observed: UI options from `set_ui_options`.
    pub ui_options: HashMap<String, String>,
    /// Heuristic: total cursor count (primary + secondary), detected via FINAL_FG + REVERSE
    /// attribute pattern in `draw` atoms. Not part of the protocol specification.
    pub cursor_count: usize,
    /// Heuristic: positions of secondary cursors (all cursors except primary).
    /// Extracted from `draw` atoms whose face has FINAL_FG + REVERSE attributes, then
    /// filtered to exclude the primary `cursor_pos`. This relies on Kakoune's internal
    /// rendering of multi-cursor selections and may change in future versions.
    pub secondary_cursors: Vec<Coord>,

    // -- Frontend Config (from user config / SetConfig commands) --
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
    pub secondary_blend_ratio: f32,
    pub smooth_scroll: bool,

    // -- Runtime / Ephemeral (not part of protocol or config) --
    pub focused: bool,
    pub drag: DragState,
    pub scroll_animation: Option<ScrollAnimation>,
    pub cols: u16,
    pub rows: u16,
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
        self.menu_position = config.menu.menu_position();
        self.search_dropdown = config.search.dropdown;
        self.status_at_top = config.ui.status_position() == StatusPosition::Top;
        self.smooth_scroll = config.scroll.smooth;
    }

    /// Reset session-owned UI state while preserving frontend configuration and dimensions.
    pub fn reset_for_session_switch(&mut self) {
        let cols = self.cols;
        let rows = self.rows;
        let focused = self.focused;
        let shadow_enabled = self.shadow_enabled;
        let padding_char = self.padding_char.clone();
        let menu_max_height = self.menu_max_height;
        let menu_position = self.menu_position;
        let search_dropdown = self.search_dropdown;
        let status_at_top = self.status_at_top;
        let scrollbar_thumb = self.scrollbar_thumb.clone();
        let scrollbar_track = self.scrollbar_track.clone();
        let assistant_art = self.assistant_art.clone();
        let plugin_config = self.plugin_config.clone();
        let secondary_blend_ratio = self.secondary_blend_ratio;
        let smooth_scroll = self.smooth_scroll;

        *self = Self::default();
        self.cols = cols;
        self.rows = rows;
        self.focused = focused;
        self.shadow_enabled = shadow_enabled;
        self.padding_char = padding_char;
        self.menu_max_height = menu_max_height;
        self.menu_position = menu_position;
        self.search_dropdown = search_dropdown;
        self.status_at_top = status_at_top;
        self.scrollbar_thumb = scrollbar_thumb;
        self.scrollbar_track = scrollbar_track;
        self.assistant_art = assistant_art;
        self.plugin_config = plugin_config;
        self.secondary_blend_ratio = secondary_blend_ratio;
        self.smooth_scroll = smooth_scroll;
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
        "smooth_scroll" => {
            state.smooth_scroll = value == "true";
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
            if key.contains('.') {
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

/// Advance the scroll animation by one frame.
/// Returns `Some(Command)` with the scroll request if animation is active, `None` otherwise.
pub fn tick_scroll_animation(state: &mut AppState) -> Option<crate::plugin::Command> {
    let anim = state.scroll_animation.as_mut()?;
    let step = anim.step.min(anim.remaining.abs()) * anim.remaining.signum();
    let req = KasaneRequest::Scroll {
        amount: step,
        line: anim.line,
        column: anim.column,
    };
    anim.remaining -= step;
    if anim.remaining == 0 {
        state.scroll_animation = None;
    }
    Some(crate::plugin::Command::SendToKakoune(req))
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
            drag: DragState::None,
            secondary_blend_ratio: 0.4,
            smooth_scroll: false,
            scroll_animation: None,
            cols: 80,
            rows: 24,
        }
    }
}
