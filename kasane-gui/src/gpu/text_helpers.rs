use super::text_pipeline::{TextArea, TextBounds};
use cosmic_text::{
    Attrs, Buffer as GlyphonBuffer, Color as GlyphonColor, FeatureTag, FontFeatures,
};
use kasane_core::element::BorderLineStyle;

/// Build TextAreas from position/buffer slices for a single layer.
///
/// `clip_bounds` provides optional per-buffer clip rects (left, top, right, bottom)
/// to restrict text rendering to a clipped region. `None` entries use full screen bounds.
pub(super) fn prepare_text_areas<'a>(
    positions: &'a [(f32, f32)],
    buffers: &'a [GlyphonBuffer],
    screen_w: f32,
    screen_h: f32,
    clip_bounds: Option<&[(i32, i32, i32, i32)]>,
) -> Vec<TextArea<'a>> {
    positions
        .iter()
        .zip(buffers.iter())
        .enumerate()
        .map(|(i, (&(left, top), buffer))| {
            let bounds = if let Some(clips) = clip_bounds {
                let (cl, ct, cr, cb) = clips[i];
                TextBounds {
                    left: cl,
                    top: ct,
                    right: cr,
                    bottom: cb,
                }
            } else {
                TextBounds {
                    left: 0,
                    top: 0,
                    right: screen_w as i32,
                    bottom: screen_h as i32,
                }
            };
            TextArea {
                buffer,
                left,
                top,
                scale: 1.0,
                bounds,
                default_color: GlyphonColor::rgb(255, 255, 255),
            }
        })
        .collect()
}

/// Map BorderLineStyle to (corner_radius, border_width).
pub(super) fn border_style_params(style: BorderLineStyle, cell_height: f32) -> (f32, f32) {
    // Scale border width with cell size so it looks proportional on any DPI.
    let base = (cell_height * 0.08).max(1.5);
    match style {
        BorderLineStyle::Single => (0.0, base),
        BorderLineStyle::Rounded => (cell_height * 0.3, base),
        BorderLineStyle::Double => (0.0, base),
        BorderLineStyle::Heavy => (0.0, base * 2.0),
        BorderLineStyle::Ascii => (0.0, base),
        BorderLineStyle::Custom(_) => (0.0, base),
    }
}

/// Build default `Attrs` with font family and discretionary ligatures enabled.
pub(super) fn default_attrs(font_family: &str) -> Attrs<'_> {
    let mut features = FontFeatures::new();
    features.enable(FeatureTag::DISCRETIONARY_LIGATURES);
    Attrs::new()
        .family(super::to_family(font_family))
        .font_features(features)
}

pub(super) fn to_glyphon_color(c: [f32; 4]) -> GlyphonColor {
    GlyphonColor::rgba(
        (c[0] * 255.0) as u8,
        (c[1] * 255.0) as u8,
        (c[2] * 255.0) as u8,
        255,
    )
}
