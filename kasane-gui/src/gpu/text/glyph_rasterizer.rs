//! Swash-based glyph rasteriser.
//!
//! Turns a `(font, glyph_id, size, subpixel_x, variations)` tuple into
//! a [`RasterizedGlyph`] (mask or colour bitmap + placement). The
//! [`swash::scale::ScaleContext`] is owned by the rasteriser and
//! reused across calls — Parley resolves font references via the
//! shaped layout, so we receive a `swash::FontRef` from the caller
//! rather than discovering fonts ourselves.
//!
//! Subpixel positioning: x-axis only, quantised to 4 levels (0/4,
//! 1/4, 2/4, 3/4). y is always pixel-aligned. The 4× atlas footprint
//! multiplier in worst case is typically smaller in practice because
//! monospace text falls on a small set of subpixel offsets.
//!
//! Color emoji: tried first via `Source::ColorOutline(0)` then
//! `Source::ColorBitmap(BestFit)`. Outline / Bitmap fall through for normal
//! glyphs.

use swash::FontRef;
use swash::scale::image::Content;
use swash::scale::{Render, ScaleContext, Source, StrikeWith};
use swash::zeno::{Format, Vector};

/// 4-level subpixel x offset. `0 → 0`, `1 → 0.25`, `2 → 0.5`, `3 → 0.75`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct SubpixelX(pub u8);

impl SubpixelX {
    /// Quantise a fractional x position into the 4-level subpixel index.
    /// Inputs outside `[0, 1)` are reduced modulo 1 first.
    #[inline]
    pub fn from_fract(x: f32) -> Self {
        let frac = x - x.floor();
        Self(((frac * 4.0).round() as i32).rem_euclid(4) as u8 & 0b11)
    }

    /// Subpixel offset in pixels (0.0..1.0 in steps of 0.25).
    #[inline]
    pub fn as_offset(self) -> f32 {
        f32::from(self.0) * 0.25
    }
}

/// Content channel of a rasterised glyph.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ContentKind {
    /// 8-bit alpha mask (one byte per pixel).
    Mask,
    /// 32-bit RGBA colour bitmap (4 bytes per pixel, premultiplied alpha).
    Color,
}

/// A glyph that has been turned into a CPU-side bitmap by swash.
#[derive(Debug, Clone)]
pub struct RasterizedGlyph {
    pub width: u16,
    pub height: u16,
    /// Pixel offset from the pen position to the top of the bitmap.
    pub top: i16,
    /// Pixel offset from the pen position to the left of the bitmap.
    pub left: i16,
    pub content: ContentKind,
    /// Raw pixels. Length is `width * height * channels(content)` where
    /// `channels(Mask) = 1` and `channels(Color) = 4`.
    pub data: Vec<u8>,
}

impl RasterizedGlyph {
    /// Bytes per pixel for this glyph's content kind.
    #[inline]
    pub fn channels(&self) -> usize {
        match self.content {
            ContentKind::Mask => 1,
            ContentKind::Color => 4,
        }
    }

    /// Computed length the underlying buffer must have.
    #[inline]
    pub fn expected_data_len(&self) -> usize {
        usize::from(self.width) * usize::from(self.height) * self.channels()
    }
}

/// Owns the swash [`ScaleContext`] and exposes a uniform `rasterize` entry
/// point. Construct once per renderer and reuse across frames; the
/// `ScaleContext` internally caches per-(font, size, hint) scaler state to
/// keep the per-glyph cost down.
pub struct GlyphRasterizer {
    scale_ctx: ScaleContext,
}

impl Default for GlyphRasterizer {
    fn default() -> Self {
        Self::new()
    }
}

impl GlyphRasterizer {
    pub fn new() -> Self {
        Self {
            scale_ctx: ScaleContext::new(),
        }
    }

    /// Construct with an explicit max-entries bound for the internal scaler
    /// cache. swash clamps to `[1, 64]`.
    pub fn with_max_entries(max_entries: usize) -> Self {
        Self {
            scale_ctx: ScaleContext::with_max_entries(max_entries),
        }
    }

    /// Rasterise a single glyph.
    ///
    /// Returns `None` when the glyph has no representable form in the font
    /// (e.g. notdef without an outline) **or** when swash produced an empty
    /// bitmap (`width == 0` or `height == 0`, e.g. whitespace glyphs).
    /// Callers should fall back to a glyph-not-found bitmap or skip the glyph
    /// entirely; whitespace already advances via the layout, so skipping is
    /// the correct behaviour.
    pub fn rasterize(
        &mut self,
        font: FontRef<'_>,
        glyph_id: u16,
        size: f32,
        subpx_x: SubpixelX,
        hint: bool,
    ) -> Option<RasterizedGlyph> {
        let mut scaler = self.scale_ctx.builder(font).size(size).hint(hint).build();

        // Source priority: COLR colour outline → embedded color bitmap →
        // standard outline → embedded alpha bitmap. swash falls through to
        // the next source if a glyph has no data in the previous one.
        let sources = [
            Source::ColorOutline(0),
            Source::ColorBitmap(StrikeWith::BestFit),
            Source::Outline,
            Source::Bitmap(StrikeWith::BestFit),
        ];

        let mut render = Render::new(&sources);
        render.format(Format::Alpha);
        render.offset(Vector::new(subpx_x.as_offset(), 0.0));
        let image = render.render(&mut scaler, glyph_id)?;

        // Whitespace and other zero-extent glyphs produce an empty placement.
        // Forwarding them as `Some(empty)` would force the atlas allocator
        // to refuse a 0×0 slot and the L2 cache to count a spurious `dropped`.
        // Treat them like notdef: the layout already carries their advance.
        if image.placement.width == 0 || image.placement.height == 0 {
            return None;
        }

        let content = match image.content {
            Content::Color => ContentKind::Color,
            // SubpixelMask is treated as Mask for now; the GPU pipeline does
            // not consume RGB-subpixel masks (deferred until LCD AA returns).
            Content::Mask | Content::SubpixelMask => ContentKind::Mask,
        };

        Some(RasterizedGlyph {
            width: image.placement.width as u16,
            height: image.placement.height as u16,
            top: image.placement.top as i16,
            left: image.placement.left as i16,
            content,
            data: image.data,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn subpixel_quantisation_buckets() {
        assert_eq!(SubpixelX::from_fract(0.0), SubpixelX(0));
        assert_eq!(SubpixelX::from_fract(0.1), SubpixelX(0));
        assert_eq!(SubpixelX::from_fract(0.2), SubpixelX(1));
        assert_eq!(SubpixelX::from_fract(0.25), SubpixelX(1));
        assert_eq!(SubpixelX::from_fract(0.5), SubpixelX(2));
        assert_eq!(SubpixelX::from_fract(0.75), SubpixelX(3));
        // 0.99 rounds up to 4 → wraps to 0 (modulo 4).
        assert_eq!(SubpixelX::from_fract(0.99), SubpixelX(0));
    }

    #[test]
    fn subpixel_extracts_only_fract() {
        assert_eq!(SubpixelX::from_fract(2.0), SubpixelX(0));
        assert_eq!(SubpixelX::from_fract(7.5), SubpixelX(2));
        // Negative inputs reduce modulo 1: -0.25 → 0.75 → bucket 3.
        assert_eq!(SubpixelX::from_fract(-0.25), SubpixelX(3));
    }

    #[test]
    fn subpixel_offset_round_trip() {
        for i in 0..4u8 {
            let s = SubpixelX(i);
            assert_eq!(s.as_offset(), f32::from(i) * 0.25);
        }
    }

    #[test]
    fn rasterizer_constructs_with_default_capacity() {
        // Smoke: ScaleContext is built and ready to receive rasterisation
        // requests. Actual rasterisation requires a FontRef, which needs
        // font data — exercised end-to-end in shaper-integration tests in a
        // later phase.
        let _r = GlyphRasterizer::new();
        let _r2 = GlyphRasterizer::with_max_entries(32);
    }

    #[test]
    fn content_kind_channels() {
        let mask = RasterizedGlyph {
            width: 2,
            height: 3,
            top: 0,
            left: 0,
            content: ContentKind::Mask,
            data: vec![0; 6],
        };
        assert_eq!(mask.channels(), 1);
        assert_eq!(mask.expected_data_len(), 6);

        let color = RasterizedGlyph {
            width: 2,
            height: 3,
            top: 0,
            left: 0,
            content: ContentKind::Color,
            data: vec![0; 24],
        };
        assert_eq!(color.channels(), 4);
        assert_eq!(color.expected_data_len(), 24);
    }
}
