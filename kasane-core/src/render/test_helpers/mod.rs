pub(in crate::render) mod info;

use super::grid::{self, CellGrid};
use crate::state::AppState;

/// Render the main buffer area (all lines except the last row which is status).
/// Retained for regression testing against the new declarative pipeline.
pub(super) fn render_buffer(state: &AppState, grid: &mut CellGrid) {
    let buffer_rows = grid.height().saturating_sub(1);

    for y in 0..buffer_rows {
        if let Some(line) = state.lines.get(y as usize) {
            grid.fill_row(y, &state.default_face);
            grid.put_line_with_base(y, 0, line, grid.width(), Some(&state.default_face));
        } else {
            // Padding row
            grid.fill_row(y, &state.padding_face);
            // Show tilde for padding like Kakoune
            grid.put_char(0, y, "~", &state.padding_face);
        }
    }
}

/// Render the status bar at the bottom row.
/// Retained for regression testing against the new declarative pipeline.
pub(super) fn render_status(state: &AppState, grid: &mut CellGrid) {
    let y = grid.height().saturating_sub(1);
    grid.fill_row(y, &state.status_default_face);

    // Status line on the left
    grid.put_line_with_base(
        y,
        0,
        &state.status_line,
        grid.width(),
        Some(&state.status_default_face),
    );

    // Mode line on the right
    let mode_width = crate::layout::line_display_width(&state.status_mode_line);
    if mode_width > 0 && grid.width() as usize > mode_width {
        let mode_x = grid.width() - mode_width as u16;
        grid.put_line_with_base(
            y,
            mode_x,
            &state.status_mode_line,
            mode_width as u16,
            Some(&state.status_default_face),
        );
    }
}

/// Retained for regression testing against the new declarative pipeline.
pub(super) fn render_frame(state: &AppState, grid: &mut CellGrid) {
    grid.clear(&state.default_face);
    render_buffer(state, grid); // Layer 0
    render_status(state, grid); // Layer 1
    super::menu::render_menu(state, grid); // Layer 2 (+ shadow)
    info::render_info(state, grid); // Layer 3 (+ shadow)
}

/// Render a protocol Line with word-boundary wrapping at `max_width` columns
/// (matching Kakoune's `wrap_lines`).
/// Returns the number of visual rows consumed.
/// `y_limit` is the exclusive upper bound for y coordinates (content must not exceed this).
pub(super) fn render_wrapped_line(
    grid: &mut CellGrid,
    y_start: u16,
    x_start: u16,
    line: &crate::protocol::Line,
    max_width: u16,
    base_face: Option<&crate::protocol::Face>,
    y_limit: u16,
) -> u16 {
    use unicode_width::UnicodeWidthStr;
    if max_width == 0 {
        return 1;
    }

    // Phase 1: collect graphemes with resolved faces and widths
    let mut graphemes: Vec<(&str, crate::protocol::Face, u16)> = Vec::new();
    for atom in line {
        let face = match base_face {
            Some(base) => grid::resolve_face(&atom.face, base),
            None => atom.face,
        };
        for grapheme in atom.contents.split_inclusive(|_: char| true) {
            if grapheme.is_empty() {
                continue;
            }
            // Skip control characters (see grid.rs for rationale).
            if grapheme.starts_with(|c: char| c.is_control()) {
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
        return 1;
    }

    // Phase 2: build metrics and compute segments
    let metrics: Vec<(u16, bool)> = graphemes
        .iter()
        .map(|(text, _, w)| (*w, !crate::layout::is_word_char(text)))
        .collect();
    let segments = crate::layout::word_wrap_segments(&metrics, max_width);

    // Phase 3: render from segments
    let mut max_row = 0u16;
    for (row_idx, seg) in segments.iter().enumerate() {
        let y = y_start + row_idx as u16;
        if y >= y_limit {
            break;
        }
        let mut col = 0u16;
        for &(grapheme, ref face, w) in &graphemes[seg.start..seg.end] {
            grid.put_char(x_start + col, y, grapheme, face);
            col += w;
        }
        max_row = row_idx as u16;
    }

    max_row + 1
}

pub(super) fn draw_border(
    grid: &mut CellGrid,
    win: &crate::layout::FloatingWindow,
    face: &crate::protocol::Face,
    truncated: bool,
    corners: (&str, &str, &str, &str), // (top-left, top-right, bottom-left, bottom-right)
) {
    use crate::element::BorderLineStyle;
    let border_style = match corners.0 {
        "╭" => BorderLineStyle::Rounded,
        "┌" => BorderLineStyle::Single,
        "╔" => BorderLineStyle::Double,
        "┏" => BorderLineStyle::Heavy,
        _ => BorderLineStyle::Ascii,
    };
    let rect = crate::layout::Rect {
        x: win.x,
        y: win.y,
        w: win.width,
        h: win.height,
    };
    super::paint::paint_border(grid, &rect, face, truncated, border_style);
}

pub(super) fn draw_shadow(grid: &mut CellGrid, win: &crate::layout::FloatingWindow) {
    let rect = crate::layout::Rect {
        x: win.x,
        y: win.y,
        w: win.width,
        h: win.height,
    };
    super::paint::paint_shadow(grid, &rect);
}
