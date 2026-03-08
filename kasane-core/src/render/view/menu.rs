use crate::element::{Edges, Element, FlexChild, Overlay, OverlayAnchor, Style};
use crate::layout::{MenuPlacement, layout_menu_inline, line_display_width};
use crate::protocol::{Atom, Face, MenuStyle};
use crate::state::{AppState, MenuState};

use super::build_styled_line_with_base;

/// Build a menu overlay using a replacement element with the same anchor as the default.
pub(super) fn build_replacement_menu_overlay(
    element: Element,
    menu: &MenuState,
    state: &AppState,
) -> Option<Overlay> {
    if menu.items.is_empty() || menu.win_height == 0 {
        return None;
    }

    let placement = menu_placement(state);

    let anchor = match menu.style {
        MenuStyle::Inline => {
            let win_w = (menu.max_item_width + 1).min(state.cols);
            let screen_h = state.available_height();
            let win = layout_menu_inline(
                &menu.anchor,
                win_w,
                menu.win_height,
                state.cols,
                screen_h,
                placement,
            );
            if win.width == 0 || win.height == 0 {
                return None;
            }
            OverlayAnchor::Absolute {
                x: win.x,
                y: win.y,
                w: win.width,
                h: win.height,
            }
        }
        MenuStyle::Prompt => {
            let status_row = state.available_height();
            let start_y = status_row.saturating_sub(menu.win_height);
            OverlayAnchor::Absolute {
                x: 0,
                y: start_y,
                w: state.cols,
                h: menu.win_height,
            }
        }
        MenuStyle::Search => {
            let status_row = state.available_height();
            let y = status_row.saturating_sub(1);
            OverlayAnchor::Absolute {
                x: 0,
                y,
                w: state.cols,
                h: 1,
            }
        }
    };

    Some(Overlay { element, anchor })
}

pub(super) fn build_menu_overlay(menu: &MenuState, state: &AppState) -> Option<Overlay> {
    if menu.items.is_empty() || menu.win_height == 0 {
        return None;
    }

    match menu.style {
        MenuStyle::Inline => build_menu_inline(menu, state),
        MenuStyle::Prompt => build_menu_prompt(menu, state),
        MenuStyle::Search => {
            if state.search_dropdown {
                build_menu_search_dropdown(menu, state)
            } else {
                build_menu_search(menu, state)
            }
        }
    }
}

/// Convert AppState menu_position config to layout MenuPlacement.
fn menu_placement(state: &AppState) -> MenuPlacement {
    MenuPlacement::from(state.menu_position)
}

fn build_menu_inline(menu: &MenuState, state: &AppState) -> Option<Overlay> {
    let win_w = (menu.max_item_width + 1).min(state.cols);
    let content_w = win_w.saturating_sub(1);
    let screen_h = state.available_height();
    let placement = menu_placement(state);

    let win = layout_menu_inline(
        &menu.anchor,
        win_w,
        menu.win_height,
        state.cols,
        screen_h,
        placement,
    );
    if win.width == 0 || win.height == 0 {
        return None;
    }

    // Build item rows
    let mut item_rows: Vec<FlexChild> = Vec::new();
    for line in 0..win.height {
        let item_idx = menu.first_item + line as usize;
        let face = if item_idx < menu.items.len() && Some(item_idx) == menu.selected {
            menu.selected_item_face
        } else {
            menu.menu_face
        };

        let item_element = if item_idx < menu.items.len() {
            build_styled_line_with_base(&menu.items[item_idx], &face, content_w)
        } else {
            Element::text("", face)
        };

        let padded = Element::Container {
            child: Box::new(item_element),
            border: None,
            shadow: false,
            padding: Edges::ZERO,
            style: Style::from(face),
            title: None,
        };

        item_rows.push(FlexChild::fixed(padded));
    }

    // Build scrollbar column
    let scrollbar = build_scrollbar(win.height, menu, &menu.menu_face);

    let content_col = Element::column(item_rows);
    let row = Element::row(vec![
        FlexChild::flexible(content_col, 1.0),
        FlexChild::fixed(scrollbar),
    ]);

    Some(Overlay {
        element: row,
        anchor: OverlayAnchor::Absolute {
            x: win.x,
            y: win.y,
            w: win.width,
            h: win.height,
        },
    })
}

fn build_menu_prompt(menu: &MenuState, state: &AppState) -> Option<Overlay> {
    if menu.columns == 0 {
        return None;
    }

    let status_row = state.available_height();
    let wh = menu.win_height;
    let columns = menu.columns as usize;
    let stride = wh as usize;
    let col_w = (state.cols.saturating_sub(1) as usize / columns).max(1);
    let first_col = menu.first_item / stride;
    let start_y = status_row.saturating_sub(wh);

    // Build grid of items as rows of columns
    let mut rows: Vec<FlexChild> = Vec::new();
    for line in 0..wh as usize {
        let mut cols: Vec<FlexChild> = Vec::new();
        for col in 0..columns {
            let item_idx = (first_col + col) * stride + line;
            let face = if item_idx < menu.items.len() && Some(item_idx) == menu.selected {
                menu.selected_item_face
            } else {
                menu.menu_face
            };

            let item_element = if item_idx < menu.items.len() {
                build_styled_line_with_base(&menu.items[item_idx], &face, col_w as u16)
            } else {
                Element::text("", face)
            };

            let padded = Element::Container {
                child: Box::new(item_element),
                border: None,
                shadow: false,
                padding: Edges::ZERO,
                style: Style::from(face),
                title: None,
            };

            cols.push(FlexChild::flexible(padded, 1.0));
        }

        rows.push(FlexChild::fixed(Element::row(cols)));
    }

    // Add scrollbar
    let scrollbar = build_scrollbar(wh, menu, &menu.menu_face);
    let content = Element::column(rows);
    let row = Element::row(vec![
        FlexChild::flexible(content, 1.0),
        FlexChild::fixed(scrollbar),
    ]);

    Some(Overlay {
        element: Element::Container {
            child: Box::new(row),
            border: None,
            shadow: false,
            padding: Edges::ZERO,
            style: Style::from(menu.menu_face),
            title: None,
        },
        anchor: OverlayAnchor::Absolute {
            x: 0,
            y: start_y,
            w: state.cols,
            h: wh,
        },
    })
}

fn build_menu_search(menu: &MenuState, state: &AppState) -> Option<Overlay> {
    let status_row = state.available_height();
    let y = status_row.saturating_sub(1);
    let screen_w = state.cols as usize;
    let first = menu.first_item;
    let has_prefix = first > 0;

    let mut atoms: Vec<Atom> = Vec::new();

    // "< " prefix
    if has_prefix {
        atoms.push(Atom {
            face: menu.menu_face,
            contents: "< ".to_string(),
        });
    }

    // Items with gaps
    let mut x = if has_prefix { 2 } else { 0 };
    for idx in first..menu.items.len() {
        let item_w = line_display_width(&menu.items[idx]);
        let has_more = idx + 1 < menu.items.len();
        let suffix_reserve = if has_more { 2 } else { 0 };

        if x + item_w + suffix_reserve > screen_w && x > 0 {
            if has_more {
                // Pad and add ">"
                let pad_len = screen_w.saturating_sub(x + 1);
                if pad_len > 0 {
                    atoms.push(Atom {
                        face: menu.menu_face,
                        contents: " ".repeat(pad_len),
                    });
                }
                atoms.push(Atom {
                    face: menu.menu_face,
                    contents: ">".to_string(),
                });
            }
            break;
        }

        let face = if Some(idx) == menu.selected {
            menu.selected_item_face
        } else {
            menu.menu_face
        };

        // Add item atoms with resolved face
        for atom in &menu.items[idx] {
            atoms.push(Atom {
                face,
                contents: atom.contents.clone(),
            });
        }
        x += item_w;

        // Gap
        if x < screen_w {
            atoms.push(Atom {
                face: menu.menu_face,
                contents: " ".to_string(),
            });
            x += 1;
        }
    }

    let element = Element::Container {
        child: Box::new(Element::StyledLine(atoms)),
        border: None,
        shadow: false,
        padding: Edges::ZERO,
        style: Style::from(menu.menu_face),
        title: None,
    };

    Some(Overlay {
        element,
        anchor: OverlayAnchor::Absolute {
            x: 0,
            y,
            w: state.cols,
            h: 1,
        },
    })
}

/// Build a search menu as a vertical dropdown instead of the default inline bar.
fn build_menu_search_dropdown(menu: &MenuState, state: &AppState) -> Option<Overlay> {
    let screen_h = state.available_height();
    let status_row = state.available_height();
    let max_h = 10u16.min(screen_h.saturating_sub(1));
    let win_h = (menu.items.len() as u16).min(max_h).max(1);
    let win_w = (menu.max_item_width + 1).min(state.cols);
    let content_w = win_w.saturating_sub(1);

    // Place above the status bar
    let y = status_row.saturating_sub(win_h);

    let mut item_rows: Vec<FlexChild> = Vec::new();
    for line in 0..win_h {
        let item_idx = menu.first_item + line as usize;
        let face = if item_idx < menu.items.len() && Some(item_idx) == menu.selected {
            menu.selected_item_face
        } else {
            menu.menu_face
        };

        let item_element = if item_idx < menu.items.len() {
            build_styled_line_with_base(&menu.items[item_idx], &face, content_w)
        } else {
            Element::text("", face)
        };

        let padded = Element::Container {
            child: Box::new(item_element),
            border: None,
            shadow: false,
            padding: Edges::ZERO,
            style: Style::from(face),
            title: None,
        };

        item_rows.push(FlexChild::fixed(padded));
    }

    let scrollbar = build_scrollbar(win_h, menu, &menu.menu_face);
    let content_col = Element::column(item_rows);
    let row = Element::row(vec![
        FlexChild::flexible(content_col, 1.0),
        FlexChild::fixed(scrollbar),
    ]);

    Some(Overlay {
        element: row,
        anchor: OverlayAnchor::Absolute {
            x: 0,
            y,
            w: win_w,
            h: win_h,
        },
    })
}

fn build_scrollbar(win_height: u16, menu: &MenuState, face: &Face) -> Element {
    let wh = win_height as usize;
    let item_count = menu.items.len();
    let columns = menu.columns as usize;
    if wh == 0 || item_count == 0 {
        return Element::Empty;
    }

    let menu_lines = item_count.div_ceil(columns);
    let mark_h = (wh * wh).div_ceil(menu_lines).min(wh);
    let menu_cols = item_count.div_ceil(wh);
    let first_col = menu.first_item / wh;
    let denom = menu_cols.saturating_sub(columns).max(1);
    let mark_y = ((wh - mark_h) * first_col / denom).min(wh - mark_h);

    let mut rows: Vec<FlexChild> = Vec::new();
    for row in 0..wh {
        let ch = if row >= mark_y && row < mark_y + mark_h {
            "█"
        } else {
            "░"
        };
        rows.push(FlexChild::fixed(Element::text(ch, *face)));
    }

    Element::column(rows)
}
