#[cfg(test)]
use crate::render::grid::CellGrid;
#[cfg(test)]
use crate::state::{AppState, InfoState};

// ---------------------------------------------------------------------------
// Kakoune assistant (clippy)
// ---------------------------------------------------------------------------

#[cfg(test)]
use crate::layout::{ASSISTANT_CLIPPY, ASSISTANT_WIDTH};

#[cfg(test)]
pub(in crate::render) fn render_info(state: &AppState, grid: &mut CellGrid) {
    let info = match state.observed.infos.first() {
        Some(i) => i,
        None => return,
    };

    let menu_rect = crate::layout::get_menu_rect(state);
    let avoid: Vec<crate::layout::Rect> = menu_rect.into_iter().collect();
    let win = crate::layout::layout_info(
        &info.title,
        &info.content,
        &info.anchor,
        info.style,
        grid.width(),
        grid.height().saturating_sub(1),
        &avoid,
    );

    use crate::protocol::InfoStyle;
    match info.style {
        InfoStyle::Prompt => render_info_prompt(info, grid, &win),
        InfoStyle::Modal => render_info_framed(info, grid, &win),
        InfoStyle::Inline | InfoStyle::InlineAbove | InfoStyle::MenuDoc => {
            render_info_nonframed(info, grid, &win)
        }
    }
}

/// Render prompt-style info with the clippy assistant (matches Kakoune's terminal UI).
#[cfg(test)]
fn render_info_prompt(info: &InfoState, grid: &mut CellGrid, win: &crate::layout::FloatingWindow) {
    use crate::layout::word_wrap_line_height;

    if win.width < ASSISTANT_WIDTH + 5 || win.height < 3 {
        return;
    }

    let y_start = win.y;
    let total_h = win.height as usize;
    let frame_x = win.x + ASSISTANT_WIDTH;
    let cw = win.width.saturating_sub(ASSISTANT_WIDTH + 4);
    if cw == 0 {
        return;
    }

    // Trim trailing empty content lines (Kakoune doesn't render them)
    let content_end = info
        .content
        .iter()
        .rposition(|line| crate::layout::line_display_width(line) > 0)
        .map(|i| i + 1)
        .unwrap_or(0);
    let trimmed = &info.content[..content_end];

    // Wrapped height for truncation detection
    let wrapped_h: usize = trimmed
        .iter()
        .map(|line| word_wrap_line_height(line, cw) as usize)
        .sum::<usize>()
        .max(1);

    // Fill entire popup area with info face
    for row in 0..total_h as u16 {
        let y = y_start + row;
        for x in win.x..win.x + win.width {
            grid.put_char(x, y, " ", &info.face.to_face());
        }
    }

    // Draw assistant (vertically centered)
    let asst_top = ((total_h as i32 - ASSISTANT_CLIPPY.len() as i32 + 1) / 2).max(0) as usize;
    for row in 0..total_h {
        let y = y_start + row as u16;
        let idx = if row >= asst_top {
            (row - asst_top).min(ASSISTANT_CLIPPY.len() - 1)
        } else {
            ASSISTANT_CLIPPY.len() - 1 // padding (empty line)
        };
        for (i, ch) in (0u16..).zip(ASSISTANT_CLIPPY[idx].chars()) {
            let x = win.x + i;
            if x < grid.width() {
                let s: String = ch.into();
                grid.put_char(x, y, &s, &info.face.to_face());
            }
        }
    }

    // --- Top border: ╭─left─┤title├─right─╮ ---
    {
        let title_w = crate::layout::line_display_width(&info.title);
        let y = y_start;
        let mut x = frame_x;
        grid.put_char(x, y, "╭", &info.face.to_face());
        x += 1;
        grid.put_char(x, y, "─", &info.face.to_face());
        x += 1;

        if info.title.is_empty() || cw < 4 {
            for _ in 0..cw {
                grid.put_char(x, y, "─", &info.face.to_face());
                x += 1;
            }
        } else {
            let max_title_w = (cw as usize).saturating_sub(2);
            let title_display_w = title_w.min(max_title_w);
            let dash_count = cw as usize - title_display_w - 2;
            let left_dashes = dash_count / 2;
            let right_dashes = dash_count - left_dashes;

            for _ in 0..left_dashes {
                grid.put_char(x, y, "─", &info.face.to_face());
                x += 1;
            }
            grid.put_char(x, y, "┤", &info.face.to_face());
            x += 1;
            grid.put_line_with_base(
                y,
                x,
                &info.title,
                title_display_w as u16,
                Some(&info.face.to_face()),
            );
            x += title_display_w as u16;
            grid.put_char(x, y, "├", &info.face.to_face());
            x += 1;
            for _ in 0..right_dashes {
                grid.put_char(x, y, "─", &info.face.to_face());
                x += 1;
            }
        }

        grid.put_char(x, y, "─", &info.face.to_face());
        x += 1;
        if x < grid.width() {
            grid.put_char(x, y, "╮", &info.face.to_face());
        }
    }

    // --- Side borders + content ---
    let max_visible_rows = (total_h - 2) as u16;
    let content_rows = (wrapped_h as u16).min(max_visible_rows);
    let truncated = wrapped_h as u16 > max_visible_rows;
    let content_x = frame_x + 2; // after "│ "
    let content_y = y_start + 1;
    let content_end_y = content_y + content_rows;

    // Draw left/right borders for all content rows
    for row in 0..content_rows {
        let y = content_y + row;
        grid.put_char(frame_x, y, "│", &info.face.to_face());
        grid.put_char(frame_x + 1, y, " ", &info.face.to_face());
        let right_space = frame_x + 2 + cw;
        let right_border = frame_x + 3 + cw;
        if right_space < grid.width() {
            grid.put_char(right_space, y, " ", &info.face.to_face());
        }
        if right_border < grid.width() {
            grid.put_char(right_border, y, "│", &info.face.to_face());
        }
    }

    // Render content with wrapping
    let mut y = content_y;
    for line in trimmed {
        if y >= content_end_y {
            break;
        }
        let rows = super::render_wrapped_line(
            grid,
            y,
            content_x,
            line,
            cw,
            Some(&info.face.to_face()),
            content_end_y,
        );
        y += rows;
    }

    // --- Bottom border: ╰─...─╯ (uses ┄ when content is truncated) ---
    let bottom_y = y_start + (content_rows as usize + 1).min(total_h - 1) as u16;
    let dash = if truncated { "┄" } else { "─" };
    {
        let mut x = frame_x;
        grid.put_char(x, bottom_y, "╰", &info.face.to_face());
        x += 1;
        grid.put_char(x, bottom_y, dash, &info.face.to_face());
        x += 1;
        for _ in 0..cw {
            grid.put_char(x, bottom_y, dash, &info.face.to_face());
            x += 1;
        }
        grid.put_char(x, bottom_y, dash, &info.face.to_face());
        x += 1;
        if x < grid.width() {
            grid.put_char(x, bottom_y, "╯", &info.face.to_face());
        }
    }

    // Ellipsis truncation post-pass
    let padding_x = frame_x + 2 + cw;
    let border_x = frame_x + 3 + cw;
    for row in 0..content_rows {
        let y = content_y + row;
        if let Some(cell) = grid.get(padding_x, y)
            && cell.grapheme != " "
        {
            grid.put_char(padding_x, y, "…", &info.face.to_face());
            grid.put_char(border_x, y, "│", &info.face.to_face());
        }
    }
}

/// Render framed info popup (modal style): border + shadow + title + content.
#[cfg(test)]
fn render_info_framed(info: &InfoState, grid: &mut CellGrid, win: &crate::layout::FloatingWindow) {
    super::draw_shadow(grid, win);

    // Framed: │ + space padding on each side → inner offset 2, inner width -4
    let inner_x = win.x + 2;
    let inner_y = win.y + 1;
    let inner_w = win.width.saturating_sub(4).max(1);
    let inner_h = win.height.saturating_sub(2);
    let y_limit = inner_y + inner_h;

    // Fill entire inner area (including space padding columns) with info face
    for row in 0..inner_h {
        let y = inner_y + row;
        for x in (win.x + 1)..(win.x + win.width).saturating_sub(1) {
            grid.put_char(x, y, " ", &info.face.to_face());
        }
    }

    // Render content lines with wrapping
    let mut y = inner_y;
    let mut all_rendered = true;
    for (i, line) in info.content.iter().enumerate() {
        if y >= y_limit {
            if i < info.content.len() {
                all_rendered = false;
            }
            break;
        }
        let rows = super::render_wrapped_line(
            grid,
            y,
            inner_x,
            line,
            inner_w,
            Some(&info.face.to_face()),
            y_limit,
        );
        y += rows;
    }
    let truncated = !all_rendered;

    super::draw_border(
        grid,
        win,
        &info.face.to_face(),
        truncated,
        ("╭", "╮", "╰", "╯"),
    );

    // Draw title on top border: ╭─┤title├─╮
    if !info.title.is_empty() {
        let title_width = crate::layout::line_display_width(&info.title);
        if title_width > 0 && win.width > 6 {
            let tx = win.x + 2; // after ╭─
            grid.put_char(tx, win.y, "┤", &info.face.to_face());
            let max_title = (win.width as usize - 6).min(title_width) as u16;
            grid.put_line_with_base(
                win.y,
                tx + 1,
                &info.title,
                max_title,
                Some(&info.face.to_face()),
            );
            let after = tx + 1 + max_title;
            if after < win.x + win.width - 2 {
                grid.put_char(after, win.y, "├", &info.face.to_face());
            }
        }
    }

    // Ellipsis truncation post-pass: if content overflowed into padding column
    let padding_x = win.x + win.width - 2; // space before right │
    let border_x = win.x + win.width - 1; // right │
    for row in 0..inner_h {
        let y = inner_y + row;
        if let Some(cell) = grid.get(padding_x, y)
            && cell.grapheme != " "
        {
            grid.put_char(padding_x, y, "…", &info.face.to_face());
            grid.put_char(border_x, y, "│", &info.face.to_face());
        }
    }
}

/// Render non-framed info popup (inline, inlineAbove, menuDoc): no border, no shadow.
#[cfg(test)]
fn render_info_nonframed(
    info: &InfoState,
    grid: &mut CellGrid,
    win: &crate::layout::FloatingWindow,
) {
    let y_limit = win.y + win.height;

    // Fill background with info face
    for row in 0..win.height {
        let y = win.y + row;
        for x in win.x..(win.x + win.width) {
            grid.put_char(x, y, " ", &info.face.to_face());
        }
    }

    // Render content directly (no border, no title)
    let mut y = win.y;
    for line in &info.content {
        if y >= y_limit {
            break;
        }
        let rows = super::render_wrapped_line(
            grid,
            y,
            win.x,
            line,
            win.width,
            Some(&info.face.to_face()),
            y_limit,
        );
        y += rows;
    }
}
