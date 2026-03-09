use super::text::trim_trailing_empty;
use super::{
    FloatingWindow, PROMPT_ASSISTANT_MIN_HEIGHT, PROMPT_ASSISTANT_WIDTH, Rect, compute_pos,
    line_display_width, word_wrap_line_height, word_wrap_max_row_width,
};
use crate::protocol::{Coord, InfoStyle, Line};

/// Lay out an info floating window.
/// Long content lines are wrapped at the popup width (matching Kakoune behaviour).
///
/// `avoid` contains screen rectangles of obstacles (menu, cursor, etc.) used for
/// obstacle avoidance. The first element, if present, is treated as the menu rect
/// for `menuDoc` placement and max-height reduction.
pub fn layout_info(
    title: &Line,
    content: &[Line],
    anchor: &Coord,
    style: InfoStyle,
    screen_w: u16,
    screen_h: u16,
    avoid: &[Rect],
) -> FloatingWindow {
    // The first avoid rect is the menu (for height reduction and menuDoc placement).
    let menu_rect = avoid.first().copied();
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

    let raw_max_content_width = content
        .iter()
        .map(|l| line_display_width(l))
        .max()
        .unwrap_or(0) as u16;
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
        InfoStyle::Prompt => layout_info_prompt(
            content,
            raw_max_content_width,
            effective_title_w,
            max_w,
            max_h,
            rect,
            avoid,
        ),
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
                    avoid,
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
                avoid,
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
                avoid,
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

/// Compute the Prompt-style info layout (with assistant width budget).
fn layout_info_prompt(
    content: &[Line],
    raw_max_content_width: u16,
    effective_title_w: u16,
    max_w: u16,
    max_h: u16,
    rect: Rect,
    avoid: &[Rect],
) -> FloatingWindow {
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
    let anchor_line = rect.h as i32;
    let anchor_col = (rect.w as i32).saturating_sub(1);
    let (y, x) = compute_pos(
        (anchor_line, anchor_col),
        (prompt_h, prompt_w),
        rect,
        avoid,
        false,
    );
    FloatingWindow {
        x,
        y,
        width: prompt_w,
        height: prompt_h,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_utils::make_line;

    #[test]
    fn test_layout_info_modal() {
        let title = make_line("Help");
        let content = vec![make_line("line1"), make_line("line2")];
        let anchor = Coord { line: 0, column: 0 };
        let win = layout_info(&title, &content, &anchor, InfoStyle::Modal, 80, 24, &[]);
        // Should be roughly centered
        assert!(win.x > 0);
        assert!(win.y > 0);
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
        let win = layout_info(
            &title,
            &content,
            &anchor,
            InfoStyle::Inline,
            80,
            24,
            &[menu],
        );
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
        let win = layout_info(
            &title,
            &content,
            &anchor,
            InfoStyle::MenuDoc,
            80,
            24,
            &[menu],
        );
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
        let win = layout_info(
            &title,
            &content,
            &anchor,
            InfoStyle::MenuDoc,
            80,
            24,
            &[menu],
        );
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
        let win_with_menu = layout_info(
            &title,
            &content,
            &anchor,
            InfoStyle::Inline,
            80,
            24,
            &[menu],
        );
        let win_no_menu = layout_info(&title, &content, &anchor, InfoStyle::Inline, 80, 24, &[]);
        // With menu, max height is reduced so the info should be shorter
        assert!(win_with_menu.height <= win_no_menu.height);
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
        let win = layout_info(&title, &content, &anchor, InfoStyle::Prompt, 80, 24, &[]);
        // The popup width should be < screen width (80)
        assert!(
            win.width < 80,
            "prompt popup width ({}) should be less than screen width (80)",
            win.width
        );
    }

    // ----- layout_info trailing empty line test -----

    #[test]
    fn test_layout_info_trailing_empty_ignored() {
        let title = make_line("test");
        let content_with_empty = vec![make_line("line1"), make_line("")];
        let content_without = vec![make_line("line1")];
        let anchor = Coord { line: 0, column: 0 };
        let w1 = layout_info(
            &title,
            &content_with_empty,
            &anchor,
            InfoStyle::Inline,
            80,
            24,
            &[],
        );
        let w2 = layout_info(
            &title,
            &content_without,
            &anchor,
            InfoStyle::Inline,
            80,
            24,
            &[],
        );
        // Trailing empty line should not affect height
        assert_eq!(w1.height, w2.height);
    }
}
