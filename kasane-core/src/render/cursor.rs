use super::CursorStyle;
use super::grid::CellGrid;
use crate::element::Element;
use crate::layout::flex::LayoutResult;
use crate::protocol::CursorMode;
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
    let cx = state.cursor_pos.column as u16;
    let cy = match state.cursor_mode {
        CursorMode::Prompt => grid.height().saturating_sub(1),
        CursorMode::Buffer => state.cursor_pos.line as u16,
    };
    let cx = match state.cursor_mode {
        CursorMode::Buffer => cx + buffer_x_offset,
        CursorMode::Prompt => cx,
    };
    (cx, cy)
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
    let cx = state.cursor_pos.column as u16;
    let cy = match state.cursor_mode {
        CursorMode::Prompt => grid.height().saturating_sub(1),
        CursorMode::Buffer => state.cursor_pos.line as u16,
    };
    let cx = match state.cursor_mode {
        CursorMode::Buffer => cx + buffer_x_offset,
        CursorMode::Prompt => cx,
    };
    let base_face = match state.cursor_mode {
        CursorMode::Buffer => &state.default_face,
        CursorMode::Prompt => &state.status_default_face,
    };
    if let Some(cell) = grid.get_mut(cx, cy) {
        cell.face = *base_face;
    }
}
