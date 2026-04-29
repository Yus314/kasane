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
use kasane_core::render::{Cell, CellGrid, CursorStyle, ImageRequest, RenderResult};

use crate::kitty::KittyState;
use crate::sgr::emit_sgr_diff_style;
use crate::terminal_style::TerminalStyle;

pub struct TuiBackend {
    stdout: Stdout,
    /// Frame buffer: all escape sequences for a frame are collected here, then
    /// written to stdout in a single `write_all()` in `flush()`.  This avoids
    /// the 8 KB auto-flush of `BufWriter` which caused the terminal to render
    /// partial frames (visible as cursor-like blocks at line ends).
    buf: Vec<u8>,
    /// Previous frame's cell buffer for incremental diff.
    previous: Vec<Cell>,
    /// Kitty Graphics Protocol state (None when protocol is Off).
    pub(crate) kitty: Option<KittyState>,
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
            kitty: None,
        })
    }

    pub fn size(&self) -> (u16, u16) {
        terminal::size().unwrap_or((80, 24))
    }

    /// Render the grid to the terminal, diffing against the previous frame.
    ///
    /// When Kitty Graphics Protocol is active, image upload bytes are written
    /// before the synchronized update, and placement/deletion bytes are written
    /// inside it (alongside CellGrid diff).
    #[allow(clippy::needless_range_loop)]
    pub fn present(
        &mut self,
        grid: &mut CellGrid,
        result: &RenderResult,
        image_requests: &[ImageRequest],
    ) -> anyhow::Result<()> {
        // --- Kitty: reconcile and write uploads outside SyncUpdate ---
        let kitty_place_bytes = if let Some(ref mut kitty) = self.kitty {
            let reconciled = crate::kitty::reconcile(kitty, image_requests);
            if !reconciled.upload_bytes.is_empty() {
                tracing::debug!(
                    upload_len = reconciled.upload_bytes.len(),
                    place_len = reconciled.place_bytes.len(),
                    "kitty: writing upload bytes outside SyncUpdate"
                );
                self.stdout.write_all(&reconciled.upload_bytes)?;
                self.stdout.flush()?;
            }
            reconciled.place_bytes
        } else {
            Vec::new()
        };

        queue!(self.buf, BeginSynchronizedUpdate, cursor::Hide)?;

        let cells = grid.cells();
        let dirty_rows = grid.dirty_rows();
        let w = grid.width() as usize;
        let full_redraw = self.previous.is_empty();

        let mut last_terminal_style: Option<TerminalStyle> = None;
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

                if last_terminal_style.as_ref() != Some(&cell.style) {
                    emit_sgr_diff_style(&mut self.buf, last_terminal_style.as_ref(), &cell.style)?;
                    last_terminal_style = Some(cell.style);
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

        // --- Kitty: deletions + placements inside SyncUpdate ---
        if !kitty_place_bytes.is_empty() {
            tracing::debug!(
                place_len = kitty_place_bytes.len(),
                total_buf_before = self.buf.len(),
                "kitty: appending placement bytes inside SyncUpdate"
            );
            self.buf.extend_from_slice(&kitty_place_bytes);
        }

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
        if let Some(ref mut kitty) = self.kitty {
            crate::kitty::clear_all(kitty, &mut self.buf);
            let _ = self.stdout.write_all(&self.buf);
            let _ = self.stdout.flush();
            self.buf.clear();
        }
    }

    pub fn cleanup(&mut self) {
        // Clean up Kitty images before leaving alternate screen
        if self.kitty.is_some() {
            crate::kitty::emit_delete_all(&mut self.buf);
            let _ = self.stdout.write_all(&self.buf);
            let _ = self.stdout.flush();
            self.buf.clear();
        }
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
