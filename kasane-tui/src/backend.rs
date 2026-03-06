use std::io::{Stdout, Write};

use crossterm::{
    cursor,
    event::{DisableFocusChange, DisableMouseCapture, EnableFocusChange, EnableMouseCapture},
    execute, queue,
    style::{
        self, Attribute as CtAttribute, Color as CtColor, SetAttribute, SetBackgroundColor,
        SetForegroundColor, SetUnderlineColor,
    },
    terminal::{
        self, BeginSynchronizedUpdate, EndSynchronizedUpdate, EnterAlternateScreen,
        LeaveAlternateScreen,
    },
};
use kasane_core::protocol::{Attributes, Color, Face, NamedColor};
use kasane_core::render::{CellDiff, CursorStyle, RenderBackend};

pub struct TuiBackend {
    stdout: Stdout,
    /// Frame buffer: all escape sequences for a frame are collected here, then
    /// written to stdout in a single `write_all()` in `flush()`.  This avoids
    /// the 8 KB auto-flush of `BufWriter` which caused the terminal to render
    /// partial frames (visible as cursor-like blocks at line ends).
    buf: Vec<u8>,
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
            cursor::Hide
        )?;
        Ok(TuiBackend {
            stdout,
            buf: Vec::with_capacity(1 << 16),
        })
    }

    pub fn cleanup(&mut self) {
        let _ = execute!(
            self.stdout,
            cursor::Show,
            DisableFocusChange,
            DisableMouseCapture,
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
}

fn convert_color(color: Color) -> CtColor {
    match color {
        Color::Default => CtColor::Reset,
        Color::Named(named) => match named {
            NamedColor::Black => CtColor::Black,
            NamedColor::Red => CtColor::DarkRed,
            NamedColor::Green => CtColor::DarkGreen,
            NamedColor::Yellow => CtColor::DarkYellow,
            NamedColor::Blue => CtColor::DarkBlue,
            NamedColor::Magenta => CtColor::DarkMagenta,
            NamedColor::Cyan => CtColor::DarkCyan,
            NamedColor::White => CtColor::Grey,
            NamedColor::BrightBlack => CtColor::DarkGrey,
            NamedColor::BrightRed => CtColor::Red,
            NamedColor::BrightGreen => CtColor::Green,
            NamedColor::BrightYellow => CtColor::Yellow,
            NamedColor::BrightBlue => CtColor::Blue,
            NamedColor::BrightMagenta => CtColor::Magenta,
            NamedColor::BrightCyan => CtColor::Cyan,
            NamedColor::BrightWhite => CtColor::White,
        },
        Color::Rgb { r, g, b } => CtColor::Rgb { r, g, b },
    }
}

/// Convert a kasane Attributes flag to a crossterm Attribute.
/// Returns None for Kakoune-internal attributes (final_*) that have no terminal equivalent.
fn convert_attribute(attr: Attributes) -> Option<CtAttribute> {
    match attr {
        Attributes::UNDERLINE => Some(CtAttribute::Underlined),
        Attributes::CURLY_UNDERLINE => Some(CtAttribute::Undercurled),
        Attributes::DOUBLE_UNDERLINE => Some(CtAttribute::DoubleUnderlined),
        Attributes::REVERSE => Some(CtAttribute::Reverse),
        Attributes::BLINK => Some(CtAttribute::SlowBlink),
        Attributes::BOLD => Some(CtAttribute::Bold),
        Attributes::DIM => Some(CtAttribute::Dim),
        Attributes::ITALIC => Some(CtAttribute::Italic),
        Attributes::STRIKETHROUGH => Some(CtAttribute::CrossedOut),
        // final_* attributes are Kakoune-internal face composition hints; skip them
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_convert_color_default() {
        assert_eq!(convert_color(Color::Default), CtColor::Reset);
    }

    #[test]
    fn test_convert_color_rgb() {
        assert_eq!(
            convert_color(Color::Rgb {
                r: 255,
                g: 0,
                b: 128
            }),
            CtColor::Rgb {
                r: 255,
                g: 0,
                b: 128
            }
        );
    }

    #[test]
    fn test_convert_color_named() {
        assert_eq!(
            convert_color(Color::Named(NamedColor::Red)),
            CtColor::DarkRed
        );
        assert_eq!(
            convert_color(Color::Named(NamedColor::BrightRed)),
            CtColor::Red
        );
    }

    #[test]
    fn test_convert_attribute() {
        assert_eq!(convert_attribute(Attributes::BOLD), Some(CtAttribute::Bold));
        assert_eq!(
            convert_attribute(Attributes::ITALIC),
            Some(CtAttribute::Italic)
        );
        assert_eq!(
            convert_attribute(Attributes::REVERSE),
            Some(CtAttribute::Reverse)
        );
        // final_* attributes should be filtered out (None)
        assert_eq!(convert_attribute(Attributes::FINAL_FG), None);
        assert_eq!(convert_attribute(Attributes::FINAL_BG), None);
        assert_eq!(convert_attribute(Attributes::FINAL_ATTR), None);
    }
}
