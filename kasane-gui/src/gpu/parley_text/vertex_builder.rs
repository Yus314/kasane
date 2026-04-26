//! Convert [`DrawableGlyph`] to wgpu vertex data (ADR-031, Phase 9b Step 2).
//!
//! Produces [`ParleyGlyphVertex`] instances ready to upload into a wgpu
//! vertex buffer. The struct layout exactly matches the existing
//! `text_pipeline::GlyphToRender` so the same shader (`shader.wgsl`) can
//! consume both — Phase 9b Step 3's `ParleyTextRenderer` reuses the
//! existing pipeline plumbing (vertex layout, bind groups) and only swaps
//! out the *source* of the vertex data.
//!
//! ## Vertex layout (wire format, must match shader.wgsl)
//!
//! ```text
//! +0   pos             [i32; 2]   top-left in physical pixels
//! +8   dim             [u16; 2]   width, height in pixels
//! +12  uv              [u16; 2]   atlas top-left in atlas pixels
//! +16  color           u32        packed linear RGBA8 (LE: R, G, B, A)
//! +20  content_type    u16        0 = Color (RGBA atlas), 1 = Mask (R8)
//! +22  srgb            u16        0 = ConvertToLinear, 1 = None (web)
//! +24  depth           f32        z layer for clip stencil
//! +28  (sizeof = 28)
//! ```
//!
//! The `content_type` discriminants follow the legacy
//! `text_pipeline::ContentType` enum (declared as `Color, Mask` so Color=0
//! and Mask=1). Our [`super::glyph_rasterizer::ContentKind`] uses the
//! reverse order (Mask declared first), so the converter explicitly maps
//! the values.
//!
//! The `srgb` flag mirrors the cosmic-text path's `ColorMode::Web` choice
//! (1 = no conversion). The Parley path passes already-linear colours; if
//! a future framebuffer change reintroduces sRGB conversion, [`SRGB_FLAG`]
//! is the single knob to flip.

use bytemuck::{Pod, Zeroable};

use super::Brush;
use super::frame_builder::DrawableGlyph;
use super::glyph_rasterizer::ContentKind;

/// Per-glyph vertex/instance data. Matches the byte layout of
/// `text_pipeline::GlyphToRender` so the existing shader consumes both.
#[repr(C)]
#[derive(Clone, Copy, Debug, Pod, Zeroable, PartialEq)]
pub struct ParleyGlyphVertex {
    pub pos: [i32; 2],
    pub dim: [u16; 2],
    pub uv: [u16; 2],
    pub color: u32,
    pub content_type: u16,
    pub srgb: u16,
    pub depth: f32,
}

/// Discriminant the shader expects for colour glyphs (RGBA atlas).
pub const CONTENT_TYPE_COLOR: u16 = 0;
/// Discriminant the shader expects for mask glyphs (R8 atlas, tinted).
pub const CONTENT_TYPE_MASK: u16 = 1;

/// `srgb` flag value for the production render path (matches
/// `ColorMode::Web` — pass-through, no extra sRGB conversion).
pub const SRGB_FLAG: u16 = 1;

impl ParleyGlyphVertex {
    /// Construct a vertex from a fully-resolved [`DrawableGlyph`].
    ///
    /// `pos` accounts for swash's per-glyph `(left, top)` placement so the
    /// caller does not need to add those again. `dim` and `uv` are the
    /// atlas slot's `(w, h)` and `(x, y)` respectively.
    pub fn from_drawable(g: &DrawableGlyph) -> Self {
        // Swash placement: `left` is the offset from the pen position to
        // the bitmap's left edge; `top` is the offset from the pen
        // position to the bitmap's *top* edge (positive = above the
        // baseline, in swash's coordinate convention).
        //
        // DrawableGlyph.py is the baseline in physical pixels; the bitmap's
        // top edge sits `top` pixels above it (subtracted because GPU
        // y grows downward).
        let left = (g.px + f32::from(g.left)).round() as i32;
        let top = (g.py - f32::from(g.top)).round() as i32;
        tracing::info!(
            target: "kasane::parley::vertex",
            "vert: baseline_x={:.2} baseline_y={:.2} swash_left={} swash_top={} dim=({},{}) -> pos=({},{})",
            g.px, g.py, g.left, g.top, g.width, g.height, left, top
        );
        Self {
            pos: [left, top],
            dim: [g.width, g.height],
            uv: [g.atlas_slot.x, g.atlas_slot.y],
            color: pack_color(g.brush, g.content),
            content_type: content_type_discriminant(g.content),
            srgb: SRGB_FLAG,
            depth: 0.0,
        }
    }
}

/// Build a contiguous `Vec<ParleyGlyphVertex>` from a slice of drawables.
/// Convenience for the common case where the renderer needs one buffer
/// upload per frame.
pub fn build_vertices(glyphs: &[DrawableGlyph]) -> Vec<ParleyGlyphVertex> {
    glyphs
        .iter()
        .map(ParleyGlyphVertex::from_drawable)
        .collect()
}

/// Pack a brush into the wire-format `u32` colour. Colour glyphs ignore
/// the brush in the shader (the bitmap supplies its own colour), but the
/// field is still written for layout uniformity.
///
/// Channel order is dictated by `text_pipeline/shader.wgsl`, which
/// extracts components as:
///
/// ```wgsl
/// R = (color & 0x00ff0000) >> 16    // byte at offset 2
/// G = (color & 0x0000ff00) >>  8    // byte at offset 1
/// B = (color & 0x000000ff)          // byte at offset 0  ← lowest byte
/// A = (color & 0xff000000) >> 24    // byte at offset 3
/// ```
///
/// So the wire layout is `0xAARRGGBB` — A in the high byte, B in the
/// low byte. Pack the brush accordingly.
#[inline]
fn pack_color(brush: Brush, content: ContentKind) -> u32 {
    match content {
        ContentKind::Color => 0xFFFF_FFFF, // shader ignores; full-alpha sentinel
        ContentKind::Mask => {
            let [r, g, b, a] = brush.0;
            u32::from(b) | (u32::from(g) << 8) | (u32::from(r) << 16) | (u32::from(a) << 24)
        }
    }
}

/// Map our content-kind discriminant onto the legacy `ContentType` shader
/// expectation (Color=0, Mask=1).
#[inline]
fn content_type_discriminant(content: ContentKind) -> u16 {
    match content {
        ContentKind::Color => CONTENT_TYPE_COLOR,
        ContentKind::Mask => CONTENT_TYPE_MASK,
    }
}

#[cfg(test)]
mod tests {
    use super::super::atlas::AtlasShelf;
    use super::*;

    fn drawable(px: f32, py: f32, content: ContentKind, brush: Brush) -> DrawableGlyph {
        // Mint a real AtlasSlot so alloc_id is valid.
        let mut shelf = AtlasShelf::new(super::super::atlas::MIN_ATLAS_SIZE);
        let slot = shelf.allocate(8, 12).expect("allocate");
        DrawableGlyph {
            px,
            py,
            width: 8,
            height: 12,
            left: 0,
            top: 12, // baseline → top of bitmap is 12 px above
            content,
            atlas_slot: slot,
            brush,
        }
    }

    #[test]
    fn vertex_layout_size_is_28_bytes() {
        // Phase 9b Step 3 uses the existing text_pipeline shader, which
        // expects 28-byte vertices. Catch any silent layout drift here.
        assert_eq!(std::mem::size_of::<ParleyGlyphVertex>(), 28);
    }

    #[test]
    fn pos_accounts_for_swash_left_top() {
        let g = drawable(100.0, 50.0, ContentKind::Mask, Brush::opaque(255, 255, 255));
        let v = ParleyGlyphVertex::from_drawable(&g);
        // left = 100 + 0 = 100; top = 50 - 12 = 38
        assert_eq!(v.pos, [100, 38]);
    }

    #[test]
    fn dim_matches_atlas_slot() {
        let g = drawable(0.0, 0.0, ContentKind::Mask, Brush::opaque(0, 0, 0));
        let v = ParleyGlyphVertex::from_drawable(&g);
        assert_eq!(v.dim, [8, 12]);
    }

    #[test]
    fn mask_color_packs_argb_for_shader() {
        // Shader expects 0xAARRGGBB; for brush (R=0x12, G=0x34, B=0x56,
        // A=0xFF) that is 0xFF12_3456.
        let g = drawable(
            0.0,
            0.0,
            ContentKind::Mask,
            Brush::rgba(0x12, 0x34, 0x56, 0xFF),
        );
        let v = ParleyGlyphVertex::from_drawable(&g);
        assert_eq!(v.color, 0xFF12_3456);
        assert_eq!(v.content_type, CONTENT_TYPE_MASK);
    }

    #[test]
    fn color_glyph_uses_full_alpha_sentinel() {
        let g = drawable(0.0, 0.0, ContentKind::Color, Brush::default());
        let v = ParleyGlyphVertex::from_drawable(&g);
        // Color glyphs ignore the brush in the shader; the sentinel just
        // keeps the field non-zero.
        assert_eq!(v.color, 0xFFFF_FFFF);
        assert_eq!(v.content_type, CONTENT_TYPE_COLOR);
    }

    #[test]
    fn srgb_flag_matches_color_mode_web() {
        let g = drawable(0.0, 0.0, ContentKind::Mask, Brush::default());
        let v = ParleyGlyphVertex::from_drawable(&g);
        // 1 = ColorMode::Web (no extra sRGB conversion); matches the
        // existing cosmic-text path.
        assert_eq!(v.srgb, SRGB_FLAG);
        assert_eq!(SRGB_FLAG, 1);
    }

    #[test]
    fn depth_starts_at_zero() {
        let g = drawable(0.0, 0.0, ContentKind::Mask, Brush::default());
        let v = ParleyGlyphVertex::from_drawable(&g);
        assert_eq!(v.depth, 0.0);
    }

    #[test]
    fn build_vertices_preserves_input_order() {
        let g0 = drawable(10.0, 20.0, ContentKind::Mask, Brush::opaque(1, 2, 3));
        let g1 = drawable(30.0, 40.0, ContentKind::Mask, Brush::opaque(4, 5, 6));
        let g2 = drawable(50.0, 60.0, ContentKind::Mask, Brush::opaque(7, 8, 9));
        let vs = build_vertices(&[g0, g1, g2]);
        assert_eq!(vs.len(), 3);
        assert_eq!(vs[0].pos[0], 10);
        assert_eq!(vs[1].pos[0], 30);
        assert_eq!(vs[2].pos[0], 50);
    }

    #[test]
    fn build_vertices_empty_input() {
        let vs = build_vertices(&[]);
        assert!(vs.is_empty());
    }

    #[test]
    fn pos_rounds_subpixel_to_nearest_pixel() {
        // px = 100.7, left = 0 → pos.x = 101 (rounded). Subpixel positioning
        // happens at raster time via SubpixelX; vertex pos rounds to whole
        // pixels because the shader cannot draw between pixel grid lines.
        let g = drawable(100.7, 50.4, ContentKind::Mask, Brush::default());
        let v = ParleyGlyphVertex::from_drawable(&g);
        assert_eq!(v.pos, [101, 38]);
    }
}
