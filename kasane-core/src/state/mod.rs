mod apply;
#[cfg(test)]
#[allow(clippy::field_reassign_with_default)]
mod tests;
mod update;

use std::collections::HashMap;

use bitflags::bitflags;

use crate::config::{Config, MenuPosition, StatusPosition};
use crate::input::MouseButton;
use crate::layout::line_display_width;
use crate::protocol::{Coord, CursorMode, Face, InfoStyle, KasaneRequest, Line, MenuStyle};

pub use update::{Msg, update};

bitflags! {
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub struct DirtyFlags: u8 {
        const BUFFER  = 0b0000_0001;
        const STATUS  = 0b0000_0010;
        const MENU    = 0b0000_0100;
        const INFO    = 0b0000_1000;
        const OPTIONS = 0b0001_0000;
        const ALL     = 0b0001_1111;
    }
}

#[derive(Debug, Clone)]
pub struct MenuState {
    pub items: Vec<Line>,
    pub anchor: Coord,
    pub selected_item_face: Face,
    pub menu_face: Face,
    pub style: MenuStyle,
    pub selected: Option<usize>,
    /// Scroll offset: index of the first visible item.
    pub first_item: usize,
    /// Number of display columns (1 for Search/Inline, N for Prompt).
    pub columns: u16,
    /// Number of visible rows in the menu window.
    pub win_height: u16,
    /// Total logical rows = ceil(items / columns).
    pub menu_lines: u16,
    /// Maximum display width of any single item.
    pub max_item_width: u16,
    /// Screen width (used for Search scroll calculation).
    pub screen_w: u16,
}

impl MenuState {
    /// Create a new MenuState with derived layout fields computed from items and screen dimensions.
    ///
    /// `screen_h` is the available height **excluding** the status bar row
    /// (i.e. `rows.saturating_sub(1)`).
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        items: Vec<Line>,
        anchor: Coord,
        selected_item_face: Face,
        menu_face: Face,
        style: MenuStyle,
        screen_w: u16,
        screen_h: u16,
        max_height_config: u16,
    ) -> Self {
        let max_item_width = items
            .iter()
            .map(line_display_width)
            .max()
            .unwrap_or(1)
            .max(1) as u16;

        let columns: u16 = match style {
            MenuStyle::Search | MenuStyle::Inline => 1,
            MenuStyle::Prompt => {
                // -1 for scrollbar column (matches Kakoune terminal_ui.cc:
                // max_width = m_dimensions.column - 1)
                ((screen_w.saturating_sub(1)) as usize / (max_item_width as usize + 1)).max(1)
                    as u16
            }
        };

        let max_height = match style {
            MenuStyle::Search => 1u16,
            MenuStyle::Inline => {
                let above = anchor.line as u16;
                let below = screen_h.saturating_sub(anchor.line as u16 + 1);
                max_height_config.min(above.max(below))
            }
            MenuStyle::Prompt => max_height_config.min(screen_h),
        };

        let item_count = items.len();
        let cols = columns as usize;
        let menu_lines = item_count.div_ceil(cols) as u16;
        let win_height = menu_lines.min(max_height);

        Self {
            items,
            anchor,
            selected_item_face,
            menu_face,
            style,
            selected: None,
            first_item: 0,
            columns,
            win_height,
            menu_lines,
            max_item_width,
            screen_w,
        }
    }

    /// Update selection and adjust scroll offset to keep the selected item visible.
    pub fn select(&mut self, selected: i32) {
        self.selected = usize::try_from(selected)
            .ok()
            .filter(|&i| i < self.items.len());
        if self.selected.is_none() || self.win_height == 0 {
            self.selected = None;
            self.first_item = 0;
            return;
        }
        match self.style {
            MenuStyle::Inline | MenuStyle::Prompt => self.scroll_column_based(),
            MenuStyle::Search => self.scroll_search(),
        }
    }

    /// Inline & Prompt: column-based scrolling (stride = win_height).
    /// Matches Kakoune terminal_ui.cc menu_select.
    fn scroll_column_based(&mut self) {
        let selected = self.selected.unwrap(); // caller guarantees Some
        let stride = self.win_height as usize;
        let selected_col = selected / stride;
        let first_col = self.first_item / stride;
        let columns = self.columns as usize;
        let menu_cols = self.items.len().div_ceil(stride);
        if selected_col < first_col {
            self.first_item = selected_col * stride;
        } else if selected_col >= first_col + columns {
            self.first_item = selected_col.min(menu_cols.saturating_sub(columns)) * stride;
        }
    }

    /// Search: stateless horizontal scroll (matches Kakoune terminal_ui.cc).
    ///
    /// Scans forward from item 0 to `self.selected`, tracking cumulative width.
    /// When adding an item would exceed the available width, resets the window
    /// start to that item. This produces a deterministic `first_item` that
    /// depends only on `selected`, not on previous scroll state.
    fn scroll_search(&mut self) {
        let selected = self.selected.unwrap(); // caller guarantees Some
        // Reserve 3 columns for "< " prefix (2) and ">" suffix (1),
        // matching Kakoune's `m_menu.size.column - 3`.
        let width = self.screen_w.saturating_sub(3) as usize;
        let mut first = 0usize;
        let mut item_col = 0usize;
        for i in 0..=selected {
            let item_w = self.items.get(i).map(line_display_width).unwrap_or(0) + 1;
            if item_col + item_w > width {
                first = i;
                item_col = item_w;
            } else {
                item_col += item_w;
            }
        }
        self.first_item = first;
    }
}

/// Identity key for deduplicating simultaneous info popups.
/// Infos with the same identity replace each other; different identities coexist.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InfoIdentity {
    pub style: InfoStyle,
    pub anchor_line: u32,
}

#[derive(Debug, Clone)]
pub struct InfoState {
    pub title: Line,
    pub content: Vec<Line>,
    pub anchor: Coord,
    pub face: Face,
    pub style: InfoStyle,
    pub identity: InfoIdentity,
    pub scroll_offset: u16,
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
    let _ = writeln!(kak_writer, "{}", req.to_json());
    let _ = kak_writer.flush();
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
