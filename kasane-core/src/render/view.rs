use unicode_width::UnicodeWidthStr;

use crate::element::{BorderConfig, BorderLineStyle, Edges, Element, FlexChild, Overlay, OverlayAnchor, Style};
use crate::layout::{
    self, MenuPlacement, layout_info, layout_menu_inline, line_display_width, word_wrap_segments,
};
use crate::plugin::{DecorateTarget, PluginRegistry, ReplaceTarget, Slot};
use crate::protocol::{Atom, Face, InfoStyle, Line, MenuStyle};
use crate::state::{AppState, InfoState, MenuState};

/// Build the full Element tree from application state.
pub fn view(state: &AppState, registry: &PluginRegistry) -> Element {
    crate::perf::perf_span!("view");
    let buffer_rows = state.rows.saturating_sub(1) as usize;

    // Collect plugin slots
    let above_buffer = registry.collect_slot(Slot::AboveBuffer, state);
    let below_buffer = registry.collect_slot(Slot::BelowBuffer, state);
    let buffer_left = registry.collect_slot(Slot::BufferLeft, state);
    let buffer_right = registry.collect_slot(Slot::BufferRight, state);
    let above_status = registry.collect_slot(Slot::AboveStatus, state);
    let status_left = registry.collect_slot(Slot::StatusLeft, state);
    let status_right = registry.collect_slot(Slot::StatusRight, state);
    let plugin_overlays = registry.collect_slot(Slot::Overlay, state);

    // Build buffer row (center area + optional sidebars)
    let buffer_element = Element::buffer_ref(0..buffer_rows);
    let buffer_row = if buffer_left.is_empty() && buffer_right.is_empty() {
        let decorated = registry.apply_decorator(DecorateTarget::Buffer, buffer_element, state);
        FlexChild::flexible(decorated, 1.0)
    } else {
        let mut row_children = Vec::new();
        for el in buffer_left {
            row_children.push(FlexChild::fixed(el));
        }
        row_children.push(FlexChild::flexible(buffer_element, 1.0));
        for el in buffer_right {
            row_children.push(FlexChild::fixed(el));
        }
        let row = Element::row(row_children);
        let decorated = registry.apply_decorator(DecorateTarget::Buffer, row, state);
        FlexChild::flexible(decorated, 1.0)
    };

    // Build status bar (with replacement + decorator support)
    let status_bar = registry
        .get_replacement(ReplaceTarget::StatusBar, state)
        .unwrap_or_else(|| build_status_bar(state, status_left, status_right));
    let status_bar = registry.apply_decorator(DecorateTarget::StatusBar, status_bar, state);

    // Build main column (status bar position: top or bottom)
    let mut column_children = Vec::new();
    if state.status_at_top {
        column_children.push(FlexChild::fixed(status_bar));
        for el in above_status {
            column_children.push(FlexChild::fixed(el));
        }
        for el in above_buffer {
            column_children.push(FlexChild::fixed(el));
        }
        column_children.push(buffer_row);
        for el in below_buffer {
            column_children.push(FlexChild::fixed(el));
        }
    } else {
        for el in above_buffer {
            column_children.push(FlexChild::fixed(el));
        }
        column_children.push(buffer_row);
        for el in below_buffer {
            column_children.push(FlexChild::fixed(el));
        }
        for el in above_status {
            column_children.push(FlexChild::fixed(el));
        }
        column_children.push(FlexChild::fixed(status_bar));
    }

    let base = Element::column(column_children);

    // Collect overlays
    let mut overlays = Vec::new();

    if let Some(ref menu) = state.menu {
        let replace_target = match menu.style {
            MenuStyle::Prompt => ReplaceTarget::MenuPrompt,
            MenuStyle::Inline => ReplaceTarget::MenuInline,
            MenuStyle::Search => ReplaceTarget::MenuSearch,
        };
        let menu_overlay = match registry.get_replacement(replace_target, state) {
            Some(replacement) => build_replacement_menu_overlay(replacement, menu, state),
            None => build_menu_overlay(menu, state),
        };
        if let Some(mut overlay) = menu_overlay {
            overlay.element =
                registry.apply_decorator(DecorateTarget::Menu, overlay.element, state);
            overlays.push(overlay);
        }
    }

    {
        let menu_rect = super::menu::get_menu_rect(state);
        let mut avoid_rects: Vec<crate::layout::Rect> = Vec::new();
        if let Some(mr) = menu_rect {
            avoid_rects.push(mr);
        }
        // Add cursor position as a 1×1 avoid rect (collision avoidance)
        avoid_rects.push(crate::layout::Rect {
            x: state.cursor_pos.column as u16,
            y: state.cursor_pos.line as u16,
            w: 1,
            h: 1,
        });

        for (info_idx, info_state) in state.infos.iter().enumerate() {
            let replace_target = match info_state.style {
                InfoStyle::Prompt => Some(ReplaceTarget::InfoPrompt),
                InfoStyle::Modal => Some(ReplaceTarget::InfoModal),
                _ => None,
            };
            let info_overlay =
                match replace_target.and_then(|t| registry.get_replacement(t, state)) {
                    Some(replacement) => {
                        build_replacement_info_overlay(
                            replacement, info_state, state, &avoid_rects,
                        )
                    }
                    None => {
                        build_info_overlay_indexed(info_state, state, &avoid_rects, info_idx)
                    }
                };
            if let Some(mut overlay) = info_overlay {
                // Track this overlay's rect for subsequent infos to avoid
                if let OverlayAnchor::Absolute { x, y, w, h } = &overlay.anchor {
                    avoid_rects.push(crate::layout::Rect {
                        x: *x,
                        y: *y,
                        w: *w,
                        h: *h,
                    });
                }
                overlay.element =
                    registry.apply_decorator(DecorateTarget::Info, overlay.element, state);
                overlays.push(overlay);
            }
        }
    }

    for el in plugin_overlays {
        overlays.push(Overlay {
            element: el,
            anchor: OverlayAnchor::Absolute {
                x: 0,
                y: 0,
                w: state.cols,
                h: state.rows,
            },
        });
    }

    if overlays.is_empty() {
        base
    } else {
        Element::stack(base, overlays)
    }
}

fn build_status_bar(
    state: &AppState,
    status_left: Vec<Element>,
    status_right: Vec<Element>,
) -> Element {
    let status_line =
        build_styled_line_with_base(&state.status_line, &state.status_default_face, 0);
    let mode_line =
        build_styled_line_with_base(&state.status_mode_line, &state.status_default_face, 0);
    let mode_width = line_display_width(&state.status_mode_line) as u16;

    // Status bar: [...status_left, status_line(flex:1.0), ...status_right, mode_line(fixed)]
    let mut children = Vec::new();
    for el in status_left {
        children.push(FlexChild::fixed(el));
    }
    children.push(FlexChild::flexible(status_line, 1.0));
    for el in status_right {
        children.push(FlexChild::fixed(el));
    }
    // Cursor count badge: show when there are multiple selections
    if state.cursor_count > 1 {
        let badge_text = format!(" {} sel ", state.cursor_count);
        let badge = Element::text(badge_text, state.status_default_face);
        children.push(FlexChild::fixed(badge));
    }

    if mode_width > 0 {
        children.push(FlexChild::fixed(mode_line));
    }

    Element::Container {
        child: Box::new(Element::row(children)),
        border: None,
        shadow: false,
        padding: Edges::ZERO,
        style: Style::from(state.status_default_face),
        title: None,
    }
}

// ---------------------------------------------------------------------------
// Menu overlay construction
// ---------------------------------------------------------------------------

/// Build a menu overlay using a replacement element with the same anchor as the default.
fn build_replacement_menu_overlay(
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
            let screen_h = state.rows.saturating_sub(1);
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
            let status_row = state.rows.saturating_sub(1);
            let start_y = status_row.saturating_sub(menu.win_height);
            OverlayAnchor::Absolute {
                x: 0,
                y: start_y,
                w: state.cols,
                h: menu.win_height,
            }
        }
        MenuStyle::Search => {
            let status_row = state.rows.saturating_sub(1);
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

/// Build an info overlay using a replacement element with the same anchor as the default.
fn build_replacement_info_overlay(
    element: Element,
    info: &InfoState,
    state: &AppState,
    avoid: &[crate::layout::Rect],
) -> Option<Overlay> {
    let screen_h = state.rows.saturating_sub(1);
    let win = layout_info(
        &info.title,
        &info.content,
        &info.anchor,
        info.style,
        state.cols,
        screen_h,
        avoid,
    );

    if win.width == 0 || win.height == 0 {
        return None;
    }

    Some(Overlay {
        element,
        anchor: OverlayAnchor::Absolute {
            x: win.x,
            y: win.y,
            w: win.width,
            h: win.height,
        },
    })
}

fn build_menu_overlay(menu: &MenuState, state: &AppState) -> Option<Overlay> {
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
    match state.menu_position {
        crate::config::MenuPosition::Above => MenuPlacement::Above,
        crate::config::MenuPosition::Below => MenuPlacement::Below,
        crate::config::MenuPosition::Auto => MenuPlacement::Auto,
    }
}

fn build_menu_inline(menu: &MenuState, state: &AppState) -> Option<Overlay> {
    let win_w = (menu.max_item_width + 1).min(state.cols);
    let content_w = win_w.saturating_sub(1);
    let screen_h = state.rows.saturating_sub(1);
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

    let status_row = state.rows.saturating_sub(1);
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
    let status_row = state.rows.saturating_sub(1);
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
    let screen_h = state.rows.saturating_sub(1);
    let status_row = state.rows.saturating_sub(1);
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

/// Build a StyledLine element from a protocol Line, resolving faces against a base.
fn build_styled_line_with_base(line: &Line, base_face: &Face, _max_width: u16) -> Element {
    let resolved: Vec<Atom> = line
        .iter()
        .map(|atom| Atom {
            face: super::grid::resolve_face(&atom.face, base_face),
            contents: atom.contents.clone(),
        })
        .collect();
    Element::StyledLine(resolved)
}

// ---------------------------------------------------------------------------
// Info overlay construction
// ---------------------------------------------------------------------------

/// The clippy assistant from Kakoune's terminal UI.
const ASSISTANT_CLIPPY: &[&str] = &[
    " ╭──╮  ",
    " │  │  ",
    " @  @  ╭",
    " ││ ││ │",
    " ││ ││ ╯",
    " │╰─╯│ ",
    " ╰───╯ ",
    "        ",
];
const ASSISTANT_WIDTH: u16 = 8;

fn build_info_overlay_indexed(
    info: &InfoState,
    state: &AppState,
    avoid: &[crate::layout::Rect],
    index: usize,
) -> Option<Overlay> {
    let screen_h = state.rows.saturating_sub(1);
    let win = layout_info(
        &info.title,
        &info.content,
        &info.anchor,
        info.style,
        state.cols,
        screen_h,
        avoid,
    );

    if win.width == 0 || win.height == 0 {
        return None;
    }

    let element = match info.style {
        InfoStyle::Prompt => build_info_prompt(info, &win),
        InfoStyle::Modal => build_info_framed(info, &win, state.shadow_enabled),
        InfoStyle::Inline | InfoStyle::InlineAbove | InfoStyle::MenuDoc => {
            build_info_nonframed(info, &win)
        }
    };

    element.map(|el| {
        // Wrap with Interactive for mouse hit testing
        let interactive_id =
            crate::element::InteractiveId(crate::element::InteractiveId::INFO_BASE + index as u32);
        let wrapped = Element::Interactive {
            child: Box::new(el),
            id: interactive_id,
        };
        Overlay {
            element: wrapped,
            anchor: OverlayAnchor::Absolute {
                x: win.x,
                y: win.y,
                w: win.width,
                h: win.height,
            },
        }
    })
}

fn build_info_prompt(info: &InfoState, win: &layout::FloatingWindow) -> Option<Element> {
    if win.width < ASSISTANT_WIDTH + 5 || win.height < 3 {
        return None;
    }

    let total_h = win.height as usize;
    let cw = win.width.saturating_sub(ASSISTANT_WIDTH + 4);
    if cw == 0 {
        return None;
    }

    // Trim trailing empty content lines
    let content_end = info
        .content
        .iter()
        .rposition(|line| line_display_width(line) > 0)
        .map(|i| i + 1)
        .unwrap_or(0);
    let trimmed = &info.content[..content_end];

    // Build assistant column
    let asst_top = ((total_h as i32 - ASSISTANT_CLIPPY.len() as i32 + 1) / 2).max(0) as usize;
    let mut asst_rows: Vec<FlexChild> = Vec::new();
    for row in 0..total_h {
        let idx = if row >= asst_top {
            (row - asst_top).min(ASSISTANT_CLIPPY.len() - 1)
        } else {
            ASSISTANT_CLIPPY.len() - 1
        };
        asst_rows.push(FlexChild::fixed(Element::text(
            ASSISTANT_CLIPPY[idx],
            info.face,
        )));
    }
    let assistant_col = Element::column(asst_rows);

    // Build content lines with word wrapping
    // Frame height is determined by content, not the full popup height
    let frame_content_h = total_h.saturating_sub(2) as u16;
    let wrapped_lines = wrap_content_lines(trimmed, cw, frame_content_h, &info.face);
    let frame_h = (wrapped_lines.len() as u16 + 2).min(total_h as u16);

    // Build framed content area
    let mut content_rows: Vec<FlexChild> = Vec::new();
    for line in &wrapped_lines {
        content_rows.push(FlexChild::fixed(Element::StyledLine(line.clone())));
    }

    let content_col = Element::column(content_rows);

    // Build bordered frame around content
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

    // Use Stack: assistant fills full popup height, frame overlays at natural height
    let frame_w = win.width.saturating_sub(ASSISTANT_WIDTH);
    let base = Element::row(vec![
        FlexChild::fixed(assistant_col),
        FlexChild::flexible(Element::text("", info.face), 1.0),
    ]);
    let container = Element::stack(
        Element::Container {
            child: Box::new(base),
            border: None,
            shadow: false,
            padding: Edges::ZERO,
            style: Style::from(info.face),
            title: None,
        },
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

fn build_info_framed(
    info: &InfoState,
    win: &layout::FloatingWindow,
    shadow: bool,
) -> Option<Element> {
    let inner_w = win.width.saturating_sub(4).max(1);
    let inner_h = win.height.saturating_sub(2);

    let wrapped_lines = wrap_content_lines(&info.content, inner_w, inner_h, &info.face);

    let mut content_rows: Vec<FlexChild> = Vec::new();
    for line in &wrapped_lines {
        content_rows.push(FlexChild::fixed(Element::StyledLine(line.clone())));
    }

    let content_col = Element::column(content_rows);

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

fn build_info_nonframed(info: &InfoState, win: &layout::FloatingWindow) -> Option<Element> {
    let wrapped_lines = wrap_content_lines(&info.content, win.width, win.height, &info.face);

    let mut content_rows: Vec<FlexChild> = Vec::new();
    for line in &wrapped_lines {
        content_rows.push(FlexChild::fixed(Element::StyledLine(line.clone())));
    }

    let content_col = Element::column(content_rows);

    let container = Element::Container {
        child: Box::new(content_col),
        border: None,
        shadow: false,
        padding: Edges::ZERO,
        style: Style::from(info.face),
        title: None,
    };

    Some(container)
}

/// Word-wrap content lines and produce resolved StyledLine atoms per visual row.
fn wrap_content_lines(
    content: &[Line],
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

        // Collect graphemes with resolved faces
        let mut graphemes: Vec<(&str, Face, u16)> = Vec::new();
        for atom in line {
            let face = super::grid::resolve_face(&atom.face, base_face);
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
                contents: String::new(),
            }]);
            continue;
        }

        let metrics: Vec<(u16, bool)> = graphemes
            .iter()
            .map(|(text, _, w)| (*w, !layout::is_word_char(text)))
            .collect();
        let segments = word_wrap_segments(&metrics, max_width);

        for seg in &segments {
            if result.len() >= max_rows as usize {
                break;
            }
            let mut row_atoms = Vec::new();
            let mut current_face: Option<Face> = None;
            let mut current_text = String::new();

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
                    current_text = grapheme.to_string();
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::element::Direction;
    use crate::protocol::{Atom, Color, Coord, Face, NamedColor};
    use crate::state::AppState;

    fn make_line(s: &str) -> Line {
        vec![Atom {
            face: Face::default(),
            contents: s.to_string(),
        }]
    }

    #[test]
    fn test_view_empty_state() {
        let state = AppState::default();
        let registry = PluginRegistry::new();
        let el = view(&state, &registry);

        // Should be a Column with BufferRef + status bar
        match el {
            Element::Flex {
                direction: Direction::Column,
                children,
                ..
            } => {
                assert_eq!(children.len(), 2); // buffer + status
            }
            _ => panic!("expected Column flex"),
        }
    }

    #[test]
    fn test_view_with_menu() {
        let mut state = AppState::default();
        state.cols = 80;
        state.rows = 24;
        state.lines = vec![make_line("hello")];
        state.apply(crate::protocol::KakouneRequest::MenuShow {
            items: vec![make_line("item1"), make_line("item2")],
            anchor: Coord { line: 1, column: 0 },
            selected_item_face: Face::default(),
            menu_face: Face::default(),
            style: MenuStyle::Inline,
        });

        let registry = PluginRegistry::new();
        let el = view(&state, &registry);

        // Should be a Stack (base Column + menu overlay)
        match el {
            Element::Stack { overlays, .. } => {
                assert!(!overlays.is_empty(), "should have menu overlay");
            }
            _ => panic!("expected Stack, got {:?}", std::mem::discriminant(&el)),
        }
    }

    #[test]
    fn test_view_with_info() {
        let mut state = AppState::default();
        state.cols = 80;
        state.rows = 24;
        state.apply(crate::protocol::KakouneRequest::InfoShow {
            title: make_line("Help"),
            content: vec![make_line("some info")],
            anchor: Coord { line: 0, column: 0 },
            face: Face::default(),
            style: InfoStyle::Modal,
        });

        let registry = PluginRegistry::new();
        let el = view(&state, &registry);

        match el {
            Element::Stack { overlays, .. } => {
                assert!(!overlays.is_empty(), "should have info overlay");
            }
            _ => panic!("expected Stack"),
        }
    }

    #[test]
    fn test_status_bar_resolves_default_face() {
        let mut state = AppState::default();
        state.status_default_face = Face {
            fg: Color::Named(NamedColor::Cyan),
            bg: Color::Named(NamedColor::Magenta),
            ..Face::default()
        };
        // Atoms with Color::Default — should be resolved to status_default_face colors
        state.status_line = vec![Atom {
            face: Face::default(),
            contents: "file.rs".to_string(),
        }];
        state.status_mode_line = vec![Atom {
            face: Face::default(),
            contents: "normal".to_string(),
        }];

        let status_bar = build_status_bar(&state, vec![], vec![]);

        // Extract StyledLine atoms from the Container > Row > children
        let row = match &status_bar {
            Element::Container { child, .. } => child.as_ref(),
            other => panic!(
                "expected Container, got {:?}",
                std::mem::discriminant(other)
            ),
        };
        let children = match row {
            Element::Flex { children, .. } => children,
            other => panic!("expected Flex row, got {:?}", std::mem::discriminant(other)),
        };

        // Check status_line atoms
        match &children[0].element {
            Element::StyledLine(atoms) => {
                for atom in atoms {
                    assert_eq!(
                        atom.face.fg,
                        Color::Named(NamedColor::Cyan),
                        "status_line fg should be resolved from status_default_face"
                    );
                    assert_eq!(
                        atom.face.bg,
                        Color::Named(NamedColor::Magenta),
                        "status_line bg should be resolved from status_default_face"
                    );
                }
            }
            other => panic!(
                "expected StyledLine, got {:?}",
                std::mem::discriminant(other)
            ),
        }

        // Check mode_line atoms
        match &children[1].element {
            Element::StyledLine(atoms) => {
                for atom in atoms {
                    assert_eq!(
                        atom.face.fg,
                        Color::Named(NamedColor::Cyan),
                        "mode_line fg should be resolved from status_default_face"
                    );
                    assert_eq!(
                        atom.face.bg,
                        Color::Named(NamedColor::Magenta),
                        "mode_line bg should be resolved from status_default_face"
                    );
                }
            }
            other => panic!(
                "expected StyledLine, got {:?}",
                std::mem::discriminant(other)
            ),
        }
    }

    #[test]
    fn test_status_left_slot_in_status_bar() {
        use crate::plugin::{Plugin, PluginId};

        struct StatusLeftPlugin;
        impl Plugin for StatusLeftPlugin {
            fn id(&self) -> PluginId {
                PluginId("status_left".into())
            }
            fn contribute(&self, slot: Slot, _state: &AppState) -> Option<Element> {
                match slot {
                    Slot::StatusLeft => Some(Element::text("[L]", Face::default())),
                    _ => None,
                }
            }
        }

        let mut state = AppState::default();
        state.status_line = make_line("status");
        state.status_mode_line = make_line("normal");

        let mut registry = PluginRegistry::new();
        registry.register(Box::new(StatusLeftPlugin));

        let el = view(&state, &registry);

        // The status bar is the last child of the root column.
        // It should now have 3 children: [L], status_line (flex), mode_line.
        match &el {
            Element::Flex { children, .. } => {
                let status = &children.last().unwrap().element;
                match status {
                    Element::Container { child, .. } => match child.as_ref() {
                        Element::Flex { children, .. } => {
                            assert_eq!(
                                children.len(),
                                3,
                                "should have status_left + status_line + mode_line"
                            );
                        }
                        _ => panic!("expected Flex row"),
                    },
                    _ => panic!("expected Container"),
                }
            }
            _ => panic!("expected Column"),
        }
    }

    #[test]
    fn test_info_framed_shadow_disabled() {
        let mut state = AppState::default();
        state.cols = 80;
        state.rows = 24;
        state.shadow_enabled = false;
        state.apply(crate::protocol::KakouneRequest::InfoShow {
            title: make_line("Help"),
            content: vec![make_line("content")],
            anchor: Coord { line: 0, column: 0 },
            face: Face::default(),
            style: InfoStyle::Modal,
        });

        let registry = PluginRegistry::new();
        let el = view(&state, &registry);

        // Find the info overlay's framed Container
        fn find_shadow(el: &Element) -> Option<bool> {
            match el {
                Element::Container {
                    shadow,
                    border: Some(_),
                    ..
                } => Some(*shadow),
                Element::Stack { overlays, .. } => {
                    overlays.iter().find_map(|o| find_shadow(&o.element))
                }
                Element::Container { child, .. } => find_shadow(child),
                Element::Interactive { child, .. } => find_shadow(child),
                Element::Flex { children, .. } => {
                    children.iter().find_map(|c| find_shadow(&c.element))
                }
                _ => None,
            }
        }

        let shadow = find_shadow(&el);
        assert_eq!(
            shadow,
            Some(false),
            "shadow should be false when shadow_enabled is false"
        );
    }

    #[test]
    fn test_view_status_bar_structure() {
        let mut state = AppState::default();
        state.status_line = make_line("status");
        state.status_mode_line = make_line("normal");

        let registry = PluginRegistry::new();
        let el = view(&state, &registry);

        match el {
            Element::Flex { children, .. } => {
                // Last child should be the status bar (Container with Row)
                let status = &children.last().unwrap().element;
                match status {
                    Element::Container { child, .. } => match child.as_ref() {
                        Element::Flex {
                            direction: Direction::Row,
                            children,
                            ..
                        } => {
                            assert_eq!(children.len(), 2); // status_line + mode_line
                        }
                        _ => panic!("expected Row inside status container"),
                    },
                    _ => panic!("expected Container for status bar"),
                }
            }
            _ => panic!("expected Column"),
        }
    }
}
