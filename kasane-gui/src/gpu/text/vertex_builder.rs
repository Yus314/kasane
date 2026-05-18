//! Convert [`DrawableGlyph`] to wgpu vertex data.
//!
//! Produces [`ParleyGlyphVertex`] instances ready to upload into a
//! wgpu vertex buffer. The struct layout exactly matches
//! [`super::wgpu_types::GlyphToRender`] so the shared
//! [`shader.wgsl`](super::wgpu_cache) consumes both — `TextRenderer`
//! reuses the same pipeline plumbing (vertex layout, bind groups).
//!
//! ## Vertex layout (wire format, must match shader.wgsl)
//!
//! ```text
//! +0   pos             [i32; 2]   top-left in physical pixels
//! +8   dim             [u16; 2]   width, height in pixels
//! +12  uv              [u16; 2]   atlas top-left in atlas pixels
//! +16  color           u32        packed linear RGBA8, wire layout 0xAARRGGBB
//! +20  content_type    u16        0 = Color (RGBA atlas), 1 = Mask (R8)
//! +22  srgb            u16        shader branch select (see [`SRGB_FLAG`])
//! +24  depth           f32        z layer for clip stencil
//! +28  (sizeof = 28)
//! ```
//!
//! The `content_type` discriminants follow the legacy
//! `ContentType` enum (declared as `Color, Mask` so Color=0
//! and Mask=1). Our [`super::glyph_rasterizer::ContentKind`] uses the
//! reverse order (Mask declared first), so the converter explicitly maps
//! the values.
//!
//! The `srgb` flag picks one of two shader branches in `shader.wgsl`:
//! `case 0u` is the pass-through path used for already-linear brushes,
//! `case 1u` applies `srgb_to_linear` for sRGB-encoded brushes. Kasane
//! always sends linear brushes, so [`SRGB_FLAG`] selects the
//! pass-through branch.

use bytemuck::{Pod, Zeroable};

use super::Brush;
use super::frame_builder::DrawableGlyph;
use super::glyph_rasterizer::ContentKind;

/// Per-glyph vertex/instance data. Matches the byte layout of
/// `super::wgpu_types::GlyphToRender` so the existing shader consumes both.
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

/// `srgb` flag value for the production render path. The brush
/// reaches the shader already in **linear** space (CPU-side
/// [`ColorResolver::resolve_style_colors_linear`] does the
/// sRGB → linear step before we pack to u8). Selecting the
/// pass-through branch (`case 0u`) in `shader.wgsl` lets the
/// `*UnormSrgb` framebuffer perform the inverse `linear → sRGB`
/// conversion at write time — symmetric with the quad / FillRect
/// path (`quad.wgsl`) which also receives linear input.
///
/// **Do not set this to 1**: that path applies `srgb_to_linear`
/// a second time, double-darkening every non-extreme brush
/// (Issue #112). Pure white (1.0) survives because it is the
/// fixed point of `srgb_to_linear`, but every other intensity
/// gets damped proportionally.
pub const SRGB_FLAG: u16 = 0;

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
/// Channel order is dictated by [`super::wgpu_cache`]'s `shader.wgsl`, which
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
        // Uses the shared shader (super::wgpu_cache::Cache builds it), which
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
    fn srgb_flag_selects_pass_through_branch() {
        let g = drawable(0.0, 0.0, ContentKind::Mask, Brush::default());
        let v = ParleyGlyphVertex::from_drawable(&g);
        // 0 = pass-through (case 0u) in shader.wgsl: the brush is already
        // in linear space when it reaches the shader (`emit_text` calls
        // `resolve_style_colors_linear`), so the shader must not apply
        // `srgb_to_linear` again — that was Issue #112.
        assert_eq!(v.srgb, SRGB_FLAG);
        assert_eq!(SRGB_FLAG, 0);
    }

    /// Issue #112 end-to-end pipeline simulation (no GPU). Mirrors the
    /// math in `emit_text` → `pack_color` → `shader.wgsl` case branch
    /// → `*UnormSrgb` framebuffer auto-conversion. Verifies that a
    /// plugin-emitted `Brush::Rgb(r,g,b)` round-trips to display value
    /// `(r,g,b)` at full mask coverage with the post-fix `SRGB_FLAG=0`.
    #[test]
    fn issue_112_end_to_end_pipeline_simulation() {
        use kasane_core::config::ColorsConfig;
        use kasane_core::protocol::Brush as KBrush;
        use kasane_core::protocol::Style;

        // Mirror shader.wgsl `srgb_to_linear`.
        fn srgb_to_linear(c: f32) -> f32 {
            if c <= 0.040_45 {
                c / 12.92
            } else {
                ((c + 0.055) / 1.055).powf(2.4)
            }
        }
        // Mirror the `*UnormSrgb` framebuffer's linear→sRGB at write.
        fn linear_to_srgb(c: f32) -> f32 {
            if c <= 0.003_130_8 {
                c * 12.92
            } else {
                1.055 * c.powf(1.0 / 2.4) - 0.055
            }
        }

        let resolver = crate::colors::ColorResolver::from_config(&ColorsConfig::default());

        let cases: &[(&str, [u8; 3])] = &[
            ("white", [0xff, 0xff, 0xff]),
            ("near-white", [0xee, 0xee, 0xee]),
            ("parchment", [0xff, 0xff, 0xe6]),
            ("pink", [0xff, 0xa0, 0xa0]),
            ("cream", [0xff, 0xea, 0xa0]),
            ("cyan", [0x00, 0xff, 0xff]),
            ("mid-gray", [0x80, 0x80, 0x80]),
            ("near-black", [0x10, 0x10, 0x10]),
        ];
        for (name, [r, g, b]) in cases {
            let style = Style {
                fg: KBrush::Solid([*r, *g, *b, 0xff]),
                ..Style::default()
            };
            // (1) emit_text pack: linear u8.
            let (visual_fg, _, _) = resolver.resolve_style_colors_linear(&style);
            let packed = [
                (visual_fg[0].clamp(0.0, 1.0) * 255.0).round() as u8,
                (visual_fg[1].clamp(0.0, 1.0) * 255.0).round() as u8,
                (visual_fg[2].clamp(0.0, 1.0) * 255.0).round() as u8,
            ];
            // (2) Shader vertex branch — SRGB_FLAG=0 means pass-through.
            let shader_fg = if SRGB_FLAG == 0 {
                [
                    packed[0] as f32 / 255.0,
                    packed[1] as f32 / 255.0,
                    packed[2] as f32 / 255.0,
                ]
            } else {
                [
                    srgb_to_linear(packed[0] as f32 / 255.0),
                    srgb_to_linear(packed[1] as f32 / 255.0),
                    srgb_to_linear(packed[2] as f32 / 255.0),
                ]
            };
            // (3) Full mask coverage → fragment outputs shader_fg directly.
            // (4) UnormSrgb framebuffer auto-converts linear→sRGB at write.
            let displayed = [
                (linear_to_srgb(shader_fg[0]).clamp(0.0, 1.0) * 255.0).round() as u8,
                (linear_to_srgb(shader_fg[1]).clamp(0.0, 1.0) * 255.0).round() as u8,
                (linear_to_srgb(shader_fg[2]).clamp(0.0, 1.0) * 255.0).round() as u8,
            ];
            eprintln!(
                "#112 {name}: input ({r:#04x},{g:#04x},{b:#04x}) → packed-linear ({:#04x},{:#04x},{:#04x}) → shader-fg ({:.3},{:.3},{:.3}) → display ({:#04x},{:#04x},{:#04x})",
                packed[0],
                packed[1],
                packed[2],
                shader_fg[0],
                shader_fg[1],
                shader_fg[2],
                displayed[0],
                displayed[1],
                displayed[2],
            );
            // Quantising linear values to u8 loses precision in the
            // bottom of the dynamic range (one linear LSB spans many
            // sRGB LSBs near zero). Allow proportionally more drift for
            // low-luminance channels; tight tolerance for mid / high.
            let tol = |expected: u8| -> u8 { if expected < 0x20 { 4 } else { 1 } };
            assert!(
                displayed[0].abs_diff(*r) <= tol(*r),
                "{name} R drift: expected 0x{r:02x}, got 0x{:02x}",
                displayed[0]
            );
            assert!(
                displayed[1].abs_diff(*g) <= tol(*g),
                "{name} G drift: expected 0x{g:02x}, got 0x{:02x}",
                displayed[1]
            );
            assert!(
                displayed[2].abs_diff(*b) <= tol(*b),
                "{name} B drift: expected 0x{b:02x}, got 0x{:02x}",
                displayed[2]
            );
        }
    }

    /// Issue #112 forensic: reproduces the **pre-fix** (`SRGB_FLAG=1`)
    /// behaviour to characterise the historical bug. Pure white is the
    /// fixed point of `srgb_to_linear`, so pre-fix it ought to still
    /// reach the framebuffer as `0xff`; if user-visible "invisible" was
    /// observed on pure white, it cannot have come from this gamma
    /// double-conversion alone — another factor is at play (font weight,
    /// AA mask, theme palette substitution).
    #[test]
    fn issue_112_prefix_double_conversion_simulation() {
        use kasane_core::config::ColorsConfig;
        use kasane_core::protocol::Brush as KBrush;
        use kasane_core::protocol::Style;

        fn srgb_to_linear(c: f32) -> f32 {
            if c <= 0.040_45 {
                c / 12.92
            } else {
                ((c + 0.055) / 1.055).powf(2.4)
            }
        }
        fn linear_to_srgb(c: f32) -> f32 {
            if c <= 0.003_130_8 {
                c * 12.92
            } else {
                1.055 * c.powf(1.0 / 2.4) - 0.055
            }
        }

        let resolver = crate::colors::ColorResolver::from_config(&ColorsConfig::default());

        let cases: &[(&str, [u8; 3])] = &[
            ("white", [0xff, 0xff, 0xff]),
            ("near-white", [0xee, 0xee, 0xee]),
        ];
        for (name, [r, g, b]) in cases {
            let style = Style {
                fg: KBrush::Solid([*r, *g, *b, 0xff]),
                ..Style::default()
            };
            let (visual_fg, _, _) = resolver.resolve_style_colors_linear(&style);
            let packed = [
                (visual_fg[0].clamp(0.0, 1.0) * 255.0).round() as u8,
                (visual_fg[1].clamp(0.0, 1.0) * 255.0).round() as u8,
                (visual_fg[2].clamp(0.0, 1.0) * 255.0).round() as u8,
            ];
            // **Force the pre-fix branch**: shader applies srgb_to_linear
            // *again* on top of the already-linear packed value.
            let shader_fg = [
                srgb_to_linear(packed[0] as f32 / 255.0),
                srgb_to_linear(packed[1] as f32 / 255.0),
                srgb_to_linear(packed[2] as f32 / 255.0),
            ];
            let displayed = [
                (linear_to_srgb(shader_fg[0]).clamp(0.0, 1.0) * 255.0).round() as u8,
                (linear_to_srgb(shader_fg[1]).clamp(0.0, 1.0) * 255.0).round() as u8,
                (linear_to_srgb(shader_fg[2]).clamp(0.0, 1.0) * 255.0).round() as u8,
            ];
            eprintln!(
                "#112 PRE-FIX {name}: input (#{r:02x},#{g:02x},#{b:02x}) → displayed (#{:02x},#{:02x},#{:02x})",
                displayed[0], displayed[1], displayed[2]
            );
        }
        // Pure white must remain a fixed point even pre-fix. This
        // formally rules out the gamma double-conversion as the cause
        // of "pure white invisible" symptoms; only mid-tones are damped.
        {
            let style = Style {
                fg: KBrush::Solid([0xff, 0xff, 0xff, 0xff]),
                ..Style::default()
            };
            let (visual_fg, _, _) = resolver.resolve_style_colors_linear(&style);
            let packed = (visual_fg[0].clamp(0.0, 1.0) * 255.0).round() as u8;
            let shader_fg = srgb_to_linear(packed as f32 / 255.0);
            let displayed = (linear_to_srgb(shader_fg).clamp(0.0, 1.0) * 255.0).round() as u8;
            assert_eq!(
                displayed, 0xff,
                "pure white should be a fixed point even pre-fix"
            );
        }
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
