//! Terminal-friendly projection of `Style` / `Face` (ADR-031 Phase 3).
//!
//! `TerminalStyle` is the SGR-emit-ready intermediate consumed by
//! [`crate::sgr::emit_sgr_diff`]. It collapses continuous fields that
//! terminals cannot represent (`FontWeight` axis, `font_variations`,
//! `letter_spacing`, `bidi_override`) into the discrete attributes
//! crossterm exposes (`bold`, `italic`, etc.).
//!
//! Two construction paths exist:
//!
//! - [`TerminalStyle::from_face`] — current call site path. The central
//!   [`kasane_core::render::Cell`] still stores [`Face`]; backend.rs
//!   converts via this constructor just before SGR emission. This
//!   replaces the previous "sgr.rs consumes Face directly" arrangement
//!   so that emission stays decoupled from the upstream cell
//!   representation.
//! - [`TerminalStyle::from_style`] — Phase 3 Step 2 forward path.
//!   Unblocks a future `Cell.style: Style` migration: when the central
//!   cell type carries `Style`, backend.rs will switch to this
//!   constructor and the upstream `Style → Face` conversion currently
//!   paid at paint time can retire (ADR-031 Phase 3 closes).
//!
//! See [ADR-031](../../../docs/decisions.md) §Phase 3 and the
//! `decisions.md` Style → TerminalStyle projection table.

use kasane_core::protocol::{
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
    /// Build from a legacy [`Face`] (current Cell-stored type).
    ///
    /// Splits the [`Attributes`] bitflag into individual booleans and
    /// maps the underline-style flag (UNDERLINE / CURLY_UNDERLINE / etc.)
    /// to the [`UnderlineKind`] enum. `final_*` resolution flags are
    /// dropped — they are Kakoune-internal and have no terminal meaning.
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
    /// This is the Phase 3 Step 2 forward path: once
    /// [`kasane_core::render::Cell`] stores `Style` instead of `Face`,
    /// backend.rs switches to this constructor and the upstream
    /// `Style → Face` conversion (currently in
    /// `kasane-core/src/render/view/mod.rs` and elsewhere) retires.
    /// Until then this constructor is exercised only by tests.
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
/// Mirrors the private `color_from_brush` helper in
/// `kasane-core/src/protocol/style.rs`; kept inline because the helper
/// is not part of kasane-core's public API.
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
    use kasane_core::protocol::{Attributes, Color, Face, NamedColor, TextDecoration};

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
        // Curly takes precedence over solid when both bits accidentally set.
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
        // FINAL_FG / FINAL_BG / FINAL_ATTR are Kakoune-internal; no terminal effect.
        let face = Face {
            attributes: Attributes::FINAL_FG | Attributes::FINAL_BG | Attributes::FINAL_ATTR,
            ..Face::default()
        };
        let ts = TerminalStyle::from_face(&face);
        // None of bold/italic/dim/blink/reverse/strikethrough should be set.
        assert!(!ts.bold && !ts.italic && !ts.dim && !ts.blink && !ts.reverse && !ts.strikethrough);
    }

    #[test]
    fn from_style_bold_threshold() {
        // FontWeight::SEMI_BOLD (600) is the lower bound for bold.
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
        // The central invariant: for any `Style`, projecting via the
        // legacy Cell.face: Face path (Style → Face → TerminalStyle)
        // yields the same TerminalStyle as the direct path
        // (Style → TerminalStyle). This keeps Phase 3 Step 2 a
        // behavioural no-op when migrating Cell to store Style.
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
                 mismatch indicates a Phase 3 Step 2 migration would change behaviour for {s:?}"
            );
        }
    }
}
