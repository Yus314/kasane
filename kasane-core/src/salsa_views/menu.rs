use unicode_width::UnicodeWidthStr;

use crate::element::{Element, FlexChild, Overlay, OverlayAnchor, Style};
use crate::layout::{MenuPlacement, layout_menu_inline, line_display_width};
use crate::protocol::{Atom, MenuStyle};
use crate::render::builders::{
    MAX_DROPDOWN_HEIGHT, PREFIX_WIDTH, SCROLLBAR_WIDTH, SUFFIX_RESERVE, build_scrollbar,
    truncate_atoms,
};
use crate::render::view::build_styled_line_with_base;
use crate::salsa_db::KasaneDb;
use crate::salsa_inputs::*;
use crate::salsa_queries;
use crate::state::snapshot::{MenuColumnsSnapshot, MenuSnapshot};

/// Pure menu overlay element (no plugin transforms).
#[salsa::tracked(no_eq)]
pub fn pure_menu_overlay(
    db: &dyn KasaneDb,
    menu_input: MenuInput,
    config: ConfigInput,
) -> Option<Overlay> {
    let menu = menu_input.menu(db);
    let menu = match menu.as_ref() {
        Some(m) if !m.items.is_empty() && m.win_height > 0 => m,
        _ => return None,
    };

    let cols = config.cols(db);
    let screen_h = salsa_queries::available_height(db, config);
    let search_dropdown = config.search_dropdown(db);
    let menu_position = config.menu_position(db);
    let scrollbar_thumb = config.scrollbar_thumb(db);
    let scrollbar_track = config.scrollbar_track(db);

    match menu.style {
        MenuStyle::Inline => build_menu_inline_pure(
            menu,
            cols,
            screen_h,
            menu_position,
            scrollbar_thumb,
            scrollbar_track,
        ),
        MenuStyle::Prompt => {
            build_menu_prompt_pure(menu, cols, screen_h, scrollbar_thumb, scrollbar_track)
        }
        MenuStyle::Search => {
            if search_dropdown {
                build_menu_search_dropdown_pure(
                    menu,
                    cols,
                    screen_h,
                    scrollbar_thumb,
                    scrollbar_track,
                )
            } else {
                build_menu_search_pure(menu, cols, screen_h)
            }
        }
    }
}

/// Pure menu item element (no plugin transform_menu_item).
fn build_menu_item_element_pure(menu: &MenuSnapshot, item_idx: usize, width: u16) -> Element {
    let selected = item_idx < menu.items.len() && Some(item_idx) == menu.selected;
    let face = if selected {
        menu.selected_item_face.to_face()
    } else {
        menu.menu_face.to_face()
    };
    let item = if item_idx < menu.items.len() {
        build_styled_line_with_base(&menu.items[item_idx], &face, width)
    } else {
        Element::text("", face)
    };
    Element::container(item, Style::from(face))
}

/// Pure split (two-column) menu item element (no plugin transform_menu_item).
fn build_split_item_element_pure(
    menu: &MenuSnapshot,
    columns: &MenuColumnsSnapshot,
    item_idx: usize,
    candidate_col_w: u16,
    _content_w: u16,
) -> Element {
    let selected = item_idx < menu.items.len() && Some(item_idx) == menu.selected;
    let face = if selected {
        menu.selected_item_face.to_face()
    } else {
        menu.menu_face.to_face()
    };

    if item_idx >= menu.items.len() {
        return Element::container(Element::text("", face), Style::from(face));
    }

    let item = &menu.items[item_idx];
    let split = &columns.splits[item_idx];

    let mut atoms: Vec<Atom> = Vec::new();

    // 1. Candidate portion
    let cand_atoms = &item[..split.candidate_end];
    let mut cand_resolved = truncate_atoms(cand_atoms, candidate_col_w, &face, "\u{2026}");
    let cand_w: usize = cand_resolved
        .iter()
        .map(|a| {
            a.contents
                .split(|c: char| c.is_control())
                .map(UnicodeWidthStr::width)
                .sum::<usize>()
        })
        .sum();
    if (cand_w as u16) < candidate_col_w {
        let pad = candidate_col_w as usize - cand_w;
        cand_resolved.push(Atom::from_face(face, " ".repeat(pad)));
    }
    atoms.extend(cand_resolved);

    // 2. Gap
    atoms.push(Atom::from_face(face, " "));

    // 3. Docstring portion
    for atom in &item[split.docstring_start..] {
        atoms.push(Atom::from_face(
            crate::protocol::resolve_face(&atom.face(), &face),
            atom.contents.clone(),
        ));
    }

    Element::container(Element::StyledLine(atoms), Style::from(face))
}

fn build_menu_inline_pure(
    menu: &MenuSnapshot,
    cols: u16,
    screen_h: u16,
    menu_position: crate::config::MenuPosition,
    scrollbar_thumb: &str,
    scrollbar_track: &str,
) -> Option<Overlay> {
    let win_w = (menu.effective_content_width(cols) + SCROLLBAR_WIDTH).min(cols);
    let content_w = win_w.saturating_sub(SCROLLBAR_WIDTH);
    let placement = MenuPlacement::from(menu_position);

    let win = layout_menu_inline(
        &menu.anchor,
        win_w,
        menu.win_height,
        cols,
        screen_h,
        placement,
    );
    if win.width == 0 || win.height == 0 {
        return None;
    }

    let candidate_col_w = menu
        .columns_split
        .as_ref()
        .map(|mc| mc.max_candidate_width.min(cols * 2 / 5));

    let item_rows: Vec<FlexChild> = (0..win.height)
        .map(|line| {
            let item_idx = menu.first_item + line as usize;
            let element = match (&menu.columns_split, candidate_col_w) {
                (Some(columns), Some(cw)) => {
                    build_split_item_element_pure(menu, columns, item_idx, cw, content_w)
                }
                _ => build_menu_item_element_pure(menu, item_idx, content_w),
            };
            FlexChild::fixed(element)
        })
        .collect();

    let menu_face = menu.menu_face.to_face();
    let scrollbar = build_scrollbar(
        win.height,
        menu.items.len(),
        menu.columns,
        menu.first_item,
        &menu_face,
        scrollbar_thumb,
        scrollbar_track,
    );

    let content_col = Element::column(item_rows);
    let row = Element::row(vec![
        FlexChild::flexible(content_col, 1.0),
        FlexChild::fixed(scrollbar),
    ]);

    Some(Overlay {
        element: row,
        anchor: win.into(),
    })
}

fn build_menu_prompt_pure(
    menu: &MenuSnapshot,
    cols: u16,
    screen_h: u16,
    scrollbar_thumb: &str,
    scrollbar_track: &str,
) -> Option<Overlay> {
    if menu.columns == 0 {
        return None;
    }

    let wh = menu.win_height;
    let columns = menu.columns as usize;
    let stride = wh as usize;
    let col_w = (cols.saturating_sub(1) as usize / columns).max(1);
    let first_col = menu.first_item / stride;
    let start_y = screen_h.saturating_sub(wh);

    let mut grid_children: Vec<Element> = Vec::with_capacity(wh as usize * columns);
    for line in 0..wh as usize {
        for col in 0..columns {
            let item_idx = (first_col + col) * stride + line;
            grid_children.push(build_menu_item_element_pure(menu, item_idx, col_w as u16));
        }
    }

    let grid_columns = vec![crate::element::GridColumn::flex(1.0); columns];
    let menu_face = menu.menu_face.to_face();
    let scrollbar = build_scrollbar(
        wh,
        menu.items.len(),
        menu.columns,
        menu.first_item,
        &menu_face,
        scrollbar_thumb,
        scrollbar_track,
    );
    let content = Element::grid(grid_columns, grid_children);
    let row = Element::row(vec![
        FlexChild::flexible(content, 1.0),
        FlexChild::fixed(scrollbar),
    ]);

    Some(Overlay {
        element: Element::container(row, Style::from(menu.menu_face.to_face())),
        anchor: OverlayAnchor::Absolute {
            x: 0,
            y: start_y,
            w: cols,
            h: wh,
        },
    })
}

fn build_menu_search_pure(menu: &MenuSnapshot, cols: u16, screen_h: u16) -> Option<Overlay> {
    let y = screen_h.saturating_sub(1);
    let screen_w = cols as usize;
    let first = menu.first_item;
    let has_prefix = first > 0;
    let menu_face = menu.menu_face.to_face();
    let selected_face = menu.selected_item_face.to_face();

    let mut atoms: Vec<Atom> = Vec::new();

    if has_prefix {
        atoms.push(Atom::from_face(menu_face, "< "));
    }

    let mut x = if has_prefix { PREFIX_WIDTH } else { 0 };
    for idx in first..menu.items.len() {
        let item_w = line_display_width(&menu.items[idx]);
        let has_more = idx + 1 < menu.items.len();
        let suffix_reserve = if has_more { SUFFIX_RESERVE } else { 0 };

        if x + item_w + suffix_reserve > screen_w && x > 0 {
            if has_more {
                let pad_len = screen_w.saturating_sub(x + 1);
                if pad_len > 0 {
                    atoms.push(Atom::from_face(menu_face, " ".repeat(pad_len)));
                }
                atoms.push(Atom::from_face(menu_face, ">"));
            }
            break;
        }

        let face = if Some(idx) == menu.selected {
            selected_face
        } else {
            menu_face
        };

        for atom in &menu.items[idx] {
            atoms.push(Atom::from_face(face, atom.contents.clone()));
        }
        x += item_w;

        if x < screen_w {
            atoms.push(Atom::from_face(menu_face, " "));
            x += 1;
        }
    }

    let element = Element::container(Element::StyledLine(atoms), Style::from(menu_face));

    Some(Overlay {
        element,
        anchor: OverlayAnchor::Absolute {
            x: 0,
            y,
            w: cols,
            h: 1,
        },
    })
}

fn build_menu_search_dropdown_pure(
    menu: &MenuSnapshot,
    cols: u16,
    screen_h: u16,
    scrollbar_thumb: &str,
    scrollbar_track: &str,
) -> Option<Overlay> {
    let max_h = MAX_DROPDOWN_HEIGHT.min(screen_h.saturating_sub(1));
    let win_h = (menu.items.len() as u16).min(max_h).max(1);
    let win_w = (menu.max_item_width + SCROLLBAR_WIDTH).min(cols);
    let content_w = win_w.saturating_sub(SCROLLBAR_WIDTH);
    let y = screen_h.saturating_sub(win_h);

    let item_rows: Vec<FlexChild> = (0..win_h)
        .map(|line| {
            let item_idx = menu.first_item + line as usize;
            FlexChild::fixed(build_menu_item_element_pure(menu, item_idx, content_w))
        })
        .collect();

    let menu_face = menu.menu_face.to_face();
    let scrollbar = build_scrollbar(
        win_h,
        menu.items.len(),
        menu.columns,
        menu.first_item,
        &menu_face,
        scrollbar_thumb,
        scrollbar_track,
    );
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
