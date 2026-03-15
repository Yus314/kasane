use std::io::{Stdout, Write};

use crossterm::{
    cursor,
    event::{
        DisableBracketedPaste, DisableFocusChange, DisableMouseCapture, EnableBracketedPaste,
        EnableFocusChange, EnableMouseCapture,
    },
    execute, queue,
    style::{
        self, Attribute as CtAttribute, SetAttribute, SetBackgroundColor, SetForegroundColor,
        SetUnderlineColor,
    },
    terminal::{
        self, BeginSynchronizedUpdate, EndSynchronizedUpdate, EnterAlternateScreen,
        LeaveAlternateScreen,
    },
};
use kasane_core::protocol::{Color, Face};
use kasane_core::render::{CellDiff, CellGrid, CursorStyle, RenderBackend};

use crate::sgr::{convert_attribute, convert_color, emit_sgr_diff};

pub struct TuiBackend {
    stdout: Stdout,
    /// Frame buffer: all escape sequences for a frame are collected here, then
    /// written to stdout in a single `write_all()` in `flush()`.  This avoids
    /// the 8 KB auto-flush of `BufWriter` which caused the terminal to render
    /// partial frames (visible as cursor-like blocks at line ends).
    buf: Vec<u8>,
    clipboard: Option<arboard::Clipboard>,
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
        let clipboard = arboard::Clipboard::new().ok();
        Ok(TuiBackend {
            stdout,
            buf: Vec::with_capacity(1 << 16),
            clipboard,
        })
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
}

impl Drop for TuiBackend {
    fn drop(&mut self) {
        self.cleanup();
    }
}

impl RenderBackend for TuiBackend {
    fn size(&self) -> (u16, u16) {
        terminal::size().unwrap_or((80, 24))
    }

    fn begin_frame(&mut self) -> anyhow::Result<()> {
        queue!(self.buf, BeginSynchronizedUpdate, cursor::Hide)?;
        Ok(())
    }

    fn end_frame(&mut self) -> anyhow::Result<()> {
        queue!(self.buf, EndSynchronizedUpdate)?;
        Ok(())
    }

    fn draw(&mut self, diffs: &[CellDiff]) -> anyhow::Result<()> {
        let mut last_face: Option<Face> = None;

        for diff in diffs {
            queue!(self.buf, cursor::MoveTo(diff.x, diff.y))?;

            let face = &diff.cell.face;
            let need_style_update = last_face.as_ref() != Some(face);

            if need_style_update {
                // Reset attributes first
                queue!(self.buf, SetAttribute(CtAttribute::Reset))?;

                queue!(
                    self.buf,
                    SetForegroundColor(convert_color(face.fg)),
                    SetBackgroundColor(convert_color(face.bg))
                )?;

                if face.underline != Color::Default {
                    queue!(self.buf, SetUnderlineColor(convert_color(face.underline)))?;
                }

                for attr in face.attributes.iter() {
                    if let Some(ct_attr) = convert_attribute(attr) {
                        queue!(self.buf, SetAttribute(ct_attr))?;
                    }
                }

                last_face = Some(*face);
            }

            let s = if diff.cell.grapheme.is_empty() {
                " "
            } else {
                &diff.cell.grapheme
            };
            queue!(self.buf, style::Print(s))?;
        }

        // Reset at the end
        queue!(self.buf, SetAttribute(CtAttribute::Reset))?;
        Ok(())
    }

    fn draw_grid(&mut self, grid: &CellGrid) -> anyhow::Result<()> {
        let mut last_face: Option<Face> = None;
        let mut last_x: u16 = u16::MAX;
        let mut last_y: u16 = u16::MAX;

        for (x, y, cell) in grid.iter_diffs() {
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

            // Track cursor position after print
            last_x = x + cell.width.max(1) as u16;
            last_y = y;
        }

        // Reset at the end
        queue!(self.buf, SetAttribute(CtAttribute::Reset))?;
        Ok(())
    }

    fn flush(&mut self) -> anyhow::Result<()> {
        self.stdout.write_all(&self.buf)?;
        self.stdout.flush()?;
        self.buf.clear();
        Ok(())
    }

    fn show_cursor(&mut self, x: u16, y: u16, style: CursorStyle) -> anyhow::Result<()> {
        let ct_style = match style {
            CursorStyle::Block => cursor::SetCursorStyle::SteadyBlock,
            CursorStyle::Bar => cursor::SetCursorStyle::SteadyBar,
            CursorStyle::Underline => cursor::SetCursorStyle::SteadyUnderScore,
            CursorStyle::Outline => cursor::SetCursorStyle::DefaultUserShape,
        };
        queue!(self.buf, cursor::MoveTo(x, y), ct_style, cursor::Show)?;
        Ok(())
    }

    fn hide_cursor(&mut self) -> anyhow::Result<()> {
        queue!(self.buf, cursor::Hide)?;
        Ok(())
    }

    fn clipboard_get(&mut self) -> Option<String> {
        self.clipboard.as_mut()?.get_text().ok()
    }

    fn clipboard_set(&mut self, text: &str) -> bool {
        if let Some(cb) = self.clipboard.as_mut() {
            cb.set_text(text.to_string()).is_ok()
        } else {
            false
        }
    }
}
