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
    pub selected: i32,
    /// Scroll offset: index of the first visible item.
    pub first_item: i32,
    /// Number of display columns (0 = Search, 1 = Inline, N = Prompt).
    pub columns: i32,
    /// Number of visible rows in the menu window.
    pub win_height: u16,
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
                let screen_w = self.cols;
                let screen_h = self.rows.saturating_sub(1); // exclude status bar

                let longest = items
                    .iter()
                    .map(line_display_width)
                    .max()
                    .unwrap_or(1)
                    .max(1);

                let columns = match style {
                    MenuStyle::Search => 0,
                    MenuStyle::Inline => 1,
                    MenuStyle::Prompt => {
                        // -1 for scrollbar column
                        ((screen_w.saturating_sub(1)) as usize / (longest + 1)).max(1) as i32
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

                let item_count = items.len() as i32;
                let effective_cols = columns.max(1);
                let menu_lines = (item_count + effective_cols - 1) / effective_cols;
                let win_height = (menu_lines as u16).min(max_height);

                self.menu = Some(MenuState {
                    items,
                    anchor,
                    selected_item_face,
                    menu_face,
                    style,
                    selected: -1,
                    first_item: 0,
                    columns,
                    win_height,
                });
                DirtyFlags::MENU
            }
            KakouneRequest::MenuSelect { selected } => {
                if let Some(menu) = &mut self.menu {
                    menu.selected = selected;

                    if selected >= 0 && menu.win_height > 0 {
                        if menu.columns >= 1 {
                            // Inline/Prompt: column-based scrolling
                            let wh = menu.win_height as i32;
                            let selected_col = selected / wh;
                            let first_col = menu.first_item / wh;
                            let menu_cols = (menu.items.len() as i32 + wh - 1) / wh;
                            if selected_col < first_col {
                                menu.first_item = selected_col * wh;
                            } else if selected_col >= first_col + menu.columns {
                                let new_first_col =
                                    selected_col - menu.columns + 1;
                                menu.first_item =
                                    new_first_col.min(menu_cols - menu.columns).max(0) * wh;
                            }
                        } else {
                            // Search (columns == 0): horizontal item scrolling.
                            // Ensure selected item is visible within screen width.
                            let screen_w = self.cols as usize;
                            if (selected as usize) < menu.first_item as usize {
                                menu.first_item = selected;
                            } else {
                                // Compute cumulative width from first_item to selected
                                let mut w = 0usize;
                                for idx in (menu.first_item as usize)..=(selected as usize) {
                                    let item_w = if let Some(item) = menu.items.get(idx) {
                                        line_display_width(item)
                                    } else {
                                        0
                                    };
                                    // Each item takes item_w + 1 (gap), except prefix/suffix
                                    w += item_w + 1;
                                }
                                // Account for "< " prefix (2 chars) when scrolled
                                let prefix = if menu.first_item > 0 { 2 } else { 0 };
                                while w + prefix > screen_w && menu.first_item < selected {
                                    let drop_w = menu
                                        .items
                                        .get(menu.first_item as usize)
                                        .map(line_display_width)
                                        .unwrap_or(0);
                                    w -= drop_w + 1;
                                    menu.first_item += 1;
                                }
                            }
                        }
                    }
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
        assert_eq!(state.menu.as_ref().unwrap().selected, -1);

        state.apply(KakouneRequest::MenuSelect { selected: 1 });
        assert_eq!(state.menu.as_ref().unwrap().selected, 1);

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
}
