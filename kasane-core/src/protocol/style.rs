//! Parley-native text style representation.
//!
//! Designed as the eventual replacement for [`Face`](super::color::Face) and
//! its companion [`Attributes`](super::color::Attributes) bitflags. During the
//! migration described in ADR-031, both representations coexist; conversion
//! helpers ([`Style::from_face`] / [`Style::to_face`]) bridge call sites that
//! have not yet been ported.
//!
//! Field meaning:
//!
//! - [`Brush::Default`] preserves the Kakoune semantics of "inherit from the
//!   containing context" (resolved later by [`resolve_style`]).
//! - The `final_fg` / `final_bg` / `final_style` flags correspond to the
//!   `FINAL_FG` / `FINAL_BG` / `FINAL_ATTR` bits of the legacy
//!   [`Attributes`](super::color::Attributes) bitflags.
//! - Continuous `FontWeight` (100..=900) supersedes the boolean
//!   `BOLD` attribute. The legacy `BOLD` maps to [`FontWeight::BOLD`] (700);
//!   `DIM` is preserved as a separate boolean because it is an opacity-like
//!   property in Kakoune, not a font-weight value.
//! - `font_variations` is empty in steady state; it is reserved for
//!   variable-font axes set by plugins (Phase 10 of ADR-031).

use serde::{Deserialize, Serialize};

use super::color::NamedColor;

// ---------------------------------------------------------------------------
// Brush
// ---------------------------------------------------------------------------

/// A paint source for foreground, background, or decoration colour.
///
/// `Default` is the Kakoune sentinel that means "inherit from the parent
/// context". Resolution against a base style happens in [`resolve_style`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Deserialize, Serialize)]
pub enum Brush {
    /// Inherit from the containing context.
    #[default]
    Default,
    /// Solid linear-space RGBA colour.
    Solid([u8; 4]),
    /// One of the 16 ANSI named colours; resolved to RGB by the renderer.
    Named(NamedColor),
}

impl Brush {
    /// Convenience constructor for opaque RGB.
    #[inline]
    pub const fn rgb(r: u8, g: u8, b: u8) -> Self {
        Self::Solid([r, g, b, 0xff])
    }

    /// Returns true when this brush should be replaced by the parent's brush
    /// during style resolution.
    #[inline]
    pub fn is_inherit(self) -> bool {
        matches!(self, Brush::Default)
    }
}

// ---------------------------------------------------------------------------
// FontWeight / FontSlant
// ---------------------------------------------------------------------------

/// CSS-style font weight (100..=900). Continuous to support variable fonts.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Deserialize, Serialize)]
#[serde(transparent)]
pub struct FontWeight(pub u16);

impl FontWeight {
    pub const THIN: Self = Self(100);
    pub const EXTRA_LIGHT: Self = Self(200);
    pub const LIGHT: Self = Self(300);
    pub const NORMAL: Self = Self(400);
    pub const MEDIUM: Self = Self(500);
    pub const SEMI_BOLD: Self = Self(600);
    pub const BOLD: Self = Self(700);
    pub const EXTRA_BOLD: Self = Self(800);
    pub const BLACK: Self = Self(900);
}

impl Default for FontWeight {
    #[inline]
    fn default() -> Self {
        Self::NORMAL
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Deserialize, Serialize)]
pub enum FontSlant {
    #[default]
    Normal,
    Italic,
    Oblique,
}

// ---------------------------------------------------------------------------
// FontFeatures (OpenType feature toggle bitset)
// ---------------------------------------------------------------------------

/// OpenType feature toggle bitset.
///
/// Current bits cover the most common programming-font use cases (ligatures
/// and contextual alternates). Additional bits may be added without breaking
/// the wire format because the field type is a transparent `u32`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Deserialize, Serialize)]
#[serde(transparent)]
pub struct FontFeatures(pub u32);

impl FontFeatures {
    /// `calt` + `clig` — common programming-font ligatures.
    pub const STANDARD_LIGATURES: u32 = 1 << 0;
    /// `dlig` — discretionary ligatures (e.g. Fira Code's arrow ligatures).
    pub const DISCRETIONARY_LIGATURES: u32 = 1 << 1;
    /// `hlig` — historical ligatures.
    pub const HISTORICAL_LIGATURES: u32 = 1 << 2;
    /// `liga` — standard ligatures.
    pub const COMMON_LIGATURES: u32 = 1 << 3;
    /// `zero` — slashed-zero alternate.
    pub const SLASHED_ZERO: u32 = 1 << 4;

    #[inline]
    pub const fn has(self, flag: u32) -> bool {
        (self.0 & flag) != 0
    }

    #[inline]
    pub fn insert(&mut self, flag: u32) {
        self.0 |= flag;
    }

    #[inline]
    pub fn remove(&mut self, flag: u32) {
        self.0 &= !flag;
    }
}

// ---------------------------------------------------------------------------
// FontVariation (variable-font axis)
// ---------------------------------------------------------------------------

/// A variable-font axis setting (e.g. `wght=350`, `wdth=80`).
///
/// `tag` is the 4-byte OpenType axis tag (LSB-first ASCII).
#[derive(Debug, Clone, Copy, PartialEq, Deserialize, Serialize)]
pub struct FontVariation {
    pub tag: [u8; 4],
    pub value: f32,
}

impl FontVariation {
    pub const fn new(tag: [u8; 4], value: f32) -> Self {
        Self { tag, value }
    }
}

// Bit-pattern Eq + Hash for use in `StyleStore`. Treats two `FontVariation`s
// as equal iff their `tag` and the bit pattern of `value` match. NaN values
// are not produced by Kasane (Kakoune wire format = strings; plugins use
// literals); the construction site should debug_assert finiteness if it ever
// accepts plugin-supplied floats.
impl Eq for FontVariation {}
impl std::hash::Hash for FontVariation {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.tag.hash(state);
        self.value.to_bits().hash(state);
    }
}

// ---------------------------------------------------------------------------
// BidiOverride
// ---------------------------------------------------------------------------

/// Explicit bidi direction override for a span. `None` (the default on
/// [`Style`]) lets ICU4X infer direction from the strong characters.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Deserialize, Serialize)]
pub enum BidiOverride {
    Ltr,
    Rtl,
}

// ---------------------------------------------------------------------------
// TextDecoration
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Deserialize, Serialize)]
pub enum DecorationStyle {
    #[default]
    Solid,
    Curly,
    Dotted,
    Dashed,
    Double,
}

/// Underline or strikethrough decoration with explicit style and colour.
///
/// `thickness` of `None` means "use the font's recommended thickness from its
/// metrics" — this is the Phase 10 behaviour replacing the legacy
/// hard-coded `cell_h * 0.2` amplitude in `quad_pipeline.rs`.
#[derive(Debug, Clone, Copy, PartialEq, Deserialize, Serialize)]
pub struct TextDecoration {
    pub style: DecorationStyle,
    pub color: Brush,
    pub thickness: Option<f32>,
}

impl TextDecoration {
    /// Convenience constructor for a solid underline that inherits its colour
    /// from the foreground.
    #[inline]
    pub const fn solid() -> Self {
        Self {
            style: DecorationStyle::Solid,
            color: Brush::Default,
            thickness: None,
        }
    }
}

impl Default for TextDecoration {
    #[inline]
    fn default() -> Self {
        Self::solid()
    }
}

// Bit-pattern Eq + Hash for `StyleStore`. See FontVariation comment for NaN.
impl Eq for TextDecoration {}
impl std::hash::Hash for TextDecoration {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.style.hash(state);
        self.color.hash(state);
        self.thickness.map(f32::to_bits).hash(state);
    }
}

// ---------------------------------------------------------------------------
// Style
// ---------------------------------------------------------------------------

/// Parley-native, post-resolution text style.
///
/// `Style` is the render-ready type: every brush is either `Brush::Default`
/// (meaning "use whatever the renderer's terminal/canvas default is") or an
/// explicit colour, and there are no Kakoune-specific resolution flags. WIT
/// exposes only this type to plugins — the parse-side
/// [`UnresolvedStyle`] is a host-internal concern.
///
/// To build a `Style` from a Kakoune wire face, the migration path is:
///
/// 1. Parse the wire face into an [`UnresolvedStyle`] via
///    [`UnresolvedStyle::from_face`].
/// 2. Resolve it against a base context via [`resolve_style`] to obtain
///    a `Style`. Kakoune's `final_fg` / `final_bg` / `final_style` flags
///    govern this step and then drop out of the type.
///
/// The convenience [`Style::from_face`] shortcut is equivalent to
/// `UnresolvedStyle::from_face(face).resolved_against_default()` — useful
/// for sites that hold an unparented Face.
#[derive(Debug, Clone, PartialEq, Default, Deserialize, Serialize)]
pub struct Style {
    // Colours
    pub fg: Brush,
    pub bg: Brush,

    // Font properties
    pub font_weight: FontWeight,
    pub font_slant: FontSlant,
    pub font_features: FontFeatures,
    /// Variable-font axis settings. Usually empty.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub font_variations: Vec<FontVariation>,
    pub letter_spacing: f32,

    // Decorations
    pub underline: Option<TextDecoration>,
    pub strikethrough: Option<TextDecoration>,

    // Bidi
    pub bidi_override: Option<BidiOverride>,

    // Behavioural flags (real terminal attributes — SGR 5/7/2)
    pub blink: bool,
    pub reverse: bool,
    pub dim: bool,
}

// Bit-pattern Eq + Hash for `StyleStore`. The derived `PartialEq` continues
// to use IEEE float equality (correct for non-NaN inputs); `Eq` and `Hash`
// here use bitwise comparison so the interner can serve as a true content
// table. Hash and Eq are mutually consistent; they may diverge from
// PartialEq only on NaN, which Kasane does not construct.
impl Eq for Style {}
impl std::hash::Hash for Style {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.fg.hash(state);
        self.bg.hash(state);
        self.font_weight.hash(state);
        self.font_slant.hash(state);
        self.font_features.hash(state);
        self.font_variations.hash(state);
        self.letter_spacing.to_bits().hash(state);
        self.underline.hash(state);
        self.strikethrough.hash(state);
        self.bidi_override.hash(state);
        self.blink.hash(state);
        self.reverse.hash(state);
        self.dim.hash(state);
    }
}

impl Style {
    /// Construct a style with defaults equivalent to [`Face::default`].
    #[inline]
    pub fn new() -> Self {
        Self::default()
    }

    /// Convert from the legacy Kakoune face representation.
    ///
    /// Drops Kakoune resolution flags (`final_fg` / `final_bg` / `final_style`).
    /// Use [`UnresolvedStyle::from_face`] when those flags must be preserved
    /// for a downstream [`resolve_style`] call.
    pub fn from_face(face: &super::color::Face) -> Self {
        UnresolvedStyle::from_face(face).style
    }

    /// Lossy conversion back to the legacy face representation.
    ///
    /// Used by sites that still consume `Face`. Preserves bold/italic/blink
    /// /reverse/dim/strike/underline-style. `font_weight` outside the discrete
    /// `{NORMAL, BOLD}` set rounds to bold when ≥ 600. `font_variations`,
    /// `letter_spacing`, and `bidi_override` are dropped. The result has no
    /// `final_*` attributes set — a `Style` is post-resolution.
    pub fn to_face(&self) -> super::color::Face {
        let (face, _) = self.to_face_with_attrs();
        face
    }

    /// Internal: project to a `Face` and return the in-progress attribute
    /// bitset. Shared by [`Style::to_face`] and [`UnresolvedStyle::to_face`]
    /// so the latter can OR in the `final_*` bits without duplicating the
    /// rest of the conversion.
    fn to_face_with_attrs(&self) -> (super::color::Face, super::color::Attributes) {
        use super::color::{Attributes, Color, Face};
        let mut attrs = Attributes::empty();

        if self.font_weight.0 >= FontWeight::SEMI_BOLD.0 {
            attrs |= Attributes::BOLD;
        }
        if matches!(self.font_slant, FontSlant::Italic | FontSlant::Oblique) {
            attrs |= Attributes::ITALIC;
        }
        if self.blink {
            attrs |= Attributes::BLINK;
        }
        if self.reverse {
            attrs |= Attributes::REVERSE;
        }
        if self.dim {
            attrs |= Attributes::DIM;
        }
        if self.strikethrough.is_some() {
            attrs |= Attributes::STRIKETHROUGH;
        }
        let underline_color: Color;
        if let Some(deco) = self.underline {
            underline_color = color_from_brush(deco.color);
            attrs |= match deco.style {
                DecorationStyle::Solid => Attributes::UNDERLINE,
                DecorationStyle::Curly => Attributes::CURLY_UNDERLINE,
                DecorationStyle::Dotted => Attributes::DOTTED_UNDERLINE,
                DecorationStyle::Dashed => Attributes::DASHED_UNDERLINE,
                DecorationStyle::Double => Attributes::DOUBLE_UNDERLINE,
            };
        } else {
            underline_color = Color::Default;
        }

        let face = Face {
            fg: color_from_brush(self.fg),
            bg: color_from_brush(self.bg),
            underline: underline_color,
            attributes: attrs,
        };
        (face, attrs)
    }
}

// ---------------------------------------------------------------------------
// UnresolvedStyle — pre-resolution form
// ---------------------------------------------------------------------------

/// Pre-resolution text style as parsed from Kakoune's wire format.
///
/// Kakoune's protocol carries a `Face` whose attribute bitflags include
/// resolution-control bits (`FINAL_FG`, `FINAL_BG`, `FINAL_ATTR`) that govern
/// how parent-context inheritance applies during resolution. `UnresolvedStyle`
/// preserves those bits alongside a [`Style`] until [`resolve_style`] consumes
/// them; the resulting [`Style`] no longer carries them, since a resolved
/// style has no parent left to inherit from.
///
/// Plugins do not see this type — the WIT boundary exposes only [`Style`].
/// The split exists so that the canonical render-ready type stays free of
/// Kakoune-specific resolution metadata.
#[derive(Debug, Clone, PartialEq, Default, Deserialize, Serialize)]
pub struct UnresolvedStyle {
    /// All non-resolution-flag style fields. The contained `Style` is what
    /// would result if this atom were resolved against a fully-default base.
    #[serde(flatten)]
    pub style: Style,

    /// Kakoune `final-fg`: atom's `fg` wins over base even when atom's `fg`
    /// is `Brush::Default` (i.e. "stay default; don't inherit").
    #[serde(default, skip_serializing_if = "is_false")]
    pub final_fg: bool,

    /// Kakoune `final-bg`: as `final_fg` but for the background brush.
    #[serde(default, skip_serializing_if = "is_false")]
    pub final_bg: bool,

    /// Kakoune `final-attr`: atom replaces base's behavioural flags / weight
    /// / slant / decorations wholesale instead of layering over them.
    #[serde(default, skip_serializing_if = "is_false")]
    pub final_style: bool,
}

#[inline]
fn is_false(b: &bool) -> bool {
    !*b
}

// Bit-pattern Eq + Hash for `StyleStore`. Same rationale as for `Style`.
impl Eq for UnresolvedStyle {}
impl std::hash::Hash for UnresolvedStyle {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.style.hash(state);
        self.final_fg.hash(state);
        self.final_bg.hash(state);
        self.final_style.hash(state);
    }
}

impl UnresolvedStyle {
    /// Convert from the legacy Kakoune face representation, preserving
    /// `final_*` resolution flags.
    pub fn from_face(face: &super::color::Face) -> Self {
        use super::color::Attributes;
        let attrs = face.attributes;

        let font_weight = if attrs.contains(Attributes::BOLD) {
            FontWeight::BOLD
        } else {
            FontWeight::NORMAL
        };

        let font_slant = if attrs.contains(Attributes::ITALIC) {
            FontSlant::Italic
        } else {
            FontSlant::Normal
        };

        let underline = if attrs.contains(Attributes::CURLY_UNDERLINE) {
            Some(TextDecoration {
                style: DecorationStyle::Curly,
                color: brush_from_color(face.underline),
                thickness: None,
            })
        } else if attrs.contains(Attributes::DOUBLE_UNDERLINE) {
            Some(TextDecoration {
                style: DecorationStyle::Double,
                color: brush_from_color(face.underline),
                thickness: None,
            })
        } else if attrs.contains(Attributes::DOTTED_UNDERLINE) {
            Some(TextDecoration {
                style: DecorationStyle::Dotted,
                color: brush_from_color(face.underline),
                thickness: None,
            })
        } else if attrs.contains(Attributes::DASHED_UNDERLINE) {
            Some(TextDecoration {
                style: DecorationStyle::Dashed,
                color: brush_from_color(face.underline),
                thickness: None,
            })
        } else if attrs.contains(Attributes::UNDERLINE) {
            Some(TextDecoration {
                style: DecorationStyle::Solid,
                color: brush_from_color(face.underline),
                thickness: None,
            })
        } else {
            None
        };

        let strikethrough = if attrs.contains(Attributes::STRIKETHROUGH) {
            Some(TextDecoration::default())
        } else {
            None
        };

        UnresolvedStyle {
            style: Style {
                fg: brush_from_color(face.fg),
                bg: brush_from_color(face.bg),
                font_weight,
                font_slant,
                font_features: FontFeatures::default(),
                font_variations: Vec::new(),
                letter_spacing: 0.0,
                underline,
                strikethrough,
                bidi_override: None,
                blink: attrs.contains(Attributes::BLINK),
                reverse: attrs.contains(Attributes::REVERSE),
                dim: attrs.contains(Attributes::DIM),
            },
            final_fg: attrs.contains(Attributes::FINAL_FG),
            final_bg: attrs.contains(Attributes::FINAL_BG),
            final_style: attrs.contains(Attributes::FINAL_ATTR),
        }
    }

    /// Lossy conversion back to the legacy face representation, preserving
    /// `final_*` flags. Round-trips with [`UnresolvedStyle::from_face`] on
    /// faces whose attributes lie within the legacy set.
    pub fn to_face(&self) -> super::color::Face {
        use super::color::Attributes;
        let (mut face, mut attrs) = self.style.to_face_with_attrs();
        if self.final_fg {
            attrs |= Attributes::FINAL_FG;
        }
        if self.final_bg {
            attrs |= Attributes::FINAL_BG;
        }
        if self.final_style {
            attrs |= Attributes::FINAL_ATTR;
        }
        face.attributes = attrs;
        face
    }
}

// ---------------------------------------------------------------------------
// Style resolution
// ---------------------------------------------------------------------------

// Bridge during ADR-031 Phase A.3: Face → Style conversion via `From`.
// Lets call sites with a `Face` (typically from KakouneRequest or theme
// tokens) flow naturally into Style-typed APIs.
impl From<super::color::Face> for Style {
    #[inline]
    fn from(face: super::color::Face) -> Self {
        Self::from_face(&face)
    }
}

impl From<&super::color::Face> for Style {
    #[inline]
    fn from(face: &super::color::Face) -> Self {
        Self::from_face(face)
    }
}

/// Resolve an [`UnresolvedStyle`] against a base context, mirroring the
/// legacy [`resolve_face`](super::color::resolve_face) semantics. Returns a
/// [`Style`] — i.e. `final_*` resolution flags are consumed and dropped.
///
/// Inheritance rules:
/// - `Brush::Default` inherits the parent's brush, unless the corresponding
///   `final_*` flag is set.
/// - When `final_style` is true, all behavioural flags / weight / slant
///   / decorations come from the atom; otherwise they layer over the base
///   (atom values take precedence where they are non-default).
pub fn resolve_style(atom: &UnresolvedStyle, base: &Style) -> Style {
    let s = &atom.style;

    let fg = if atom.final_fg || !s.fg.is_inherit() {
        s.fg
    } else {
        base.fg
    };
    let bg = if atom.final_bg || !s.bg.is_inherit() {
        s.bg
    } else {
        base.bg
    };

    if atom.final_style {
        let mut out = s.clone();
        out.fg = fg;
        out.bg = bg;
        return out;
    }

    Style {
        fg,
        bg,
        font_weight: if s.font_weight == FontWeight::default() {
            base.font_weight
        } else {
            s.font_weight
        },
        font_slant: if matches!(s.font_slant, FontSlant::Normal) {
            base.font_slant
        } else {
            s.font_slant
        },
        font_features: FontFeatures(s.font_features.0 | base.font_features.0),
        font_variations: if s.font_variations.is_empty() {
            base.font_variations.clone()
        } else {
            s.font_variations.clone()
        },
        letter_spacing: if s.letter_spacing != 0.0 {
            s.letter_spacing
        } else {
            base.letter_spacing
        },
        underline: s.underline.or(base.underline),
        strikethrough: s.strikethrough.or(base.strikethrough),
        bidi_override: s.bidi_override.or(base.bidi_override),
        blink: s.blink || base.blink,
        reverse: s.reverse || base.reverse,
        dim: s.dim || base.dim,
    }
}

// ---------------------------------------------------------------------------
// Brush ↔ Color migration helpers
// ---------------------------------------------------------------------------

fn brush_from_color(c: super::color::Color) -> Brush {
    use super::color::Color;
    match c {
        Color::Default => Brush::Default,
        Color::Named(n) => Brush::Named(n),
        Color::Rgb { r, g, b } => Brush::rgb(r, g, b),
    }
}

fn color_from_brush(b: Brush) -> super::color::Color {
    use super::color::Color;
    match b {
        Brush::Default => Color::Default,
        Brush::Named(n) => Color::Named(n),
        Brush::Solid([r, g, b, _a]) => Color::Rgb { r, g, b },
    }
}

// ---------------------------------------------------------------------------
// Default-style sharing helper (ADR-031 Phase A — B-wide)
// ---------------------------------------------------------------------------
//
// The previous Phase A.2 design routed every `Atom`'s style through a
// process-global `Mutex<StyleStore>`. Profiling for the B-wide PR showed
// that lock acquisition on the hot path (per-atom `Atom::face()` calls
// from `pipeline.rs`, `walk_scene.rs`, `paint.rs`, …) was the dominant
// share of the 1-line-frame perf gap. Atoms now hold an
// `Arc<UnresolvedStyle>` directly, with parse-time interning handled in
// `protocol::parse` (`HashMap<UnresolvedStyle, Arc<UnresolvedStyle>>`).
//
// The default style is special: it is the most common style in any
// frame and is constructed millions of times (`Atom::plain`, padding
// rows, etc.). Pre-allocating one shared `Arc` removes the repeated
// allocation entirely and keeps the default code path cheaper than the
// pre-A.2 inline-`Face` form.

/// Return the shared `Arc` for [`UnresolvedStyle::default`]. Cheap to
/// clone — the underlying allocation is performed at most once per
/// process and reused for every `Atom::plain` and bench fixture.
pub fn default_unresolved_style() -> std::sync::Arc<UnresolvedStyle> {
    static DEFAULT: std::sync::OnceLock<std::sync::Arc<UnresolvedStyle>> =
        std::sync::OnceLock::new();
    DEFAULT
        .get_or_init(|| std::sync::Arc::new(UnresolvedStyle::default()))
        .clone()
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::protocol::color::{Attributes, Color, Face};

    #[test]
    fn brush_default_is_inherit() {
        assert!(Brush::default().is_inherit());
        assert!(!Brush::rgb(1, 2, 3).is_inherit());
        assert!(!Brush::Named(NamedColor::Red).is_inherit());
    }

    #[test]
    fn brush_rgb_sets_alpha_opaque() {
        assert_eq!(
            Brush::rgb(0x10, 0x20, 0x30),
            Brush::Solid([0x10, 0x20, 0x30, 0xff])
        );
    }

    #[test]
    fn font_weight_constants() {
        assert_eq!(FontWeight::default(), FontWeight::NORMAL);
        assert_eq!(FontWeight::NORMAL.0, 400);
        assert_eq!(FontWeight::BOLD.0, 700);
    }

    #[test]
    fn font_features_bitset() {
        let mut f = FontFeatures::default();
        assert!(!f.has(FontFeatures::STANDARD_LIGATURES));
        f.insert(FontFeatures::STANDARD_LIGATURES);
        assert!(f.has(FontFeatures::STANDARD_LIGATURES));
        assert!(!f.has(FontFeatures::DISCRETIONARY_LIGATURES));
        f.insert(FontFeatures::DISCRETIONARY_LIGATURES);
        assert!(f.has(FontFeatures::DISCRETIONARY_LIGATURES));
        f.remove(FontFeatures::STANDARD_LIGATURES);
        assert!(!f.has(FontFeatures::STANDARD_LIGATURES));
        assert!(f.has(FontFeatures::DISCRETIONARY_LIGATURES));
    }

    #[test]
    fn style_default_matches_face_default_round_trip() {
        let face = Face::default();
        let style = Style::from_face(&face);
        assert_eq!(style, Style::default());
        assert_eq!(style.to_face(), face);
    }

    #[test]
    fn from_face_preserves_named_colours() {
        let face = Face {
            fg: Color::Named(NamedColor::Red),
            bg: Color::Named(NamedColor::Blue),
            underline: Color::Default,
            attributes: Attributes::empty(),
        };
        let style = Style::from_face(&face);
        assert_eq!(style.fg, Brush::Named(NamedColor::Red));
        assert_eq!(style.bg, Brush::Named(NamedColor::Blue));
        assert_eq!(style.underline, None);
    }

    #[test]
    fn from_face_preserves_rgb_colours() {
        let face = Face {
            fg: Color::Rgb { r: 1, g: 2, b: 3 },
            bg: Color::Rgb { r: 4, g: 5, b: 6 },
            underline: Color::Default,
            attributes: Attributes::empty(),
        };
        let style = Style::from_face(&face);
        assert_eq!(style.fg, Brush::rgb(1, 2, 3));
        assert_eq!(style.bg, Brush::rgb(4, 5, 6));
    }

    #[test]
    fn from_face_maps_bold_italic() {
        let face = Face {
            fg: Color::Default,
            bg: Color::Default,
            underline: Color::Default,
            attributes: Attributes::BOLD | Attributes::ITALIC,
        };
        let style = Style::from_face(&face);
        assert_eq!(style.font_weight, FontWeight::BOLD);
        assert_eq!(style.font_slant, FontSlant::Italic);
    }

    #[test]
    fn from_face_maps_blink_reverse_dim() {
        let face = Face {
            fg: Color::Default,
            bg: Color::Default,
            underline: Color::Default,
            attributes: Attributes::BLINK | Attributes::REVERSE | Attributes::DIM,
        };
        let style = Style::from_face(&face);
        assert!(style.blink);
        assert!(style.reverse);
        assert!(style.dim);
    }

    #[test]
    fn from_face_maps_all_underline_styles() {
        let cases = [
            (Attributes::UNDERLINE, DecorationStyle::Solid),
            (Attributes::CURLY_UNDERLINE, DecorationStyle::Curly),
            (Attributes::DOUBLE_UNDERLINE, DecorationStyle::Double),
            (Attributes::DOTTED_UNDERLINE, DecorationStyle::Dotted),
            (Attributes::DASHED_UNDERLINE, DecorationStyle::Dashed),
        ];
        for (attr, expected) in cases {
            let face = Face {
                fg: Color::Default,
                bg: Color::Default,
                underline: Color::Named(NamedColor::Red),
                attributes: attr,
            };
            let style = Style::from_face(&face);
            let deco = style.underline.unwrap();
            assert_eq!(deco.style, expected);
            assert_eq!(deco.color, Brush::Named(NamedColor::Red));
        }
    }

    #[test]
    fn from_face_curly_takes_precedence_over_solid() {
        let face = Face {
            fg: Color::Default,
            bg: Color::Default,
            underline: Color::Default,
            attributes: Attributes::UNDERLINE | Attributes::CURLY_UNDERLINE,
        };
        let style = Style::from_face(&face);
        assert_eq!(style.underline.unwrap().style, DecorationStyle::Curly);
    }

    #[test]
    fn from_face_strikethrough() {
        let face = Face {
            fg: Color::Default,
            bg: Color::Default,
            underline: Color::Default,
            attributes: Attributes::STRIKETHROUGH,
        };
        let style = Style::from_face(&face);
        assert!(style.strikethrough.is_some());
    }

    #[test]
    fn from_face_final_flags() {
        let face = Face {
            fg: Color::Default,
            bg: Color::Default,
            underline: Color::Default,
            attributes: Attributes::FINAL_FG | Attributes::FINAL_BG | Attributes::FINAL_ATTR,
        };
        let unresolved = UnresolvedStyle::from_face(&face);
        assert!(unresolved.final_fg);
        assert!(unresolved.final_bg);
        assert!(unresolved.final_style);

        // The post-resolve `Style` form drops these flags entirely.
        let style = Style::from_face(&face);
        let _ = style; // no `final_fg` field exists on `Style` post-split
    }

    #[test]
    fn to_face_round_trip_preserves_legacy_set() {
        // Any Face whose attributes lie within the legacy set (including
        // FINAL_*) should round trip exactly through
        // UnresolvedStyle::from_face → UnresolvedStyle::to_face.
        let face = Face {
            fg: Color::Rgb {
                r: 10,
                g: 20,
                b: 30,
            },
            bg: Color::Named(NamedColor::Black),
            underline: Color::Named(NamedColor::Cyan),
            attributes: Attributes::BOLD
                | Attributes::ITALIC
                | Attributes::DIM
                | Attributes::CURLY_UNDERLINE
                | Attributes::FINAL_FG,
        };
        let unresolved = UnresolvedStyle::from_face(&face);
        assert_eq!(unresolved.to_face(), face);

        // Without FINAL_*, `Style::to_face` should also round trip.
        let mut face_no_final = face;
        face_no_final.attributes.remove(Attributes::FINAL_FG);
        let style = Style::from_face(&face_no_final);
        assert_eq!(style.to_face(), face_no_final);
    }

    /// Lift a `Style` body into an `UnresolvedStyle` with no final flags
    /// set. Lets pre-existing tests keep their `Style { .. }` literals while
    /// using the new `resolve_style(&UnresolvedStyle, &Style)` signature.
    fn unresolved(style: Style) -> UnresolvedStyle {
        UnresolvedStyle {
            style,
            ..UnresolvedStyle::default()
        }
    }

    #[test]
    fn resolve_style_inherits_default_brush() {
        let base = Style {
            fg: Brush::Named(NamedColor::White),
            bg: Brush::Named(NamedColor::Black),
            ..Style::default()
        };
        let atom = unresolved(Style::default());
        let resolved = resolve_style(&atom, &base);
        assert_eq!(resolved.fg, Brush::Named(NamedColor::White));
        assert_eq!(resolved.bg, Brush::Named(NamedColor::Black));
    }

    #[test]
    fn resolve_style_atom_brush_overrides_base() {
        let base = Style {
            fg: Brush::Named(NamedColor::White),
            bg: Brush::Named(NamedColor::Black),
            ..Style::default()
        };
        let atom = unresolved(Style {
            fg: Brush::Named(NamedColor::Red),
            ..Style::default()
        });
        let resolved = resolve_style(&atom, &base);
        assert_eq!(resolved.fg, Brush::Named(NamedColor::Red));
        assert_eq!(resolved.bg, Brush::Named(NamedColor::Black));
    }

    #[test]
    fn resolve_style_final_fg_blocks_inheritance() {
        let base = Style {
            fg: Brush::Named(NamedColor::White),
            ..Style::default()
        };
        let atom = UnresolvedStyle {
            style: Style {
                fg: Brush::Default,
                ..Style::default()
            },
            final_fg: true,
            ..UnresolvedStyle::default()
        };
        let resolved = resolve_style(&atom, &base);
        // final_fg=true means atom's fg (Default) wins, so the resolved fg is
        // also Default — no inheritance from base.
        assert_eq!(resolved.fg, Brush::Default);
    }

    #[test]
    fn resolve_style_final_style_replaces_base() {
        let base = Style {
            font_weight: FontWeight::BOLD,
            blink: true,
            ..Style::default()
        };
        let atom = UnresolvedStyle {
            style: Style {
                fg: Brush::Named(NamedColor::Red),
                ..Style::default()
            },
            final_style: true,
            ..UnresolvedStyle::default()
        };
        let resolved = resolve_style(&atom, &base);
        assert_eq!(resolved.font_weight, FontWeight::NORMAL);
        assert!(!resolved.blink);
        // Brushes still resolve normally even when final_style is set.
        assert_eq!(resolved.fg, Brush::Named(NamedColor::Red));
    }

    #[test]
    fn resolve_style_layers_underline() {
        let base = Style::default();
        let atom = unresolved(Style {
            underline: Some(TextDecoration::default()),
            ..Style::default()
        });
        let resolved = resolve_style(&atom, &base);
        assert!(resolved.underline.is_some());
    }

    #[test]
    fn resolve_style_layers_font_features() {
        let base = Style {
            font_features: FontFeatures(FontFeatures::STANDARD_LIGATURES),
            ..Style::default()
        };
        let atom = unresolved(Style {
            font_features: FontFeatures(FontFeatures::DISCRETIONARY_LIGATURES),
            ..Style::default()
        });
        let resolved = resolve_style(&atom, &base);
        assert!(resolved.font_features.has(FontFeatures::STANDARD_LIGATURES));
        assert!(
            resolved
                .font_features
                .has(FontFeatures::DISCRETIONARY_LIGATURES)
        );
    }

    #[test]
    fn font_variation_constructor() {
        let v = FontVariation::new(*b"wght", 350.0);
        assert_eq!(v.tag, *b"wght");
        assert_eq!(v.value, 350.0);
    }

    #[test]
    fn brush_serde_round_trip() {
        let cases = [
            Brush::Default,
            Brush::Named(NamedColor::Red),
            Brush::Solid([1, 2, 3, 4]),
        ];
        for brush in cases {
            let json = serde_json::to_string(&brush).unwrap();
            let parsed: Brush = serde_json::from_str(&json).unwrap();
            assert_eq!(brush, parsed);
        }
    }

    #[test]
    fn style_serde_round_trip() {
        let style = Style {
            fg: Brush::rgb(255, 0, 0),
            bg: Brush::Named(NamedColor::Black),
            font_weight: FontWeight::BOLD,
            font_slant: FontSlant::Italic,
            font_features: FontFeatures(FontFeatures::STANDARD_LIGATURES),
            font_variations: vec![FontVariation::new(*b"wght", 350.0)],
            letter_spacing: 0.5,
            underline: Some(TextDecoration {
                style: DecorationStyle::Curly,
                color: Brush::Named(NamedColor::Cyan),
                thickness: Some(1.5),
            }),
            strikethrough: None,
            bidi_override: Some(BidiOverride::Rtl),
            blink: true,
            reverse: false,
            dim: false,
        };
        let json = serde_json::to_string(&style).unwrap();
        let parsed: Style = serde_json::from_str(&json).unwrap();
        assert_eq!(style, parsed);
    }

    #[test]
    fn unresolved_style_serde_round_trip() {
        let unresolved = UnresolvedStyle {
            style: Style {
                fg: Brush::rgb(1, 2, 3),
                bg: Brush::Default,
                font_weight: FontWeight::SEMI_BOLD,
                ..Style::default()
            },
            final_fg: true,
            final_bg: false,
            final_style: true,
        };
        let json = serde_json::to_string(&unresolved).unwrap();
        let parsed: UnresolvedStyle = serde_json::from_str(&json).unwrap();
        assert_eq!(unresolved, parsed);
    }

    // ---------------------------------------------------------------------
    // default_unresolved_style sharing (ADR-031 Phase A — B-wide)
    // ---------------------------------------------------------------------

    #[test]
    fn default_unresolved_style_shares_arc() {
        let a = default_unresolved_style();
        let b = default_unresolved_style();
        // Same content.
        assert_eq!(*a, UnresolvedStyle::default());
        // Same allocation: clone of a OnceLock-backed Arc.
        assert!(std::sync::Arc::ptr_eq(&a, &b));
    }
}
