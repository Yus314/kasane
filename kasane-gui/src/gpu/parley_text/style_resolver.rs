//! Kasane [`Style`] → Parley [`StyleProperty`] conversion (ADR-031, Phase 6).
//!
//! Pushed into `parley::RangedBuilder` as a sequence of properties at shape
//! time. Each call to [`apply_style_to_builder`] (Phase 7) emits roughly
//! `O(non_default_fields(style))` properties.
//!
//! Inheritance: `Style::fg = Brush::Default` is resolved upstream by
//! [`kasane_core::protocol::resolve_style`] before we get here, so the
//! Parley-side brush is always concrete. This separation lets the L1
//! LayoutCache key on the resolved brush directly.

use std::sync::Arc;

use kasane_core::protocol::{
    Brush as KBrush, DecorationStyle, FontFeatures as KFontFeatures, FontSlant,
    FontVariation as KFontVariation, FontWeight as KFontWeight, NamedColor, Style, TextDecoration,
};
use parley::{FontStyle as PFontStyle, FontWeight as PFontWeight};

use super::Brush;

/// Convert a Kasane [`Brush`](KBrush) to the layout-time GPU brush.
///
/// `Brush::Default` is unexpected here: the caller must resolve inheritance
/// against a base style before invoking this. We fall back to opaque black
/// to avoid a panic, with a debug assert to catch the bug in tests.
pub fn brush_from_kasane(brush: KBrush) -> Brush {
    match brush {
        KBrush::Default => {
            debug_assert!(false, "unresolved Brush::Default reached parley layer");
            Brush::opaque(0, 0, 0)
        }
        KBrush::Solid([r, g, b, a]) => Brush::rgba(r, g, b, a),
        KBrush::Named(n) => named_to_brush(n),
    }
}

/// Map a 16-colour ANSI [`NamedColor`] to a fixed RGB. The values match
/// [`NamedColor::to_rgb`] in `kasane-core/src/protocol/color.rs`; we duplicate
/// them here only to avoid a kasane-core round-trip in the hot path.
fn named_to_brush(n: NamedColor) -> Brush {
    let (r, g, b) = match n {
        NamedColor::Black => (0, 0, 0),
        NamedColor::Red => (205, 0, 0),
        NamedColor::Green => (0, 205, 0),
        NamedColor::Yellow => (205, 205, 0),
        NamedColor::Blue => (0, 0, 238),
        NamedColor::Magenta => (205, 0, 205),
        NamedColor::Cyan => (0, 205, 205),
        NamedColor::White => (229, 229, 229),
        NamedColor::BrightBlack => (127, 127, 127),
        NamedColor::BrightRed => (255, 0, 0),
        NamedColor::BrightGreen => (0, 255, 0),
        NamedColor::BrightYellow => (255, 255, 0),
        NamedColor::BrightBlue => (92, 92, 255),
        NamedColor::BrightMagenta => (255, 0, 255),
        NamedColor::BrightCyan => (0, 255, 255),
        NamedColor::BrightWhite => (255, 255, 255),
    };
    Brush::opaque(r, g, b)
}

/// Convert Kasane's continuous [`FontWeight`](KFontWeight) (100..=900) to
/// Parley's [`PFontWeight`]. The numeric values are identical; this is a
/// thin wrapper to keep the boundary explicit.
#[inline]
pub fn weight_from_kasane(weight: KFontWeight) -> PFontWeight {
    PFontWeight::new(weight.0 as f32)
}

/// Convert Kasane's [`FontSlant`] to Parley's [`PFontStyle`].
#[inline]
pub fn slant_from_kasane(slant: FontSlant) -> PFontStyle {
    match slant {
        FontSlant::Normal => PFontStyle::Normal,
        FontSlant::Italic => PFontStyle::Italic,
        FontSlant::Oblique => PFontStyle::Oblique(None),
    }
}

/// Outcome of decoration conversion, ready for the call site to feed into
/// `StyleProperty::Underline` / `StyleProperty::Strikethrough` and the
/// matching brush / size properties.
pub struct DecorationProperties {
    pub enabled: bool,
    pub brush: Option<Brush>,
    pub size: Option<f32>,
}

/// Convert a Kasane [`TextDecoration`] into the per-axis properties Parley
/// expects. Returns disabled / no-brush / no-size when the decoration is
/// `None` so the caller can write a single uniform `apply()` loop.
///
/// Note: at Phase 6 we collapse the four "non-solid" decoration styles
/// (Curly / Dotted / Dashed / Double) to plain underline at the Parley layer
/// because Parley does not yet expose styled underline kinds. The actual
/// decoration style is preserved on the `Style` side and re-applied in the
/// quad pipeline (Phase 10).
pub fn decoration_properties(deco: Option<TextDecoration>) -> DecorationProperties {
    match deco {
        None => DecorationProperties {
            enabled: false,
            brush: None,
            size: None,
        },
        Some(d) => DecorationProperties {
            enabled: true,
            brush: match d.color {
                KBrush::Default => None, // inherit text colour at the renderer
                other => Some(brush_from_kasane(other)),
            },
            size: d.thickness,
        },
    }
}

/// Helper kept for future call sites: returns whether a decoration kind
/// requires special quad-pipeline handling rather than relying on the Parley
/// straight underline.
#[inline]
pub fn decoration_needs_custom_quad(style: DecorationStyle) -> bool {
    !matches!(style, DecorationStyle::Solid)
}

/// Slant axis projected for Parley. Italic and oblique are mutually
/// exclusive in OpenType / CSS; modelling them as a single enum makes the
/// invalid `italic && oblique` combination unrepresentable.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SlantKind {
    Normal,
    Italic,
    Oblique,
}

/// Resolved style ready to be pushed into `parley::RangedBuilder`.
///
/// Stored types are **kasane-native** (`FontFeatures` bitset, `Arc<[FontVariation]>`)
/// rather than `parley::FontFeature` / `parley::FontVariation` slices.
/// Reasons:
///
/// - Kasane types implement `Hash` and `PartialEq`, so the L1
///   [`super::layout_cache::LayoutCache`] hash and the run-merge
///   equality check in [`super::styled_line::StyledLine::from_atoms`]
///   stay derive-trivial.
/// - The conversion to `parley::FontFeature` / `parley::FontVariation`
///   (at most 5 features × 1 toggle bit, typically 0–2 variations) is
///   small enough not to be a hot-path concern. Done in
///   [`super::shaper::shape_line`].
#[derive(Debug, Clone, PartialEq)]
pub struct ResolvedParleyStyle {
    pub fg: Brush,
    pub bg: Brush,
    pub weight: f32,
    pub slant: SlantKind,
    pub letter_spacing: f32,
    pub underline: DecorationKind,
    pub strikethrough: DecorationKind,
    /// OpenType feature toggles (calt / clig / dlig / hlig / liga / zero).
    /// Empty bitset (`0`) means "use font defaults".
    pub font_features: KFontFeatures,
    /// Variable-font axis settings. Empty slice in the steady-state
    /// (kasane fonts are mostly static); `Arc` makes per-run cloning
    /// cheap when a plugin assigns the same axis values to many atoms.
    pub font_variations: Arc<[KFontVariation]>,
}

/// Decoration projected to what is actually drawn. `Custom` carries the
/// original style so the quad pipeline can render curly / dotted / dashed /
/// double underlines that Parley itself does not support.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum DecorationKind {
    None,
    Solid {
        brush: Brush,
        thickness: Option<f32>,
    },
    Custom {
        style: DecorationStyle,
        brush: Brush,
        thickness: Option<f32>,
    },
}

/// Project a [`Style`] (with brushes already resolved against a base context)
/// into [`ResolvedParleyStyle`].
///
/// Panics in debug builds if `style.fg` / `style.bg` are still
/// `Brush::Default`; this is a programming error caught early.
pub fn resolve_for_parley(style: &Style, fallback_text_color: Brush) -> ResolvedParleyStyle {
    let fg = match style.fg {
        KBrush::Default => fallback_text_color,
        other => brush_from_kasane(other),
    };
    let bg = match style.bg {
        KBrush::Default => Brush::default(),
        other => brush_from_kasane(other),
    };

    ResolvedParleyStyle {
        fg,
        bg,
        weight: style.font_weight.0 as f32,
        slant: match style.font_slant {
            FontSlant::Normal => SlantKind::Normal,
            FontSlant::Italic => SlantKind::Italic,
            FontSlant::Oblique => SlantKind::Oblique,
        },
        letter_spacing: style.letter_spacing,
        underline: project_decoration(style.underline, fg),
        strikethrough: project_decoration(style.strikethrough, fg),
        font_features: style.font_features,
        // Empty `Vec`s share an `Arc::from(&[][..])`-allocated zero-length
        // slice, so the steady-state cost is one Arc clone per run.
        font_variations: Arc::from(style.font_variations.as_slice()),
    }
}

/// Convert a kasane [`FontFeatures`](KFontFeatures) bitset into the
/// `parley::FontFeature` list the shaper pushes via
/// [`StyleProperty::FontFeatures`](parley::StyleProperty::FontFeatures).
///
/// Each set bit produces one or two OpenType features. The mapping mirrors
/// the docstring on `FontFeatures` in `kasane-core/src/protocol/style.rs`:
///
/// | bit                     | OpenType tag(s) |
/// |-------------------------|-----------------|
/// | `STANDARD_LIGATURES`    | `calt`, `clig`  |
/// | `DISCRETIONARY_LIGATURES` | `dlig`        |
/// | `HISTORICAL_LIGATURES`  | `hlig`          |
/// | `COMMON_LIGATURES`      | `liga`          |
/// | `SLASHED_ZERO`          | `zero`          |
///
/// Returns an empty `Vec` for an empty bitset; the caller skips the
/// `StyleProperty::FontFeatures` push in that case so Parley falls back to
/// the font's default feature set.
pub fn kasane_features_to_parley(features: KFontFeatures) -> Vec<parley::FontFeature> {
    let mut out = Vec::new();
    if features.has(KFontFeatures::STANDARD_LIGATURES) {
        out.push(parley::FontFeature::new(
            parley::setting::Tag::new(b"calt"),
            1,
        ));
        out.push(parley::FontFeature::new(
            parley::setting::Tag::new(b"clig"),
            1,
        ));
    }
    if features.has(KFontFeatures::DISCRETIONARY_LIGATURES) {
        out.push(parley::FontFeature::new(
            parley::setting::Tag::new(b"dlig"),
            1,
        ));
    }
    if features.has(KFontFeatures::HISTORICAL_LIGATURES) {
        out.push(parley::FontFeature::new(
            parley::setting::Tag::new(b"hlig"),
            1,
        ));
    }
    if features.has(KFontFeatures::COMMON_LIGATURES) {
        out.push(parley::FontFeature::new(
            parley::setting::Tag::new(b"liga"),
            1,
        ));
    }
    if features.has(KFontFeatures::SLASHED_ZERO) {
        out.push(parley::FontFeature::new(
            parley::setting::Tag::new(b"zero"),
            1,
        ));
    }
    out
}

/// Convert a slice of kasane [`FontVariation`](KFontVariation) into the
/// `parley::FontVariation` list the shaper pushes via
/// [`StyleProperty::FontVariations`](parley::StyleProperty::FontVariations).
pub fn kasane_variations_to_parley(variations: &[KFontVariation]) -> Vec<parley::FontVariation> {
    variations
        .iter()
        .map(|v| parley::FontVariation::new(parley::setting::Tag::new(&v.tag), v.value))
        .collect()
}

fn project_decoration(deco: Option<TextDecoration>, fg: Brush) -> DecorationKind {
    match deco {
        None => DecorationKind::None,
        Some(d) => {
            let brush = match d.color {
                KBrush::Default => fg,
                other => brush_from_kasane(other),
            };
            if matches!(d.style, DecorationStyle::Solid) {
                DecorationKind::Solid {
                    brush,
                    thickness: d.thickness,
                }
            } else {
                DecorationKind::Custom {
                    style: d.style,
                    brush,
                    thickness: d.thickness,
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn brush_from_solid_passes_through() {
        assert_eq!(
            brush_from_kasane(KBrush::Solid([1, 2, 3, 4])),
            Brush::rgba(1, 2, 3, 4)
        );
    }

    #[test]
    fn brush_from_named_red() {
        // Matches NamedColor::Red.to_rgb() = (205, 0, 0)
        assert_eq!(
            brush_from_kasane(KBrush::Named(NamedColor::Red)),
            Brush::opaque(205, 0, 0)
        );
    }

    #[test]
    fn weight_passthrough() {
        assert_eq!(weight_from_kasane(KFontWeight::NORMAL).value(), 400.0);
        assert_eq!(weight_from_kasane(KFontWeight::BOLD).value(), 700.0);
        assert_eq!(weight_from_kasane(KFontWeight(350)).value(), 350.0);
    }

    #[test]
    fn slant_mapping() {
        assert_eq!(slant_from_kasane(FontSlant::Normal), PFontStyle::Normal);
        assert_eq!(slant_from_kasane(FontSlant::Italic), PFontStyle::Italic);
        assert_eq!(
            slant_from_kasane(FontSlant::Oblique),
            PFontStyle::Oblique(None)
        );
    }

    #[test]
    fn decoration_none_disabled() {
        let p = decoration_properties(None);
        assert!(!p.enabled);
        assert!(p.brush.is_none());
        assert!(p.size.is_none());
    }

    #[test]
    fn decoration_with_named_color() {
        let p = decoration_properties(Some(TextDecoration {
            style: DecorationStyle::Solid,
            color: KBrush::Named(NamedColor::Red),
            thickness: Some(2.5),
        }));
        assert!(p.enabled);
        assert_eq!(p.brush, Some(Brush::opaque(205, 0, 0)));
        assert_eq!(p.size, Some(2.5));
    }

    #[test]
    fn decoration_default_color_inherits() {
        let p = decoration_properties(Some(TextDecoration::default()));
        assert!(p.enabled);
        assert!(
            p.brush.is_none(),
            "Default brush should leave inheritance to renderer"
        );
    }

    #[test]
    fn custom_quad_needed_for_non_solid() {
        assert!(!decoration_needs_custom_quad(DecorationStyle::Solid));
        assert!(decoration_needs_custom_quad(DecorationStyle::Curly));
        assert!(decoration_needs_custom_quad(DecorationStyle::Dotted));
        assert!(decoration_needs_custom_quad(DecorationStyle::Dashed));
        assert!(decoration_needs_custom_quad(DecorationStyle::Double));
    }

    #[test]
    fn resolve_for_parley_basic() {
        let style = Style {
            fg: KBrush::Named(NamedColor::Red),
            bg: KBrush::Named(NamedColor::Black),
            font_weight: KFontWeight::BOLD,
            font_slant: FontSlant::Italic,
            ..Style::default()
        };
        let resolved = resolve_for_parley(&style, Brush::opaque(255, 255, 255));
        assert_eq!(resolved.fg, Brush::opaque(205, 0, 0));
        assert_eq!(resolved.bg, Brush::opaque(0, 0, 0));
        assert_eq!(resolved.weight, 700.0);
        assert_eq!(resolved.slant, SlantKind::Italic);
    }

    #[test]
    fn resolve_for_parley_default_fg_uses_fallback() {
        let style = Style::default();
        let fallback = Brush::opaque(50, 100, 150);
        let resolved = resolve_for_parley(&style, fallback);
        assert_eq!(resolved.fg, fallback);
    }

    #[test]
    fn resolve_for_parley_curly_underline() {
        let style = Style {
            fg: KBrush::Named(NamedColor::White),
            underline: Some(TextDecoration {
                style: DecorationStyle::Curly,
                color: KBrush::Named(NamedColor::Red),
                thickness: None,
            }),
            ..Style::default()
        };
        let resolved = resolve_for_parley(&style, Brush::default());
        match resolved.underline {
            DecorationKind::Custom {
                style,
                brush,
                thickness,
            } => {
                assert_eq!(style, DecorationStyle::Curly);
                assert_eq!(brush, Brush::opaque(205, 0, 0));
                assert!(thickness.is_none());
            }
            other => panic!("expected Custom underline, got {other:?}"),
        }
    }

    #[test]
    fn resolve_for_parley_solid_underline_inherits_fg() {
        let style = Style {
            fg: KBrush::Named(NamedColor::Cyan),
            underline: Some(TextDecoration {
                style: DecorationStyle::Solid,
                color: KBrush::Default,
                thickness: None,
            }),
            ..Style::default()
        };
        let resolved = resolve_for_parley(&style, Brush::default());
        match resolved.underline {
            DecorationKind::Solid { brush, .. } => {
                // fg = Named(Cyan) → (0, 205, 205)
                assert_eq!(brush, Brush::opaque(0, 205, 205));
            }
            other => panic!("expected Solid underline, got {other:?}"),
        }
    }
}
