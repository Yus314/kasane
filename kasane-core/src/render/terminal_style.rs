//! Terminal-friendly projection of `Style` / `Face` (ADR-031 Phase 3, design δ).
//!
//! `TerminalStyle` is the SGR-emit-ready, [`Copy`]-able compact projection
//! of a styled atom. It is the canonical representation stored on
//! [`crate::render::Cell`] under the design-δ migration: the cell-grid
//! is the rasterised TUI output and the renderer must already have
//! decided which terminal-expressible attributes to emit, so storing the
//! richer [`Style`] would be wasted memory and CPU.
//!
//! Continuous fields the terminal cannot represent (`FontWeight` axis,
//! `font_variations`, `letter_spacing`, `bidi_override`) collapse here
//! into discrete attributes (`bold`, `italic`, etc.).
//!
//! Two construction paths:
//!
//! - [`TerminalStyle::from_face`] — bridge from the legacy [`Face`]
//!   representation. Used while [`Face`] is still the upstream protocol
//!   shape; retires when Phase B3 removes [`Face`] entirely.
//! - [`TerminalStyle::from_style`] — direct projection from the post-resolve
//!   [`Style`]. Used by call sites that already hold a [`Style`] (atom
//!   conversion, plugin output).

use crate::protocol::{
    Attributes, Brush, Color, DecorationStyle, Face, FontSlant, FontWeight, Style,
};

/// Discrete underline kind that terminals can render.
///
/// Mirrors crossterm's underline-style escape sequences. `None` means
/// no underline; `Solid` is the historical SGR 4; the curly / dotted /
/// dashed / double variants are kitty / vte extensions accepted by
/// modern terminals.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum UnderlineKind {
    #[default]
    None,
    Solid,
    Curly,
    Dotted,
    Dashed,
    Double,
}

/// Render-ready projection of `Style` / `Face` for terminal SGR emission.
///
/// All fields map 1:1 to crossterm calls:
///
/// - `fg` / `bg` / `underline_color` → `SetForegroundColor` /
///   `SetBackgroundColor` / `SetUnderlineColor`
/// - `bold` → `SetAttribute(Bold)` (`FontWeight ≥ 600` collapses to bold)
/// - `italic` → `SetAttribute(Italic)` (`FontSlant::Italic | Oblique`
///   both collapse to italic — terminals cannot represent the
///   distinction)
/// - `dim` / `blink` / `reverse` / `strikethrough` → matching SGR codes
/// - `underline` → one of `SetAttribute(Underlined / Undercurled /
///   Underdotted / Underdashed / DoubleUnderlined)`
///
/// Fields dropped from upstream `Style`: `font_features`,
/// `font_variations`, `letter_spacing`, `bidi_override`. None of these
/// have terminal equivalents.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct TerminalStyle {
    pub fg: Color,
    pub bg: Color,
    pub underline_color: Color,
    pub underline: UnderlineKind,
    pub bold: bool,
    pub italic: bool,
    pub dim: bool,
    pub blink: bool,
    pub reverse: bool,
    pub strikethrough: bool,
}

impl TerminalStyle {
    /// **Wire-format conversion only.** Build from a Kakoune wire-format
    /// [`Face`]. Splits the [`Attributes`] bitflag into individual
    /// booleans and maps the underline-style flag (UNDERLINE /
    /// CURLY_UNDERLINE / etc.) to the [`UnderlineKind`] enum. `final_*`
    /// resolution flags are dropped — they are Kakoune-internal and
    /// have no terminal meaning. Production paint code uses
    /// [`Self::from_style`]; this constructor exists for the protocol
    /// parser bridge and test fixtures that declare style in `Face`
    /// shape.
    #[doc(hidden)]
    pub fn from_face(face: &Face) -> Self {
        let attrs = face.attributes;
        let underline = if attrs.contains(Attributes::CURLY_UNDERLINE) {
            UnderlineKind::Curly
        } else if attrs.contains(Attributes::DOTTED_UNDERLINE) {
            UnderlineKind::Dotted
        } else if attrs.contains(Attributes::DASHED_UNDERLINE) {
            UnderlineKind::Dashed
        } else if attrs.contains(Attributes::DOUBLE_UNDERLINE) {
            UnderlineKind::Double
        } else if attrs.contains(Attributes::UNDERLINE) {
            UnderlineKind::Solid
        } else {
            UnderlineKind::None
        };
        Self {
            fg: face.fg,
            bg: face.bg,
            underline_color: face.underline,
            underline,
            bold: attrs.contains(Attributes::BOLD),
            italic: attrs.contains(Attributes::ITALIC),
            dim: attrs.contains(Attributes::DIM),
            blink: attrs.contains(Attributes::BLINK),
            reverse: attrs.contains(Attributes::REVERSE),
            strikethrough: attrs.contains(Attributes::STRIKETHROUGH),
        }
    }

    /// Build directly from a resolved [`Style`].
    ///
    /// `font_weight ≥ FontWeight::SEMI_BOLD (600)` collapses to bold;
    /// `font_slant` of either `Italic` or `Oblique` collapses to italic.
    /// Variable-axis settings, font features, letter spacing, and
    /// bidi override are dropped.
    pub fn from_style(style: &Style) -> Self {
        let underline = match style.underline.as_ref().map(|d| d.style) {
            None => UnderlineKind::None,
            Some(DecorationStyle::Solid) => UnderlineKind::Solid,
            Some(DecorationStyle::Curly) => UnderlineKind::Curly,
            Some(DecorationStyle::Dotted) => UnderlineKind::Dotted,
            Some(DecorationStyle::Dashed) => UnderlineKind::Dashed,
            Some(DecorationStyle::Double) => UnderlineKind::Double,
        };
        let underline_color = style
            .underline
            .as_ref()
            .map(|d| brush_to_color(d.color))
            .unwrap_or(Color::Default);
        Self {
            fg: brush_to_color(style.fg),
            bg: brush_to_color(style.bg),
            underline_color,
            underline,
            bold: style.font_weight.0 >= FontWeight::SEMI_BOLD.0,
            italic: matches!(style.font_slant, FontSlant::Italic | FontSlant::Oblique),
            dim: style.dim,
            blink: style.blink,
            reverse: style.reverse,
            strikethrough: style.strikethrough.is_some(),
        }
    }
}

/// Project a kasane-core `Brush` to the terminal-side `Color` enum.
///
/// `Brush::Default` becomes `Color::Default` (terminal default colour).
fn brush_to_color(brush: Brush) -> Color {
    match brush {
        Brush::Default => Color::Default,
        Brush::Named(n) => Color::Named(n),
        Brush::Solid([r, g, b, _a]) => Color::Rgb { r, g, b },
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::protocol::{Attributes, Color, Face, NamedColor, TextDecoration};

    #[test]
    fn from_face_default_is_default() {
        let ts = TerminalStyle::from_face(&Face::default());
        assert_eq!(ts, TerminalStyle::default());
    }

    #[test]
    fn from_face_splits_attribute_bitflag() {
        let face = Face {
            fg: Color::Named(NamedColor::Red),
            attributes: Attributes::BOLD | Attributes::ITALIC | Attributes::REVERSE,
            ..Face::default()
        };
        let ts = TerminalStyle::from_face(&face);
        assert!(ts.bold);
        assert!(ts.italic);
        assert!(ts.reverse);
        assert!(!ts.dim);
        assert!(!ts.blink);
        assert_eq!(ts.fg, Color::Named(NamedColor::Red));
    }

    #[test]
    fn from_face_underline_style_priority() {
        let face = Face {
            attributes: Attributes::UNDERLINE | Attributes::CURLY_UNDERLINE,
            ..Face::default()
        };
        let ts = TerminalStyle::from_face(&face);
        assert_eq!(ts.underline, UnderlineKind::Curly);
    }

    #[test]
    fn from_face_double_underline() {
        let face = Face {
            attributes: Attributes::DOUBLE_UNDERLINE,
            ..Face::default()
        };
        let ts = TerminalStyle::from_face(&face);
        assert_eq!(ts.underline, UnderlineKind::Double);
    }

    #[test]
    fn from_face_no_underline() {
        let ts = TerminalStyle::from_face(&Face::default());
        assert_eq!(ts.underline, UnderlineKind::None);
    }

    #[test]
    fn from_face_drops_final_attrs() {
        let face = Face {
            attributes: Attributes::FINAL_FG | Attributes::FINAL_BG | Attributes::FINAL_ATTR,
            ..Face::default()
        };
        let ts = TerminalStyle::from_face(&face);
        assert!(!ts.bold && !ts.italic && !ts.dim && !ts.blink && !ts.reverse && !ts.strikethrough);
    }

    #[test]
    fn from_style_bold_threshold() {
        let mut s = Style::default();
        s.font_weight = FontWeight(599);
        assert!(!TerminalStyle::from_style(&s).bold);
        s.font_weight = FontWeight::SEMI_BOLD;
        assert!(TerminalStyle::from_style(&s).bold);
        s.font_weight = FontWeight::BOLD;
        assert!(TerminalStyle::from_style(&s).bold);
    }

    #[test]
    fn from_style_oblique_collapses_to_italic() {
        let mut s = Style::default();
        s.font_slant = FontSlant::Oblique;
        assert!(TerminalStyle::from_style(&s).italic);
        s.font_slant = FontSlant::Italic;
        assert!(TerminalStyle::from_style(&s).italic);
        s.font_slant = FontSlant::Normal;
        assert!(!TerminalStyle::from_style(&s).italic);
    }

    #[test]
    fn from_style_curly_underline_propagates() {
        let s = Style {
            underline: Some(TextDecoration {
                style: DecorationStyle::Curly,
                color: Brush::Named(NamedColor::Red),
                thickness: None,
            }),
            ..Style::default()
        };
        let ts = TerminalStyle::from_style(&s);
        assert_eq!(ts.underline, UnderlineKind::Curly);
        assert_eq!(ts.underline_color, Color::Named(NamedColor::Red));
    }

    #[test]
    fn from_style_strikethrough_collapses_to_bool() {
        let s = Style {
            strikethrough: Some(TextDecoration::default()),
            ..Style::default()
        };
        assert!(TerminalStyle::from_style(&s).strikethrough);
    }

    #[test]
    fn from_face_and_from_style_agree_via_to_face() {
        // Invariant: for any `Style`, projecting via the legacy
        // Cell.face: Face path (Style → Face → TerminalStyle) yields the
        // same TerminalStyle as the direct path (Style → TerminalStyle).
        // Pinning this lets the design-δ migration be a behavioural no-op.
        let cases = vec![
            Style::default(),
            Style {
                fg: Brush::Named(NamedColor::Red),
                bg: Brush::Named(NamedColor::Blue),
                font_weight: FontWeight::BOLD,
                font_slant: FontSlant::Italic,
                blink: true,
                reverse: true,
                dim: true,
                strikethrough: Some(TextDecoration::default()),
                underline: Some(TextDecoration {
                    style: DecorationStyle::Curly,
                    color: Brush::Named(NamedColor::Yellow),
                    thickness: None,
                }),
                ..Style::default()
            },
        ];
        for s in cases {
            let via_face = TerminalStyle::from_face(&s.to_face());
            let direct = TerminalStyle::from_style(&s);
            assert_eq!(
                via_face, direct,
                "TerminalStyle::from_face(style.to_face()) must equal TerminalStyle::from_style(style); \
                 mismatch for {s:?}"
            );
        }
    }
}
