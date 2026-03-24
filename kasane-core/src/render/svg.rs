//! SVG rendering via `resvg`. Feature-gated behind `svg`.

use std::sync::{Arc, OnceLock};

/// Result of rasterizing an SVG to RGBA pixels.
pub struct SvgRenderResult {
    /// Straight-alpha RGBA8 pixel data.
    pub data: Vec<u8>,
    pub width: u32,
    pub height: u32,
}

/// Maximum pixel dimension for rasterized output (matches GPU MAX_TEXTURE_DIM).
const MAX_PIXEL_DIM: u32 = 8192;

/// Lazily-initialized shared font database for SVG text rendering.
static FONTDB: OnceLock<Arc<resvg::usvg::fontdb::Database>> = OnceLock::new();

fn fontdb() -> Arc<resvg::usvg::fontdb::Database> {
    FONTDB
        .get_or_init(|| {
            let mut db = resvg::usvg::fontdb::Database::new();
            db.load_system_fonts();
            Arc::new(db)
        })
        .clone()
}

fn make_options() -> resvg::usvg::Options<'static> {
    resvg::usvg::Options {
        resources_dir: None, // Security: no external resource loading
        fontdb: fontdb(),
        ..Default::default()
    }
}

/// Rasterize SVG data to RGBA at a specific target pixel size.
///
/// Used by the TUI halfblock path which knows the exact target dimensions.
/// Returns straight-alpha RGBA8 data.
pub fn render_svg_to_rgba(
    svg_data: &[u8],
    target_w: u32,
    target_h: u32,
) -> Result<SvgRenderResult, String> {
    if target_w == 0 || target_h == 0 {
        return Err("zero target dimensions".into());
    }
    if target_w > MAX_PIXEL_DIM || target_h > MAX_PIXEL_DIM {
        return Err(format!("target too large: {target_w}x{target_h}"));
    }

    let options = make_options();
    let tree = resvg::usvg::Tree::from_data(svg_data, &options)
        .map_err(|e| format!("SVG parse error: {e}"))?;

    let mut pixmap = resvg::tiny_skia::Pixmap::new(target_w, target_h)
        .ok_or_else(|| format!("failed to create pixmap {target_w}x{target_h}"))?;

    // Compute scale to fit SVG viewBox into target dimensions
    let svg_size = tree.size();
    let sx = target_w as f32 / svg_size.width();
    let sy = target_h as f32 / svg_size.height();
    let transform = resvg::tiny_skia::Transform::from_scale(sx, sy);

    resvg::render(&tree, transform, &mut pixmap.as_mut());

    let data = unpremultiply(pixmap.take());

    Ok(SvgRenderResult {
        data,
        width: target_w,
        height: target_h,
    })
}

/// Rasterize SVG file from disk at a specific target size.
pub fn render_svg_file_to_rgba(
    path: &str,
    target_w: u32,
    target_h: u32,
) -> Result<SvgRenderResult, String> {
    let data = std::fs::read(path).map_err(|e| format!("failed to read {path}: {e}"))?;
    render_svg_to_rgba(&data, target_w, target_h)
}

/// Rasterize SVG at its intrinsic viewBox size (clamped to max_dim).
///
/// Used by the GPU path which handles fitting at the shader level.
pub fn render_svg_to_rgba_intrinsic(
    svg_data: &[u8],
    max_dim: u32,
) -> Result<SvgRenderResult, String> {
    let options = make_options();
    let tree = resvg::usvg::Tree::from_data(svg_data, &options)
        .map_err(|e| format!("SVG parse error: {e}"))?;

    let svg_size = tree.size();
    let mut w = svg_size.width().ceil() as u32;
    let mut h = svg_size.height().ceil() as u32;

    // Clamp to max_dim preserving aspect ratio
    if w > max_dim || h > max_dim {
        let scale = (max_dim as f32 / w as f32).min(max_dim as f32 / h as f32);
        w = (w as f32 * scale).ceil() as u32;
        h = (h as f32 * scale).ceil() as u32;
    }
    w = w.clamp(1, MAX_PIXEL_DIM);
    h = h.clamp(1, MAX_PIXEL_DIM);

    let mut pixmap = resvg::tiny_skia::Pixmap::new(w, h)
        .ok_or_else(|| format!("failed to create pixmap {w}x{h}"))?;

    resvg::render(
        &tree,
        resvg::tiny_skia::Transform::default(),
        &mut pixmap.as_mut(),
    );

    let data = unpremultiply(pixmap.take());
    Ok(SvgRenderResult {
        data,
        width: w,
        height: h,
    })
}

/// Rasterize SVG file at intrinsic size.
pub fn render_svg_file_to_rgba_intrinsic(
    path: &str,
    max_dim: u32,
) -> Result<SvgRenderResult, String> {
    let data = std::fs::read(path).map_err(|e| format!("failed to read {path}: {e}"))?;
    render_svg_to_rgba_intrinsic(&data, max_dim)
}

/// Check if a file path has an SVG extension.
pub fn is_svg_path(path: &str) -> bool {
    let lower = path.to_ascii_lowercase();
    lower.ends_with(".svg") || lower.ends_with(".svgz")
}

/// Convert premultiplied RGBA to straight RGBA.
fn unpremultiply(mut data: Vec<u8>) -> Vec<u8> {
    for chunk in data.chunks_exact_mut(4) {
        let a = chunk[3] as u16;
        if a > 0 && a < 255 {
            chunk[0] = ((chunk[0] as u16 * 255 + a / 2) / a).min(255) as u8;
            chunk[1] = ((chunk[1] as u16 * 255 + a / 2) / a).min(255) as u8;
            chunk[2] = ((chunk[2] as u16 * 255 + a / 2) / a).min(255) as u8;
        }
    }
    data
}

#[cfg(test)]
mod tests {
    use super::*;

    const SIMPLE_SVG: &[u8] = br#"<svg xmlns="http://www.w3.org/2000/svg" width="100" height="100">
        <rect width="100" height="100" fill="red"/>
    </svg>"#;

    #[test]
    fn render_simple_svg() {
        let result = render_svg_to_rgba(SIMPLE_SVG, 10, 10).unwrap();
        assert_eq!(result.width, 10);
        assert_eq!(result.height, 10);
        assert_eq!(result.data.len(), 10 * 10 * 4);
        // First pixel should be red (fully opaque)
        assert!(result.data[0] > 200); // R
        assert!(result.data[1] < 50); // G
        assert!(result.data[2] < 50); // B
        assert_eq!(result.data[3], 255); // A
    }

    #[test]
    fn render_intrinsic_size() {
        let result = render_svg_to_rgba_intrinsic(SIMPLE_SVG, 8192).unwrap();
        assert_eq!(result.width, 100);
        assert_eq!(result.height, 100);
    }

    #[test]
    fn render_intrinsic_clamped() {
        let result = render_svg_to_rgba_intrinsic(SIMPLE_SVG, 50).unwrap();
        assert!(result.width <= 50);
        assert!(result.height <= 50);
    }

    #[test]
    fn reject_zero_dimensions() {
        assert!(render_svg_to_rgba(SIMPLE_SVG, 0, 10).is_err());
    }

    #[test]
    fn reject_invalid_svg() {
        assert!(render_svg_to_rgba(b"not svg", 10, 10).is_err());
    }

    #[test]
    fn is_svg_path_works() {
        assert!(is_svg_path("diagram.svg"));
        assert!(is_svg_path("DIAGRAM.SVG"));
        assert!(is_svg_path("file.svgz"));
        assert!(!is_svg_path("file.png"));
    }

    #[test]
    fn unpremultiply_opaque() {
        let data = vec![255, 0, 0, 255];
        let result = unpremultiply(data);
        assert_eq!(result, vec![255, 0, 0, 255]);
    }

    #[test]
    fn unpremultiply_transparent() {
        let data = vec![0, 0, 0, 0];
        let result = unpremultiply(data);
        assert_eq!(result, vec![0, 0, 0, 0]);
    }

    #[test]
    fn unpremultiply_half_alpha() {
        // Premultiplied: R=128 with A=128 means straight R~255
        let data = vec![128, 0, 0, 128];
        let result = unpremultiply(data);
        assert!(result[0] >= 254); // ~255
        assert_eq!(result[1], 0);
        assert_eq!(result[2], 0);
        assert_eq!(result[3], 128);
    }
}
