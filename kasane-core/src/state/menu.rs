use crate::layout::line_display_width;
use crate::protocol::{Coord, Face, Line, MenuStyle};

/// Parameters for constructing a [`MenuState`].
///
/// Groups the configuration and layout context that `MenuState::new()` needs
/// (everything except the item list itself).
#[derive(Debug, Clone)]
pub struct MenuParams {
    pub anchor: Coord,
    pub selected_item_face: Face,
    pub menu_face: Face,
    pub style: MenuStyle,
    pub screen_w: u16,
    pub screen_h: u16,
    pub max_height: u16,
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
    /// `params.screen_h` is the available height **excluding** the status bar row
    /// (i.e. `AppState::available_height()`).
    pub fn new(items: Vec<Line>, params: MenuParams) -> Self {
        let max_item_width = items
            .iter()
            .map(|l| line_display_width(l))
            .max()
            .unwrap_or(1)
            .max(1) as u16;

        let columns: u16 = match params.style {
            MenuStyle::Search | MenuStyle::Inline => 1,
            MenuStyle::Prompt => {
                // -1 for scrollbar column (matches Kakoune terminal_ui.cc:
                // max_width = m_dimensions.column - 1)
                ((params.screen_w.saturating_sub(1)) as usize / (max_item_width as usize + 1))
                    .max(1) as u16
            }
        };

        let max_height = match params.style {
            MenuStyle::Search => 1u16,
            MenuStyle::Inline => {
                let above = params.anchor.line as u16;
                let below = params
                    .screen_h
                    .saturating_sub(params.anchor.line as u16 + 1);
                params.max_height.min(above.max(below))
            }
            MenuStyle::Prompt => params.max_height.min(params.screen_h),
        };

        let item_count = items.len();
        let cols = columns as usize;
        let menu_lines = item_count.div_ceil(cols) as u16;
        let win_height = menu_lines.min(max_height);

        Self {
            items,
            anchor: params.anchor,
            selected_item_face: params.selected_item_face,
            menu_face: params.menu_face,
            style: params.style,
            selected: None,
            first_item: 0,
            columns,
            win_height,
            menu_lines,
            max_item_width,
            screen_w: params.screen_w,
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
            let item_w = self
                .items
                .get(i)
                .map(|l| line_display_width(l))
                .unwrap_or(0)
                + 1;
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
