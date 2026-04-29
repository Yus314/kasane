use crate::element::{
    BorderConfig, BorderLineStyle, Edges, Element, ElementStyle, FlexChild, Overlay, OverlayAnchor,
};
use crate::layout::{self, ASSISTANT_CLIPPY, ASSISTANT_WIDTH, MenuPlacement, layout_info};
use crate::protocol::{InfoStyle, MenuStyle};
use crate::render::builders::{build_content_column, wrap_content_lines};
use crate::salsa_db::KasaneDb;
use crate::salsa_inputs::*;
use crate::salsa_queries;
use crate::state::snapshot::{InfoSnapshot, MenuSnapshot};

/// Pure info overlay elements (no plugin transforms).
///
/// Returns `(InfoStyle, Overlay)` pairs so callers can select the correct
/// transform target refinement (e.g. `InfoPrompt` vs `InfoModal`).
#[salsa::tracked(no_eq)]
pub fn pure_info_overlays(
    db: &dyn KasaneDb,
    info_input: InfoInput,
    menu_input: MenuInput,
    buffer: BufferInput,
    config: ConfigInput,
) -> Vec<(InfoStyle, Overlay)> {
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
            let interactive_id = crate::element::InteractiveId::framework(
                crate::element::InteractiveId::INFO_BASE + info_idx as u32,
            );
            o.element = Element::Interactive {
                child: Box::new(o.element),
                id: interactive_id,
            };
            overlays.push((info.style, o));
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
            let win = crate::layout::layout_menu_inline(
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
        .rposition(|line| layout::line_display_width(line) > 0)
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
        asst_rows.push(FlexChild::fixed(Element::text_with_style(
            line_str,
            info.face.clone(),
        )));
    }
    let assistant_col = Element::column(asst_rows);

    let frame_content_h = total_h.saturating_sub(2) as u16;
    let wrapped_lines = wrap_content_lines(trimmed, cw, frame_content_h, &info.face);
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
        style: ElementStyle::from(info.face.clone()),
        title: if info.title.is_empty() {
            None
        } else {
            Some(info.title.clone())
        },
    };

    let frame_w = win.width.saturating_sub(ASSISTANT_WIDTH);
    let base = Element::row(vec![
        FlexChild::fixed(assistant_col),
        FlexChild::flexible(Element::text_with_style("", info.face.clone()), 1.0),
    ]);
    let container = Element::stack(
        Element::container(base, ElementStyle::from(info.face.clone())),
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

    let content_col = build_content_column(&info.content, inner_w, inner_h, &info.face);

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
        style: ElementStyle::from(info.face.clone()),
        title: if info.title.is_empty() {
            None
        } else {
            Some(info.title.clone())
        },
    };

    Some(framed)
}

fn build_info_nonframed_pure(info: &InfoSnapshot, win: &layout::FloatingWindow) -> Option<Element> {
    let content_col = build_content_column(&info.content, win.width, win.height, &info.face);
    Some(Element::container(
        content_col,
        ElementStyle::from(info.face.clone()),
    ))
}
