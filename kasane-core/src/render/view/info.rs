use unicode_width::UnicodeWidthStr;

use crate::element::{
    BorderConfig, BorderLineStyle, Edges, Element, FlexChild, Overlay, OverlayAnchor, Style,
};
use crate::layout::{
    self, ASSISTANT_CLIPPY, ASSISTANT_WIDTH, layout_info, line_display_width, word_wrap_segments,
};
use crate::protocol::{Atom, Face, InfoStyle, Line};
use crate::state::{AppState, InfoState};

/// Build an info overlay using a replacement element with the same anchor as the default.
pub(super) fn build_replacement_info_overlay(
    element: Element,
    info: &InfoState,
    state: &AppState,
    avoid: &[crate::layout::Rect],
) -> Option<Overlay> {
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

pub(super) fn build_info_overlay_indexed(
    info: &InfoState,
    state: &AppState,
    avoid: &[crate::layout::Rect],
    index: usize,
) -> Option<Overlay> {
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
            let face = super::super::grid::resolve_face(&atom.face, base_face);
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
