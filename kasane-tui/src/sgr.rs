//! SGR (Select Graphic Rendition) helper functions for converting
//! [`crate::terminal_style::TerminalStyle`] to crossterm terminal escape
//! sequences.
//!
//! The previous Face-consuming API ([`emit_sgr_diff(buf, Option<&Face>,
//! &Face)`]) is preserved as a thin shim that converts via
//! [`TerminalStyle::from_face`] and dispatches to the new
//! [`emit_sgr_diff_style`]. This keeps backend.rs call sites
//! source-stable while ADR-031 Phase 3 Step 1 ships the type-system
//! foundation; Step 2 will switch backend.rs to the
//! `TerminalStyle`-direct call once `Cell` migrates to `Style`.

use crossterm::{
    queue,
    style::{
        Attribute as CtAttribute, Color as CtColor, SetAttribute, SetBackgroundColor,
        SetForegroundColor, SetUnderlineColor,
    },
};
use kasane_core::protocol::{Attributes, Color, Face, NamedColor};

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

/// Emit only the SGR codes that differ between `old` and `new` faces.
///
/// Legacy entry point. Internally projects to [`TerminalStyle`] via
/// [`TerminalStyle::from_face`] and dispatches to
/// [`emit_sgr_diff_style`]. Will retire when `Cell` migrates to
/// store `Style` directly (ADR-031 Phase 3 Step 2).
pub fn emit_sgr_diff(buf: &mut Vec<u8>, old: Option<&Face>, new: &Face) -> anyhow::Result<()> {
    let new_ts = TerminalStyle::from_face(new);
    let old_ts = old.map(TerminalStyle::from_face);
    emit_sgr_diff_style(buf, old_ts.as_ref(), &new_ts)
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
///
/// Retained for plug-in / external consumers; the SGR-emit path now
/// goes through [`TerminalStyle`] and does not call this directly.
pub fn convert_attribute(attr: Attributes) -> Option<CtAttribute> {
    match attr {
        Attributes::UNDERLINE => Some(CtAttribute::Underlined),
        Attributes::CURLY_UNDERLINE => Some(CtAttribute::Undercurled),
        Attributes::DOUBLE_UNDERLINE => Some(CtAttribute::DoubleUnderlined),
        Attributes::DOTTED_UNDERLINE => Some(CtAttribute::Underdotted),
        Attributes::DASHED_UNDERLINE => Some(CtAttribute::Underdashed),
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

    #[test]
    fn emit_sgr_diff_legacy_face_matches_new_path() {
        // The Face-consuming shim must produce byte-identical output to
        // calling emit_sgr_diff_style with the projected TerminalStyle.
        // This pins the Phase 3 Step 1 invariant: introducing
        // TerminalStyle does not change observable terminal behaviour.
        let face_old = Face {
            fg: Color::Named(NamedColor::Red),
            attributes: Attributes::BOLD,
            ..Face::default()
        };
        let face_new = Face {
            fg: Color::Named(NamedColor::Blue),
            attributes: Attributes::BOLD | Attributes::ITALIC,
            ..Face::default()
        };

        let mut via_face = Vec::new();
        emit_sgr_diff(&mut via_face, Some(&face_old), &face_new).unwrap();

        let ts_old = TerminalStyle::from_face(&face_old);
        let ts_new = TerminalStyle::from_face(&face_new);
        let mut via_style = Vec::new();
        emit_sgr_diff_style(&mut via_style, Some(&ts_old), &ts_new).unwrap();

        assert_eq!(
            via_face, via_style,
            "Face shim must match TerminalStyle direct path byte-for-byte"
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
