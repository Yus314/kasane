//! Parley-based text rendering scaffold (ADR-031, Phase 6).
//!
//! This module is the future replacement for the cosmic-text-derived
//! [`text_pipeline`](super::text_pipeline) and the cosmic-text-driven path in
//! [`scene_renderer`](super::scene_renderer). At Phase 6 it contains only the
//! shared contexts and conversion helpers; Phase 7 adds shaping and the L1
//! `LayoutCache`, Phase 8 adds the swash rasteriser and L2/L3 caches, and
//! Phase 9 wires it into `SceneRenderer`.
//!
//! Until Phase 11 cuts the legacy path, the cosmic-text pipeline remains the
//! production renderer and this module is exercised only by unit tests.

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
pub mod parley_text_renderer;
pub mod raster_cache;
pub mod shaper;
pub mod style_resolver;
pub mod styled_line;
pub mod vertex_builder;

#[cfg(test)]
mod integration_test;

use kasane_core::config::FontConfig;
use parley::{FontContext, LayoutContext};
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
///
/// `Brush` is the GPU-side colour type that flows through the layout. At the
/// scaffold stage it is a linear-space RGBA8 quad; later phases may upgrade
/// it to a richer brush enum (gradients, patterns) without breaking callers.
pub struct ParleyText {
    pub font_cx: FontContext,
    pub layout_cx: LayoutContext<Brush>,
    pub scale_cx: ScaleContext,
}

impl ParleyText {
    /// Create a new Parley state from the user's font configuration.
    ///
    /// Loads the system font collection lazily through Fontique, so the cost
    /// of this constructor is mostly the initial font index scan (~5–20 ms
    /// on Linux/macOS depending on installed fonts).
    pub fn new(_font_config: &FontConfig) -> Self {
        // Fontique walks the system font directories on FontContext::new().
        // The font_config is currently unused; Phase 7 will pass it into
        // font_stack::resolve_primary to build the default FontStack.
        Self {
            font_cx: FontContext::new(),
            layout_cx: LayoutContext::new(),
            scale_cx: ScaleContext::new(),
        }
    }
}

/// Linear-space RGBA8 brush flowing through Parley layout.
///
/// Layout-time `Brush` does not carry "default / inherited" semantics: any
/// inheritance is resolved by [`crate::gpu::parley_text::style_resolver`]
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
    fn brush_constructors() {
        assert_eq!(
            Brush::opaque(0x10, 0x20, 0x30),
            Brush([0x10, 0x20, 0x30, 0xff])
        );
        assert_eq!(Brush::rgba(1, 2, 3, 4), Brush([1, 2, 3, 4]));
        assert_eq!(Brush::default(), Brush([0, 0, 0, 0]));
    }
}
