//! SGR (Select Graphic Rendition) helper functions for converting kasane
//! protocol types to crossterm terminal escape sequences.

use crossterm::{
    queue,
    style::{
        Attribute as CtAttribute, Color as CtColor, SetAttribute, SetBackgroundColor,
        SetForegroundColor, SetUnderlineColor,
    },
};
use kasane_core::protocol::{Attributes, Color, Face, NamedColor};

/// Emit only the SGR codes that differ between `old` and `new` faces.
/// When `old` is None (first cell), emits a full reset + set.
pub fn emit_sgr_diff(buf: &mut Vec<u8>, old: Option<&Face>, new: &Face) -> anyhow::Result<()> {
    match old {
        None => {
            // First cell: full reset + set
            queue!(buf, SetAttribute(CtAttribute::Reset))?;
            queue!(
                buf,
                SetForegroundColor(convert_color(new.fg)),
                SetBackgroundColor(convert_color(new.bg))
            )?;
            if new.underline != Color::Default {
                queue!(buf, SetUnderlineColor(convert_color(new.underline)))?;
            }
            for attr in new.attributes.iter() {
                if let Some(ct_attr) = convert_attribute(attr) {
                    queue!(buf, SetAttribute(ct_attr))?;
                }
            }
        }
        Some(old) => {
            // Attributes changed: we must reset and re-apply since there's no
            // individual "unset bold" etc. that works reliably across terminals.
            if old.attributes != new.attributes {
                queue!(buf, SetAttribute(CtAttribute::Reset))?;
                queue!(
                    buf,
                    SetForegroundColor(convert_color(new.fg)),
                    SetBackgroundColor(convert_color(new.bg))
                )?;
                if new.underline != Color::Default {
                    queue!(buf, SetUnderlineColor(convert_color(new.underline)))?;
                }
                for attr in new.attributes.iter() {
                    if let Some(ct_attr) = convert_attribute(attr) {
                        queue!(buf, SetAttribute(ct_attr))?;
                    }
                }
            } else {
                // Same attributes — only emit changed colors
                if old.fg != new.fg {
                    queue!(buf, SetForegroundColor(convert_color(new.fg)))?;
                }
                if old.bg != new.bg {
                    queue!(buf, SetBackgroundColor(convert_color(new.bg)))?;
                }
                if old.underline != new.underline {
                    queue!(buf, SetUnderlineColor(convert_color(new.underline)))?;
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

/// Convert a kasane Attributes flag to a crossterm Attribute.
/// Returns None for Kakoune-internal attributes (final_*) that have no terminal equivalent.
pub fn convert_attribute(attr: Attributes) -> Option<CtAttribute> {
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
