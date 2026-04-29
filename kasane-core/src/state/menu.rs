use crate::layout::line_display_width;
use crate::protocol::{Atom, Color, Coord, Line, MenuStyle};

// ---------------------------------------------------------------------------
// Two-column split for inline completion menus
// ---------------------------------------------------------------------------

/// Per-item column split indices into the original `Line` atoms.
#[derive(Debug, Clone)]
pub struct ItemSplit {
    /// Exclusive end index of candidate atoms (excludes trailing padding).
    pub candidate_end: usize,
    /// Start index of docstring atoms.
    pub docstring_start: usize,
    /// Display width of candidate portion.
    pub candidate_width: u16,
    /// Display width of docstring portion.
    pub docstring_width: u16,
}

/// Precomputed two-column layout metrics for inline menus.
#[derive(Debug, Clone)]
pub struct MenuColumns {
    /// Per-item splits (same indices as `MenuState::items`).
    pub splits: Vec<ItemSplit>,
    /// Max candidate width across all items (uncapped).
    pub max_candidate_width: u16,
    /// Max docstring width across all items.
    pub max_docstring_width: u16,
}

/// Compute the display width of a slice of atoms.
fn atoms_display_width(atoms: &[Atom]) -> usize {
    atoms
        .iter()
        .map(|a| {
            a.contents
                .split(|c: char| c.is_control())
                .map(unicode_width::UnicodeWidthStr::width)
                .sum::<usize>()
        })
        .sum()
}

/// Split a single menu item into candidate and docstring columns.
///
/// # Inference Rule: I-4
/// **Assumption**: Kakoune inserts a whitespace-only padding atom between the
/// candidate text and the docstring, and the docstring atom has a non-Default
/// fg color. This pattern is stable across Kakoune versions.
/// **Failure mode**: If the padding/color convention changes, all items appear
/// as single-column (candidate only) — the docstring column is not shown.
/// **Severity**: Cosmetic (completion menu loses type annotations)
///
/// Detection heuristic: find the first atom with non-Default fg color,
/// preceded by a whitespace-only atom (the alignment padding Kakoune inserts).
pub fn split_single_item(item: &Line) -> ItemSplit {
    for i in 1..item.len() {
        if item[i].face().fg != Color::Default && item[i - 1].contents.chars().all(|c| c == ' ') {
            // Strip trailing whitespace-only atoms from candidate
            let mut cand_end = i - 1;
            while cand_end > 0 && item[cand_end - 1].contents.chars().all(|c| c == ' ') {
                cand_end -= 1;
            }
            return ItemSplit {
                candidate_end: cand_end,
                docstring_start: i,
                candidate_width: atoms_display_width(&item[..cand_end]) as u16,
                docstring_width: atoms_display_width(&item[i..]) as u16,
            };
        }
    }
    // No split found: entire item is a single candidate column.
    ItemSplit {
        candidate_end: item.len(),
        docstring_start: item.len(),
        candidate_width: atoms_display_width(item) as u16,
        docstring_width: 0,
    }
}

/// Build two-column layout metrics for a set of menu items.
///
/// Returns `None` when no item has a docstring (single-column fallback).
pub fn split_item_columns(items: &[Line]) -> Option<MenuColumns> {
    let splits: Vec<ItemSplit> = items.iter().map(split_single_item).collect();
    let max_candidate_width = splits.iter().map(|s| s.candidate_width).max().unwrap_or(0);
    let max_docstring_width = splits.iter().map(|s| s.docstring_width).max().unwrap_or(0);
    if max_docstring_width == 0 {
        return None;
    }
    Some(MenuColumns {
        splits,
        max_candidate_width,
        max_docstring_width,
    })
}

/// Parameters for constructing a [`MenuState`].
///
/// Groups the configuration and layout context that `MenuState::new()` needs
/// (everything except the item list itself).
#[derive(Debug, Clone)]
pub struct MenuParams {
    pub anchor: Coord,
    /// ADR-031 Phase A.3: Style.
    pub selected_item_face: crate::protocol::Style,
    /// ADR-031 Phase A.3: Style.
    pub menu_face: crate::protocol::Style,
    pub style: MenuStyle,
    pub screen_w: u16,
    pub screen_h: u16,
    pub max_height: u16,
}

#[derive(Debug, Clone)]
pub struct MenuState {
    pub items: Vec<Line>,
    pub anchor: Coord,
    /// ADR-031 Phase A.3: Style.
    pub selected_item_face: crate::protocol::Style,
    /// ADR-031 Phase A.3: Style.
    pub menu_face: crate::protocol::Style,
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
    /// Two-column split for inline completion menus (None = single-column).
    pub columns_split: Option<MenuColumns>,
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

        let columns_split = match params.style {
            MenuStyle::Inline => split_item_columns(&items),
            _ => None,
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
            columns_split,
        }
    }

    /// Content width accounting for two-column layout when present.
    ///
    /// For two-column menus: `min(max_candidate, cols*2/5) + 1 + max_docstring`,
    /// capped at `cols - 1` (leaving room for scrollbar).
    /// For single-column menus: `max_item_width`.
    pub fn effective_content_width(&self, cols: u16) -> u16 {
        match &self.columns_split {
            Some(mc) => {
                let cand_w = mc.max_candidate_width.min(cols * 2 / 5);
                (cand_w + 1 + mc.max_docstring_width).min(cols.saturating_sub(1))
            }
            None => self.max_item_width,
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
        let selected = self.selected.expect("select() guarantees Some");
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
        let selected = self.selected.expect("select() guarantees Some");
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::protocol::{Color, Face};

    /// Helper: build a 3-atom completion item: candidate + padding + colored docstring.
    fn make_completion_item(candidate: &str, padding: &str, docstring: &str) -> Line {
        vec![
            Atom::plain(candidate),
            Atom::plain(padding),
            Atom::from_face(
                Face {
                    fg: Color::Named(crate::protocol::NamedColor::Cyan),
                    ..Face::default()
                },
                docstring,
            ),
        ]
    }

    #[test]
    fn test_split_standard_completion() {
        let item = make_completion_item("foo", "   ", "{string}");
        let split = split_single_item(&item);
        assert_eq!(split.candidate_end, 1); // atom 0 = "foo"
        assert_eq!(split.docstring_start, 2); // atom 2 = "{string}"
        assert_eq!(split.candidate_width, 3);
        assert_eq!(split.docstring_width, 8);
    }

    #[test]
    fn test_split_no_docstring() {
        let item = vec![Atom::plain("hello_world")];
        let split = split_single_item(&item);
        assert_eq!(split.candidate_end, 1);
        assert_eq!(split.docstring_start, 1);
        assert_eq!(split.candidate_width, 11);
        assert_eq!(split.docstring_width, 0);
    }

    #[test]
    fn test_split_no_padding() {
        // Two atoms but no whitespace-only padding between them → no split.
        let item = vec![
            Atom::plain("foo"),
            Atom::from_face(
                Face {
                    fg: Color::Named(crate::protocol::NamedColor::Cyan),
                    ..Face::default()
                },
                "bar",
            ),
        ];
        let split = split_single_item(&item);
        // No split: "foo" is not all-spaces, so heuristic doesn't fire.
        assert_eq!(split.candidate_end, 2);
        assert_eq!(split.docstring_start, 2);
        assert_eq!(split.docstring_width, 0);
    }

    #[test]
    fn test_split_columns_none_when_no_docstrings() {
        let items = vec![vec![Atom::plain("abc")], vec![Atom::plain("defgh")]];
        assert!(split_item_columns(&items).is_none());
    }

    #[test]
    fn test_split_columns_some() {
        let items = vec![
            make_completion_item("foo", "    ", "{string}"),
            make_completion_item("barbaz", " ", "{int}"),
        ];
        let mc = split_item_columns(&items).unwrap();
        assert_eq!(mc.max_candidate_width, 6); // "barbaz"
        assert_eq!(mc.max_docstring_width, 8); // "{string}"
        assert_eq!(mc.splits.len(), 2);
    }

    #[test]
    fn test_effective_content_width_single_column() {
        let menu = MenuState::new(
            vec![vec![Atom::plain("hello")]],
            MenuParams {
                anchor: Coord { line: 5, column: 0 },
                selected_item_face: Face::default().into(),
                menu_face: Face::default().into(),
                style: MenuStyle::Inline,
                screen_w: 80,
                screen_h: 24,
                max_height: 10,
            },
        );
        assert!(menu.columns_split.is_none());
        assert_eq!(menu.effective_content_width(80), 5);
    }

    #[test]
    fn test_effective_content_width_two_column() {
        let items = vec![
            make_completion_item("foo", "    ", "{string}"),
            make_completion_item("barbaz", " ", "{int}"),
        ];
        let menu = MenuState::new(
            items,
            MenuParams {
                anchor: Coord { line: 5, column: 0 },
                selected_item_face: Face::default().into(),
                menu_face: Face::default().into(),
                style: MenuStyle::Inline,
                screen_w: 80,
                screen_h: 24,
                max_height: 10,
            },
        );
        assert!(menu.columns_split.is_some());
        // max_candidate=6, cap=80*2/5=32, so cand_w=6
        // effective = 6 + 1 + 8 = 15
        assert_eq!(menu.effective_content_width(80), 15);
    }

    #[test]
    fn test_effective_content_width_capped() {
        // Very long candidate names → capped at cols*2/5
        let items = vec![make_completion_item(&"x".repeat(50), " ", "{docstring}")];
        let menu = MenuState::new(
            items,
            MenuParams {
                anchor: Coord { line: 5, column: 0 },
                selected_item_face: Face::default().into(),
                menu_face: Face::default().into(),
                style: MenuStyle::Inline,
                screen_w: 80,
                screen_h: 24,
                max_height: 10,
            },
        );
        let mc = menu.columns_split.as_ref().unwrap();
        assert_eq!(mc.max_candidate_width, 50);
        // cap = 80*2/5 = 32; "{docstring}" = 11 chars; effective = 32 + 1 + 11 = 44
        assert_eq!(menu.effective_content_width(80), 44);
    }
}
