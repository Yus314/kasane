use std::hash::{Hash, Hasher};

use kasane_core::render::{CellGrid, CursorStyle};

use super::{CURSOR_BAR_WIDTH, CURSOR_OUTLINE_THICKNESS, CURSOR_UNDERLINE_HEIGHT};
use crate::colors::ColorResolver;

// ---------------------------------------------------------------------------
// Free functions (used by benchmarks and SceneRenderer)
// ---------------------------------------------------------------------------

/// Build background instance data (Vec of floats) from a CellGrid.
///
/// Each instance is 8 floats: x, y, w, h, r, g, b, a.
/// Includes cursor overlay if provided.
pub fn build_bg_instances(
    grid: &CellGrid,
    color_resolver: &ColorResolver,
    cell_w: f32,
    cell_h: f32,
    cursor: Option<(u16, u16, CursorStyle)>,
    out: &mut Vec<f32>,
) {
    for row in 0..grid.height() {
        let y = row as f32 * cell_h;
        for col in 0..grid.width() {
            let cell = grid
                .get(col, row)
                .expect("grid bounds in build_bg_instances");
            let bg = color_resolver.resolve(cell.face.bg, false);
            let x = col as f32 * cell_w;
            out.extend_from_slice(&[x, y, cell_w, cell_h, bg[0], bg[1], bg[2], bg[3]]);
        }
    }

    // Add cursor overlay
    if let Some((cx, cy, style)) = cursor {
        let x = cx as f32 * cell_w;
        let y = cy as f32 * cell_h;
        let cc = color_resolver.resolve(kasane_core::protocol::Color::Default, true);
        let push = |out: &mut Vec<f32>, x: f32, y: f32, w: f32, h: f32, c: [f32; 4]| {
            out.extend_from_slice(&[x, y, w, h, c[0], c[1], c[2], c[3]]);
        };
        match style {
            CursorStyle::Block => push(out, x, y, cell_w, cell_h, cc),
            CursorStyle::Bar => push(out, x, y, CURSOR_BAR_WIDTH, cell_h, cc),
            CursorStyle::Underline => push(
                out,
                x,
                y + cell_h - CURSOR_UNDERLINE_HEIGHT,
                cell_w,
                CURSOR_UNDERLINE_HEIGHT,
                cc,
            ),
            CursorStyle::Outline => {
                let t = CURSOR_OUTLINE_THICKNESS;
                push(out, x, y, cell_w, t, cc); // Top
                push(out, x, y + cell_h - t, cell_w, t, cc); // Bottom
                push(out, x, y, t, cell_h, cc); // Left
                push(out, x + cell_w - t, y, t, cell_h, cc); // Right
            }
        }
    }
}

/// Compute a hash of a row's content and foreground colors for dirty tracking.
pub fn compute_row_hash(grid: &CellGrid, row: u16, color_resolver: &ColorResolver) -> u64 {
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    for col in 0..grid.width() {
        let cell = grid.get(col, row).expect("grid bounds in compute_row_hash");
        cell.grapheme.hash(&mut hasher);
        std::mem::discriminant(&cell.face.fg).hash(&mut hasher);
        let fg_bits = color_resolver.resolve(cell.face.fg, true);
        fg_bits[0].to_bits().hash(&mut hasher);
        fg_bits[1].to_bits().hash(&mut hasher);
        fg_bits[2].to_bits().hash(&mut hasher);
    }
    hasher.finish()
}

/// Build text string and color span ranges for a single row.
///
/// Clears and populates the provided `row_text` and `span_ranges` buffers.
/// Each span is `(byte_start, byte_end, [r, g, b, a])`.
pub fn build_row_spans(
    grid: &CellGrid,
    row: u16,
    color_resolver: &ColorResolver,
    row_text: &mut String,
    span_ranges: &mut Vec<(usize, usize, [f32; 4])>,
) {
    row_text.clear();
    span_ranges.clear();

    for col in 0..grid.width() {
        let cell = grid.get(col, row).expect("grid bounds in build_row_spans");
        if cell.width == 0 {
            continue;
        }
        let start = row_text.len();
        let grapheme = if cell.grapheme.is_empty() {
            " "
        } else {
            &cell.grapheme
        };
        row_text.push_str(grapheme);
        let fg = color_resolver.resolve(cell.face.fg, true);
        span_ranges.push((start, row_text.len(), fg));
    }
}
