use glyphon::{
    Attrs, Buffer as GlyphonBuffer, Color as GlyphonColor, TextArea, TextBounds,
    cosmic_text::{FeatureTag, FontFeatures},
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
                custom_glyphs: &[],
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

/// Insert Word Joiners (U+2060) after characters whose Unicode line-break
/// class allows a break, preventing `cosmic-text` from splitting operator
/// sequences (e.g. `->`, `!=`, `|>`, `/*`) into separate shaping words.
///
/// Without this, `unicode_linebreak` treats `-` (HY), `!` (EX), `/` (SY),
/// `|` (BA), and `+` (PR) as break opportunities, which splits them from
/// the following character and prevents ligature formation in harfrust.
pub(super) fn insert_word_joiners(text: &mut String, spans: &mut [(usize, usize, [f32; 4])]) {
    const WJ: &str = "\u{2060}";
    const WJ_LEN: usize = 3; // UTF-8 byte length of U+2060

    let bytes = text.as_bytes();
    let mut insert_positions = Vec::new();

    for i in 0..bytes.len().saturating_sub(1) {
        if matches!(bytes[i], b'-' | b'!' | b'/' | b'|' | b'+') {
            // Only insert WJ if the next byte is a printable ASCII non-space
            // character (potential ligature partner).
            let next = bytes[i + 1];
            if next > b' ' && next < 0x7F {
                insert_positions.push(i + 1);
            }
        }
    }

    if insert_positions.is_empty() {
        return;
    }

    // Build new string with WJs inserted at each position.
    let mut new_text = String::with_capacity(text.len() + insert_positions.len() * WJ_LEN);
    let mut last = 0;
    for &pos in &insert_positions {
        new_text.push_str(&text[last..pos]);
        new_text.push_str(WJ);
        last = pos;
    }
    new_text.push_str(&text[last..]);

    // Adjust span byte ranges: each WJ inserted at position p shifts all
    // offsets after p by WJ_LEN bytes.
    for span in spans.iter_mut() {
        let start_shift = insert_positions.partition_point(|&p| p <= span.0) * WJ_LEN;
        let end_shift = insert_positions.partition_point(|&p| p <= span.1) * WJ_LEN;
        span.0 += start_shift;
        span.1 += end_shift;
    }

    *text = new_text;
}

pub(super) fn to_glyphon_color(c: [f32; 4]) -> GlyphonColor {
    GlyphonColor::rgba(
        (c[0] * 255.0) as u8,
        (c[1] * 255.0) as u8,
        (c[2] * 255.0) as u8,
        255,
    )
}
