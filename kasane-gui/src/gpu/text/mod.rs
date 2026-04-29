//! Parley + swash text pipeline.
//!
//! End-to-end:
//! `Atom` → [`styled_line::StyledLine`] → [`shaper::shape_line`] (Parley
//! `RangedBuilder`) → [`layout::ParleyLayout`] (cached by
//! [`layout_cache::LayoutCache`]) → [`glyph_emitter::emit`] /
//! [`frame_builder::build_frame`] → swash raster ([`glyph_rasterizer`]) →
//! L2 [`raster_cache::GlyphRasterCache`] + L3 atlas
//! ([`atlas`] / [`gpu_atlas`]) → [`text_renderer::TextRenderer`].
//!
//! [`mouse hit_test`](hit_test) operates on a shaped layout and
//! returns byte-precise cluster positions; the production
//! [`SceneRenderer::hit_test`](crate::gpu::scene_renderer) translates to
//! the cell grid because Kakoune is cell-based — see that method's
//! docstring for the design rationale.

pub mod atlas;
pub mod font_id;
pub mod font_stack;
pub mod frame_builder;
pub mod glyph_emitter;
pub mod glyph_rasterizer;
pub mod gpu_atlas;
pub mod hit_test;
pub mod layout;
pub mod layout_cache;
pub mod metrics;
pub mod raster_cache;
pub mod raster_cache_glue;
pub mod shaper;
pub mod style_resolver;
pub mod styled_line;
pub mod text_renderer;
pub mod vertex_builder;

#[cfg(test)]
mod integration_test;

use kasane_core::config::FontConfig;
use parley::{FontContext, FontFamily, LayoutContext};
use swash::scale::ScaleContext;

/// Per-renderer Parley state.
///
/// Owns the long-lived contexts that Parley and swash expect to be reused
/// across frames:
///
/// - [`FontContext`] caches the system font collection and font data, both of
///   which are expensive to build from scratch.
/// - [`LayoutContext`] is the reusable scratch for `RangedBuilder` /
///   `Layout::break_all_lines` allocations.
/// - [`ScaleContext`] caches per-(font, size, hint) scaler state used by
///   `swash::scale::Render` during glyph rasterisation.
/// - `default_family` is the resolved [`FontFamily`] derived from the user's
///   [`FontConfig`] at construction time. Built once via
///   [`font_stack::resolve_stack`] so the production hot path
///   ([`Self::shape`]) does not need to re-derive the family on every line.
///   Rebuild with [`Self::set_default_family`] when the font config changes.
///
/// `Brush` is the GPU-side colour type that flows through the layout. It is a
/// linear-space RGBA8 quad; richer brushes (gradients, patterns) are tracked
/// as a Phase 12 question.
pub struct ParleyText {
    pub font_cx: FontContext,
    pub layout_cx: LayoutContext<Brush>,
    pub scale_cx: ScaleContext,
    /// User-configured family stack: primary family followed by
    /// `FontConfig.fallback_list`. Cached so `shape()` is allocation-free.
    default_family: FontFamily<'static>,
}

impl ParleyText {
    /// Create a new Parley state from the user's font configuration.
    ///
    /// Loads the system font collection lazily through Fontique, so the cost
    /// of this constructor is mostly the initial font index scan (~5–20 ms
    /// on Linux/macOS depending on installed fonts) plus the family-stack
    /// resolution (negligible).
    pub fn new(font_config: &FontConfig) -> Self {
        // Fontique walks the system font directories on FontContext::new().
        Self {
            font_cx: FontContext::new(),
            layout_cx: LayoutContext::new(),
            scale_cx: ScaleContext::new(),
            default_family: font_stack::resolve_stack(font_config),
        }
    }

    /// Replace the cached default family stack. Called on font config /
    /// scale-factor change so the next shape picks up the new fallback
    /// list. The L1 LayoutCache must be invalidated separately by the
    /// caller — see `SceneRenderer::resize`.
    pub fn set_default_family(&mut self, font_config: &FontConfig) {
        self.default_family = font_stack::resolve_stack(font_config);
    }

    /// Shape `line` against the cached default family stack.
    ///
    /// This is the production entry point that respects the user's
    /// `FontConfig.family` and `FontConfig.fallback_list`. Equivalent to
    /// `shaper::shape_line(self, line, self.default_family.clone())` but
    /// avoids the `clone()` at the call site.
    pub fn shape(&mut self, line: &styled_line::StyledLine) -> layout::ParleyLayout {
        let family = self.default_family.clone();
        shaper::shape_line(self, line, family)
    }

    /// Read-only access to the resolved family stack. Mostly useful for
    /// diagnostics and tests; production code should call [`Self::shape`].
    pub fn default_family(&self) -> &FontFamily<'static> {
        &self.default_family
    }
}

/// Linear-space RGBA8 brush flowing through Parley layout.
///
/// Layout-time `Brush` does not carry "default / inherited" semantics: any
/// inheritance is resolved by [`crate::gpu::text::style_resolver`]
/// before the brush enters Parley. This keeps the Layout immutable across
/// resolution changes and lets the L1 `LayoutCache` (Phase 7) hash it
/// trivially.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct Brush(pub [u8; 4]);

impl Brush {
    #[inline]
    pub const fn rgba(r: u8, g: u8, b: u8, a: u8) -> Self {
        Self([r, g, b, a])
    }

    #[inline]
    pub const fn opaque(r: u8, g: u8, b: u8) -> Self {
        Self([r, g, b, 0xff])
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parley_text_constructs() {
        // Smoke: FontContext / LayoutContext / ScaleContext are all
        // constructible. This exercises the dependency graph (parley +
        // fontique + skrifa + harfrust + swash) without requiring an actual
        // shape or raster.
        let cfg = FontConfig::default();
        let _text = ParleyText::new(&cfg);
    }

    #[test]
    fn font_config_propagates_to_default_family() {
        // ADR-031 Wave 1.2 regression pin. Before this PR, ParleyText
        // ignored its `font_config` argument (parameter was prefixed
        // `_font_config`) and the production hot path always shaped
        // against `FontFamily::Single(Generic(Monospace))`. This test
        // pins that user-supplied family + fallback list reach the
        // cached `default_family` so subsequent shape calls use them.
        use parley::{FontFamily, FontFamilyName};

        let cfg = FontConfig {
            family: "Inconsolata".into(),
            fallback_list: vec!["Noto Color Emoji".into(), "monospace".into()],
            ..FontConfig::default()
        };
        let text = ParleyText::new(&cfg);
        match text.default_family() {
            FontFamily::List(list) => {
                assert_eq!(list.len(), 3, "primary + 2 fallbacks");
                match &list[0] {
                    FontFamilyName::Named(n) => assert_eq!(n.as_ref(), "Inconsolata"),
                    other => panic!("expected primary Named, got {other:?}"),
                }
                match &list[1] {
                    FontFamilyName::Named(n) => assert_eq!(n.as_ref(), "Noto Color Emoji"),
                    other => panic!("expected fallback Named, got {other:?}"),
                }
            }
            other => panic!("expected List family, got {other:?}"),
        }
    }

    #[test]
    fn set_default_family_replaces_cached_stack() {
        // resize() on font config change calls set_default_family. Pin
        // that the cached stack actually changes after the call.
        let initial = FontConfig::default();
        let mut text = ParleyText::new(&initial);
        let updated = FontConfig {
            family: "Cascadia Code".into(),
            ..FontConfig::default()
        };
        text.set_default_family(&updated);
        match text.default_family() {
            parley::FontFamily::Single(parley::FontFamilyName::Named(n)) => {
                assert_eq!(n.as_ref(), "Cascadia Code");
            }
            other => panic!("expected Single Named, got {other:?}"),
        }
    }

    #[test]
    fn brush_constructors() {
        assert_eq!(
            Brush::opaque(0x10, 0x20, 0x30),
            Brush([0x10, 0x20, 0x30, 0xff])
        );
        assert_eq!(Brush::rgba(1, 2, 3, 4), Brush([1, 2, 3, 4]));
        assert_eq!(Brush::default(), Brush([0, 0, 0, 0]));
    }
}
