//! SGR (Select Graphic Rendition) helper functions for converting
//! [`crate::terminal_style::TerminalStyle`] to crossterm terminal escape
//! sequences.

use crossterm::{
    queue,
    style::{
        Attribute as CtAttribute, Color as CtColor, SetAttribute, SetBackgroundColor,
        SetForegroundColor, SetUnderlineColor,
    },
};
use kasane_core::protocol::{Color, NamedColor};

use crate::terminal_style::{TerminalStyle, UnderlineKind};

/// Emit a full reset followed by all colors and attributes for `style`.
fn emit_full_style(buf: &mut Vec<u8>, style: &TerminalStyle) -> anyhow::Result<()> {
    queue!(buf, SetAttribute(CtAttribute::Reset))?;
    queue!(
        buf,
        SetForegroundColor(convert_color(style.fg)),
        SetBackgroundColor(convert_color(style.bg))
    )?;
    if style.underline_color != Color::Default {
        queue!(buf, SetUnderlineColor(convert_color(style.underline_color)))?;
    }
    apply_style_attributes(buf, style)?;
    Ok(())
}

/// Apply the attribute booleans of `style` as crossterm `SetAttribute`
/// calls. Caller is responsible for resetting first when transitioning
/// from a different attribute set.
fn apply_style_attributes(buf: &mut Vec<u8>, style: &TerminalStyle) -> anyhow::Result<()> {
    if style.bold {
        queue!(buf, SetAttribute(CtAttribute::Bold))?;
    }
    if style.italic {
        queue!(buf, SetAttribute(CtAttribute::Italic))?;
    }
    if style.dim {
        queue!(buf, SetAttribute(CtAttribute::Dim))?;
    }
    if style.blink {
        queue!(buf, SetAttribute(CtAttribute::SlowBlink))?;
    }
    if style.reverse {
        queue!(buf, SetAttribute(CtAttribute::Reverse))?;
    }
    if style.strikethrough {
        queue!(buf, SetAttribute(CtAttribute::CrossedOut))?;
    }
    let underline_attr = match style.underline {
        UnderlineKind::None => None,
        UnderlineKind::Solid => Some(CtAttribute::Underlined),
        UnderlineKind::Curly => Some(CtAttribute::Undercurled),
        UnderlineKind::Dotted => Some(CtAttribute::Underdotted),
        UnderlineKind::Dashed => Some(CtAttribute::Underdashed),
        UnderlineKind::Double => Some(CtAttribute::DoubleUnderlined),
    };
    if let Some(attr) = underline_attr {
        queue!(buf, SetAttribute(attr))?;
    }
    Ok(())
}

/// True iff the two styles share every attribute (colour fields excluded).
///
/// When attributes match, [`emit_sgr_diff_style`] can avoid the full
/// reset + re-apply path and emit only the changed colour codes.
fn attributes_eq(a: &TerminalStyle, b: &TerminalStyle) -> bool {
    a.bold == b.bold
        && a.italic == b.italic
        && a.dim == b.dim
        && a.blink == b.blink
        && a.reverse == b.reverse
        && a.strikethrough == b.strikethrough
        && a.underline == b.underline
}

/// Emit only the SGR codes that differ between `old` and `new` styles.
///
/// When `old` is `None` (first cell), emits a full reset + set. When the
/// attribute booleans differ from `old`, emits a reset + full re-set
/// because there is no portable "unset bold" SGR code. Otherwise emits
/// just the colour deltas.
pub fn emit_sgr_diff_style(
    buf: &mut Vec<u8>,
    old: Option<&TerminalStyle>,
    new: &TerminalStyle,
) -> anyhow::Result<()> {
    match old {
        None => {
            emit_full_style(buf, new)?;
        }
        Some(old) => {
            if !attributes_eq(old, new) {
                emit_full_style(buf, new)?;
            } else {
                if old.fg != new.fg {
                    queue!(buf, SetForegroundColor(convert_color(new.fg)))?;
                }
                if old.bg != new.bg {
                    queue!(buf, SetBackgroundColor(convert_color(new.bg)))?;
                }
                if old.underline_color != new.underline_color {
                    queue!(buf, SetUnderlineColor(convert_color(new.underline_color)))?;
                }
            }
        }
    }
    Ok(())
}

pub fn convert_color(color: Color) -> CtColor {
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
    fn emit_sgr_diff_style_first_cell_emits_full_reset() {
        let style = TerminalStyle::default();
        let mut buf = Vec::new();
        emit_sgr_diff_style(&mut buf, None, &style).unwrap();
        // Should start with a Reset SGR (\x1b[0m)
        assert!(
            buf.windows(4).any(|w| w == b"\x1b[0m"),
            "first-cell emit must include reset; got {buf:?}"
        );
    }

    #[test]
    fn emit_sgr_diff_style_color_only_change_skips_reset() {
        let s1 = TerminalStyle {
            fg: Color::Named(NamedColor::Red),
            bold: true,
            ..TerminalStyle::default()
        };
        let s2 = TerminalStyle {
            fg: Color::Named(NamedColor::Blue),
            bold: true,
            ..TerminalStyle::default()
        };
        let mut buf = Vec::new();
        emit_sgr_diff_style(&mut buf, Some(&s1), &s2).unwrap();
        // Same attributes — no full reset emitted.
        assert!(
            !buf.windows(4).any(|w| w == b"\x1b[0m"),
            "color-only change must not include reset; got {buf:?}"
        );
    }

    #[test]
    fn emit_sgr_diff_style_attribute_change_emits_reset() {
        let s1 = TerminalStyle {
            bold: true,
            ..TerminalStyle::default()
        };
        let s2 = TerminalStyle {
            italic: true,
            ..TerminalStyle::default()
        };
        let mut buf = Vec::new();
        emit_sgr_diff_style(&mut buf, Some(&s1), &s2).unwrap();
        assert!(
            buf.windows(4).any(|w| w == b"\x1b[0m"),
            "attribute change must include reset; got {buf:?}"
        );
    }
}
