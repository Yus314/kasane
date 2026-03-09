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
pub use menu::{MenuParams, MenuState};
pub use update::{Msg, update};

bitflags! {
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub struct DirtyFlags: u16 {
        const BUFFER          = 1 << 0;
        const STATUS          = 1 << 1;
        const MENU_STRUCTURE  = 1 << 2;
        const MENU_SELECTION  = 1 << 3;
        const INFO            = 1 << 4;
        const OPTIONS         = 1 << 5;

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

#[derive(Debug, Clone)]
pub struct AppState {
    // Buffer
    pub lines: Vec<Line>,
    pub default_face: Face,
    pub padding_face: Face,

    // Cursor (from set_cursor)
    pub cursor_mode: CursorMode,
    pub cursor_pos: Coord,

    // Status
    pub status_line: Line,
    pub status_mode_line: Line,
    pub status_default_face: Face,

    // Floating windows
    pub menu: Option<MenuState>,
    pub infos: Vec<InfoState>,

    // Options
    pub ui_options: HashMap<String, String>,

    // Focus
    pub focused: bool,

    // Config-driven UI settings
    pub shadow_enabled: bool,
    pub padding_char: String,
    pub menu_max_height: u16,
    pub menu_position: MenuPosition,
    pub search_dropdown: bool,
    pub status_at_top: bool,

    // Derived state
    pub cursor_count: usize,

    // Mouse drag state
    pub drag: DragState,

    // Scroll settings
    pub smooth_scroll: bool,

    // Scroll animation state
    pub scroll_animation: Option<ScrollAnimation>,

    // Screen size
    pub cols: u16,
    pub rows: u16,
}

impl AppState {
    /// ステータスバー行を除いた利用可能な高さを返す。
    pub fn available_height(&self) -> u16 {
        self.rows.saturating_sub(1)
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
}

/// スクロールアニメーションを1フレーム進める。
/// アニメーションが存在した場合 true を返す。
pub fn tick_scroll_animation(state: &mut AppState, kak_writer: &mut impl std::io::Write) -> bool {
    let Some(ref mut anim) = state.scroll_animation else {
        return false;
    };
    let step = anim.step.min(anim.remaining.abs()) * anim.remaining.signum();
    let req = KasaneRequest::Scroll {
        amount: step,
        line: anim.line,
        column: anim.column,
    };
    crate::io::send_request(kak_writer, &req);
    anim.remaining -= step;
    if anim.remaining == 0 {
        state.scroll_animation = None;
    }
    true
}

impl Default for AppState {
    fn default() -> Self {
        AppState {
            lines: Vec::new(),
            default_face: Face::default(),
            padding_face: Face::default(),
            cursor_mode: CursorMode::Buffer,
            cursor_pos: Coord::default(),
            status_line: Vec::new(),
            status_mode_line: Vec::new(),
            status_default_face: Face::default(),
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
            cursor_count: 0,
            drag: DragState::None,
            smooth_scroll: false,
            scroll_animation: None,
            cols: 80,
            rows: 24,
        }
    }
}
