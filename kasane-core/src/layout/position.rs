use super::flex;
use super::{FloatingWindow, MenuPlacement, Rect};
use crate::protocol::{Coord, MenuStyle};
use crate::state::AppState;

/// Kakoune-compatible positioning algorithm (`compute_pos` in `terminal_ui.cc`).
///
/// 1. Vertical: place below anchor+1 (or above anchor if `prefer_above`); flip if
///    the window overflows the available area.
/// 2. Horizontal: clamp so the window stays inside `rect`.
/// 3. Menu avoidance: if the result overlaps `to_avoid`, move above or below it.
pub fn compute_pos(
    anchor: (i32, i32), // (line, column)
    size: (u16, u16),   // (height, width)
    rect: Rect,         // available area
    to_avoid: &[Rect],
    prefer_above: bool,
) -> (u16, u16) {
    let (h, w) = size;
    let (anchor_line, anchor_col) = anchor;

    // --- Phase 1: vertical (Kakoune-compatible fallthrough) ---
    let rect_end_line = rect.y as i32 + rect.h as i32;
    let mut prefer_above = prefer_above;
    let mut line = 0i32;

    if prefer_above {
        line = anchor_line - h as i32;
        if line < rect.y as i32 {
            prefer_above = false; // fallthrough to below
        }
    }
    if !prefer_above {
        line = anchor_line + 1;
        if line + h as i32 >= rect_end_line {
            line = (rect.y as i32).max(anchor_line - h as i32);
        }
    }

    // --- Phase 2: horizontal clamp ---
    let mut col = anchor_col;
    let rect_right = rect.x as i32 + rect.w as i32;
    if col + w as i32 > rect_right {
        col = rect_right - w as i32;
    }
    if col < rect.x as i32 {
        col = rect.x as i32;
    }

    // --- Phase 3: obstacle avoidance ---
    // Matches Kakoune: uses min(obstacle.pos.line, anchor.line) to avoid both
    // the obstacle rectangle and the anchor line.
    for menu in to_avoid {
        if menu.h == 0 {
            continue;
        }
        let menu_top = menu.y as i32;
        let menu_bot = menu.y as i32 + menu.h as i32;
        let win_top = line;
        let win_bot = line + h as i32;
        let col_end = col + w as i32;
        let menu_right = menu.x as i32 + menu.w as i32;
        // Check intersection (both vertical and horizontal)
        if !(win_bot <= menu_top
            || col_end <= menu.x as i32
            || win_top >= menu_bot
            || col >= menu_right)
        {
            // Place above whichever is higher: obstacle or anchor
            line = menu_top.min(anchor_line) - h as i32;
            // If that goes off-screen, try below whichever is lower
            if line < rect.y as i32 {
                line = menu_bot.max(anchor_line);
            }
        }
    }

    // Clamp final result into rect
    let y = line
        .max(rect.y as i32)
        .min((rect.y as i32 + rect.h as i32).saturating_sub(h as i32)) as u16;
    let x = col
        .max(rect.x as i32)
        .min((rect.x as i32 + rect.w as i32).saturating_sub(w as i32)) as u16;
    (y, x)
}

/// Lay out a single overlay element against a root area.
/// Shared between `flex::place_stack` and the scene cache pipeline.
pub fn layout_single_overlay(
    overlay: &crate::element::Overlay,
    root_area: Rect,
    state: &AppState,
) -> flex::LayoutResult {
    let (ox, oy, ow, oh) = match &overlay.anchor {
        crate::element::OverlayAnchor::Absolute { x, y, w, h } => {
            (root_area.x + *x, root_area.y + *y, *w, *h)
        }
        crate::element::OverlayAnchor::AnchorPoint {
            coord,
            prefer_above,
            avoid,
        } => {
            let overlay_size = flex::measure(
                &overlay.element,
                flex::Constraints::loose(root_area.w, root_area.h),
                state,
            );
            let (y, x) = compute_pos(
                (coord.line, coord.column),
                (overlay_size.height, overlay_size.width),
                root_area,
                avoid,
                *prefer_above,
            );
            (x, y, overlay_size.width, overlay_size.height)
        }
    };

    let overlay_area = Rect {
        x: ox,
        y: oy,
        w: ow,
        h: oh,
    };

    flex::place(&overlay.element, overlay_area, state)
}

/// Lay out an inline menu floating window (no borders).
///
/// `win_width` and `win_height` are the content dimensions (already computed
/// by the caller from item count, scrollbar, etc.).
/// `screen_h` should exclude the status bar row.
pub fn layout_menu_inline(
    anchor: &Coord,
    win_width: u16,
    win_height: u16,
    screen_w: u16,
    screen_h: u16,
    placement: MenuPlacement,
) -> FloatingWindow {
    if win_width == 0 || win_height == 0 {
        return FloatingWindow {
            x: 0,
            y: 0,
            width: 0,
            height: 0,
        };
    }

    let win_w = win_width.min(screen_w);
    let win_h = win_height.min(screen_h);

    let ax = (anchor.column as u16).min(screen_w.saturating_sub(win_w));
    let ay = anchor.line as u16 + 1; // below the anchor

    let (y, height) = match placement {
        MenuPlacement::Above => {
            if (anchor.line as u16) >= win_h {
                (anchor.line as u16 - win_h, win_h)
            } else if ay + win_h <= screen_h {
                // Can't fit above, fall back to below
                (ay, win_h)
            } else {
                // Best effort above
                (0, (anchor.line as u16).max(1))
            }
        }
        MenuPlacement::Below => {
            let avail = screen_h.saturating_sub(ay);
            (ay, win_h.min(avail).max(1))
        }
        MenuPlacement::Auto => {
            if ay + win_h <= screen_h {
                (ay, win_h)
            } else if (anchor.line as u16) >= win_h {
                // Flip above
                (anchor.line as u16 - win_h, win_h)
            } else {
                // Best effort
                let avail = screen_h.saturating_sub(ay);
                (ay, avail.max(1))
            }
        }
    };

    FloatingWindow {
        x: ax,
        y,
        width: win_w,
        height,
    }
}

/// Compute the screen rectangle of the active menu, for use in info popup placement.
/// Returns `None` when there is no active menu (or it has zero size).
pub fn get_menu_rect(state: &AppState) -> Option<Rect> {
    let menu = state.menu.as_ref()?;
    if menu.items.is_empty() || menu.win_height == 0 {
        return None;
    }

    match menu.style {
        MenuStyle::Prompt => {
            let status_row = state.available_height();
            let start_y = status_row.saturating_sub(menu.win_height);
            Some(Rect {
                x: 0,
                y: start_y,
                w: state.cols,
                h: menu.win_height,
            })
        }
        MenuStyle::Search => {
            let status_row = state.available_height();
            Some(Rect {
                x: 0,
                y: status_row.saturating_sub(1),
                w: state.cols,
                h: 1,
            })
        }
        MenuStyle::Inline => {
            let screen_h = state.available_height();
            // +1 for scrollbar
            let win_w = (menu.max_item_width + 1).min(state.cols);
            let placement = MenuPlacement::from(state.menu_position);
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
            Some(Rect {
                x: win.x,
                y: win.y,
                w: win.width,
                h: win.height,
            })
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn screen_rect(w: u16, h: u16) -> Rect {
        Rect { x: 0, y: 0, w, h }
    }

    #[test]
    fn test_compute_pos_below() {
        let (y, x) = compute_pos((5, 2), (3, 10), screen_rect(80, 24), &[], false);
        assert_eq!(y, 6);
        assert_eq!(x, 2);
    }

    #[test]
    fn test_compute_pos_above() {
        let (y, x) = compute_pos((10, 5), (3, 10), screen_rect(80, 24), &[], true);
        assert_eq!(y, 7);
        assert_eq!(x, 5);
    }

    #[test]
    fn test_compute_pos_flip_below_to_above() {
        let (y, _x) = compute_pos((22, 0), (5, 10), screen_rect(80, 24), &[], false);
        assert_eq!(y, 17);
    }

    #[test]
    fn test_compute_pos_flip_above_to_below() {
        let (y, _x) = compute_pos((1, 0), (5, 10), screen_rect(80, 24), &[], true);
        assert_eq!(y, 2);
    }

    #[test]
    fn test_compute_pos_horizontal_clamp() {
        let (y, x) = compute_pos((5, 75), (3, 10), screen_rect(80, 24), &[], false);
        assert_eq!(y, 6);
        assert_eq!(x, 70);
    }

    #[test]
    fn test_compute_pos_menu_avoidance() {
        let menu = Rect {
            x: 0,
            y: 6,
            w: 20,
            h: 4,
        };
        let (y, _x) = compute_pos((5, 0), (3, 10), screen_rect(80, 24), &[menu], false);
        assert_eq!(y, 2);
    }

    #[test]
    fn test_compute_pos_menu_avoidance_above() {
        let menu = Rect {
            x: 0,
            y: 10,
            w: 20,
            h: 4,
        };
        let (y, _x) = compute_pos((10, 0), (3, 10), screen_rect(80, 24), &[menu], false);
        assert_eq!(y, 7);
    }

    #[test]
    fn test_compute_pos_multiple_avoid() {
        let menu = Rect {
            x: 0,
            y: 6,
            w: 20,
            h: 4,
        };
        let cursor = Rect {
            x: 5,
            y: 5,
            w: 1,
            h: 1,
        };
        let (y, _x) = compute_pos((5, 0), (3, 10), screen_rect(80, 24), &[menu, cursor], false);
        // Should avoid both menu (6..10) and cursor (5..6)
        assert!(y + 3 <= 5 || y >= 10, "should not overlap menu or cursor");
    }

    #[test]
    fn test_layout_menu_inline() {
        let anchor = Coord { line: 2, column: 5 };
        let win = layout_menu_inline(&anchor, 12, 2, 80, 24, MenuPlacement::Auto);
        assert!(win.y > 2);
        assert_eq!(win.width, 12);
        assert_eq!(win.height, 2);
    }

    #[test]
    fn test_layout_menu_inline_above() {
        let anchor = Coord {
            line: 10,
            column: 5,
        };
        let win = layout_menu_inline(&anchor, 12, 3, 80, 24, MenuPlacement::Above);
        assert_eq!(win.y, 7); // 10 - 3
        assert_eq!(win.height, 3);
    }

    #[test]
    fn test_layout_menu_inline_below() {
        let anchor = Coord {
            line: 20,
            column: 0,
        };
        let win = layout_menu_inline(&anchor, 12, 5, 80, 24, MenuPlacement::Below);
        assert_eq!(win.y, 21); // forced below even though space is tight
        assert!(win.height <= 3); // only 3 rows available (24-21)
    }
}
