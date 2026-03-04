use unicode_width::UnicodeWidthStr;

use crate::protocol::{Coord, InfoStyle, Line, MenuStyle};

#[derive(Debug, Clone)]
pub struct FloatingWindow {
    pub x: u16,
    pub y: u16,
    pub width: u16,
    pub height: u16,
}

/// A rectangle on screen (used for obstacle avoidance).
#[derive(Debug, Clone, Copy)]
pub struct Rect {
    pub x: u16,
    pub y: u16,
    pub w: u16,
    pub h: u16,
}

/// Kakoune-compatible word character test (`is_word` in `unicode.hh`).
///
/// A character is a "word" character if it is alphanumeric (ASCII or Unicode)
/// or underscore. Non-word characters serve as word boundaries for line wrapping.
pub fn is_word_char(grapheme: &str) -> bool {
    // Match Kakoune: alphanumeric or underscore (extra_word_chars default)
    grapheme.chars().next().is_some_and(|c| {
        if c.is_ascii() {
            c.is_ascii_alphanumeric() || c == '_'
        } else {
            c.is_alphanumeric()
        }
    })
}

/// Width of the Kakoune clippy assistant drawn beside prompt info.
pub const PROMPT_ASSISTANT_WIDTH: u16 = 8;
/// Minimum height for prompt info (to fit the assistant).
pub const PROMPT_ASSISTANT_MIN_HEIGHT: u16 = 7;

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
    to_avoid: Option<Rect>,
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

    // --- Phase 3: menu avoidance ---
    // Matches Kakoune: uses min(to_avoid.pos.line, anchor.line) to avoid both
    // the menu rectangle and the anchor line.
    if let Some(menu) = to_avoid
        && menu.h > 0
    {
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
            // Place above whichever is higher: menu or anchor
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

/// Lay out a menu floating window.
/// `screen_h` should exclude the status bar row.
pub fn layout_menu(
    anchor: &Coord,
    items: &[Line],
    style: MenuStyle,
    screen_w: u16,
    screen_h: u16,
) -> FloatingWindow {
    let item_count = items.len().min(screen_h.saturating_sub(2) as usize);
    if item_count == 0 {
        return FloatingWindow {
            x: 0,
            y: 0,
            width: 0,
            height: 0,
        };
    }

    let max_item_width = items
        .iter()
        .take(item_count)
        .map(line_display_width)
        .max()
        .unwrap_or(0) as u16;

    let content_w = max_item_width.max(1);
    let win_w = (content_w + 2).min(screen_w); // +2 for borders
    let win_h = (item_count as u16 + 2).min(screen_h); // +2 for borders

    match style {
        MenuStyle::Prompt | MenuStyle::Search => {
            // Show above the status bar
            let y = screen_h.saturating_sub(win_h);
            let x = 0u16;
            FloatingWindow {
                x,
                y,
                width: win_w,
                height: win_h,
            }
        }
        MenuStyle::Inline => {
            // Anchor-relative
            let ax = (anchor.column as u16).min(screen_w.saturating_sub(win_w));
            let ay = anchor.line as u16 + 1; // below the anchor

            let (y, height) = if ay + win_h <= screen_h {
                (ay, win_h)
            } else if (anchor.line as u16) >= win_h {
                // Flip above
                (anchor.line as u16 - win_h, win_h)
            } else {
                // Best effort
                let avail = screen_h.saturating_sub(ay);
                (ay, avail.max(3))
            };

            FloatingWindow {
                x: ax,
                y,
                width: win_w,
                height,
            }
        }
    }
}

/// Lay out an info floating window.
/// Long content lines are wrapped at the popup width (matching Kakoune behaviour).
///
/// `menu_rect` is the screen rectangle of the active menu (if any), used for
/// obstacle avoidance and `menuDoc` placement.
pub fn layout_info(
    title: &Line,
    content: &[Line],
    anchor: &Coord,
    style: InfoStyle,
    screen_w: u16,
    screen_h: u16,
    menu_rect: Option<Rect>,
) -> FloatingWindow {
    // --- max_size adjustment (Kakoune terminal_ui.cc:1326-1331) ---
    let menu_lines = menu_rect.map_or(0u16, |r| r.h);
    let (max_w, max_h) = match style {
        InfoStyle::MenuDoc => {
            // For menuDoc, max width is the larger side of the screen around the menu.
            let mw = menu_rect.map_or(screen_w, |mr| {
                let right_of_menu = screen_w.saturating_sub(mr.x + mr.w);
                let left_of_menu = mr.x;
                right_of_menu.max(left_of_menu)
            });
            (mw, screen_h)
        }
        InfoStyle::Modal => (screen_w, screen_h),
        _ => {
            // Non-modal, non-menuDoc: subtract menu lines from available height
            (screen_w, screen_h.saturating_sub(menu_lines))
        }
    };

    // Trim trailing empty content lines (Kakoune doesn't render them)
    let content_len = trim_trailing_empty(content);
    let content = &content[..content_len];

    let framed = style.is_framed();

    let raw_max_content_width = content.iter().map(line_display_width).max().unwrap_or(0) as u16;
    let title_width = line_display_width(title) as u16;
    // Kakoune: framed → title + 2 (for ┤├ decorators)
    let effective_title_w = if framed {
        title_width.saturating_add(2)
    } else {
        title_width
    };

    // Kakoune 2-pass: if raw content is wider than the budget, wrap first and
    // use the actual wrapped max-row-width (which can be < budget due to word
    // boundaries not landing exactly at the limit).
    let max_content_budget = if framed {
        max_w.saturating_sub(4)
    } else {
        max_w
    };
    let actual_content_w = if raw_max_content_width > max_content_budget {
        content
            .iter()
            .map(|line| word_wrap_max_row_width(line, max_content_budget))
            .max()
            .unwrap_or(0)
    } else {
        raw_max_content_width
    };

    let content_w = actual_content_w.max(effective_title_w).max(1);

    // Kakoune: framed → +4 columns (│ + space on each side), non-framed → +0
    let (win_w, inner_w) = if framed {
        let ww = (content_w + 4).min(max_w);
        (ww, ww.saturating_sub(4).max(1))
    } else {
        let ww = content_w.min(max_w);
        (ww, ww)
    };

    // Compute wrapped content height with word-boundary wrapping
    let wrapped_h: u16 = content
        .iter()
        .map(|line| word_wrap_line_height(line, inner_w))
        .sum::<u16>()
        .max(if content.is_empty() { 1 } else { 0 });

    // Kakoune: framed → +2 rows (top/bottom border), non-framed → +0
    let win_h = if framed {
        (wrapped_h + 2).min(max_h)
    } else {
        wrapped_h.max(1).min(max_h)
    };

    let rect = Rect {
        x: 0,
        y: 0,
        w: screen_w,
        h: screen_h,
    };

    match style {
        InfoStyle::Modal => {
            // Center on screen
            let x = screen_w.saturating_sub(win_w) / 2;
            let y = screen_h.saturating_sub(win_h) / 2;
            FloatingWindow {
                x,
                y,
                width: win_w,
                height: win_h,
            }
        }
        InfoStyle::Prompt => {
            // Kakoune 2-pass for prompt: budget also subtracts assistant width
            let max_prompt_content = max_w.saturating_sub(4 + PROMPT_ASSISTANT_WIDTH);
            let actual_prompt_content_w = if raw_max_content_width > max_prompt_content {
                content
                    .iter()
                    .map(|line| word_wrap_max_row_width(line, max_prompt_content))
                    .max()
                    .unwrap_or(0)
            } else {
                raw_max_content_width
            };
            let prompt_content_w = actual_prompt_content_w.max(effective_title_w).max(1);
            let prompt_w = (prompt_content_w + 4 + PROMPT_ASSISTANT_WIDTH).min(max_w);
            let prompt_inner_w = prompt_w.saturating_sub(4 + PROMPT_ASSISTANT_WIDTH).max(1);
            let prompt_wrapped_h: u16 = content
                .iter()
                .map(|line| word_wrap_line_height(line, prompt_inner_w))
                .sum::<u16>()
                .max(if content.is_empty() { 1 } else { 0 });
            let prompt_h = (prompt_wrapped_h + 2)
                .max(PROMPT_ASSISTANT_MIN_HEIGHT)
                .min(max_h);

            // Kakoune: anchor = {m_dimensions.line, m_dimensions.column - 1}
            let anchor_line = screen_h as i32;
            let anchor_col = (screen_w as i32).saturating_sub(1);
            let (y, x) = compute_pos(
                (anchor_line, anchor_col),
                (prompt_h, prompt_w),
                rect,
                menu_rect,
                false,
            );
            FloatingWindow {
                x,
                y,
                width: prompt_w,
                height: prompt_h,
            }
        }
        InfoStyle::MenuDoc => {
            // Place beside the menu: prefer right side, fall back to left.
            if let Some(mr) = menu_rect {
                let right_space = screen_w.saturating_sub(mr.x + mr.w);
                let left_space = mr.x;

                let (x, avail_w) = if win_w <= right_space || right_space >= left_space {
                    // Place to the right of the menu
                    (mr.x + mr.w, right_space)
                } else {
                    // Place to the left of the menu
                    let w = win_w.min(left_space);
                    (mr.x.saturating_sub(w), left_space)
                };

                let final_w = win_w.min(avail_w);
                let y = mr.y.min(screen_h.saturating_sub(win_h));

                FloatingWindow {
                    x,
                    y,
                    width: final_w,
                    height: win_h,
                }
            } else {
                // No menu — fall back to inline-style placement
                let (y, x) = compute_pos(
                    (anchor.line, anchor.column),
                    (win_h, win_w),
                    rect,
                    None,
                    false,
                );
                FloatingWindow {
                    x,
                    y,
                    width: win_w,
                    height: win_h,
                }
            }
        }
        InfoStyle::InlineAbove => {
            let (y, x) = compute_pos(
                (anchor.line, anchor.column),
                (win_h, win_w),
                rect,
                menu_rect,
                true,
            );
            FloatingWindow {
                x,
                y,
                width: win_w,
                height: win_h,
            }
        }
        InfoStyle::Inline => {
            let (y, x) = compute_pos(
                (anchor.line, anchor.column),
                (win_h, win_w),
                rect,
                menu_rect,
                false,
            );
            FloatingWindow {
                x,
                y,
                width: win_w,
                height: win_h,
            }
        }
    }
}

fn line_display_width(line: &Line) -> usize {
    line.iter()
        .map(|atom| UnicodeWidthStr::width(atom.contents.as_str()))
        .sum()
}

/// Compute the number of visual rows a line occupies when wrapped at word boundaries
/// (matching Kakoune's `wrap_lines`). Returns at least 1 for non-empty lines.
pub fn word_wrap_line_height(line: &Line, max_width: u16) -> u16 {
    if max_width == 0 {
        return 1;
    }

    let mut metrics: Vec<(u16, bool)> = Vec::new();
    for atom in line {
        for grapheme in atom.contents.split_inclusive(|_: char| true) {
            if grapheme.is_empty() {
                continue;
            }
            let w = UnicodeWidthStr::width(grapheme) as u16;
            if w == 0 {
                continue;
            }
            metrics.push((w, !is_word_char(grapheme)));
        }
    }

    if metrics.is_empty() {
        return 1;
    }

    let mut rows = 0u16;
    let mut col = 0u16;
    let mut last_break_idx: Option<usize> = None;
    let mut i = 0;

    while i < metrics.len() {
        let (w, is_boundary) = metrics[i];

        if col + w > max_width {
            if col == 0 {
                // Single grapheme wider than max_width: force-place it
                rows += 1;
                i += 1;
                last_break_idx = None;
                continue;
            }
            rows += 1;
            col = 0;
            if let Some(brk) = last_break_idx {
                i = brk;
                last_break_idx = None;
            }
            continue;
        }

        col += w;
        if is_boundary {
            last_break_idx = Some(i + 1);
        }
        i += 1;
    }

    rows + 1
}

/// Return the maximum display width of any row after word-wrapping a line
/// at `max_width` (matching Kakoune's `compute_size` after `wrap_lines`).
///
/// This is the width counterpart of [`word_wrap_line_height`]: same wrapping
/// logic, but tracks the widest row instead of counting rows.
pub fn word_wrap_max_row_width(line: &Line, max_width: u16) -> u16 {
    if max_width == 0 {
        return 0;
    }

    let mut metrics: Vec<(u16, bool)> = Vec::new();
    for atom in line {
        for grapheme in atom.contents.split_inclusive(|_: char| true) {
            if grapheme.is_empty() {
                continue;
            }
            let w = UnicodeWidthStr::width(grapheme) as u16;
            if w == 0 {
                continue;
            }
            metrics.push((w, !is_word_char(grapheme)));
        }
    }

    if metrics.is_empty() {
        return 0;
    }

    let mut max_row_w = 0u16;
    let mut col = 0u16;
    let mut last_break_idx: Option<usize> = None;
    let mut last_break_col = 0u16;
    let mut i = 0;

    while i < metrics.len() {
        let (w, is_boundary) = metrics[i];

        if col + w > max_width {
            if col == 0 {
                // Single grapheme wider than max_width: force-place it
                max_row_w = max_row_w.max(w);
                i += 1;
                last_break_idx = None;
                continue;
            }
            if let Some(brk) = last_break_idx {
                max_row_w = max_row_w.max(last_break_col);
                i = brk;
                last_break_idx = None;
            } else {
                max_row_w = max_row_w.max(col);
            }
            col = 0;
            continue;
        }

        col += w;
        if is_boundary {
            last_break_idx = Some(i + 1);
            last_break_col = col;
        }
        i += 1;
    }

    // Account for the last row
    max_row_w.max(col)
}

/// Return the index (exclusive) of the last non-empty line in `content`,
/// effectively trimming trailing empty lines.
fn trim_trailing_empty(content: &[Line]) -> usize {
    content
        .iter()
        .rposition(|line| line_display_width(line) > 0)
        .map(|i| i + 1)
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::protocol::{Atom, Face};

    fn make_line(s: &str) -> Line {
        vec![Atom {
            face: Face::default(),
            contents: s.to_string(),
        }]
    }

    #[test]
    fn test_layout_menu_inline() {
        let items = vec![make_line("item1"), make_line("longer item")];
        let anchor = Coord { line: 2, column: 5 };
        let win = layout_menu(&anchor, &items, MenuStyle::Inline, 80, 24);
        assert!(win.y > 2); // below anchor
        assert!(win.width >= 2 + "longer item".len() as u16);
        assert_eq!(win.height, 4); // 2 items + 2 borders
    }

    #[test]
    fn test_layout_menu_prompt() {
        let items = vec![make_line("a"), make_line("b")];
        let anchor = Coord { line: 0, column: 0 };
        let win = layout_menu(&anchor, &items, MenuStyle::Prompt, 80, 24);
        // prompt style: above status bar
        assert!(win.y + win.height <= 24);
    }

    #[test]
    fn test_layout_info_modal() {
        let title = make_line("Help");
        let content = vec![make_line("line1"), make_line("line2")];
        let anchor = Coord { line: 0, column: 0 };
        let win = layout_info(&title, &content, &anchor, InfoStyle::Modal, 80, 24, None);
        // Should be roughly centered
        assert!(win.x > 0);
        assert!(win.y > 0);
    }

    // ----- compute_pos tests -----

    fn screen_rect(w: u16, h: u16) -> Rect {
        Rect { x: 0, y: 0, w, h }
    }

    #[test]
    fn test_compute_pos_below() {
        // Place a 3×10 popup below anchor at (5, 2) on an 80×24 screen
        let (y, x) = compute_pos((5, 2), (3, 10), screen_rect(80, 24), None, false);
        assert_eq!(y, 6); // anchor_line + 1
        assert_eq!(x, 2);
    }

    #[test]
    fn test_compute_pos_above() {
        // prefer_above: place above anchor at (10, 5)
        let (y, x) = compute_pos((10, 5), (3, 10), screen_rect(80, 24), None, true);
        assert_eq!(y, 7); // anchor_line - height = 10 - 3
        assert_eq!(x, 5);
    }

    #[test]
    fn test_compute_pos_flip_below_to_above() {
        // Anchor near bottom: placing below would overflow, should flip above
        let (y, _x) = compute_pos((22, 0), (5, 10), screen_rect(80, 24), None, false);
        assert_eq!(y, 17); // 22 - 5
    }

    #[test]
    fn test_compute_pos_flip_above_to_below() {
        // Anchor near top: placing above would overflow, should flip below
        let (y, _x) = compute_pos((1, 0), (5, 10), screen_rect(80, 24), None, true);
        assert_eq!(y, 2); // anchor_line + 1
    }

    #[test]
    fn test_compute_pos_horizontal_clamp() {
        // Anchor column near right edge — should clamp left
        let (y, x) = compute_pos((5, 75), (3, 10), screen_rect(80, 24), None, false);
        assert_eq!(y, 6);
        assert_eq!(x, 70); // 80 - 10
    }

    #[test]
    fn test_compute_pos_menu_avoidance() {
        // Menu occupies rows 6..10.  Popup would land at y=6 (below anchor at 5).
        // Kakoune avoidance: min(menu_top, anchor) - h = min(6,5) - 3 = 2.
        let menu = Rect {
            x: 0,
            y: 6,
            w: 20,
            h: 4,
        };
        let (y, _x) = compute_pos((5, 0), (3, 10), screen_rect(80, 24), Some(menu), false);
        assert_eq!(y, 2); // above both: min(menu_top(6), anchor(5)) - h(3) = 2
    }

    #[test]
    fn test_compute_pos_menu_avoidance_above() {
        // Menu occupies rows 10..14. Popup (h=3) would land at y=11 (below anchor 10).
        // Overlaps menu → try above menu: y = 10 - 3 = 7.
        let menu = Rect {
            x: 0,
            y: 10,
            w: 20,
            h: 4,
        };
        let (y, _x) = compute_pos((10, 0), (3, 10), screen_rect(80, 24), Some(menu), false);
        assert_eq!(y, 7); // above menu
    }

    // ----- layout_info with menu_rect tests -----

    #[test]
    fn test_layout_info_inline_avoids_menu() {
        let title = make_line("");
        let content = vec![make_line("hello")];
        let anchor = Coord { line: 5, column: 0 };
        let menu = Rect {
            x: 0,
            y: 6,
            w: 20,
            h: 5,
        };
        let win = layout_info(&title, &content, &anchor, InfoStyle::Inline, 80, 24, Some(menu));
        // Should not overlap with menu (y=6..11)
        let win_bot = win.y + win.height;
        assert!(win.y >= 11 || win_bot <= 6, "info should not overlap menu");
    }

    #[test]
    fn test_layout_info_menudoc_right_of_menu() {
        let title = make_line("");
        let content = vec![make_line("doc text")];
        let anchor = Coord {
            line: 5,
            column: 10,
        };
        // Menu at left side of screen: x=0, w=20 → right_space=60, left_space=0
        let menu = Rect {
            x: 0,
            y: 5,
            w: 20,
            h: 8,
        };
        let win = layout_info(&title, &content, &anchor, InfoStyle::MenuDoc, 80, 24, Some(menu));
        // Should be placed to the right of the menu
        assert!(win.x >= 20, "menuDoc should be to the right of menu");
    }

    #[test]
    fn test_layout_info_menudoc_left_of_menu() {
        let title = make_line("");
        let content = vec![make_line("doc")];
        let anchor = Coord {
            line: 5,
            column: 70,
        };
        // Menu at right side of screen: x=60, w=20 → right_space=0, left_space=60
        let menu = Rect {
            x: 60,
            y: 5,
            w: 20,
            h: 8,
        };
        let win = layout_info(&title, &content, &anchor, InfoStyle::MenuDoc, 80, 24, Some(menu));
        // Should be placed to the left of the menu
        assert!(
            win.x + win.width <= 60,
            "menuDoc should be to the left of menu"
        );
    }

    #[test]
    fn test_layout_info_max_height_reduced_by_menu() {
        let title = make_line("");
        // 30 content lines — would normally need 32 rows (+ borders)
        let content: Vec<Line> = (0..30).map(|i| make_line(&format!("line {i}"))).collect();
        let anchor = Coord { line: 0, column: 0 };
        let menu = Rect {
            x: 0,
            y: 15,
            w: 20,
            h: 5,
        };
        let win_with_menu = layout_info(&title, &content, &anchor, InfoStyle::Inline, 80, 24, Some(menu));
        let win_no_menu = layout_info(&title, &content, &anchor, InfoStyle::Inline, 80, 24, None);
        // With menu, max height is reduced so the info should be shorter
        assert!(win_with_menu.height <= win_no_menu.height);
    }

    // ----- word_wrap_line_height tests -----

    #[test]
    fn test_word_wrap_no_wrap() {
        let line = make_line("hello world");
        assert_eq!(word_wrap_line_height(&line, 20), 1);
    }

    #[test]
    fn test_word_wrap_at_word_boundary() {
        // "hello world" (11 chars) in 8-col width
        // Should break at the space: "hello " (6) + "world" (5)
        let line = make_line("hello world");
        assert_eq!(word_wrap_line_height(&line, 8), 2);
    }

    #[test]
    fn test_word_wrap_no_boundary_forces_char_break() {
        // "abcdefghij" (10 chars) in 5-col width — no spaces, forced break
        let line = make_line("abcdefghij");
        assert_eq!(word_wrap_line_height(&line, 5), 2);
    }

    #[test]
    fn test_word_wrap_multiple_rows() {
        // "aa bb cc dd ee" in 6-col width
        // Row 1: "aa bb " (6), Row 2: "cc dd " (6), Row 3: "ee" (2)
        let line = make_line("aa bb cc dd ee");
        assert_eq!(word_wrap_line_height(&line, 6), 3);
    }

    #[test]
    fn test_word_wrap_empty_line() {
        let line = make_line("");
        assert_eq!(word_wrap_line_height(&line, 10), 1);
    }

    #[test]
    fn test_word_wrap_exact_fit() {
        // "hello" in 5-col width — exactly fits, no wrap
        let line = make_line("hello");
        assert_eq!(word_wrap_line_height(&line, 5), 1);
    }

    // ----- word_wrap_max_row_width tests -----

    #[test]
    fn test_max_row_width_no_wrap() {
        // Short line that fits — returns raw width
        let line = make_line("hello");
        assert_eq!(word_wrap_max_row_width(&line, 20), 5);
    }

    #[test]
    fn test_max_row_width_word_boundary() {
        // "hello world" (11 chars) wraps at 8: "hello " (6) + "world" (5) → max = 6
        let line = make_line("hello world");
        assert_eq!(word_wrap_max_row_width(&line, 8), 6);
    }

    #[test]
    fn test_max_row_width_forced_break() {
        // "abcdefghij" (10 chars, no spaces) in 5-col: forced break at 5 → max = 5
        let line = make_line("abcdefghij");
        assert_eq!(word_wrap_max_row_width(&line, 5), 5);
    }

    #[test]
    fn test_max_row_width_empty_line() {
        let line = make_line("");
        assert_eq!(word_wrap_max_row_width(&line, 10), 0);
    }

    #[test]
    fn test_max_row_width_multiple_rows() {
        // "aa bb cc dd ee" in 6-col: "aa bb " (6) + "cc dd " (6) + "ee" (2) → max = 6
        let line = make_line("aa bb cc dd ee");
        assert_eq!(word_wrap_max_row_width(&line, 6), 6);
    }

    #[test]
    fn test_max_row_width_less_than_budget() {
        // Verify that wrapped width can be strictly less than max_width.
        // "aaa bbb ccc" in 8-col: "aaa bbb " (8)? Let's check:
        // a a a ' ' b b b ' ' c c c
        // col: 1,2,3, 4(brk), 5,6,7, 8(brk), c: 9>8 → break at brk@8 → row=8
        // Actually 8 == max_width, so let's use a different example.
        // "aaa bbbbb ccc" in 10-col:
        // a a a ' ' b b b b b ' ' c c c
        // col: 1,2,3, 4(brk), 5,6,7,8,9, 10(brk), c: 11>10 → break at brk@10 → row=10
        // That's 10==max_width again.
        // "aaa bbbb" in 10-col: fits entirely (8 chars) → 8 < 10 ✓
        let line = make_line("aaa bbbb");
        assert_eq!(word_wrap_max_row_width(&line, 10), 8);
        // But wrapping is only triggered when raw > budget, so let's test via layout_info
    }

    // ----- layout_info popup width tests -----

    #[test]
    fn test_layout_info_prompt_width_less_than_screen() {
        let title = make_line("info");
        // Create a long content line that will need wrapping.
        // On an 80-col screen, prompt budget = 80 - 4 - 8 = 68.
        // A line of "word " repeated many times will wrap at word boundaries,
        // and the actual row width should be <= 68 but potentially < 68.
        let long_text = "word ".repeat(20); // 100 chars
        let content = vec![make_line(long_text.trim_end())];
        let anchor = Coord { line: 0, column: 0 };
        let win = layout_info(&title, &content, &anchor, InfoStyle::Prompt, 80, 24, None);
        // The popup width should be < screen width (80)
        assert!(
            win.width < 80,
            "prompt popup width ({}) should be less than screen width (80)",
            win.width
        );
    }

    // ----- trim_trailing_empty tests -----

    #[test]
    fn test_trim_trailing_empty_lines() {
        let content = vec![make_line("hello"), make_line(""), make_line("")];
        assert_eq!(trim_trailing_empty(&content), 1);
    }

    #[test]
    fn test_trim_no_trailing_empty() {
        let content = vec![make_line("hello"), make_line("world")];
        assert_eq!(trim_trailing_empty(&content), 2);
    }

    #[test]
    fn test_trim_all_empty() {
        let content = vec![make_line(""), make_line("")];
        assert_eq!(trim_trailing_empty(&content), 0);
    }

    #[test]
    fn test_trim_middle_empty_preserved() {
        let content = vec![make_line("a"), make_line(""), make_line("b"), make_line("")];
        assert_eq!(trim_trailing_empty(&content), 3);
    }

    // ----- layout_info trailing empty line test -----

    #[test]
    fn test_layout_info_trailing_empty_ignored() {
        let title = make_line("test");
        let content_with_empty = vec![make_line("line1"), make_line("")];
        let content_without = vec![make_line("line1")];
        let anchor = Coord { line: 0, column: 0 };
        let w1 = layout_info(&title, &content_with_empty, &anchor, InfoStyle::Inline, 80, 24, None);
        let w2 = layout_info(&title, &content_without, &anchor, InfoStyle::Inline, 80, 24, None);
        // Trailing empty line should not affect height
        assert_eq!(w1.height, w2.height);
    }
}
