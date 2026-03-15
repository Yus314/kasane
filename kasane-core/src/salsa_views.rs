//! Salsa tracked view functions (Phase 2 — pure Element generation).
//!
//! These tracked functions produce Element trees from Salsa inputs
//! WITHOUT any plugin interaction. Plugin contributions, transforms,
//! and annotations are applied in Stage 2 (outside Salsa).
//!
//! All functions use `#[salsa::tracked(no_eq)]` because `Element` does
//! not implement `PartialEq`. Memoization still works: if inputs haven't
//! changed, the cached result is returned without re-execution.

use unicode_width::UnicodeWidthStr;

use crate::element::{
    BorderConfig, BorderLineStyle, Edges, Element, FlexChild, Overlay, OverlayAnchor, Style,
};
use crate::layout::{
    self, ASSISTANT_CLIPPY, ASSISTANT_WIDTH, MenuPlacement, layout_info, layout_menu_inline,
    line_display_width,
};
use crate::protocol::{Atom, Face, InfoStyle, MenuStyle};
use crate::render::view::build_styled_line_with_base;
use crate::salsa_db::KasaneDb;
use crate::salsa_inputs::*;
use crate::salsa_queries;
use crate::state::snapshot::{InfoSnapshot, MenuColumnsSnapshot, MenuSnapshot};

// ---------------------------------------------------------------------------
// Status bar
// ---------------------------------------------------------------------------

/// Pure status bar element: status_line + mode_line in a row.
#[salsa::tracked(no_eq)]
pub fn pure_status_element(db: &dyn KasaneDb, status: StatusInput) -> Element {
    let status_line = status.status_line(db);
    let mode_line = status.status_mode_line(db);
    let default_face = status.status_default_face(db);

    let status_el = build_styled_line_with_base(status_line, &default_face, 0);
    let mode_el = build_styled_line_with_base(mode_line, &default_face, 0);
    let mode_width = line_display_width(mode_line) as u16;

    let mut children = Vec::new();
    children.push(FlexChild::flexible(status_el, 1.0));
    if mode_width > 0 {
        children.push(FlexChild::fixed(mode_el));
    }
    Element::row(children)
}

// ---------------------------------------------------------------------------
// Buffer
// ---------------------------------------------------------------------------

/// Pure buffer element: a BufferRef spanning the available height.
#[salsa::tracked(no_eq)]
pub fn pure_buffer_element(db: &dyn KasaneDb, config: ConfigInput) -> Element {
    let height = salsa_queries::available_height(db, config) as usize;
    Element::buffer_ref(0..height)
}

// ---------------------------------------------------------------------------
// Menu
// ---------------------------------------------------------------------------

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
        menu.selected_item_face
    } else {
        menu.menu_face
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
        menu.selected_item_face
    } else {
        menu.menu_face
    };

    if item_idx >= menu.items.len() {
        return Element::container(Element::text("", face), Style::from(face));
    }

    let item = &menu.items[item_idx];
    let split = &columns.splits[item_idx];

    let mut atoms: Vec<Atom> = Vec::new();

    // 1. Candidate portion
    let cand_atoms = &item[..split.candidate_end];
    let mut cand_resolved = truncate_atoms_pure(cand_atoms, candidate_col_w, &face);
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
        cand_resolved.push(Atom {
            face,
            contents: " ".repeat(pad).into(),
        });
    }
    atoms.extend(cand_resolved);

    // 2. Gap
    atoms.push(Atom {
        face,
        contents: " ".into(),
    });

    // 3. Docstring portion
    for atom in &item[split.docstring_start..] {
        atoms.push(Atom {
            face: crate::protocol::resolve_face(&atom.face, &face),
            contents: atom.contents.clone(),
        });
    }

    Element::container(Element::StyledLine(atoms), Style::from(face))
}

fn truncate_atoms_pure(atoms: &[Atom], max_width: u16, base_face: &Face) -> Vec<Atom> {
    let max_w = max_width as usize;
    let total: usize = atoms
        .iter()
        .map(|a| {
            a.contents
                .split(|c: char| c.is_control())
                .map(UnicodeWidthStr::width)
                .sum::<usize>()
        })
        .sum();

    if total <= max_w {
        return atoms
            .iter()
            .map(|a| Atom {
                face: crate::protocol::resolve_face(&a.face, base_face),
                contents: a.contents.clone(),
            })
            .collect();
    }

    let limit = max_w.saturating_sub(1);
    let mut result = Vec::new();
    let mut used = 0usize;
    for atom in atoms {
        let face = crate::protocol::resolve_face(&atom.face, base_face);
        let mut buf = String::new();
        for ch in atom.contents.chars() {
            let cw = if ch.is_control() {
                0
            } else {
                UnicodeWidthStr::width(ch.encode_utf8(&mut [0; 4]) as &str)
            };
            if used + cw > limit {
                break;
            }
            buf.push(ch);
            used += cw;
        }
        if !buf.is_empty() {
            result.push(Atom {
                face,
                contents: buf.into(),
            });
        }
        if used >= limit {
            break;
        }
    }
    result.push(Atom {
        face: *base_face,
        contents: "\u{2026}".into(),
    });
    result
}

fn build_scrollbar_pure(
    win_height: u16,
    item_count: usize,
    columns: u16,
    first_item: usize,
    face: &Face,
    thumb: &str,
    track: &str,
) -> Element {
    let wh = win_height as usize;
    if wh == 0 || item_count == 0 {
        return Element::Empty;
    }

    let menu_lines = item_count.div_ceil(columns as usize);
    let mark_h = (wh * wh).div_ceil(menu_lines).min(wh);
    let menu_cols = item_count.div_ceil(wh);
    let first_col = first_item / wh;
    let denom = menu_cols.saturating_sub(columns as usize).max(1);
    let mark_y = ((wh - mark_h) * first_col / denom).min(wh - mark_h);

    let mut rows: Vec<FlexChild> = Vec::new();
    for row in 0..wh {
        let ch = if row >= mark_y && row < mark_y + mark_h {
            thumb
        } else {
            track
        };
        rows.push(FlexChild::fixed(Element::text(ch, *face)));
    }

    Element::column(rows)
}

fn build_menu_inline_pure(
    menu: &MenuSnapshot,
    cols: u16,
    screen_h: u16,
    menu_position: crate::config::MenuPosition,
    scrollbar_thumb: &str,
    scrollbar_track: &str,
) -> Option<Overlay> {
    let win_w = (menu.effective_content_width(cols) + 1).min(cols);
    let content_w = win_w.saturating_sub(1);
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

    let scrollbar = build_scrollbar_pure(
        win.height,
        menu.items.len(),
        menu.columns,
        menu.first_item,
        &menu.menu_face,
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
    let scrollbar = build_scrollbar_pure(
        wh,
        menu.items.len(),
        menu.columns,
        menu.first_item,
        &menu.menu_face,
        scrollbar_thumb,
        scrollbar_track,
    );
    let content = Element::grid(grid_columns, grid_children);
    let row = Element::row(vec![
        FlexChild::flexible(content, 1.0),
        FlexChild::fixed(scrollbar),
    ]);

    Some(Overlay {
        element: Element::container(row, Style::from(menu.menu_face)),
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

    let mut atoms: Vec<Atom> = Vec::new();

    if has_prefix {
        atoms.push(Atom {
            face: menu.menu_face,
            contents: "< ".into(),
        });
    }

    let mut x = if has_prefix { 2 } else { 0 };
    for idx in first..menu.items.len() {
        let item_w = line_display_width(&menu.items[idx]);
        let has_more = idx + 1 < menu.items.len();
        let suffix_reserve = if has_more { 2 } else { 0 };

        if x + item_w + suffix_reserve > screen_w && x > 0 {
            if has_more {
                let pad_len = screen_w.saturating_sub(x + 1);
                if pad_len > 0 {
                    atoms.push(Atom {
                        face: menu.menu_face,
                        contents: " ".repeat(pad_len).into(),
                    });
                }
                atoms.push(Atom {
                    face: menu.menu_face,
                    contents: ">".into(),
                });
            }
            break;
        }

        let face = if Some(idx) == menu.selected {
            menu.selected_item_face
        } else {
            menu.menu_face
        };

        for atom in &menu.items[idx] {
            atoms.push(Atom {
                face,
                contents: atom.contents.clone(),
            });
        }
        x += item_w;

        if x < screen_w {
            atoms.push(Atom {
                face: menu.menu_face,
                contents: " ".into(),
            });
            x += 1;
        }
    }

    let element = Element::container(Element::StyledLine(atoms), Style::from(menu.menu_face));

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
    let max_h = 10u16.min(screen_h.saturating_sub(1));
    let win_h = (menu.items.len() as u16).min(max_h).max(1);
    let win_w = (menu.max_item_width + 1).min(cols);
    let content_w = win_w.saturating_sub(1);
    let y = screen_h.saturating_sub(win_h);

    let item_rows: Vec<FlexChild> = (0..win_h)
        .map(|line| {
            let item_idx = menu.first_item + line as usize;
            FlexChild::fixed(build_menu_item_element_pure(menu, item_idx, content_w))
        })
        .collect();

    let scrollbar = build_scrollbar_pure(
        win_h,
        menu.items.len(),
        menu.columns,
        menu.first_item,
        &menu.menu_face,
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

// ---------------------------------------------------------------------------
// Info
// ---------------------------------------------------------------------------

/// Pure info overlay elements (no plugin transforms).
#[salsa::tracked(no_eq)]
pub fn pure_info_overlays(
    db: &dyn KasaneDb,
    info_input: InfoInput,
    menu_input: MenuInput,
    buffer: BufferInput,
    config: ConfigInput,
) -> Vec<Overlay> {
    let infos = info_input.infos(db);
    if infos.is_empty() {
        return vec![];
    }

    let cols = config.cols(db);
    let screen_h = salsa_queries::available_height(db, config);
    let shadow_enabled = config.shadow_enabled(db);
    let cursor_pos = buffer.cursor_pos(db);
    let assistant_art = config.assistant_art(db);

    // Build avoid rects: menu rect + cursor position
    let mut avoid_rects: Vec<crate::layout::Rect> = Vec::new();
    if let Some(menu_rect) = compute_menu_rect(
        menu_input.menu(db),
        cols,
        screen_h,
        config.menu_position(db),
    ) {
        avoid_rects.push(menu_rect);
    }
    avoid_rects.push(crate::layout::Rect {
        x: cursor_pos.column as u16,
        y: cursor_pos.line as u16,
        w: 1,
        h: 1,
    });

    let mut overlays = Vec::new();
    for (info_idx, info) in infos.iter().enumerate() {
        let overlay = build_info_overlay_pure(
            info,
            cols,
            screen_h,
            shadow_enabled,
            assistant_art,
            &avoid_rects,
            info_idx,
        );
        if let Some(mut o) = overlay {
            if let OverlayAnchor::Absolute { x, y, w, h } = &o.anchor {
                avoid_rects.push(crate::layout::Rect {
                    x: *x,
                    y: *y,
                    w: *w,
                    h: *h,
                });
            }
            // Wrap with Interactive for mouse hit testing
            let interactive_id = crate::element::InteractiveId(
                crate::element::InteractiveId::INFO_BASE + info_idx as u32,
            );
            o.element = Element::Interactive {
                child: Box::new(o.element),
                id: interactive_id,
            };
            overlays.push(o);
        }
    }
    overlays
}

/// Compute the menu rectangle from a `MenuSnapshot`, mirroring `get_menu_rect()`.
fn compute_menu_rect(
    menu: &Option<MenuSnapshot>,
    cols: u16,
    screen_h: u16,
    menu_position: crate::config::MenuPosition,
) -> Option<crate::layout::Rect> {
    let menu = menu.as_ref()?;
    if menu.items.is_empty() || menu.win_height == 0 {
        return None;
    }
    match menu.style {
        MenuStyle::Prompt => {
            let start_y = screen_h.saturating_sub(menu.win_height);
            Some(crate::layout::Rect {
                x: 0,
                y: start_y,
                w: cols,
                h: menu.win_height,
            })
        }
        MenuStyle::Search => Some(crate::layout::Rect {
            x: 0,
            y: screen_h.saturating_sub(1),
            w: cols,
            h: 1,
        }),
        MenuStyle::Inline => {
            let win_w = (menu.effective_content_width(cols) + 1).min(cols);
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
            Some(crate::layout::Rect {
                x: win.x,
                y: win.y,
                w: win.width,
                h: win.height,
            })
        }
    }
}

fn build_info_overlay_pure(
    info: &InfoSnapshot,
    cols: u16,
    screen_h: u16,
    shadow_enabled: bool,
    assistant_art: &Option<Vec<String>>,
    avoid: &[crate::layout::Rect],
    _index: usize,
) -> Option<Overlay> {
    let win = layout_info(
        &info.title,
        &info.content,
        &info.anchor,
        info.style,
        cols,
        screen_h,
        avoid,
    );

    if win.width == 0 || win.height == 0 {
        return None;
    }

    let element = match info.style {
        InfoStyle::Prompt => build_info_prompt_pure(info, &win, assistant_art),
        InfoStyle::Modal => build_info_framed_pure(info, &win, shadow_enabled),
        InfoStyle::Inline | InfoStyle::InlineAbove | InfoStyle::MenuDoc => {
            build_info_nonframed_pure(info, &win)
        }
    };

    element.map(|el| Overlay {
        element: el,
        anchor: win.into(),
    })
}

fn build_info_prompt_pure(
    info: &InfoSnapshot,
    win: &layout::FloatingWindow,
    assistant_art: &Option<Vec<String>>,
) -> Option<Element> {
    if win.width < ASSISTANT_WIDTH + 5 || win.height < 3 {
        return None;
    }

    let total_h = win.height as usize;
    let cw = win.width.saturating_sub(ASSISTANT_WIDTH + 4);
    if cw == 0 {
        return None;
    }

    let content_end = info
        .content
        .iter()
        .rposition(|line| line_display_width(line) > 0)
        .map(|i| i + 1)
        .unwrap_or(0);
    let trimmed = &info.content[..content_end];

    let art_len = assistant_art
        .as_ref()
        .map_or(ASSISTANT_CLIPPY.len(), |a| a.len());
    let asst_top = ((total_h as i32 - art_len as i32 + 1) / 2).max(0) as usize;
    let mut asst_rows: Vec<FlexChild> = Vec::new();
    for row in 0..total_h {
        let idx = if row >= asst_top {
            (row - asst_top).min(art_len - 1)
        } else {
            art_len - 1
        };
        let line_str: &str = match assistant_art {
            Some(custom) => &custom[idx],
            None => ASSISTANT_CLIPPY[idx],
        };
        asst_rows.push(FlexChild::fixed(Element::text(line_str, info.face)));
    }
    let assistant_col = Element::column(asst_rows);

    let frame_content_h = total_h.saturating_sub(2) as u16;
    let wrapped_lines = wrap_content_lines_pure(trimmed, cw, frame_content_h, &info.face);
    let frame_h = (wrapped_lines.len() as u16 + 2).min(total_h as u16);

    let content_rows: Vec<FlexChild> = wrapped_lines
        .iter()
        .map(|line| FlexChild::fixed(Element::StyledLine(line.clone())))
        .collect();
    let content_col = Element::column(content_rows);

    let framed_content = Element::Container {
        child: Box::new(content_col),
        border: Some(BorderConfig::from(BorderLineStyle::Rounded)),
        shadow: false,
        padding: Edges {
            top: 0,
            right: 1,
            bottom: 0,
            left: 1,
        },
        style: Style::from(info.face),
        title: if info.title.is_empty() {
            None
        } else {
            Some(info.title.clone())
        },
    };

    let frame_w = win.width.saturating_sub(ASSISTANT_WIDTH);
    let base = Element::row(vec![
        FlexChild::fixed(assistant_col),
        FlexChild::flexible(Element::text("", info.face), 1.0),
    ]);
    let container = Element::stack(
        Element::container(base, Style::from(info.face)),
        vec![Overlay {
            element: framed_content,
            anchor: OverlayAnchor::Absolute {
                x: ASSISTANT_WIDTH,
                y: 0,
                w: frame_w,
                h: frame_h,
            },
        }],
    );

    Some(container)
}

fn build_info_framed_pure(
    info: &InfoSnapshot,
    win: &layout::FloatingWindow,
    shadow: bool,
) -> Option<Element> {
    let inner_w = win.width.saturating_sub(4).max(1);
    let inner_h = win.height.saturating_sub(2);

    let content_col = build_content_column_pure(&info.content, inner_w, inner_h, &info.face);

    let framed = Element::Container {
        child: Box::new(content_col),
        border: Some(BorderConfig::from(BorderLineStyle::Rounded)),
        shadow,
        padding: Edges {
            top: 0,
            right: 1,
            bottom: 0,
            left: 1,
        },
        style: Style::from(info.face),
        title: if info.title.is_empty() {
            None
        } else {
            Some(info.title.clone())
        },
    };

    Some(framed)
}

fn build_info_nonframed_pure(info: &InfoSnapshot, win: &layout::FloatingWindow) -> Option<Element> {
    let content_col = build_content_column_pure(&info.content, win.width, win.height, &info.face);
    Some(Element::container(content_col, Style::from(info.face)))
}

fn build_content_column_pure(
    content: &[crate::protocol::Line],
    max_w: u16,
    max_h: u16,
    face: &Face,
) -> Element {
    let wrapped_lines = wrap_content_lines_pure(content, max_w, max_h, face);
    let content_rows: Vec<FlexChild> = wrapped_lines
        .iter()
        .map(|line| FlexChild::fixed(Element::StyledLine(line.clone())))
        .collect();
    Element::column(content_rows)
}

fn wrap_content_lines_pure(
    content: &[crate::protocol::Line],
    max_width: u16,
    max_rows: u16,
    base_face: &Face,
) -> Vec<Vec<Atom>> {
    if max_width == 0 {
        return vec![];
    }

    let mut result = Vec::new();

    for line in content {
        if result.len() >= max_rows as usize {
            break;
        }

        let mut graphemes: Vec<(&str, Face, u16)> = Vec::new();
        for atom in line {
            let face = crate::protocol::resolve_face(&atom.face, base_face);
            for grapheme in atom.contents.split_inclusive(|_: char| true) {
                if grapheme.is_empty() || grapheme.starts_with(|c: char| c.is_control()) {
                    continue;
                }
                let w = UnicodeWidthStr::width(grapheme) as u16;
                if w == 0 {
                    continue;
                }
                graphemes.push((grapheme, face, w));
            }
        }

        if graphemes.is_empty() {
            result.push(vec![Atom {
                face: *base_face,
                contents: compact_str::CompactString::default(),
            }]);
            continue;
        }

        let metrics: Vec<(u16, bool)> = graphemes
            .iter()
            .map(|(text, _, w)| (*w, !layout::is_word_char(text)))
            .collect();
        let segments = layout::word_wrap_segments(&metrics, max_width);

        for seg in &segments {
            if result.len() >= max_rows as usize {
                break;
            }
            let mut row_atoms = Vec::new();
            let mut current_face: Option<Face> = None;
            let mut current_text = compact_str::CompactString::default();

            for &(grapheme, face, _) in &graphemes[seg.start..seg.end] {
                if current_face == Some(face) {
                    current_text.push_str(grapheme);
                } else {
                    if let Some(cf) = current_face {
                        row_atoms.push(Atom {
                            face: cf,
                            contents: std::mem::take(&mut current_text),
                        });
                    }
                    current_face = Some(face);
                    current_text = compact_str::CompactString::from(grapheme);
                }
            }
            if let Some(cf) = current_face {
                row_atoms.push(Atom {
                    face: cf,
                    contents: current_text,
                });
            }

            result.push(row_atoms);
        }
    }

    result
}
