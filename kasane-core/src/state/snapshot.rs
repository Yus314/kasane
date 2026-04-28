//! Immutable snapshot types for the Salsa input boundary (Layer 2).
//!
//! These types capture the read-only view of mutable state structures
//! (`MenuState`, `InfoState`) as `PartialEq`-implementing owned data,
//! enabling Salsa's Early Cutoff optimization.

use crate::protocol::{Coord, InfoStyle, Line, MenuStyle};

use super::info::{InfoIdentity, InfoState};
use super::menu::{ItemSplit, MenuColumns, MenuState};

/// Immutable snapshot of menu state for Salsa input.
#[derive(Clone, Debug, PartialEq)]
pub struct MenuSnapshot {
    pub items: Vec<Line>,
    pub anchor: Coord,
    /// ADR-031 Phase A.3: Style.
    pub selected_item_face: crate::protocol::Style,
    /// ADR-031 Phase A.3: Style.
    pub menu_face: crate::protocol::Style,
    pub style: MenuStyle,
    pub selected: Option<usize>,
    pub first_item: usize,
    pub columns: u16,
    pub win_height: u16,
    pub menu_lines: u16,
    pub max_item_width: u16,
    pub screen_w: u16,
    pub columns_split: Option<MenuColumnsSnapshot>,
}

/// Immutable snapshot of two-column menu layout.
#[derive(Clone, Debug, PartialEq)]
pub struct MenuColumnsSnapshot {
    pub splits: Vec<ItemSplitSnapshot>,
    pub max_candidate_width: u16,
    pub max_docstring_width: u16,
}

/// Immutable snapshot of a single item's column split.
#[derive(Clone, Debug, PartialEq)]
pub struct ItemSplitSnapshot {
    pub candidate_end: usize,
    pub docstring_start: usize,
    pub candidate_width: u16,
    pub docstring_width: u16,
}

impl MenuSnapshot {
    pub fn from_menu_state(menu: &MenuState) -> Self {
        Self {
            items: menu.items.clone(),
            anchor: menu.anchor,
            selected_item_face: menu.selected_item_face.clone(),
            menu_face: menu.menu_face.clone(),
            style: menu.style,
            selected: menu.selected,
            first_item: menu.first_item,
            columns: menu.columns,
            win_height: menu.win_height,
            menu_lines: menu.menu_lines,
            max_item_width: menu.max_item_width,
            screen_w: menu.screen_w,
            columns_split: menu
                .columns_split
                .as_ref()
                .map(MenuColumnsSnapshot::from_menu_columns),
        }
    }

    /// Content width accounting for two-column layout when present.
    pub fn effective_content_width(&self, cols: u16) -> u16 {
        match &self.columns_split {
            Some(mc) => {
                let cand_w = mc.max_candidate_width.min(cols * 2 / 5);
                (cand_w + 1 + mc.max_docstring_width).min(cols.saturating_sub(1))
            }
            None => self.max_item_width,
        }
    }
}

impl MenuColumnsSnapshot {
    pub fn from_menu_columns(mc: &MenuColumns) -> Self {
        Self {
            splits: mc
                .splits
                .iter()
                .map(ItemSplitSnapshot::from_item_split)
                .collect(),
            max_candidate_width: mc.max_candidate_width,
            max_docstring_width: mc.max_docstring_width,
        }
    }
}

impl ItemSplitSnapshot {
    pub fn from_item_split(split: &ItemSplit) -> Self {
        Self {
            candidate_end: split.candidate_end,
            docstring_start: split.docstring_start,
            candidate_width: split.candidate_width,
            docstring_width: split.docstring_width,
        }
    }
}

/// Immutable snapshot of info popup state for Salsa input.
#[derive(Clone, Debug, PartialEq)]
pub struct InfoSnapshot {
    pub title: Line,
    pub content: Vec<Line>,
    pub anchor: Coord,
    /// ADR-031 Phase A.3: Style.
    pub face: crate::protocol::Style,
    pub style: InfoStyle,
    pub identity: InfoIdentity,
    pub scroll_offset: u16,
}

impl InfoSnapshot {
    pub fn from_info_state(info: &InfoState) -> Self {
        Self {
            title: info.title.clone(),
            content: info.content.clone(),
            anchor: info.anchor,
            face: info.face.clone(),
            style: info.style,
            identity: info.identity.clone(),
            scroll_offset: info.scroll_offset,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::protocol::{Atom, Color, NamedColor};
    use crate::state::menu::MenuParams;

    fn make_atom(text: &str) -> Atom {
        Atom::from_face(Face::default(), text)
    }

    #[test]
    fn menu_snapshot_roundtrip() {
        let menu = MenuState::new(
            vec![vec![make_atom("item1")], vec![make_atom("item2")]],
            MenuParams {
                anchor: Coord { line: 5, column: 0 },
                selected_item_face: Face::default().into(),
                menu_face: Face::default().into(),
                style: MenuStyle::Inline,
                screen_w: 80,
                screen_h: 23,
                max_height: 10,
            },
        );
        let snapshot = MenuSnapshot::from_menu_state(&menu);
        assert_eq!(snapshot.items.len(), 2);
        assert_eq!(snapshot.style, MenuStyle::Inline);
        assert_eq!(snapshot.selected, None);
        assert_eq!(snapshot.win_height, menu.win_height);
    }

    #[test]
    fn menu_snapshot_equality() {
        let params = MenuParams {
            anchor: Coord { line: 5, column: 0 },
            selected_item_face: Face::default().into(),
            menu_face: Face::default().into(),
            style: MenuStyle::Inline,
            screen_w: 80,
            screen_h: 23,
            max_height: 10,
        };
        let menu1 = MenuState::new(vec![vec![make_atom("a")]], params.clone());
        let menu2 = MenuState::new(vec![vec![make_atom("a")]], params);
        assert_eq!(
            MenuSnapshot::from_menu_state(&menu1),
            MenuSnapshot::from_menu_state(&menu2),
        );
    }

    #[test]
    fn info_snapshot_roundtrip() {
        let info = InfoState {
            title: vec![make_atom("Title")],
            content: vec![vec![make_atom("Body")]],
            anchor: Coord {
                line: 3,
                column: 10,
            },
            face: Face {
                fg: Color::Named(NamedColor::Yellow),
                ..Face::default()
            }
            .into(),
            style: InfoStyle::Prompt,
            identity: InfoIdentity {
                style: InfoStyle::Prompt,
                anchor_line: 3,
            },
            scroll_offset: 2,
        };
        let snapshot = InfoSnapshot::from_info_state(&info);
        assert_eq!(snapshot.title.len(), 1);
        assert_eq!(snapshot.content.len(), 1);
        assert_eq!(snapshot.scroll_offset, 2);
        assert_eq!(snapshot.style, InfoStyle::Prompt);
    }
}
