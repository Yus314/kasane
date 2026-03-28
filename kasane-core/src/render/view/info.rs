use crate::element::{
    BorderConfig, BorderLineStyle, Edges, Element, FlexChild, Overlay, OverlayAnchor, Style,
    StyleToken,
};
use crate::layout::{self, ASSISTANT_CLIPPY, ASSISTANT_WIDTH, layout_info, line_display_width};
use crate::protocol::{Face, InfoStyle};
use crate::render::builders::{build_content_column, wrap_content_lines};
use crate::state::{AppState, InfoState};

/// Resolve info face: theme override takes precedence, protocol face as fallback.
fn resolve_info_face(info: &InfoState, state: &AppState) -> Face {
    state
        .theme
        .resolve_with_protocol_fallback(&StyleToken::INFO_TEXT, info.face)
}

/// Compute the floating window for an info popup, returning `None` if zero-size.
fn compute_info_window(
    info: &InfoState,
    state: &AppState,
    avoid: &[crate::layout::Rect],
) -> Option<layout::FloatingWindow> {
    let screen_h = state.available_height();
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

    Some(win)
}

#[crate::kasane_component]
pub(crate) fn build_info_overlay_indexed(
    info: &InfoState,
    state: &AppState,
    avoid: &[crate::layout::Rect],
    index: usize,
) -> Option<Overlay> {
    let win = compute_info_window(info, state, avoid)?;

    let element = match info.style {
        InfoStyle::Prompt => build_info_prompt(info, &win, state),
        InfoStyle::Modal => build_info_framed(info, &win, state.shadow_enabled, state),
        InfoStyle::Inline | InfoStyle::InlineAbove | InfoStyle::MenuDoc => {
            build_info_nonframed(info, &win, state)
        }
    };

    element.map(|el| {
        // Wrap with Interactive for mouse hit testing
        let interactive_id = crate::element::InteractiveId::framework(
            crate::element::InteractiveId::INFO_BASE + index as u32,
        );
        let wrapped = Element::Interactive {
            child: Box::new(el),
            id: interactive_id,
        };
        Overlay {
            element: wrapped,
            anchor: win.into(),
        }
    })
}

fn build_info_prompt(
    info: &InfoState,
    win: &layout::FloatingWindow,
    state: &AppState,
) -> Option<Element> {
    if win.width < ASSISTANT_WIDTH + 5 || win.height < 3 {
        return None;
    }

    let face = resolve_info_face(info, state);

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

    // Build assistant column (use custom art if configured)
    let art_len = state
        .assistant_art
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
        let line_str: &str = match &state.assistant_art {
            Some(custom) => &custom[idx],
            None => ASSISTANT_CLIPPY[idx],
        };
        asst_rows.push(FlexChild::fixed(Element::text(line_str, face)));
    }
    let assistant_col = Element::column(asst_rows);

    // Build content lines with word wrapping
    // Frame height is determined by content, not the full popup height
    let frame_content_h = total_h.saturating_sub(2) as u16;
    let wrapped_lines = wrap_content_lines(trimmed, cw, frame_content_h, &face);
    let frame_h = (wrapped_lines.len() as u16 + 2).min(total_h as u16);

    // Build framed content area
    let content_rows: Vec<FlexChild> = wrapped_lines
        .iter()
        .map(|line| FlexChild::fixed(Element::StyledLine(line.clone())))
        .collect();
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
        style: Style::from(face),
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
        FlexChild::flexible(Element::text("", face), 1.0),
    ]);
    let container = Element::stack(
        Element::container(base, Style::from(face)),
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
    state: &AppState,
) -> Option<Element> {
    let face = resolve_info_face(info, state);
    let inner_w = win.width.saturating_sub(4).max(1);
    let inner_h = win.height.saturating_sub(2);

    let content_col = build_content_column(&info.content, inner_w, inner_h, &face);

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
        style: Style::from(face),
        title: if info.title.is_empty() {
            None
        } else {
            Some(info.title.clone())
        },
    };

    Some(framed)
}

fn build_info_nonframed(
    info: &InfoState,
    win: &layout::FloatingWindow,
    state: &AppState,
) -> Option<Element> {
    let face = resolve_info_face(info, state);
    let content_col = build_content_column(&info.content, win.width, win.height, &face);

    Some(Element::container(content_col, Style::from(face)))
}
