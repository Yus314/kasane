use super::CursorStyle;
use super::grid::CellGrid;
use crate::element::Element;
use crate::layout::flex::LayoutResult;
use crate::layout::line_display_width;
use crate::protocol::{Attributes, Color, CursorMode, Face};
use crate::state::AppState;

/// Find the x offset of the BufferRef element in the layout tree.
/// Returns 0 when no gutter is present (the common case).
pub fn find_buffer_x_offset(element: &Element, layout: &LayoutResult) -> u16 {
    match element {
        Element::BufferRef { .. } => layout.area.x,
        Element::Flex { children, .. } => {
            for (i, child) in children.iter().enumerate() {
                if let Some(child_layout) = layout.children.get(i) {
                    let x = find_buffer_x_offset(&child.element, child_layout);
                    if x > 0 {
                        return x;
                    }
                    // Also check if this child IS the BufferRef (x might be 0 when no gutter)
                    if matches!(child.element, Element::BufferRef { .. }) {
                        return child_layout.area.x;
                    }
                }
            }
            0
        }
        Element::Container { child, .. } | Element::Interactive { child, .. } => layout
            .children
            .first()
            .map(|cl| find_buffer_x_offset(child, cl))
            .unwrap_or(0),
        Element::Stack { base, .. } => layout
            .children
            .first()
            .map(|cl| find_buffer_x_offset(base, cl))
            .unwrap_or(0),
        _ => 0,
    }
}

/// Compute the terminal cursor position from the application state.
/// `buffer_x_offset` accounts for left gutter columns.
/// Returns (x, y) coordinates for the terminal cursor.
pub fn cursor_position(state: &AppState, grid: &CellGrid, buffer_x_offset: u16) -> (u16, u16) {
    match state.cursor_mode {
        CursorMode::Buffer => {
            let cx = state.cursor_pos.column as u16 + buffer_x_offset;
            let cy = state.cursor_pos.line as u16;
            (cx, cy)
        }
        CursorMode::Prompt => {
            let prompt_width = line_display_width(&state.status_prompt) as u16;
            let cx = prompt_width + (state.status_content_cursor_pos.max(0) as u16);
            let cy = if state.status_at_top {
                0
            } else {
                grid.height().saturating_sub(1)
            };
            (cx, cy)
        }
    }
}

/// Determine the cursor style from the application state.
///
/// Priority: ui_option `kasane_cursor_style` > prompt mode > mode_line heuristic > Block.
pub fn cursor_style(state: &AppState) -> CursorStyle {
    if let Some(style) = state.ui_options.get("kasane_cursor_style") {
        return match style.as_str() {
            "bar" => CursorStyle::Bar,
            "underline" => CursorStyle::Underline,
            _ => CursorStyle::Block,
        };
    }
    if !state.focused {
        return CursorStyle::Outline;
    }
    if state.cursor_mode == CursorMode::Prompt {
        return CursorStyle::Bar;
    }
    let mode = state
        .status_mode_line
        .iter()
        .find_map(|atom| match atom.contents.as_str() {
            "insert" => Some(CursorStyle::Bar),
            "replace" => Some(CursorStyle::Underline),
            _ => None,
        });
    mode.unwrap_or(CursorStyle::Block)
}

/// In non-block cursor modes (insert/replace), clear the PrimaryCursor face
/// highlight from the cursor cell so the terminal cursor shape is visible.
pub fn clear_block_cursor_face(
    state: &AppState,
    grid: &mut CellGrid,
    style: CursorStyle,
    buffer_x_offset: u16,
) {
    if style == CursorStyle::Block || style == CursorStyle::Outline {
        return;
    }
    let (cx, cy) = match state.cursor_mode {
        CursorMode::Buffer => {
            let cx = state.cursor_pos.column as u16 + buffer_x_offset;
            let cy = state.cursor_pos.line as u16;
            (cx, cy)
        }
        CursorMode::Prompt => {
            let prompt_width = line_display_width(&state.status_prompt) as u16;
            let cx = prompt_width + (state.status_content_cursor_pos.max(0) as u16);
            let cy = if state.status_at_top {
                0
            } else {
                grid.height().saturating_sub(1)
            };
            (cx, cy)
        }
    };
    let base_face = match state.cursor_mode {
        CursorMode::Buffer => &state.default_face,
        CursorMode::Prompt => &state.status_default_face,
    };
    if let Some(cell) = grid.get_mut(cx, cy) {
        cell.face = *base_face;
    }
}

/// Blend ratio for secondary cursor background: 40% cursor color, 60% background.
const SECONDARY_BLEND_RATIO: f32 = 0.4;

/// Resolve a color to RGB, falling back to a default RGB when the color is `Default`.
fn color_to_rgb(color: Color, fallback: (u8, u8, u8)) -> (u8, u8, u8) {
    color.to_rgb().unwrap_or(fallback)
}

/// Linearly blend two RGB colors: result = a * ratio + b * (1 - ratio).
fn blend_rgb(a: (u8, u8, u8), b: (u8, u8, u8), ratio: f32) -> (u8, u8, u8) {
    let blend =
        |a: u8, b: u8| -> u8 { (a as f32 * ratio + b as f32 * (1.0 - ratio)).round() as u8 };
    (blend(a.0, b.0), blend(a.1, b.1), blend(a.2, b.2))
}

/// Generate a face for secondary cursors.
///
/// Removes REVERSE from the cursor face and sets bg to a blended color
/// (40% cursor color + 60% background) to visually differentiate from primary.
pub fn make_secondary_cursor_face(cursor_face: &Face, default_face: &Face) -> Face {
    // The cursor face has REVERSE set, so fg and bg are swapped visually.
    // The "cursor color" is the visual foreground (which is face.bg when REVERSE is on,
    // but in Kakoune's FINAL_FG+REVERSE scheme the visual highlight comes from
    // the face as-is after reversal). We want to show:
    //   fg = original fg (text color)
    //   bg = blend of cursor color and background
    //
    // With REVERSE, the terminal shows: visual_fg=face.bg, visual_bg=face.fg
    // So the "cursor color" that makes the cell stand out is face.fg (displayed as bg).
    // When we remove REVERSE:
    //   fg should be face.fg (the original text, which was shown as bg under REVERSE)
    //   bg should be a dimmed version of the cursor highlight

    let default_fg_rgb = color_to_rgb(default_face.fg, (255, 255, 255));
    let default_bg_rgb = color_to_rgb(default_face.bg, (0, 0, 0));

    // cursor_face.fg is the color that was displayed as background under REVERSE
    // (i.e., the cursor highlight color)
    let cursor_color_rgb = color_to_rgb(cursor_face.fg, default_fg_rgb);
    let bg_rgb = color_to_rgb(cursor_face.bg, default_bg_rgb);

    let blended = blend_rgb(cursor_color_rgb, bg_rgb, SECONDARY_BLEND_RATIO);

    Face {
        fg: cursor_face.bg, // text color (was displayed as fg under REVERSE)
        bg: Color::Rgb {
            r: blended.0,
            g: blended.1,
            b: blended.2,
        },
        underline: cursor_face.underline,
        attributes: cursor_face.attributes & !(Attributes::REVERSE),
    }
}

/// Apply secondary cursor face differentiation to the grid.
/// Rewrites face at each secondary cursor position.
pub fn apply_secondary_cursor_faces(state: &AppState, grid: &mut CellGrid, buffer_x_offset: u16) {
    for coord in &state.secondary_cursors {
        let x = coord.column as u16 + buffer_x_offset;
        let y = coord.line as u16;
        if let Some(cell) = grid.get_mut(x, y) {
            cell.face = make_secondary_cursor_face(&cell.face, &state.default_face);
        }
    }
}
