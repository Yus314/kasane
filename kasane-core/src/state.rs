use std::collections::HashMap;

use bitflags::bitflags;

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
                self.menu = Some(MenuState {
                    items,
                    anchor,
                    selected_item_face,
                    menu_face,
                    style,
                    selected: -1,
                });
                DirtyFlags::MENU
            }
            KakouneRequest::MenuSelect { selected } => {
                if let Some(menu) = &mut self.menu {
                    menu.selected = selected;
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
            mode_line: make_line("[normal]"),
            default_face: Face::default(),
        });
        assert!(flags.contains(DirtyFlags::STATUS));
        assert_eq!(state.status_line[0].contents, ":q");
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
