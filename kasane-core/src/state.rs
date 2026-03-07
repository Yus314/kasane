use std::collections::HashMap;

use bitflags::bitflags;

use crate::config::MenuPosition;
use crate::input;
use crate::input::{Key, KeyEvent, MouseButton, MouseEvent};
use crate::layout::line_display_width;
use crate::plugin::{Command, PluginRegistry};
use crate::protocol::{
    Attributes, Coord, CursorMode, Face, InfoStyle, KakouneRequest, KasaneRequest, Line,
    MenuStyle,
};
use crate::render::CellGrid;

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

impl AppState {
    pub fn apply(&mut self, request: KakouneRequest) -> DirtyFlags {
        match request {
            KakouneRequest::Draw {
                lines,
                default_face,
                padding_face,
            } => {
                // Count cursor positions: atoms with FINAL_FG + REVERSE indicate cursor faces
                self.cursor_count = lines
                    .iter()
                    .flat_map(|line| line.iter())
                    .filter(|atom| {
                        atom.face.attributes.contains(Attributes::FINAL_FG)
                            && atom.face.attributes.contains(Attributes::REVERSE)
                    })
                    .count();
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
                    items,
                    anchor,
                    selected_item_face,
                    menu_face,
                    style,
                    self.cols,
                    screen_h,
                    self.menu_max_height,
                ));
                DirtyFlags::MENU
            }
            KakouneRequest::MenuSelect { selected } => {
                if let Some(menu) = &mut self.menu {
                    tracing::debug!(
                        "MenuSelect: selected={}, first_item={}, win_height={}, items={}, columns={}",
                        selected,
                        menu.first_item,
                        menu.win_height,
                        menu.items.len(),
                        menu.columns,
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
                let identity = InfoIdentity {
                    style,
                    anchor_line: anchor.line as u32,
                };
                let new_info = InfoState {
                    title,
                    content,
                    anchor,
                    face,
                    style,
                    identity: identity.clone(),
                    scroll_offset: 0,
                };
                // Replace existing info with same identity, or add new
                if let Some(pos) = self.infos.iter().position(|i| i.identity == identity) {
                    self.infos[pos] = new_info;
                } else {
                    self.infos.push(new_info);
                }
                DirtyFlags::INFO
            }
            KakouneRequest::InfoHide => {
                // Remove the most recently added/updated info
                self.infos.pop();
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

/// Check if a scroll event hits an info popup and adjust its scroll_offset.
/// Returns true if the scroll was consumed by an info popup.
fn handle_info_scroll(state: &mut AppState, mouse: &input::MouseEvent) -> bool {
    let screen_h = state.rows.saturating_sub(1);
    let mut avoid: Vec<crate::layout::Rect> = Vec::new();
    if let Some(mr) = crate::render::menu::get_menu_rect(state) {
        avoid.push(mr);
    }

    for info in state.infos.iter_mut().rev() {
        let win = crate::layout::layout_info(
            &info.title,
            &info.content,
            &info.anchor,
            info.style,
            state.cols,
            screen_h,
            &avoid,
        );
        if win.width == 0 || win.height == 0 {
            continue;
        }

        let mx = mouse.column as u16;
        let my = mouse.line as u16;
        if mx >= win.x && mx < win.x + win.width && my >= win.y && my < win.y + win.height {
            // Compute content height for scroll bounds
            let content_h = info
                .content
                .iter()
                .map(|line| {
                    crate::layout::word_wrap_line_height(line, win.width.saturating_sub(4).max(1))
                })
                .sum::<u16>();
            let visible_h = win.height.saturating_sub(2).max(1); // subtract borders

            match mouse.kind {
                input::MouseEventKind::ScrollUp => {
                    info.scroll_offset = info.scroll_offset.saturating_sub(3);
                }
                input::MouseEventKind::ScrollDown => {
                    let max_offset = content_h.saturating_sub(visible_h);
                    info.scroll_offset = (info.scroll_offset + 3).min(max_offset);
                }
                _ => {}
            }
            return true;
        }
    }
    false
}

// ---------------------------------------------------------------------------
// TEA: Msg → update() → Vec<Command>
// ---------------------------------------------------------------------------

/// Messages that drive the application state machine.
pub enum Msg {
    Kakoune(KakouneRequest),
    Key(KeyEvent),
    Mouse(MouseEvent),
    Paste,
    Resize { cols: u16, rows: u16 },
    FocusGained,
    FocusLost,
}

/// Process a message, updating state and returning dirty flags + side-effect commands.
pub fn update(
    state: &mut AppState,
    msg: Msg,
    registry: &mut PluginRegistry,
    grid: &mut CellGrid,
    scroll_amount: i32,
) -> (DirtyFlags, Vec<Command>) {
    match msg {
        Msg::Kakoune(req) => {
            let flags = state.apply(req);
            (flags, vec![])
        }
        Msg::Key(key) => {
            // PageUp/PageDown intercept: convert to scroll commands
            if key.modifiers.is_empty() {
                match key.key {
                    Key::PageUp => {
                        let visible_lines = compute_visible_lines(state);
                        let cmd = Command::SendToKakoune(KasaneRequest::Scroll {
                            amount: -(visible_lines as i32),
                            line: state.cursor_pos.line as u32,
                            column: state.cursor_pos.column as u32,
                        });
                        return (DirtyFlags::empty(), vec![cmd]);
                    }
                    Key::PageDown => {
                        let visible_lines = compute_visible_lines(state);
                        let cmd = Command::SendToKakoune(KasaneRequest::Scroll {
                            amount: visible_lines as i32,
                            line: state.cursor_pos.line as u32,
                            column: state.cursor_pos.column as u32,
                        });
                        return (DirtyFlags::empty(), vec![cmd]);
                    }
                    _ => {}
                }
            }

            // Ask plugins to handle the key first
            for plugin in registry.plugins_mut() {
                if let Some(commands) = plugin.handle_key(&key, state) {
                    return (DirtyFlags::empty(), commands);
                }
            }
            // No plugin handled it → forward to Kakoune
            let kak_key = input::key_to_kakoune(&key);
            let cmd = Command::SendToKakoune(KasaneRequest::Keys(vec![kak_key]));
            (DirtyFlags::empty(), vec![cmd])
        }
        Msg::Mouse(mouse) => {
            // Update drag state
            match mouse.kind {
                input::MouseEventKind::Press(button) => {
                    state.drag = DragState::Active {
                        button,
                        start_line: mouse.line,
                        start_column: mouse.column,
                    };
                }
                input::MouseEventKind::Release(_) => {
                    state.drag = DragState::None;
                }
                _ => {}
            }

            // Selection-during-scroll: when dragging with left button and scrolling,
            // send scroll + mouse_move to extend selection (R-046)
            if let DragState::Active {
                button: MouseButton::Left,
                ..
            } = &state.drag
                && matches!(
                    mouse.kind,
                    input::MouseEventKind::ScrollUp | input::MouseEventKind::ScrollDown
                )
            {
                // Check info scroll first
                if handle_info_scroll(state, &mouse) {
                    return (DirtyFlags::INFO, vec![]);
                }
                if let Some(scroll_req) = input::mouse_to_kakoune(&mouse, scroll_amount) {
                    let edge_line = match mouse.kind {
                        input::MouseEventKind::ScrollUp => 0,
                        _ => state.rows.saturating_sub(2) as u32,
                    };
                    let move_req = KasaneRequest::MouseMove {
                        line: edge_line,
                        column: mouse.column,
                    };
                    return (
                        DirtyFlags::empty(),
                        vec![
                            Command::SendToKakoune(scroll_req),
                            Command::SendToKakoune(move_req),
                        ],
                    );
                }
            }

            // Check if mouse scroll targets an info popup
            if matches!(
                mouse.kind,
                input::MouseEventKind::ScrollUp | input::MouseEventKind::ScrollDown
            ) && handle_info_scroll(state, &mouse)
            {
                return (DirtyFlags::INFO, vec![]);
            }

            // Smooth scrolling: set up animation instead of immediate scroll
            if state.smooth_scroll
                && matches!(
                    mouse.kind,
                    input::MouseEventKind::ScrollUp | input::MouseEventKind::ScrollDown
                )
            {
                let amount = match mouse.kind {
                    input::MouseEventKind::ScrollUp => -scroll_amount,
                    _ => scroll_amount,
                };
                if let Some(ref mut anim) = state.scroll_animation {
                    // Accumulate into existing animation
                    anim.remaining += amount;
                    anim.line = mouse.line;
                    anim.column = mouse.column;
                } else {
                    state.scroll_animation = Some(ScrollAnimation {
                        remaining: amount,
                        step: 1,
                        line: mouse.line,
                        column: mouse.column,
                    });
                }
                return (DirtyFlags::empty(), vec![]);
            }

            let cmds = if let Some(req) = input::mouse_to_kakoune(&mouse, scroll_amount) {
                vec![Command::SendToKakoune(req)]
            } else {
                vec![]
            };
            (DirtyFlags::empty(), cmds)
        }
        Msg::Paste => (DirtyFlags::empty(), vec![Command::Paste]),
        Msg::Resize { cols, rows } => {
            state.cols = cols;
            state.rows = rows;
            grid.resize(cols, rows);
            grid.invalidate_all();
            let cmd = Command::SendToKakoune(KasaneRequest::Resize {
                rows: rows.saturating_sub(1),
                cols,
            });
            (DirtyFlags::ALL, vec![cmd])
        }
        Msg::FocusGained => {
            state.focused = true;
            (DirtyFlags::ALL, vec![])
        }
        Msg::FocusLost => {
            state.focused = false;
            (DirtyFlags::ALL, vec![])
        }
    }
}

/// Compute the number of visible buffer lines for PageUp/PageDown scrolling.
fn compute_visible_lines(state: &AppState) -> u16 {
    // Subtract 1 for the status bar row
    state.rows.saturating_sub(1)
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
        assert_eq!(state.infos.len(), 1);

        let flags = state.apply(KakouneRequest::InfoHide);
        assert!(state.infos.is_empty());
        assert!(flags.contains(DirtyFlags::INFO));
    }

    #[test]
    fn test_apply_multiple_infos() {
        let mut state = AppState::default();

        // Show first info (Modal at line 0)
        state.apply(KakouneRequest::InfoShow {
            title: make_line("Help"),
            content: vec![make_line("content1")],
            anchor: Coord { line: 0, column: 0 },
            face: Face::default(),
            style: InfoStyle::Modal,
        });
        assert_eq!(state.infos.len(), 1);

        // Show second info (Inline at line 5) — different identity, coexists
        state.apply(KakouneRequest::InfoShow {
            title: make_line("Lint"),
            content: vec![make_line("error here")],
            anchor: Coord { line: 5, column: 0 },
            face: Face::default(),
            style: InfoStyle::Inline,
        });
        assert_eq!(state.infos.len(), 2);

        // Show info with same identity (Modal at line 0) — replaces first
        state.apply(KakouneRequest::InfoShow {
            title: make_line("Updated Help"),
            content: vec![make_line("new content")],
            anchor: Coord { line: 0, column: 0 },
            face: Face::default(),
            style: InfoStyle::Modal,
        });
        assert_eq!(state.infos.len(), 2);
        assert_eq!(state.infos[0].title[0].contents, "Updated Help");

        // Hide removes most recent
        state.apply(KakouneRequest::InfoHide);
        assert_eq!(state.infos.len(), 1);
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

    // --- TEA update() tests ---

    #[test]
    fn test_update_key_forwards_to_kakoune() {
        let mut state = AppState::default();
        let mut registry = PluginRegistry::new();
        let mut grid = CellGrid::new(80, 24);
        let key = crate::input::KeyEvent {
            key: crate::input::Key::Char('a'),
            modifiers: crate::input::Modifiers::empty(),
        };
        let (flags, commands) = update(&mut state, Msg::Key(key), &mut registry, &mut grid, 3);
        assert!(flags.is_empty());
        assert_eq!(commands.len(), 1);
        match &commands[0] {
            Command::SendToKakoune(req) => {
                assert_eq!(
                    *req,
                    crate::protocol::KasaneRequest::Keys(vec!["a".to_string()])
                );
            }
            _ => panic!("expected SendToKakoune"),
        }
    }

    #[test]
    fn test_update_kakoune_draw() {
        let mut state = AppState::default();
        let mut registry = PluginRegistry::new();
        let mut grid = CellGrid::new(80, 24);
        let (flags, commands) = update(
            &mut state,
            Msg::Kakoune(KakouneRequest::Draw {
                lines: vec![make_line("hello")],
                default_face: Face::default(),
                padding_face: Face::default(),
            }),
            &mut registry,
            &mut grid,
            3,
        );
        assert!(flags.contains(DirtyFlags::BUFFER));
        assert!(commands.is_empty());
        assert_eq!(state.lines.len(), 1);
    }

    #[test]
    fn test_update_focus_lost() {
        let mut state = AppState::default();
        let mut registry = PluginRegistry::new();
        let mut grid = CellGrid::new(80, 24);
        let (flags, _) = update(&mut state, Msg::FocusLost, &mut registry, &mut grid, 3);
        assert_eq!(flags, DirtyFlags::ALL);
        assert!(!state.focused);
    }

    #[test]
    fn test_update_focus_gained() {
        let mut state = AppState::default();
        state.focused = false;
        let mut registry = PluginRegistry::new();
        let mut grid = CellGrid::new(80, 24);
        let (flags, _) = update(&mut state, Msg::FocusGained, &mut registry, &mut grid, 3);
        assert_eq!(flags, DirtyFlags::ALL);
        assert!(state.focused);
    }

    #[test]
    fn test_update_plugin_handles_key() {
        use crate::plugin::{Plugin, PluginId};

        struct KeyPlugin;
        impl Plugin for KeyPlugin {
            fn id(&self) -> PluginId {
                PluginId("key_plugin".into())
            }
            fn handle_key(
                &mut self,
                _key: &crate::input::KeyEvent,
                _state: &AppState,
            ) -> Option<Vec<Command>> {
                Some(vec![Command::SendToKakoune(
                    crate::protocol::KasaneRequest::Keys(vec!["<esc>".to_string()]),
                )])
            }
        }

        let mut state = AppState::default();
        let mut registry = PluginRegistry::new();
        registry.register(Box::new(KeyPlugin));
        let mut grid = CellGrid::new(80, 24);
        let key = crate::input::KeyEvent {
            key: crate::input::Key::Char('a'),
            modifiers: crate::input::Modifiers::empty(),
        };
        let (flags, commands) = update(&mut state, Msg::Key(key), &mut registry, &mut grid, 3);
        assert!(flags.is_empty());
        assert_eq!(commands.len(), 1);
        match &commands[0] {
            Command::SendToKakoune(req) => {
                assert_eq!(
                    *req,
                    crate::protocol::KasaneRequest::Keys(vec!["<esc>".to_string()])
                );
            }
            _ => panic!("expected SendToKakoune from plugin"),
        }
    }

    // --- Phase 3: Drag state tests ---

    #[test]
    fn test_drag_state_press_activates() {
        let mut state = AppState::default();
        let mut registry = PluginRegistry::new();
        let mut grid = CellGrid::new(80, 24);
        let mouse = crate::input::MouseEvent {
            kind: crate::input::MouseEventKind::Press(crate::input::MouseButton::Left),
            line: 5,
            column: 10,
        };
        update(&mut state, Msg::Mouse(mouse), &mut registry, &mut grid, 3);
        assert_eq!(
            state.drag,
            DragState::Active {
                button: crate::input::MouseButton::Left,
                start_line: 5,
                start_column: 10,
            }
        );
    }

    #[test]
    fn test_drag_state_release_clears() {
        let mut state = AppState::default();
        state.drag = DragState::Active {
            button: crate::input::MouseButton::Left,
            start_line: 0,
            start_column: 0,
        };
        let mut registry = PluginRegistry::new();
        let mut grid = CellGrid::new(80, 24);
        let mouse = crate::input::MouseEvent {
            kind: crate::input::MouseEventKind::Release(crate::input::MouseButton::Left),
            line: 5,
            column: 10,
        };
        update(&mut state, Msg::Mouse(mouse), &mut registry, &mut grid, 3);
        assert_eq!(state.drag, DragState::None);
    }

    #[test]
    fn test_drag_state_drag_keeps_active() {
        let mut state = AppState::default();
        state.drag = DragState::Active {
            button: crate::input::MouseButton::Left,
            start_line: 0,
            start_column: 0,
        };
        let mut registry = PluginRegistry::new();
        let mut grid = CellGrid::new(80, 24);
        let mouse = crate::input::MouseEvent {
            kind: crate::input::MouseEventKind::Drag(crate::input::MouseButton::Left),
            line: 3,
            column: 7,
        };
        let (_, commands) = update(&mut state, Msg::Mouse(mouse), &mut registry, &mut grid, 3);
        // Drag sends MouseMove
        assert_eq!(commands.len(), 1);
        match &commands[0] {
            Command::SendToKakoune(req) => {
                assert_eq!(
                    *req,
                    KasaneRequest::MouseMove {
                        line: 3,
                        column: 7,
                    }
                );
            }
            _ => panic!("expected SendToKakoune MouseMove"),
        }
        // Drag state remains Active
        assert!(matches!(state.drag, DragState::Active { .. }));
    }

    #[test]
    fn test_selection_scroll_generates_two_commands() {
        let mut state = AppState::default();
        state.rows = 24;
        state.drag = DragState::Active {
            button: crate::input::MouseButton::Left,
            start_line: 5,
            start_column: 10,
        };
        let mut registry = PluginRegistry::new();
        let mut grid = CellGrid::new(80, 24);
        let mouse = crate::input::MouseEvent {
            kind: crate::input::MouseEventKind::ScrollDown,
            line: 10,
            column: 5,
        };
        let (_, commands) = update(&mut state, Msg::Mouse(mouse), &mut registry, &mut grid, 3);
        assert_eq!(commands.len(), 2, "scroll + mouse_move expected");
        // First: Scroll
        match &commands[0] {
            Command::SendToKakoune(KasaneRequest::Scroll { amount, .. }) => {
                assert_eq!(*amount, 3);
            }
            _ => panic!("expected Scroll command"),
        }
        // Second: MouseMove to edge
        match &commands[1] {
            Command::SendToKakoune(KasaneRequest::MouseMove { line, column }) => {
                assert_eq!(*line, 22); // rows - 2
                assert_eq!(*column, 5);
            }
            _ => panic!("expected MouseMove command"),
        }
    }

    #[test]
    fn test_selection_scroll_up_edge() {
        let mut state = AppState::default();
        state.rows = 24;
        state.drag = DragState::Active {
            button: crate::input::MouseButton::Left,
            start_line: 5,
            start_column: 10,
        };
        let mut registry = PluginRegistry::new();
        let mut grid = CellGrid::new(80, 24);
        let mouse = crate::input::MouseEvent {
            kind: crate::input::MouseEventKind::ScrollUp,
            line: 10,
            column: 5,
        };
        let (_, commands) = update(&mut state, Msg::Mouse(mouse), &mut registry, &mut grid, 3);
        assert_eq!(commands.len(), 2);
        match &commands[1] {
            Command::SendToKakoune(KasaneRequest::MouseMove { line, .. }) => {
                assert_eq!(*line, 0); // edge is top
            }
            _ => panic!("expected MouseMove command"),
        }
    }

    #[test]
    fn test_paste_produces_paste_command() {
        let mut state = AppState::default();
        let mut registry = PluginRegistry::new();
        let mut grid = CellGrid::new(80, 24);
        let (flags, commands) = update(&mut state, Msg::Paste, &mut registry, &mut grid, 3);
        assert!(flags.is_empty());
        assert_eq!(commands.len(), 1);
        assert!(matches!(commands[0], Command::Paste));
    }

    #[test]
    fn test_pageup_intercept() {
        let mut state = AppState::default();
        state.rows = 24;
        state.cursor_pos = Coord { line: 10, column: 5 };
        let mut registry = PluginRegistry::new();
        let mut grid = CellGrid::new(80, 24);
        let key = crate::input::KeyEvent {
            key: crate::input::Key::PageUp,
            modifiers: crate::input::Modifiers::empty(),
        };
        let (_, commands) = update(&mut state, Msg::Key(key), &mut registry, &mut grid, 3);
        assert_eq!(commands.len(), 1);
        match &commands[0] {
            Command::SendToKakoune(KasaneRequest::Scroll { amount, line, column }) => {
                assert_eq!(*amount, -23); // -(rows - 1)
                assert_eq!(*line, 10);
                assert_eq!(*column, 5);
            }
            _ => panic!("expected Scroll command"),
        }
    }

    #[test]
    fn test_pagedown_intercept() {
        let mut state = AppState::default();
        state.rows = 24;
        state.cursor_pos = Coord { line: 10, column: 5 };
        let mut registry = PluginRegistry::new();
        let mut grid = CellGrid::new(80, 24);
        let key = crate::input::KeyEvent {
            key: crate::input::Key::PageDown,
            modifiers: crate::input::Modifiers::empty(),
        };
        let (_, commands) = update(&mut state, Msg::Key(key), &mut registry, &mut grid, 3);
        assert_eq!(commands.len(), 1);
        match &commands[0] {
            Command::SendToKakoune(KasaneRequest::Scroll { amount, .. }) => {
                assert_eq!(*amount, 23); // rows - 1
            }
            _ => panic!("expected Scroll command"),
        }
    }

    #[test]
    fn test_pageup_with_modifier_not_intercepted() {
        let mut state = AppState::default();
        let mut registry = PluginRegistry::new();
        let mut grid = CellGrid::new(80, 24);
        let key = crate::input::KeyEvent {
            key: crate::input::Key::PageUp,
            modifiers: crate::input::Modifiers::CTRL,
        };
        let (_, commands) = update(&mut state, Msg::Key(key), &mut registry, &mut grid, 3);
        // With modifier, PageUp should be forwarded as key, not intercepted
        assert_eq!(commands.len(), 1);
        match &commands[0] {
            Command::SendToKakoune(KasaneRequest::Keys(keys)) => {
                assert_eq!(keys, &vec!["<c-pageup>".to_string()]);
            }
            _ => panic!("expected Keys command"),
        }
    }

    #[test]
    fn test_compute_visible_lines() {
        let mut state = AppState::default();
        state.rows = 24;
        assert_eq!(compute_visible_lines(&state), 23);

        state.rows = 1;
        assert_eq!(compute_visible_lines(&state), 0);
    }
}
