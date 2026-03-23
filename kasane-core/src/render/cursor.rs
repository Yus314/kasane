use super::grid::CellGrid;
use super::{CursorStyle, CursorStyleHint};
use crate::display::DisplayMap;
use crate::element::Element;
use crate::layout::flex::LayoutResult;
use crate::layout::line_display_width;
use crate::plugin::AppView;
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
        Element::ResolvedSlot { children, .. } => {
            for (i, child) in children.iter().enumerate() {
                if let Some(child_layout) = layout.children.get(i) {
                    let x = find_buffer_x_offset(&child.element, child_layout);
                    if x > 0 {
                        return x;
                    }
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
        Element::SlotPlaceholder { .. } => {
            debug_assert!(false, "unresolved SlotPlaceholder reached cursor lookup");
            0
        }
        _ => 0,
    }
}

/// Find the absolute (x, y) origin of the BufferRef element within a given focus rectangle.
/// Used in multi-pane layout to locate the buffer origin in the focused pane.
pub fn find_buffer_origin_in_rect(
    element: &Element,
    layout: &LayoutResult,
    focus_rect: &crate::layout::Rect,
) -> Option<(u16, u16)> {
    match element {
        Element::BufferRef { .. } => {
            let a = &layout.area;
            if a.x >= focus_rect.x
                && a.y >= focus_rect.y
                && a.x < focus_rect.x + focus_rect.w
                && a.y < focus_rect.y + focus_rect.h
            {
                Some((a.x, a.y))
            } else {
                None
            }
        }
        Element::Flex { children, .. } | Element::ResolvedSlot { children, .. } => {
            for (i, child) in children.iter().enumerate() {
                if let Some(child_layout) = layout.children.get(i)
                    && let Some(pos) =
                        find_buffer_origin_in_rect(&child.element, child_layout, focus_rect)
                {
                    return Some(pos);
                }
            }
            None
        }
        Element::Container { child, .. } | Element::Interactive { child, .. } => layout
            .children
            .first()
            .and_then(|cl| find_buffer_origin_in_rect(child, cl, focus_rect)),
        Element::Stack { base, .. } => layout
            .children
            .first()
            .and_then(|cl| find_buffer_origin_in_rect(base, cl, focus_rect)),
        _ => None,
    }
}

/// Collect all BufferRef element origins `(x, y)` in the layout tree.
fn collect_buffer_origins(element: &Element, layout: &LayoutResult, origins: &mut Vec<(u16, u16)>) {
    match element {
        Element::BufferRef { .. } => {
            origins.push((layout.area.x, layout.area.y));
        }
        Element::Flex { children, .. } | Element::ResolvedSlot { children, .. } => {
            for (i, child) in children.iter().enumerate() {
                if let Some(child_layout) = layout.children.get(i) {
                    collect_buffer_origins(&child.element, child_layout, origins);
                }
            }
        }
        Element::Container { child, .. } | Element::Interactive { child, .. } => {
            if let Some(cl) = layout.children.first() {
                collect_buffer_origins(child, cl, origins);
            }
        }
        Element::Stack { base, .. } => {
            if let Some(cl) = layout.children.first() {
                collect_buffer_origins(base, cl, origins);
            }
        }
        _ => {}
    }
}

/// Reset cursor highlighting in BufferRef areas outside the focused pane.
///
/// Both panes paint from the same `state.lines` which has cursor faces baked in
/// by Kakoune. This function finds BufferRef origins that fall outside the
/// focused pane rectangle and resets the cursor cell faces to `default_face`.
pub fn neutralize_unfocused_cursors(
    state: &AppState,
    element: &Element,
    layout: &LayoutResult,
    grid: &mut CellGrid,
    focus_rect: &crate::layout::Rect,
    display_map: Option<&DisplayMap>,
    display_scroll_offset: u16,
) {
    let mut origins = Vec::new();
    collect_buffer_origins(element, layout, &mut origins);

    for (ox, oy) in origins {
        // Skip origins inside the focused pane
        if ox >= focus_rect.x
            && oy >= focus_rect.y
            && ox < focus_rect.x + focus_rect.w
            && oy < focus_rect.y + focus_rect.h
        {
            continue;
        }

        // Primary cursor
        let cx = state.cursor_pos.column as u16 + ox;
        let cy = display_map
            .and_then(|dm| {
                if dm.is_identity() {
                    None
                } else {
                    dm.buffer_to_display(state.cursor_pos.line as usize)
                        .map(|y| y as u16)
                }
            })
            .unwrap_or(state.cursor_pos.line as u16)
            .saturating_sub(display_scroll_offset)
            + oy;
        if let Some(cell) = grid.get_mut(cx, cy) {
            cell.face = state.default_face;
        }

        // Secondary cursors
        for coord in &state.secondary_cursors {
            let sx = coord.column as u16 + ox;
            let sy = display_map
                .and_then(|dm| {
                    if dm.is_identity() {
                        None
                    } else {
                        dm.buffer_to_display(coord.line as usize).map(|y| y as u16)
                    }
                })
                .unwrap_or(coord.line as u16)
                .saturating_sub(display_scroll_offset)
                + oy;
            if let Some(cell) = grid.get_mut(sx, sy) {
                cell.face = state.default_face;
            }
        }
    }
}

/// Compute the terminal cursor position from the application state.
/// `buffer_x_offset` accounts for left gutter columns.
/// `display_map` transforms buffer line to display line when active.
/// Returns (x, y) coordinates for the terminal cursor.
pub fn cursor_position(
    state: &AppState,
    grid: &CellGrid,
    buffer_x_offset: u16,
    display_map: Option<&DisplayMap>,
    buffer_y_offset: u16,
    display_scroll_offset: u16,
) -> (u16, u16) {
    match state.cursor_mode {
        CursorMode::Buffer => {
            let cx = state.cursor_pos.column as u16 + buffer_x_offset;
            let cy = display_map
                .and_then(|dm| {
                    if dm.is_identity() {
                        None
                    } else {
                        dm.buffer_to_display(state.cursor_pos.line as usize)
                            .map(|y| y as u16)
                    }
                })
                .unwrap_or(state.cursor_pos.line as u16)
                .saturating_sub(display_scroll_offset)
                + buffer_y_offset;
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

/// Determine the cursor style hint (shape + blink + movement) from the application state.
///
/// Priority: plugin override > ui_option `kasane_cursor_style` > prompt mode > mode_line heuristic > Block.
pub fn cursor_style_hint(
    state: &AppState,
    registry: &crate::plugin::PluginView<'_>,
) -> CursorStyleHint {
    if let Some(hint) = registry.cursor_style_override(&AppView::new(state)) {
        return hint;
    }
    cursor_style_default(state).into()
}

/// Determine the cursor style (shape only) from the application state.
///
/// Convenience wrapper that extracts just the shape from `cursor_style_hint`.
pub fn cursor_style(state: &AppState, registry: &crate::plugin::PluginView<'_>) -> CursorStyle {
    cursor_style_hint(state, registry).shape
}

/// Default cursor style logic without plugin overrides.
pub fn cursor_style_default(state: &AppState) -> CursorStyle {
    crate::state::derived::derive_cursor_style(
        &state.ui_options,
        state.focused,
        state.cursor_mode,
        &state.status_mode_line,
    )
}

/// In non-block cursor modes (insert/replace), clear the PrimaryCursor face
/// highlight from the cursor cell so the terminal cursor shape is visible.
pub fn clear_block_cursor_face(
    state: &AppState,
    grid: &mut CellGrid,
    style: CursorStyle,
    buffer_x_offset: u16,
    display_map: Option<&DisplayMap>,
    buffer_y_offset: u16,
    display_scroll_offset: u16,
) {
    if style == CursorStyle::Block || style == CursorStyle::Outline {
        return;
    }
    let (cx, cy) = match state.cursor_mode {
        CursorMode::Buffer => {
            let cx = state.cursor_pos.column as u16 + buffer_x_offset;
            let cy = display_map
                .and_then(|dm| {
                    if dm.is_identity() {
                        None
                    } else {
                        dm.buffer_to_display(state.cursor_pos.line as usize)
                            .map(|y| y as u16)
                    }
                })
                .unwrap_or(state.cursor_pos.line as u16)
                .saturating_sub(display_scroll_offset)
                + buffer_y_offset;
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

/// Resolve a color to RGB, falling back to a default RGB when the color is `Default`.
fn color_to_rgb(color: Color, fallback: (u8, u8, u8)) -> (u8, u8, u8) {
    color.to_rgb().unwrap_or(fallback)
}

/// Generate a face for secondary cursors.
///
/// # Inference Rule: I-6
/// **Assumption**: The cursor face uses `REVERSE` attribute for visual highlighting.
/// Under REVERSE, `face.fg` is displayed as background (the cursor color) and
/// `face.bg` is displayed as foreground (the text color).
/// **Failure mode**: If a theme sets cursor faces without REVERSE, the blending
/// logic produces incorrect colors (fg/bg swap is wrong).
/// **Severity**: Cosmetic (secondary cursors have wrong colors)
///
/// Removes REVERSE from the cursor face and sets bg to a blended color
/// using the given blend ratio (default 0.4 = 40% cursor color + 60% background)
/// to visually differentiate from primary.
pub fn make_secondary_cursor_face(
    cursor_face: &Face,
    default_face: &Face,
    blend_ratio: f32,
) -> Face {
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

    // Guard: handle non-REVERSE cursor faces (rare, but possible with custom themes)
    if !cursor_face.attributes.contains(Attributes::REVERSE) {
        let default_bg_rgb = color_to_rgb(default_face.bg, (0, 0, 0));
        let cursor_bg_rgb = color_to_rgb(cursor_face.bg, default_bg_rgb);
        return Face {
            fg: cursor_face.fg,
            bg: super::color_context::linear_blend(
                cursor_bg_rgb,
                default_bg_rgb,
                1.0 - blend_ratio,
            ),
            underline: cursor_face.underline,
            attributes: cursor_face.attributes,
        };
    }

    let default_fg_rgb = color_to_rgb(default_face.fg, (255, 255, 255));
    let default_bg_rgb = color_to_rgb(default_face.bg, (0, 0, 0));

    // cursor_face.fg is the color that was displayed as background under REVERSE
    // (i.e., the cursor highlight color)
    let cursor_color_rgb = color_to_rgb(cursor_face.fg, default_fg_rgb);
    let bg_rgb = color_to_rgb(cursor_face.bg, default_bg_rgb);

    Face {
        fg: cursor_face.bg, // text color (was displayed as fg under REVERSE)
        bg: super::color_context::linear_blend(cursor_color_rgb, bg_rgb, blend_ratio),
        underline: cursor_face.underline,
        attributes: cursor_face.attributes & !(Attributes::REVERSE),
    }
}

/// Apply secondary cursor face differentiation to the grid.
/// Rewrites face at each secondary cursor position.
pub fn apply_secondary_cursor_faces(
    state: &AppState,
    grid: &mut CellGrid,
    buffer_x_offset: u16,
    display_map: Option<&DisplayMap>,
    buffer_y_offset: u16,
    display_scroll_offset: u16,
) {
    for coord in &state.secondary_cursors {
        let x = coord.column as u16 + buffer_x_offset;
        let y = display_map
            .and_then(|dm| {
                if dm.is_identity() {
                    None
                } else {
                    dm.buffer_to_display(coord.line as usize).map(|y| y as u16)
                }
            })
            .unwrap_or(coord.line as u16)
            .saturating_sub(display_scroll_offset)
            + buffer_y_offset;
        if let Some(cell) = grid.get_mut(x, y) {
            cell.face = make_secondary_cursor_face(
                &cell.face,
                &state.default_face,
                state.secondary_blend_ratio,
            );
        }
    }
}
