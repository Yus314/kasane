use std::collections::HashMap;

use bitflags::bitflags;

use crate::layout::line_display_width;
use crate::protocol::{Coord, CursorMode, Face, InfoStyle, KakouneRequest, Line, MenuStyle};

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
    pub fn new(
        items: Vec<Line>,
        anchor: Coord,
        selected_item_face: Face,
        menu_face: Face,
        style: MenuStyle,
        screen_w: u16,
        screen_h: u16,
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
                // -1 for scrollbar column
                ((screen_w.saturating_sub(1)) as usize / (max_item_width as usize + 1)).max(1)
                    as u16
            }
        };

        let max_height = match style {
            MenuStyle::Search => 1u16,
            MenuStyle::Inline => {
                let above = anchor.line as u16;
                let below = screen_h.saturating_sub(anchor.line as u16 + 1);
                10u16.min(above.max(below))
            }
            MenuStyle::Prompt => 10u16.min(screen_h),
        };

        let item_count = items.len();
        let cols = columns as usize;
        let menu_lines = ((item_count + cols - 1) / cols) as u16;
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
        let menu_cols = (self.items.len() + stride - 1) / stride;
        if selected_col < first_col {
            self.first_item = selected_col * stride;
        } else if selected_col >= first_col + columns {
            self.first_item =
                selected_col.min(menu_cols.saturating_sub(columns)) * stride;
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

#[derive(Debug, Clone)]
pub struct InfoState {
    pub title: Line,
    pub content: Vec<Line>,
    pub anchor: Coord,
    pub face: Face,
    pub style: InfoStyle,
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
    pub info: Option<InfoState>,

    // Options
    pub ui_options: HashMap<String, String>,

    // Screen size
    pub cols: u16,
    pub rows: u16,
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
            info: None,
            ui_options: HashMap::new(),
            cols: 80,
            rows: 24,
        }
    }
}

impl AppState {
    pub fn apply(&mut self, request: KakouneRequest) -> DirtyFlags {
        match request {
            KakouneRequest::Draw {
                lines,
                default_face,
                padding_face,
            } => {
                self.lines = lines;
                self.default_face = default_face;
                self.padding_face = padding_face;
                DirtyFlags::BUFFER
            }
            KakouneRequest::DrawStatus {
                status_line,
                mode_line,
                default_face,
            } => {
                self.status_line = status_line;
                self.status_mode_line = mode_line;
                self.status_default_face = default_face;
                DirtyFlags::STATUS
            }
            KakouneRequest::SetCursor { mode, coord } => {
                self.cursor_mode = mode;
                self.cursor_pos = coord;
                DirtyFlags::BUFFER
            }
            KakouneRequest::MenuShow {
                items,
                anchor,
                selected_item_face,
                menu_face,
                style,
            } => {
                let screen_h = self.rows.saturating_sub(1);
                self.menu = Some(MenuState::new(
                    items, anchor, selected_item_face, menu_face, style, self.cols, screen_h,
                ));
                DirtyFlags::MENU
            }
            KakouneRequest::MenuSelect { selected } => {
                if let Some(menu) = &mut self.menu {
                    tracing::debug!(
                        "MenuSelect: selected={}, first_item={}, win_height={}, items={}, columns={}",
                        selected, menu.first_item, menu.win_height, menu.items.len(), menu.columns,
                    );
                    menu.select(selected);
                }
                DirtyFlags::MENU
            }
            KakouneRequest::MenuHide => {
                self.menu = None;
                DirtyFlags::MENU | DirtyFlags::BUFFER
            }
            KakouneRequest::InfoShow {
                title,
                content,
                anchor,
                face,
                style,
            } => {
                self.info = Some(InfoState {
                    title,
                    content,
                    anchor,
                    face,
                    style,
                });
                DirtyFlags::INFO
            }
            KakouneRequest::InfoHide => {
                self.info = None;
                DirtyFlags::INFO | DirtyFlags::BUFFER
            }
            KakouneRequest::SetUiOptions { options } => {
                self.ui_options = options;
                DirtyFlags::OPTIONS
            }
            KakouneRequest::Refresh { force } => {
                if force {
                    DirtyFlags::ALL
                } else {
                    DirtyFlags::BUFFER | DirtyFlags::STATUS
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::protocol::{Atom, Face};

    fn make_line(s: &str) -> Line {
        vec![Atom {
            face: Face::default(),
            contents: s.to_string(),
        }]
    }

    #[test]
    fn test_apply_draw() {
        let mut state = AppState::default();
        let flags = state.apply(KakouneRequest::Draw {
            lines: vec![make_line("hello")],
            default_face: Face::default(),
            padding_face: Face::default(),
        });
        assert!(flags.contains(DirtyFlags::BUFFER));
        assert_eq!(state.lines.len(), 1);
    }

    #[test]
    fn test_apply_set_cursor() {
        let mut state = AppState::default();
        let flags = state.apply(KakouneRequest::SetCursor {
            mode: CursorMode::Buffer,
            coord: Coord { line: 0, column: 3 },
        });
        assert!(flags.contains(DirtyFlags::BUFFER));
        assert_eq!(state.cursor_pos.column, 3);
        assert_eq!(state.cursor_mode, CursorMode::Buffer);
    }

    #[test]
    fn test_apply_draw_status() {
        let mut state = AppState::default();
        let flags = state.apply(KakouneRequest::DrawStatus {
            status_line: make_line(":q"),
            mode_line: make_line("insert"),
            default_face: Face::default(),
        });
        assert!(flags.contains(DirtyFlags::STATUS));
        assert_eq!(state.status_line[0].contents, ":q");
        assert_eq!(state.status_mode_line[0].contents, "insert");
    }

    #[test]
    fn test_apply_menu_show_select_hide() {
        let mut state = AppState::default();

        state.apply(KakouneRequest::MenuShow {
            items: vec![make_line("a"), make_line("b")],
            anchor: Coord { line: 1, column: 0 },
            selected_item_face: Face::default(),
            menu_face: Face::default(),
            style: MenuStyle::Inline,
        });
        assert!(state.menu.is_some());
        assert_eq!(state.menu.as_ref().unwrap().selected, None);

        state.apply(KakouneRequest::MenuSelect { selected: 1 });
        assert_eq!(state.menu.as_ref().unwrap().selected, Some(1));

        let flags = state.apply(KakouneRequest::MenuHide);
        assert!(state.menu.is_none());
        assert!(flags.contains(DirtyFlags::MENU));
    }

    #[test]
    fn test_apply_info_show_hide() {
        let mut state = AppState::default();

        state.apply(KakouneRequest::InfoShow {
            title: make_line("Help"),
            content: vec![make_line("content")],
            anchor: Coord { line: 0, column: 0 },
            face: Face::default(),
            style: InfoStyle::Modal,
        });
        assert!(state.info.is_some());

        let flags = state.apply(KakouneRequest::InfoHide);
        assert!(state.info.is_none());
        assert!(flags.contains(DirtyFlags::INFO));
    }

    #[test]
    fn test_apply_set_ui_options() {
        let mut state = AppState::default();
        let mut opts = std::collections::HashMap::new();
        opts.insert("key".to_string(), "value".to_string());
        let flags = state.apply(KakouneRequest::SetUiOptions { options: opts });
        assert!(flags.contains(DirtyFlags::OPTIONS));
        assert_eq!(state.ui_options.get("key"), Some(&"value".to_string()));
    }

    #[test]
    fn test_apply_refresh() {
        let mut state = AppState::default();
        let flags = state.apply(KakouneRequest::Refresh { force: true });
        assert_eq!(flags, DirtyFlags::ALL);

        let flags = state.apply(KakouneRequest::Refresh { force: false });
        assert!(flags.contains(DirtyFlags::BUFFER));
        assert!(flags.contains(DirtyFlags::STATUS));
    }

    /// Helper: build an Inline MenuState with given items and win_height.
    fn make_inline_menu(items: Vec<Line>, win_height: u16) -> MenuState {
        MenuState {
            items,
            anchor: Coord::default(),
            selected_item_face: Face::default(),
            menu_face: Face::default(),
            style: MenuStyle::Inline,
            selected: None,
            first_item: 0,
            columns: 1,
            win_height,
            menu_lines: 0, // unused in scroll logic
            max_item_width: 0,
            screen_w: 80,
        }
    }

    /// Helper: build a Prompt MenuState with given items, win_height, and columns.
    fn make_prompt_menu(items: Vec<Line>, win_height: u16, columns: u16) -> MenuState {
        MenuState {
            items,
            anchor: Coord::default(),
            selected_item_face: Face::default(),
            menu_face: Face::default(),
            style: MenuStyle::Prompt,
            selected: None,
            first_item: 0,
            columns,
            win_height,
            menu_lines: 0,
            max_item_width: 0,
            screen_w: 80,
        }
    }

    /// Helper: build a Search MenuState with given items and screen_w.
    fn make_search_menu(items: Vec<Line>, screen_w: u16) -> MenuState {
        MenuState {
            items,
            anchor: Coord::default(),
            selected_item_face: Face::default(),
            menu_face: Face::default(),
            style: MenuStyle::Search,
            selected: None,
            first_item: 0,
            columns: 1,
            win_height: 1,
            menu_lines: 0,
            max_item_width: 0,
            screen_w,
        }
    }

    #[test]
    fn test_select_column_scroll_down() {
        // 5 items, win_height=3 → stride=3, so items 0-2 are col 0, items 3-4 are col 1
        let items: Vec<Line> = (0..5).map(|i| make_line(&format!("item{i}"))).collect();
        let mut menu = make_inline_menu(items, 3);

        // Select item 0: stays in col 0, first_item stays 0
        menu.select(0);
        assert_eq!(menu.first_item, 0);

        // Select item 3: moves to col 1, first_item should scroll to 3
        menu.select(3);
        assert_eq!(menu.first_item, 3);
    }

    #[test]
    fn test_select_column_scroll_up() {
        let items: Vec<Line> = (0..6).map(|i| make_line(&format!("item{i}"))).collect();
        let mut menu = make_inline_menu(items, 3);

        // Scroll forward to col 1
        menu.select(3);
        assert_eq!(menu.first_item, 3);

        // Select item 1: back in col 0, first_item should scroll back to 0
        menu.select(1);
        assert_eq!(menu.first_item, 0);
    }

    #[test]
    fn test_select_prompt_multi_column() {
        // 12 items, win_height=3, columns=2 → stride=3
        // col 0: items 0-2, col 1: items 3-5, col 2: items 6-8, col 3: items 9-11
        // Visible: 2 columns at a time
        let items: Vec<Line> = (0..12).map(|i| make_line(&format!("item{i}"))).collect();
        let mut menu = make_prompt_menu(items, 3, 2);

        // Select item 6 (col 2): needs to scroll since only 2 cols visible
        // col 2 becomes the leftmost visible column → first_item = 2*3 = 6
        menu.select(6);
        assert_eq!(menu.first_item, 6);

        // Select item 9 (col 3): already visible (cols 2-3 shown), no scroll
        menu.select(9);
        assert_eq!(menu.first_item, 6);
    }

    #[test]
    fn test_select_search_stateless() {
        // Items: "aa" (2), "bb" (2), "cc" (2), "dd" (2), "ee" (2)
        // Each takes width+1 = 3 in search bar
        // screen_w = 15 → available width = 15 - 3 = 12
        // Cumulative: aa=3, bb=6, cc=9, dd=12, ee=15 (exceeds 12)
        let items: Vec<Line> = ["aa", "bb", "cc", "dd", "ee"]
            .iter()
            .map(|s| make_line(s))
            .collect();

        // Path A: select directly to item 4
        let mut menu_a = make_search_menu(items.clone(), 15);
        menu_a.select(4);

        // Path B: select 0, then 1, ..., then 4
        let mut menu_b = make_search_menu(items, 15);
        for i in 0..=4 {
            menu_b.select(i);
        }

        // Stateless: same selected → same first_item regardless of path
        assert_eq!(menu_a.first_item, menu_b.first_item);
        assert_eq!(menu_a.selected, Some(4));
        assert_eq!(menu_b.selected, Some(4));
    }

    #[test]
    fn test_select_search_fits_in_width() {
        // Items: "a" (1), "b" (1), "c" (1) → each takes 2 (width+1)
        // screen_w = 80 → available = 77, total = 6, fits easily
        let items: Vec<Line> = ["a", "b", "c"].iter().map(|s| make_line(s)).collect();
        let mut menu = make_search_menu(items, 80);

        menu.select(2);
        assert_eq!(menu.first_item, 0);
    }

    #[test]
    fn test_select_out_of_range_resets() {
        let items: Vec<Line> = (0..3).map(|i| make_line(&format!("item{i}"))).collect();
        let mut menu = make_inline_menu(items, 3);

        // Select valid item first
        menu.select(1);
        assert_eq!(menu.selected, Some(1));

        // Select -1 → resets
        menu.select(-1);
        assert_eq!(menu.selected, None);
        assert_eq!(menu.first_item, 0);

        // Select beyond length → resets
        menu.select(1);
        menu.select(3);
        assert_eq!(menu.selected, None);
        assert_eq!(menu.first_item, 0);
    }
}
