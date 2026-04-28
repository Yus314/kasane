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

/// Parley-native text style.
///
/// Replaces the combination of [`Face`](super::color::Face) +
/// [`Attributes`](super::color::Attributes) used by the cosmic-text-era
/// pipeline. The wire format from Kakoune is still translated through the
/// legacy `Face` deserialiser; conversion to `Style` happens at the protocol
/// boundary via [`Style::from_face`].
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

    // Behavioural flags
    pub blink: bool,
    pub reverse: bool,
    pub dim: bool,

    // Inheritance control (Kakoune `final_*` semantics)
    pub final_fg: bool,
    pub final_bg: bool,
    pub final_style: bool,
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
        self.final_fg.hash(state);
        self.final_bg.hash(state);
        self.final_style.hash(state);
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
    /// Called at the protocol boundary during the ADR-031 migration; will be
    /// removed once the legacy [`Face`](super::color::Face) type is deleted.
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

        Style {
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
            final_fg: attrs.contains(Attributes::FINAL_FG),
            final_bg: attrs.contains(Attributes::FINAL_BG),
            final_style: attrs.contains(Attributes::FINAL_ATTR),
        }
    }

    /// Lossy conversion back to the legacy face representation.
    ///
    /// Used by sites that have not yet migrated; preserves bold/italic/blink
    /// /reverse/dim/strike/underline-style. `font_weight` outside the discrete
    /// `{NORMAL, BOLD}` set rounds to bold when ≥ 600. `font_variations`,
    /// `letter_spacing`, and `bidi_override` are dropped.
    pub fn to_face(&self) -> super::color::Face {
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
        if self.final_fg {
            attrs |= Attributes::FINAL_FG;
        }
        if self.final_bg {
            attrs |= Attributes::FINAL_BG;
        }
        if self.final_style {
            attrs |= Attributes::FINAL_ATTR;
        }

        Face {
            fg: color_from_brush(self.fg),
            bg: color_from_brush(self.bg),
            underline: underline_color,
            attributes: attrs,
        }
    }
}

// ---------------------------------------------------------------------------
// Style resolution
// ---------------------------------------------------------------------------

/// Resolve a style against a base context, mirroring the legacy
/// [`resolve_face`](super::color::resolve_face) semantics.
///
/// Inheritance rules:
/// - `Brush::Default` inherits the parent's brush, unless the corresponding
///   `final_*` flag is set.
/// - When `final_style` is true, all behavioural flags / weight / slant
///   / decorations come from the atom; otherwise they layer over the base
///   (atom values take precedence where they are non-default).
pub fn resolve_style(atom: &Style, base: &Style) -> Style {
    let fg = if atom.final_fg || !atom.fg.is_inherit() {
        atom.fg
    } else {
        base.fg
    };
    let bg = if atom.final_bg || !atom.bg.is_inherit() {
        atom.bg
    } else {
        base.bg
    };

    if atom.final_style {
        let mut out = atom.clone();
        out.fg = fg;
        out.bg = bg;
        return out;
    }

    Style {
        fg,
        bg,
        font_weight: if atom.font_weight == FontWeight::default() {
            base.font_weight
        } else {
            atom.font_weight
        },
        font_slant: if matches!(atom.font_slant, FontSlant::Normal) {
            base.font_slant
        } else {
            atom.font_slant
        },
        font_features: FontFeatures(atom.font_features.0 | base.font_features.0),
        font_variations: if atom.font_variations.is_empty() {
            base.font_variations.clone()
        } else {
            atom.font_variations.clone()
        },
        letter_spacing: if atom.letter_spacing != 0.0 {
            atom.letter_spacing
        } else {
            base.letter_spacing
        },
        underline: atom.underline.or(base.underline),
        strikethrough: atom.strikethrough.or(base.strikethrough),
        bidi_override: atom.bidi_override.or(base.bidi_override),
        blink: atom.blink || base.blink,
        reverse: atom.reverse || base.reverse,
        dim: atom.dim || base.dim,
        final_fg: atom.final_fg,
        final_bg: atom.final_bg,
        final_style: atom.final_style,
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
// StyleId / StyleStore — content-addressed style interning (ADR-031 Phase A)
// ---------------------------------------------------------------------------

/// Stable identifier for a [`Style`] interned in a [`StyleStore`].
///
/// `StyleId(0)` is reserved for the default style ([`DEFAULT_STYLE_ID`]); a
/// freshly-constructed `StyleStore` already contains it and `StyleId::default`
/// returns it. IDs are assigned sequentially as new styles are interned;
/// store lifetime is tied to whatever owns it (typically `AppState`), and
/// IDs are not portable across stores.
///
/// Why an interner: `Style` is ~100 bytes; an `Atom` carrying a full `Style`
/// inflates the per-atom payload ~10× over the legacy `Face` representation.
/// An interned id is 4 bytes, equality is identity (no field walk), and hash
/// keys for the L1 LayoutCache become trivial. Editors use small style
/// vocabularies (5–20 distinct styles in steady state), so the table stays
/// compact.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Deserialize, Serialize)]
#[serde(transparent)]
pub struct StyleId(pub u32);

impl StyleId {
    /// Look the style body up in a [`StyleStore`]. Panics if the id was
    /// produced by a different store; for a non-panicking variant use
    /// [`StyleStore::try_get`].
    #[inline]
    pub fn get(self, store: &StyleStore) -> &Style {
        store.get(self)
    }
}

/// The pre-interned id for [`Style::default`]. Always present in any
/// freshly-constructed [`StyleStore`].
pub const DEFAULT_STYLE_ID: StyleId = StyleId(0);

// ---------------------------------------------------------------------------
// Process-global style table
// ---------------------------------------------------------------------------
//
// ADR-031 Phase A.2: a single process-global [`StyleStore`] backs every
// `Atom`'s `style_id`. The alternative (per-`AppState` stores) requires
// threading a store handle through every Atom-construction site and reader
// thread; a global behind a `Mutex` removes the plumbing entirely with
// negligible cost (interns are rare; per-frame intern count is bounded by
// the viewport's distinct styles, typically 5–20).
//
// A future refactor (Phase E candidate) may move the store into AppState
// once the surrounding APIs are settled. The public interface
// (`Atom::face`, `Atom::style`, `intern_style_global`) is designed to
// survive that move with minimal call-site churn.

static GLOBAL_STORE: std::sync::OnceLock<std::sync::Mutex<StyleStore>> = std::sync::OnceLock::new();

fn global_store() -> &'static std::sync::Mutex<StyleStore> {
    GLOBAL_STORE.get_or_init(|| std::sync::Mutex::new(StyleStore::new()))
}

/// Intern a `Style` in the process-global table and return its id.
pub fn intern_style_global(style: Style) -> StyleId {
    global_store()
        .lock()
        .expect("global style store poisoned")
        .intern(style)
}

/// Run `f` with a borrow of the [`Style`] at `id` from the process-global
/// table. Holds the global lock for the duration of `f`.
pub fn with_global_style<R>(id: StyleId, f: impl FnOnce(&Style) -> R) -> R {
    let store = global_store().lock().expect("global style store poisoned");
    f(store.get(id))
}

/// Return a copy of the [`Style`] at `id` from the process-global table.
pub fn style_clone_global(id: StyleId) -> Style {
    global_store()
        .lock()
        .expect("global style store poisoned")
        .get(id)
        .clone()
}

/// Content-addressed style table.
///
/// Hand-rolled (not `salsa::interned`) because Salsa's interned types carry
/// a `'db` lifetime parameter that does not fit `Atom`'s position inside
/// Salsa-owned `Vec<Line>` inputs. See ADR-031 plan §A.1 for the rationale.
///
/// Internal layout: `Vec<Style>` (id → style) + `HashMap<Style, StyleId>`
/// (style → id). The map is the slow path (one lookup per intern); the vec
/// is the hot path (one indexed read per `get`). Memory cost is roughly
/// `2 × |unique styles| × sizeof(Style)`, which for a typical session
/// (≈20 styles) is ≈4 KB.
#[derive(Debug, Clone)]
pub struct StyleStore {
    by_id: Vec<Style>,
    by_style: std::collections::HashMap<Style, StyleId>,
}

impl Default for StyleStore {
    fn default() -> Self {
        Self::new()
    }
}

impl StyleStore {
    /// Create a new store, pre-populated with [`Style::default`] at
    /// [`DEFAULT_STYLE_ID`].
    pub fn new() -> Self {
        let default_style = Style::default();
        let mut by_style = std::collections::HashMap::new();
        by_style.insert(default_style.clone(), DEFAULT_STYLE_ID);
        Self {
            by_id: vec![default_style],
            by_style,
        }
    }

    /// Return the id for `style`, interning it if not already present.
    pub fn intern(&mut self, style: Style) -> StyleId {
        if let Some(&id) = self.by_style.get(&style) {
            return id;
        }
        let id = StyleId(self.by_id.len() as u32);
        self.by_id.push(style.clone());
        self.by_style.insert(style, id);
        id
    }

    /// Look up the style body by id. Panics if the id is not present in this
    /// store; use [`Self::try_get`] when validating cross-store ids.
    #[inline]
    pub fn get(&self, id: StyleId) -> &Style {
        &self.by_id[id.0 as usize]
    }

    /// Non-panicking lookup. Returns `None` for ids out of range for this
    /// store.
    #[inline]
    pub fn try_get(&self, id: StyleId) -> Option<&Style> {
        self.by_id.get(id.0 as usize)
    }

    /// Number of distinct interned styles, including the pre-interned
    /// default.
    pub fn len(&self) -> usize {
        self.by_id.len()
    }

    pub fn is_empty(&self) -> bool {
        self.by_id.is_empty()
    }
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
        let style = Style::from_face(&face);
        assert!(style.final_fg);
        assert!(style.final_bg);
        assert!(style.final_style);
    }

    #[test]
    fn to_face_round_trip_preserves_legacy_set() {
        // Any Face whose attributes lie within the legacy set should round
        // trip exactly through Style::from_face → Style::to_face.
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
        let style = Style::from_face(&face);
        assert_eq!(style.to_face(), face);
    }

    #[test]
    fn resolve_style_inherits_default_brush() {
        let base = Style {
            fg: Brush::Named(NamedColor::White),
            bg: Brush::Named(NamedColor::Black),
            ..Style::default()
        };
        let atom = Style::default();
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
        let atom = Style {
            fg: Brush::Named(NamedColor::Red),
            ..Style::default()
        };
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
        let atom = Style {
            fg: Brush::Default,
            final_fg: true,
            ..Style::default()
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
        let atom = Style {
            fg: Brush::Named(NamedColor::Red),
            final_style: true,
            ..Style::default()
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
        let atom = Style {
            underline: Some(TextDecoration::default()),
            ..Style::default()
        };
        let resolved = resolve_style(&atom, &base);
        assert!(resolved.underline.is_some());
    }

    #[test]
    fn resolve_style_layers_font_features() {
        let base = Style {
            font_features: FontFeatures(FontFeatures::STANDARD_LIGATURES),
            ..Style::default()
        };
        let atom = Style {
            font_features: FontFeatures(FontFeatures::DISCRETIONARY_LIGATURES),
            ..Style::default()
        };
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
            final_fg: true,
            final_bg: false,
            final_style: false,
        };
        let json = serde_json::to_string(&style).unwrap();
        let parsed: Style = serde_json::from_str(&json).unwrap();
        assert_eq!(style, parsed);
    }

    // ---------------------------------------------------------------------
    // StyleStore / StyleId tests (ADR-031 Phase A.1)
    // ---------------------------------------------------------------------

    fn red_style() -> Style {
        Style {
            fg: Brush::Named(NamedColor::Red),
            ..Style::default()
        }
    }

    fn bold_red_style() -> Style {
        Style {
            fg: Brush::Named(NamedColor::Red),
            font_weight: FontWeight::BOLD,
            ..Style::default()
        }
    }

    #[test]
    fn store_starts_with_default_at_id_zero() {
        let store = StyleStore::new();
        assert_eq!(store.len(), 1);
        assert_eq!(store.get(DEFAULT_STYLE_ID), &Style::default());
        assert_eq!(StyleId::default(), DEFAULT_STYLE_ID);
    }

    #[test]
    fn intern_default_returns_zero() {
        let mut store = StyleStore::new();
        assert_eq!(store.intern(Style::default()), DEFAULT_STYLE_ID);
        assert_eq!(store.len(), 1, "default must not duplicate");
    }

    #[test]
    fn intern_is_idempotent() {
        let mut store = StyleStore::new();
        let id1 = store.intern(red_style());
        let id2 = store.intern(red_style());
        assert_eq!(id1, id2);
        assert_eq!(store.len(), 2); // default + red
    }

    #[test]
    fn distinct_styles_get_distinct_ids() {
        let mut store = StyleStore::new();
        let red = store.intern(red_style());
        let bold_red = store.intern(bold_red_style());
        assert_ne!(red, bold_red);
        assert_eq!(store.len(), 3); // default + red + bold_red
    }

    #[test]
    fn get_round_trip() {
        let mut store = StyleStore::new();
        let id = store.intern(red_style());
        assert_eq!(store.get(id), &red_style());
        assert_eq!(id.get(&store), &red_style());
    }

    #[test]
    fn try_get_out_of_range_returns_none() {
        let store = StyleStore::new();
        assert!(store.try_get(StyleId(999)).is_none());
    }

    #[test]
    fn ids_assigned_sequentially() {
        let mut store = StyleStore::new();
        let id1 = store.intern(red_style());
        let id2 = store.intern(bold_red_style());
        assert_eq!(id1, StyleId(1));
        assert_eq!(id2, StyleId(2));
    }

    #[test]
    fn style_with_f32_fields_interns_correctly() {
        // letter_spacing and font_variations both contain f32; verify the
        // bit-pattern Hash + Eq impls let them intern as expected.
        let mut store = StyleStore::new();
        let s = Style {
            letter_spacing: 1.5,
            font_variations: vec![FontVariation::new(*b"wght", 350.0)],
            ..Style::default()
        };
        let id_a = store.intern(s.clone());
        let id_b = store.intern(s);
        assert_eq!(id_a, id_b);
    }

    #[test]
    fn styles_differing_only_in_letter_spacing_distinct() {
        let mut store = StyleStore::new();
        let a = Style {
            letter_spacing: 1.0,
            ..Style::default()
        };
        let b = Style {
            letter_spacing: 2.0,
            ..Style::default()
        };
        assert_ne!(store.intern(a), store.intern(b));
    }
}
