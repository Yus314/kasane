//! TUI backend using crossterm.
//!
//! Changes here should be coordinated with `kasane-core/src/render/` which defines
//! the pipeline, `CellGrid`, and `Cell` types consumed by this backend.

use std::io::{Stdout, Write};

use crossterm::{
    cursor,
    event::{
        DisableBracketedPaste, DisableFocusChange, DisableMouseCapture, EnableBracketedPaste,
        EnableFocusChange, EnableMouseCapture,
    },
    execute, queue,
    style::{self, Attribute as CtAttribute, SetAttribute},
    terminal::{
        self, BeginSynchronizedUpdate, EndSynchronizedUpdate, EnterAlternateScreen,
        LeaveAlternateScreen,
    },
};
use kasane_core::protocol::Face;
use kasane_core::render::{Cell, CellGrid, CursorStyle, RenderResult};

use crate::sgr::emit_sgr_diff;

pub struct TuiBackend {
    stdout: Stdout,
    /// Frame buffer: all escape sequences for a frame are collected here, then
    /// written to stdout in a single `write_all()` in `flush()`.  This avoids
    /// the 8 KB auto-flush of `BufWriter` which caused the terminal to render
    /// partial frames (visible as cursor-like blocks at line ends).
    buf: Vec<u8>,
    /// Previous frame's cell buffer for incremental diff.
    previous: Vec<Cell>,
}

impl TuiBackend {
    pub fn new() -> anyhow::Result<Self> {
        let mut stdout = std::io::stdout();
        terminal::enable_raw_mode()?;
        execute!(
            stdout,
            EnterAlternateScreen,
            EnableMouseCapture,
            EnableFocusChange,
            EnableBracketedPaste,
            cursor::Hide
        )?;
        Ok(TuiBackend {
            stdout,
            buf: Vec::with_capacity(1 << 16),
            previous: Vec::new(),
        })
    }

    pub fn size(&self) -> (u16, u16) {
        terminal::size().unwrap_or((80, 24))
    }

    /// Render the grid to the terminal, diffing against the previous frame.
    ///
    /// This replaces the old `begin_frame` + `draw_grid` + `show_cursor` +
    /// `end_frame` + `flush` + `grid.swap_with_dirty()` sequence.
    #[allow(clippy::needless_range_loop)]
    pub fn present(&mut self, grid: &mut CellGrid, result: RenderResult) -> anyhow::Result<()> {
        queue!(self.buf, BeginSynchronizedUpdate, cursor::Hide)?;

        let cells = grid.cells();
        let dirty_rows = grid.dirty_rows();
        let w = grid.width() as usize;
        let full_redraw = self.previous.is_empty();

        let mut last_face: Option<Face> = None;
        let mut last_x: u16 = u16::MAX;
        let mut last_y: u16 = u16::MAX;

        for row in 0..grid.height() as usize {
            if !full_redraw && !dirty_rows[row] {
                continue;
            }
            let row_start = row * w;
            let row_end = row_start + w;
            for i in row_start..row_end {
                let cell = &cells[i];
                if cell.width == 0 {
                    continue;
                }
                if !full_redraw && *cell == self.previous[i] {
                    continue;
                }

                let x = (i % w) as u16;
                let y = row as u16;

                // Cursor auto-advance: skip MoveTo when the terminal cursor is
                // already at the right position (previous print advanced it).
                let expected_x = if last_y == y { last_x } else { u16::MAX };
                if x != expected_x {
                    queue!(self.buf, cursor::MoveTo(x, y))?;
                }

                let face = &cell.face;
                if last_face.as_ref() != Some(face) {
                    emit_sgr_diff(&mut self.buf, last_face.as_ref(), face)?;
                    last_face = Some(*face);
                }

                let s = if cell.grapheme.is_empty() {
                    " "
                } else {
                    &cell.grapheme
                };
                queue!(self.buf, style::Print(s))?;

                last_x = x + cell.width.max(1) as u16;
                last_y = y;
            }
        }

        // Reset SGR
        queue!(self.buf, SetAttribute(CtAttribute::Reset))?;

        // Show cursor — use blinking variants when blink hint is enabled
        let blink_enabled = result.cursor_blink.as_ref().is_some_and(|b| b.enabled);
        let ct_style = match (result.cursor_style, blink_enabled) {
            (CursorStyle::Block, true) => cursor::SetCursorStyle::BlinkingBlock,
            (CursorStyle::Block, false) => cursor::SetCursorStyle::SteadyBlock,
            (CursorStyle::Bar, true) => cursor::SetCursorStyle::BlinkingBar,
            (CursorStyle::Bar, false) => cursor::SetCursorStyle::SteadyBar,
            (CursorStyle::Underline, true) => cursor::SetCursorStyle::BlinkingUnderScore,
            (CursorStyle::Underline, false) => cursor::SetCursorStyle::SteadyUnderScore,
            (CursorStyle::Outline, _) => cursor::SetCursorStyle::DefaultUserShape,
        };
        queue!(
            self.buf,
            cursor::MoveTo(result.cursor_x, result.cursor_y),
            ct_style,
            cursor::Show
        )?;

        queue!(self.buf, EndSynchronizedUpdate)?;

        // Flush to terminal
        self.stdout.write_all(&self.buf)?;
        self.stdout.flush()?;
        self.buf.clear();

        // Update previous buffer (dirty rows only)
        self.update_previous(grid);
        grid.clear_dirty();

        Ok(())
    }

    /// Invalidate the previous frame buffer, forcing a full redraw on the next
    /// `present()` call. Call this after terminal resize.
    pub fn invalidate(&mut self) {
        self.previous.clear();
    }

    pub fn cleanup(&mut self) {
        let _ = execute!(
            self.stdout,
            cursor::Show,
            DisableFocusChange,
            DisableMouseCapture,
            DisableBracketedPaste,
            LeaveAlternateScreen
        );
        let _ = terminal::disable_raw_mode();
    }

    /// Copy dirty rows from `grid.cells()` into `self.previous`.
    #[allow(clippy::needless_range_loop)]
    fn update_previous(&mut self, grid: &CellGrid) {
        let cells = grid.cells();
        let dirty_rows = grid.dirty_rows();
        let w = grid.width() as usize;
        let size = w * grid.height() as usize;

        if self.previous.len() != size {
            // First frame or resize: full copy
            self.previous = cells.to_vec();
            return;
        }

        for y in 0..grid.height() as usize {
            if dirty_rows[y] {
                let start = y * w;
                let end = start + w;
                self.previous[start..end].clone_from_slice(&cells[start..end]);
            }
        }
    }
}

impl Drop for TuiBackend {
    fn drop(&mut self) {
        self.cleanup();
    }
}
