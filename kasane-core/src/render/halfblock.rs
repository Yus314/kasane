//! Halfblock image rendering for TUI.
//!
//! Uses Unicode upper half block `▀` (U+2580) to render images at
//! 1-cell = 2-pixel-rows resolution (fg = top row, bg = bottom row).
//! Works on any TrueColor-capable terminal, fitting entirely within the
//! existing CellGrid model.

use crate::element::{ImageFit, ImageSource};
use crate::layout::Rect;
use crate::protocol::{Color, WireFace};

use super::grid::CellGrid;
use super::paint::paint_text;

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

/// A single halfblock cell: upper pixel (fg) and lower pixel (bg).
#[derive(Debug, Clone, Copy)]
pub struct HalfblockCell {
    pub top: (u8, u8, u8),
    pub bot: (u8, u8, u8),
}

/// Result of fitting an image into a cell area.
#[derive(Debug, Clone)]
pub struct FitResult {
    /// Offset within the target area (cells).
    pub dst_x: u16,
    pub dst_y: u16,
    /// Render size in cells.
    pub dst_w: u16,
    pub dst_h: u16,
    /// Source image crop rectangle (pixels).
    pub crop_x: u32,
    pub crop_y: u32,
    pub crop_w: u32,
    pub crop_h: u32,
}

// ---------------------------------------------------------------------------
// compute_fit_cells  (unconditional — no feature gate)
// ---------------------------------------------------------------------------

/// Compute how an image fits into a cell area, analogous to the GPU
/// `compute_fit` but operating in cell coordinates.
///
/// Halfblock effective pixel resolution: `(area_w, area_h * 2)`.
pub fn compute_fit_cells(
    img_w: u32,
    img_h: u32,
    area_w: u16,
    area_h: u16,
    fit: ImageFit,
) -> FitResult {
    if img_w == 0 || img_h == 0 || area_w == 0 || area_h == 0 {
        return FitResult {
            dst_x: 0,
            dst_y: 0,
            dst_w: 0,
            dst_h: 0,
            crop_x: 0,
            crop_y: 0,
            crop_w: img_w,
            crop_h: img_h,
        };
    }

    match fit {
        ImageFit::Fill => FitResult {
            dst_x: 0,
            dst_y: 0,
            dst_w: area_w,
            dst_h: area_h,
            crop_x: 0,
            crop_y: 0,
            crop_w: img_w,
            crop_h: img_h,
        },
        ImageFit::Contain => {
            // Effective pixel resolution of the area.
            let px_w = area_w as f64;
            let px_h = (area_h as f64) * 2.0;

            let img_aspect = img_w as f64 / img_h as f64;
            let area_aspect = px_w / px_h;

            let (fit_px_w, fit_px_h) = if img_aspect > area_aspect {
                // Image wider → fit to width
                (px_w, px_w / img_aspect)
            } else {
                // Image taller → fit to height
                (px_h * img_aspect, px_h)
            };

            // Convert back to cells. Height is in half-rows, so divide by 2.
            let dst_w = (fit_px_w.round() as u16).min(area_w).max(1);
            let dst_h = ((fit_px_h / 2.0).round() as u16).min(area_h).max(1);

            let dst_x = (area_w.saturating_sub(dst_w)) / 2;
            let dst_y = (area_h.saturating_sub(dst_h)) / 2;

            FitResult {
                dst_x,
                dst_y,
                dst_w,
                dst_h,
                crop_x: 0,
                crop_y: 0,
                crop_w: img_w,
                crop_h: img_h,
            }
        }
        ImageFit::Cover => {
            // Fill entire area, crop source to preserve aspect ratio.
            let px_w = area_w as f64;
            let px_h = (area_h as f64) * 2.0;

            let img_aspect = img_w as f64 / img_h as f64;
            let area_aspect = px_w / px_h;

            let (crop_w, crop_h, crop_x, crop_y) = if img_aspect > area_aspect {
                // Image wider → crop sides
                let visible_w = (img_h as f64 * area_aspect).round() as u32;
                let visible_w = visible_w.min(img_w).max(1);
                let offset = (img_w.saturating_sub(visible_w)) / 2;
                (visible_w, img_h, offset, 0u32)
            } else {
                // Image taller → crop top/bottom
                let visible_h = (img_w as f64 / area_aspect).round() as u32;
                let visible_h = visible_h.min(img_h).max(1);
                let offset = (img_h.saturating_sub(visible_h)) / 2;
                (img_w, visible_h, 0u32, offset)
            };

            FitResult {
                dst_x: 0,
                dst_y: 0,
                dst_w: area_w,
                dst_h: area_h,
                crop_x,
                crop_y,
                crop_w,
                crop_h,
            }
        }
    }
}

// ---------------------------------------------------------------------------
// paint_halfblock  (unconditional)
// ---------------------------------------------------------------------------

/// Write halfblock cells into a CellGrid at the given area + fit offset.
pub fn paint_halfblock(grid: &mut CellGrid, area: &Rect, cells: &[HalfblockCell], fit: &FitResult) {
    if fit.dst_w == 0 || fit.dst_h == 0 {
        return;
    }
    for cy in 0..fit.dst_h {
        for cx in 0..fit.dst_w {
            let idx = cy as usize * fit.dst_w as usize + cx as usize;
            if idx >= cells.len() {
                return;
            }
            let cell = &cells[idx];
            let gx = area.x + fit.dst_x + cx;
            let gy = area.y + fit.dst_y + cy;
            let face = WireFace {
                fg: Color::Rgb {
                    r: cell.top.0,
                    g: cell.top.1,
                    b: cell.top.2,
                },
                bg: Color::Rgb {
                    r: cell.bot.0,
                    g: cell.bot.1,
                    b: cell.bot.2,
                },
                ..WireFace::default()
            };
            let style = crate::render::TerminalStyle::from_face(&face);
            grid.put_char(gx, gy, "\u{2580}", &style);
        }
    }
}

// ---------------------------------------------------------------------------
// Cache
// ---------------------------------------------------------------------------

/// LRU cache for decoded halfblock image data.
///
/// Always defined so that pipeline types can reference it unconditionally.
/// Internal methods are only available when `tui-image` is enabled.
pub struct HalfblockCache {
    #[cfg(feature = "tui-image")]
    entries: Vec<(CacheKey, CacheEntry)>,
    #[cfg(feature = "tui-image")]
    capacity: usize,
}

impl HalfblockCache {
    pub fn new(_capacity: usize) -> Self {
        Self {
            #[cfg(feature = "tui-image")]
            entries: Vec::new(),
            #[cfg(feature = "tui-image")]
            capacity: _capacity,
        }
    }
}

#[cfg(feature = "tui-image")]
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
enum CacheKey {
    FilePath(String, u16, u16),
    Inline(u64, u16, u16),
}

#[cfg(feature = "tui-image")]
enum CacheEntry {
    Ready(Vec<HalfblockCell>, FitResult),
    Failed,
}

#[cfg(feature = "tui-image")]
impl HalfblockCache {
    fn get(&mut self, key: &CacheKey) -> Option<&CacheEntry> {
        // Move to end on hit (LRU)
        let pos = self.entries.iter().position(|(k, _)| k == key)?;
        let entry = self.entries.remove(pos);
        self.entries.push(entry);
        Some(&self.entries.last().unwrap().1)
    }

    fn insert(&mut self, key: CacheKey, entry: CacheEntry) {
        // Evict oldest if at capacity
        if self.entries.len() >= self.capacity {
            self.entries.remove(0);
        }
        self.entries.push((key, entry));
    }

    #[cfg(test)]
    fn len(&self) -> usize {
        self.entries.len()
    }
}

#[cfg(feature = "tui-image")]
fn make_cache_key(source: &ImageSource, area_w: u16, area_h: u16) -> CacheKey {
    match source {
        ImageSource::FilePath(path) => CacheKey::FilePath(path.clone(), area_w, area_h),
        ImageSource::Rgba {
            data,
            width,
            height,
        } => {
            let hash = inline_hash(data, *width, *height);
            CacheKey::Inline(hash, area_w, area_h)
        }
        ImageSource::SvgData { data } => {
            let hash = inline_hash(data, 0, 0);
            CacheKey::Inline(hash, area_w, area_h)
        }
    }
}

/// Content-sampling hash for inline RGBA data (matches texture_cache.rs logic).
#[cfg(feature = "tui-image")]
fn inline_hash(data: &[u8], width: u32, height: u32) -> u64 {
    use std::hash::{Hash, Hasher};
    let mut hasher = std::hash::DefaultHasher::new();
    width.hash(&mut hasher);
    height.hash(&mut hasher);
    data.len().hash(&mut hasher);
    const SAMPLE: usize = 64;
    data[..data.len().min(SAMPLE)].hash(&mut hasher);
    if data.len() > SAMPLE * 2 {
        let mid = data.len() / 2;
        data[mid..mid + SAMPLE.min(data.len() - mid)].hash(&mut hasher);
    }
    if data.len() > SAMPLE {
        data[data.len() - SAMPLE..].hash(&mut hasher);
    }
    hasher.finish()
}

// ---------------------------------------------------------------------------
// Feature-gated decode + render
// ---------------------------------------------------------------------------

#[cfg(feature = "tui-image")]
mod decode {
    use super::*;

    /// Decode an image source and produce halfblock cells + fit result.
    pub(super) fn decode_and_render(
        source: &ImageSource,
        area_w: u16,
        area_h: u16,
        fit: ImageFit,
    ) -> Result<(Vec<HalfblockCell>, FitResult), String> {
        let (img_w, img_h, rgba) = match source {
            ImageSource::FilePath(path) => {
                #[cfg(feature = "svg")]
                if super::super::svg::is_svg_path(path) {
                    let target_px_w = area_w as u32;
                    let target_px_h = area_h as u32 * 2;
                    if target_px_w == 0 || target_px_h == 0 {
                        return Ok((vec![], compute_fit_cells(1, 1, area_w, area_h, fit)));
                    }
                    let r =
                        super::super::svg::render_svg_file_to_rgba(path, target_px_w, target_px_h)
                            .map_err(|e| format!("SVG render failed for {path}: {e}"))?;
                    let img = image::RgbaImage::from_raw(r.width, r.height, r.data)
                        .ok_or_else(|| "SVG RGBA conversion failed".to_string())?;
                    (r.width, r.height, img)
                } else {
                    let img = image::open(path)
                        .map_err(|e| format!("failed to open {path}: {e}"))?
                        .to_rgba8();
                    let (w, h) = img.dimensions();
                    (w, h, img)
                }
                #[cfg(not(feature = "svg"))]
                {
                    let img = image::open(path)
                        .map_err(|e| format!("failed to open {path}: {e}"))?
                        .to_rgba8();
                    let (w, h) = img.dimensions();
                    (w, h, img)
                }
            }
            ImageSource::Rgba {
                data,
                width,
                height,
            } => {
                let img = image::RgbaImage::from_raw(*width, *height, data.to_vec())
                    .ok_or_else(|| "invalid RGBA dimensions".to_string())?;
                (*width, *height, img)
            }
            ImageSource::SvgData { data } => {
                #[cfg(feature = "svg")]
                {
                    let target_px_w = area_w as u32;
                    let target_px_h = area_h as u32 * 2;
                    if target_px_w == 0 || target_px_h == 0 {
                        return Ok((vec![], compute_fit_cells(1, 1, area_w, area_h, fit)));
                    }
                    let r = super::super::svg::render_svg_to_rgba(data, target_px_w, target_px_h)
                        .map_err(|e| format!("SVG render failed: {e}"))?;
                    let img = image::RgbaImage::from_raw(r.width, r.height, r.data)
                        .ok_or_else(|| "SVG RGBA conversion failed".to_string())?;
                    (r.width, r.height, img)
                }
                #[cfg(not(feature = "svg"))]
                return Err("SVG support not enabled".into());
            }
        };

        let fit_result = compute_fit_cells(img_w, img_h, area_w, area_h, fit);
        if fit_result.dst_w == 0 || fit_result.dst_h == 0 {
            return Ok((vec![], fit_result));
        }

        // Crop for Cover mode (or use full image for Fill/Contain)
        let cropped = if fit_result.crop_x != 0
            || fit_result.crop_y != 0
            || fit_result.crop_w != img_w
            || fit_result.crop_h != img_h
        {
            image::imageops::crop_imm(
                &rgba,
                fit_result.crop_x,
                fit_result.crop_y,
                fit_result.crop_w,
                fit_result.crop_h,
            )
            .to_image()
        } else {
            rgba
        };

        // Resize to target pixel resolution (dst_w × dst_h*2)
        let target_px_w = fit_result.dst_w as u32;
        let target_px_h = fit_result.dst_h as u32 * 2;
        let resized = image::imageops::resize(
            &cropped,
            target_px_w,
            target_px_h,
            image::imageops::Triangle,
        );

        // Convert pixel pairs to halfblock cells
        let mut cells = Vec::with_capacity(fit_result.dst_w as usize * fit_result.dst_h as usize);
        for row in 0..fit_result.dst_h as u32 {
            for col in 0..fit_result.dst_w as u32 {
                let top_px = resized.get_pixel(col, row * 2);
                let bot_px = resized.get_pixel(col, row * 2 + 1);
                cells.push(HalfblockCell {
                    top: (top_px[0], top_px[1], top_px[2]),
                    bot: (bot_px[0], bot_px[1], bot_px[2]),
                });
            }
        }

        Ok((cells, fit_result))
    }
}

/// Attempt to render an image as halfblock characters into the grid.
///
/// Returns `true` if the image was rendered (or previously failed and cached),
/// `false` only when the feature is disabled.
#[cfg(feature = "tui-image")]
pub fn render_to_grid(
    grid: &mut CellGrid,
    source: &ImageSource,
    fit: ImageFit,
    area: &Rect,
    cache: &mut HalfblockCache,
) -> bool {
    let key = make_cache_key(source, area.w, area.h);

    if let Some(entry) = cache.get(&key) {
        return match entry {
            CacheEntry::Ready(cells, fit_result) => {
                paint_halfblock(grid, area, cells, fit_result);
                true
            }
            CacheEntry::Failed => false,
        };
    }

    match decode::decode_and_render(source, area.w, area.h, fit) {
        Ok((cells, fit_result)) => {
            paint_halfblock(grid, area, &cells, &fit_result);
            cache.insert(key, CacheEntry::Ready(cells, fit_result));
            true
        }
        Err(msg) => {
            tracing::warn!("halfblock decode failed: {msg}");
            cache.insert(key, CacheEntry::Failed);
            false
        }
    }
}

// ---------------------------------------------------------------------------
// Fallback text placeholder (used when render_to_grid returns false or
// feature is off)
// ---------------------------------------------------------------------------

/// Paint the text fallback placeholder for an image element.
pub(crate) fn paint_image_fallback(grid: &mut CellGrid, source: &ImageSource, area: &Rect) {
    let label = match source {
        ImageSource::FilePath(path) => {
            let filename = std::path::Path::new(path)
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or(path);
            format!("[IMAGE: {filename}]")
        }
        ImageSource::Rgba { width, height, .. } => {
            format!("[IMAGE: {width}\u{00d7}{height}]")
        }
        ImageSource::SvgData { .. } => "[SVG]".to_string(),
    };
    let face = WireFace {
        attributes: crate::protocol::Attributes::DIM,
        ..WireFace::default()
    };
    paint_text(grid, area, &label, &face);
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    // -- compute_fit_cells tests --

    #[test]
    fn fit_fill_uses_full_area() {
        let r = compute_fit_cells(100, 200, 20, 10, ImageFit::Fill);
        assert_eq!((r.dst_x, r.dst_y, r.dst_w, r.dst_h), (0, 0, 20, 10));
        assert_eq!((r.crop_x, r.crop_y, r.crop_w, r.crop_h), (0, 0, 100, 200));
    }

    #[test]
    fn fit_contain_wider_image() {
        // Image 200×100 into area 20×10 (pixel area 20×20)
        // aspect = 2.0 > 1.0 → fit to width: px_w=20, px_h=10
        // dst_w = 20, dst_h = 10/2 = 5, centered vertically
        let r = compute_fit_cells(200, 100, 20, 10, ImageFit::Contain);
        assert_eq!(r.dst_w, 20);
        assert_eq!(r.dst_h, 5);
        assert_eq!(r.dst_y, 2); // (10-5)/2 = 2
        assert_eq!(r.dst_x, 0);
    }

    #[test]
    fn fit_contain_taller_image() {
        // Image 100×400 into area 20×10 (pixel area 20×20)
        // aspect = 0.25 < 1.0 → fit to height: px_h=20, px_w=5
        // dst_w = 5, dst_h = 10, centered horizontally
        let r = compute_fit_cells(100, 400, 20, 10, ImageFit::Contain);
        assert_eq!(r.dst_h, 10);
        assert_eq!(r.dst_w, 5);
        assert_eq!(r.dst_x, 7); // (20-5)/2 = 7
        assert_eq!(r.dst_y, 0);
    }

    #[test]
    fn fit_contain_exact_aspect() {
        // Image 40×20 into area 20×10 (pixel area 20×20)
        // aspect = 2.0 > 1.0 → fit to width
        let r = compute_fit_cells(40, 20, 20, 10, ImageFit::Contain);
        assert_eq!(r.dst_w, 20);
        assert_eq!(r.dst_h, 5);
    }

    #[test]
    fn fit_cover_wider_image() {
        // Image 200×100 into area 10×10 (pixel area 10×20)
        // img_aspect=2.0 > area_aspect=0.5 → crop sides
        let r = compute_fit_cells(200, 100, 10, 10, ImageFit::Cover);
        assert_eq!((r.dst_x, r.dst_y, r.dst_w, r.dst_h), (0, 0, 10, 10));
        // visible_w = 100 * 0.5 = 50, offset = (200-50)/2 = 75
        assert_eq!(r.crop_x, 75);
        assert_eq!(r.crop_w, 50);
        assert_eq!(r.crop_y, 0);
        assert_eq!(r.crop_h, 100);
    }

    #[test]
    fn fit_cover_taller_image() {
        // Image 100×400 into area 20×5 (pixel area 20×10)
        // img_aspect=0.25, area_aspect=2.0 → taller → crop top/bottom
        let r = compute_fit_cells(100, 400, 20, 5, ImageFit::Cover);
        assert_eq!((r.dst_x, r.dst_y, r.dst_w, r.dst_h), (0, 0, 20, 5));
        // visible_h = 100/2.0 = 50, offset = (400-50)/2 = 175
        assert_eq!(r.crop_y, 175);
        assert_eq!(r.crop_h, 50);
        assert_eq!(r.crop_x, 0);
        assert_eq!(r.crop_w, 100);
    }

    #[test]
    fn fit_zero_area() {
        let r = compute_fit_cells(100, 100, 0, 10, ImageFit::Fill);
        assert_eq!(r.dst_w, 0);
        assert_eq!(r.dst_h, 0);
    }

    #[test]
    fn fit_zero_image() {
        let r = compute_fit_cells(0, 100, 20, 10, ImageFit::Contain);
        assert_eq!(r.dst_w, 0);
        assert_eq!(r.dst_h, 0);
    }

    #[test]
    fn paint_halfblock_writes_cells() {
        let mut grid = CellGrid::new(10, 5);
        let area = Rect {
            x: 1,
            y: 1,
            w: 4,
            h: 3,
        };
        let cells = vec![
            HalfblockCell {
                top: (255, 0, 0),
                bot: (0, 255, 0),
            },
            HalfblockCell {
                top: (0, 0, 255),
                bot: (128, 128, 128),
            },
        ];
        let fit = FitResult {
            dst_x: 0,
            dst_y: 0,
            dst_w: 2,
            dst_h: 1,
            crop_x: 0,
            crop_y: 0,
            crop_w: 2,
            crop_h: 2,
        };
        paint_halfblock(&mut grid, &area, &cells, &fit);

        let c0 = grid.get(1, 1).unwrap();
        assert_eq!(c0.grapheme.as_str(), "\u{2580}");
        assert_eq!(c0.style.fg, Color::Rgb { r: 255, g: 0, b: 0 });
        assert_eq!(c0.style.bg, Color::Rgb { r: 0, g: 255, b: 0 });

        let c1 = grid.get(2, 1).unwrap();
        assert_eq!(c1.grapheme.as_str(), "\u{2580}");
        assert_eq!(c1.style.fg, Color::Rgb { r: 0, g: 0, b: 255 });
        assert_eq!(
            c1.style.bg,
            Color::Rgb {
                r: 128,
                g: 128,
                b: 128
            }
        );
    }

    #[test]
    fn paint_halfblock_with_offset() {
        let mut grid = CellGrid::new(10, 10);
        let area = Rect {
            x: 0,
            y: 0,
            w: 10,
            h: 10,
        };
        let cells = vec![HalfblockCell {
            top: (1, 2, 3),
            bot: (4, 5, 6),
        }];
        let fit = FitResult {
            dst_x: 3,
            dst_y: 4,
            dst_w: 1,
            dst_h: 1,
            crop_x: 0,
            crop_y: 0,
            crop_w: 1,
            crop_h: 2,
        };
        paint_halfblock(&mut grid, &area, &cells, &fit);

        // Should be at grid position (3, 4)
        let c = grid.get(3, 4).unwrap();
        assert_eq!(c.grapheme.as_str(), "\u{2580}");
        assert_eq!(c.style.fg, Color::Rgb { r: 1, g: 2, b: 3 });
    }

    // -- Feature-gated decode tests --

    #[cfg(feature = "tui-image")]
    mod decode_tests {
        use super::*;
        use std::sync::Arc;

        /// Helper trait for color assertions in tests.
        trait ColorAssert {
            fn is_red_ish(&self) -> bool;
            fn is_blue_ish(&self) -> bool;
            fn green_channel(&self) -> u8;
        }
        impl ColorAssert for Color {
            fn is_red_ish(&self) -> bool {
                matches!(self, Color::Rgb { r, g, b } if *r > 200 && *g < 50 && *b < 50)
            }
            fn is_blue_ish(&self) -> bool {
                matches!(self, Color::Rgb { r, g, b } if *r < 50 && *g < 50 && *b > 200)
            }
            fn green_channel(&self) -> u8 {
                match self {
                    Color::Rgb { g, .. } => *g,
                    _ => 0,
                }
            }
        }

        #[test]
        fn decode_rgba_4x4() {
            // Create a 4x4 RGBA image: top half red, bottom half blue
            let mut data = vec![0u8; 4 * 4 * 4];
            for y in 0..4u32 {
                for x in 0..4u32 {
                    let i = (y * 4 + x) as usize * 4;
                    if y < 2 {
                        data[i] = 255; // R
                        data[i + 3] = 255; // A
                    } else {
                        data[i + 2] = 255; // B
                        data[i + 3] = 255; // A
                    }
                }
            }
            let source = ImageSource::Rgba {
                data: Arc::from(data),
                width: 4,
                height: 4,
            };
            let result = decode::decode_and_render(&source, 4, 2, ImageFit::Fill);
            let (cells, fit) = result.unwrap();
            assert_eq!(fit.dst_w, 4);
            assert_eq!(fit.dst_h, 2);
            assert_eq!(cells.len(), 8); // 4*2

            // First row: top=red(ish), bot should transition
            // (resize from 4px height to 4px height, so row0 top=row0 red, bot=row1 red)
            let c = &cells[0];
            assert!(c.top.0 > 200, "expected reddish top, got {:?}", c.top); // red channel high
            assert!(c.top.2 < 50, "expected low blue in top"); // blue channel low
        }

        #[test]
        fn cache_hit_avoids_decode() {
            let data: Arc<[u8]> = vec![128u8; 4 * 2 * 2].into();
            let source = ImageSource::Rgba {
                data,
                width: 2,
                height: 2,
            };
            let mut grid = CellGrid::new(10, 5);
            let area = Rect {
                x: 0,
                y: 0,
                w: 4,
                h: 2,
            };
            let mut cache = HalfblockCache::new(16);

            // First call: cache miss → decode
            let ok1 = render_to_grid(&mut grid, &source, ImageFit::Fill, &area, &mut cache);
            assert!(ok1);
            assert_eq!(cache.len(), 1);

            // Second call: cache hit
            let ok2 = render_to_grid(&mut grid, &source, ImageFit::Fill, &area, &mut cache);
            assert!(ok2);
            assert_eq!(cache.len(), 1); // no new entries
        }

        #[test]
        fn error_fallback_records_failed() {
            let source = ImageSource::FilePath("/nonexistent/image.png".into());
            let mut grid = CellGrid::new(20, 3);
            let area = Rect {
                x: 0,
                y: 0,
                w: 20,
                h: 3,
            };
            let mut cache = HalfblockCache::new(16);

            let ok = render_to_grid(&mut grid, &source, ImageFit::Fill, &area, &mut cache);
            assert!(!ok); // Failed → returns false for fallback
            assert_eq!(cache.len(), 1);

            // Second call also returns false (cached failure)
            let ok2 = render_to_grid(&mut grid, &source, ImageFit::Fill, &area, &mut cache);
            assert!(!ok2);
            assert_eq!(cache.len(), 1);
        }

        /// End-to-end: solid red 2×2 image → Fill into 2×1 area → all cells red.
        #[test]
        fn e2e_solid_red_fill() {
            let data: Arc<[u8]> = vec![
                255, 0, 0, 255, 255, 0, 0, 255, 255, 0, 0, 255, 255, 0, 0, 255,
            ]
            .into();
            let source = ImageSource::Rgba {
                data,
                width: 2,
                height: 2,
            };
            let mut grid = CellGrid::new(2, 1);
            let area = Rect {
                x: 0,
                y: 0,
                w: 2,
                h: 1,
            };
            let mut cache = HalfblockCache::new(16);

            let ok = render_to_grid(&mut grid, &source, ImageFit::Fill, &area, &mut cache);
            assert!(ok);
            for x in 0..2u16 {
                let c = grid.get(x, 0).unwrap();
                assert_eq!(c.grapheme.as_str(), "\u{2580}", "cell ({x},0)");
                assert_eq!(c.style.fg, Color::Rgb { r: 255, g: 0, b: 0 }, "fg ({x},0)");
                assert_eq!(c.style.bg, Color::Rgb { r: 255, g: 0, b: 0 }, "bg ({x},0)");
            }
        }

        /// End-to-end: top-red/bottom-blue 2×4 image → Fill into 2×2 → row0 red/red, row1 blue/blue.
        #[test]
        fn e2e_two_color_vertical_fill() {
            // 2×4 image: rows 0-1 red, rows 2-3 blue
            let mut data = vec![0u8; 2 * 4 * 4];
            for y in 0..4u32 {
                for x in 0..2u32 {
                    let i = (y * 2 + x) as usize * 4;
                    if y < 2 {
                        data[i] = 255;
                        data[i + 3] = 255; // red
                    } else {
                        data[i + 2] = 255;
                        data[i + 3] = 255; // blue
                    }
                }
            }
            let source = ImageSource::Rgba {
                data: Arc::from(data),
                width: 2,
                height: 4,
            };
            let mut grid = CellGrid::new(2, 2);
            let area = Rect {
                x: 0,
                y: 0,
                w: 2,
                h: 2,
            };
            let mut cache = HalfblockCache::new(16);

            let ok = render_to_grid(&mut grid, &source, ImageFit::Fill, &area, &mut cache);
            assert!(ok);

            // Row 0: halfblock top=row0 (red), bot=row1 (red)
            for x in 0..2u16 {
                let c = grid.get(x, 0).unwrap();
                assert_eq!(c.grapheme.as_str(), "\u{2580}");
                assert!(
                    c.style.fg.is_red_ish(),
                    "row0 fg should be red: {:?}",
                    c.style.fg
                );
                assert!(
                    c.style.fg.is_red_ish(),
                    "row0 bg should be red: {:?}",
                    c.style.bg
                );
            }
            // Row 1: halfblock top=row2 (blue), bot=row3 (blue)
            for x in 0..2u16 {
                let c = grid.get(x, 1).unwrap();
                assert_eq!(c.grapheme.as_str(), "\u{2580}");
                assert!(
                    c.style.fg.is_blue_ish(),
                    "row1 fg should be blue: {:?}",
                    c.style.fg
                );
                assert!(
                    c.style.bg.is_blue_ish(),
                    "row1 bg should be blue: {:?}",
                    c.style.bg
                );
            }
        }

        /// End-to-end: Contain with non-matching aspect leaves letterbox.
        #[test]
        fn e2e_contain_letterbox() {
            // 4×2 image (wide) into 4×4 area → should letterbox vertically
            let data: Arc<[u8]> = vec![0, 255, 0, 255].repeat(4 * 2).into();
            let source = ImageSource::Rgba {
                data,
                width: 4,
                height: 2,
            };
            let mut grid = CellGrid::new(4, 4);
            let area = Rect {
                x: 0,
                y: 0,
                w: 4,
                h: 4,
            };
            let mut cache = HalfblockCache::new(16);

            let ok = render_to_grid(&mut grid, &source, ImageFit::Contain, &area, &mut cache);
            assert!(ok);

            // With a 4×2 image in a 4×4 area (pixel 4×8), aspect=2.0 > 0.5
            // fit to width: px_w=4, px_h=2 → dst_w=4, dst_h=1, centered at y=1
            // Row 0 and rows 2-3 should be empty (default)
            let empty_0 = grid.get(0, 0).unwrap();
            assert_ne!(
                empty_0.grapheme.as_str(),
                "\u{2580}",
                "row 0 should be letterbox"
            );

            // The rendered row should have halfblock chars
            let fit = compute_fit_cells(4, 2, 4, 4, ImageFit::Contain);
            let rendered_y = fit.dst_y;
            let rendered = grid.get(0, rendered_y).unwrap();
            assert_eq!(
                rendered.grapheme.as_str(),
                "\u{2580}",
                "rendered row should have halfblock"
            );
            assert_eq!(
                rendered.style.fg,
                Color::Rgb { r: 0, g: 255, b: 0 },
                "rendered pixel should be green"
            );
        }

        /// End-to-end: Cover with wider image crops sides.
        #[test]
        fn e2e_cover_crops() {
            // 8×2 image into 2×1 area → Cover crops sides
            // Left 3px red, middle 2px green, right 3px blue
            let mut data = vec![0u8; 8 * 2 * 4];
            for y in 0..2u32 {
                for x in 0..8u32 {
                    let i = (y * 8 + x) as usize * 4;
                    data[i + 3] = 255; // alpha
                    if x < 3 {
                        data[i] = 255; // red
                    } else if x < 5 {
                        data[i + 1] = 255; // green
                    } else {
                        data[i + 2] = 255; // blue
                    }
                }
            }
            let source = ImageSource::Rgba {
                data: Arc::from(data),
                width: 8,
                height: 2,
            };
            let mut grid = CellGrid::new(2, 1);
            let area = Rect {
                x: 0,
                y: 0,
                w: 2,
                h: 1,
            };
            let mut cache = HalfblockCache::new(16);

            let ok = render_to_grid(&mut grid, &source, ImageFit::Cover, &area, &mut cache);
            assert!(ok);

            // Cover: area pixel 2×2, img aspect 4.0 > area aspect 1.0
            // visible_w = 2 * (2/2) / (8/2) = ... let's just check center is green
            for x in 0..2u16 {
                let c = grid.get(x, 0).unwrap();
                assert_eq!(c.grapheme.as_str(), "\u{2580}", "cover cell ({x},0)");
                // The center crop should be green-ish (the middle of the image)
                assert!(
                    c.style.fg.green_channel() > 100,
                    "cover center should have green: {:?}",
                    c.style.fg
                );
            }
        }

        /// Different area size creates a new cache entry.
        #[test]
        fn cache_different_size_is_miss() {
            let data: Arc<[u8]> = vec![128u8; 4 * 2 * 2].into();
            let source = ImageSource::Rgba {
                data,
                width: 2,
                height: 2,
            };
            let mut cache = HalfblockCache::new(16);

            let mut grid1 = CellGrid::new(4, 2);
            let area1 = Rect {
                x: 0,
                y: 0,
                w: 4,
                h: 2,
            };
            render_to_grid(&mut grid1, &source, ImageFit::Fill, &area1, &mut cache);
            assert_eq!(cache.len(), 1);

            let mut grid2 = CellGrid::new(8, 4);
            let area2 = Rect {
                x: 0,
                y: 0,
                w: 8,
                h: 4,
            };
            render_to_grid(&mut grid2, &source, ImageFit::Fill, &area2, &mut cache);
            assert_eq!(
                cache.len(),
                2,
                "different area size should create new cache entry"
            );
        }

        /// LRU eviction when cache is full.
        #[test]
        fn cache_lru_eviction() {
            let mut cache = HalfblockCache::new(2);
            let mut grid = CellGrid::new(4, 2);

            for i in 0..3u8 {
                let data: Arc<[u8]> = vec![i; 4 * 2 * 2].into();
                let source = ImageSource::Rgba {
                    data,
                    width: 2,
                    height: 2,
                };
                let area = Rect {
                    x: 0,
                    y: 0,
                    w: 4,
                    h: 2,
                };
                render_to_grid(&mut grid, &source, ImageFit::Fill, &area, &mut cache);
            }
            // Capacity is 2, so oldest entry should have been evicted
            assert_eq!(cache.len(), 2);
        }

        #[cfg(feature = "svg")]
        mod svg_integration {
            use super::*;

            const RED_SVG: &[u8] =
                br#"<svg xmlns="http://www.w3.org/2000/svg" width="100" height="100">
                <rect width="100" height="100" fill="red"/>
            </svg>"#;

            const RED_BLUE_SVG: &[u8] =
                br#"<svg xmlns="http://www.w3.org/2000/svg" width="100" height="100">
                <rect y="0" width="100" height="50" fill="red"/>
                <rect y="50" width="100" height="50" fill="blue"/>
            </svg>"#;

            /// SvgData decode_and_render produces valid halfblock cells.
            #[test]
            fn svg_data_decode_renders_red() {
                let source = ImageSource::SvgData {
                    data: Arc::from(RED_SVG),
                };
                let (cells, fit) =
                    decode::decode_and_render(&source, 4, 2, ImageFit::Fill).unwrap();
                assert_eq!(fit.dst_w, 4);
                assert_eq!(fit.dst_h, 2);
                assert_eq!(cells.len(), 8); // 4 * 2
                // All cells should be red
                for (i, c) in cells.iter().enumerate() {
                    assert!(c.top.0 > 200, "cell {i} top.R={} expected red", c.top.0);
                    assert!(c.top.1 < 50, "cell {i} top.G={} expected low", c.top.1);
                    assert!(c.bot.0 > 200, "cell {i} bot.R={} expected red", c.bot.0);
                }
            }

            /// SvgData end-to-end through render_to_grid writes halfblock chars.
            #[test]
            fn svg_data_e2e_grid() {
                let source = ImageSource::SvgData {
                    data: Arc::from(RED_SVG),
                };
                let mut grid = CellGrid::new(4, 2);
                let area = Rect {
                    x: 0,
                    y: 0,
                    w: 4,
                    h: 2,
                };
                let mut cache = HalfblockCache::new(16);
                let ok = render_to_grid(&mut grid, &source, ImageFit::Fill, &area, &mut cache);
                assert!(ok, "render_to_grid should succeed for SvgData");
                // Verify halfblock chars written
                let c = grid.get(0, 0).unwrap();
                assert_eq!(c.grapheme.as_str(), "\u{2580}");
                assert!(
                    c.style.fg.is_red_ish(),
                    "fg should be red: {:?}",
                    c.style.fg
                );
            }

            /// Two-color SVG: top red, bottom blue → row 0 red, row 1 blue.
            #[test]
            fn svg_data_two_color_vertical() {
                let source = ImageSource::SvgData {
                    data: Arc::from(RED_BLUE_SVG),
                };
                let (cells, fit) =
                    decode::decode_and_render(&source, 2, 2, ImageFit::Fill).unwrap();
                assert_eq!(fit.dst_w, 2);
                assert_eq!(fit.dst_h, 2);
                // Row 0 (top half of SVG) → red
                assert!(cells[0].top.0 > 200, "row0 top should be red");
                assert!(cells[0].bot.0 > 200, "row0 bot should be red");
                // Row 1 (bottom half of SVG) → blue
                assert!(cells[2].top.2 > 200, "row1 top should be blue");
                assert!(cells[2].bot.2 > 200, "row1 bot should be blue");
            }

            /// SvgData cache hit.
            #[test]
            fn svg_data_cache_hit() {
                let source = ImageSource::SvgData {
                    data: Arc::from(RED_SVG),
                };
                let mut grid = CellGrid::new(4, 2);
                let area = Rect {
                    x: 0,
                    y: 0,
                    w: 4,
                    h: 2,
                };
                let mut cache = HalfblockCache::new(16);
                render_to_grid(&mut grid, &source, ImageFit::Fill, &area, &mut cache);
                assert_eq!(cache.len(), 1);
                render_to_grid(&mut grid, &source, ImageFit::Fill, &area, &mut cache);
                assert_eq!(cache.len(), 1, "second call should hit cache");
            }

            /// SvgData fallback writes [SVG] label to grid.
            #[test]
            fn svg_fallback_label() {
                let source = ImageSource::SvgData {
                    data: Arc::from(RED_SVG),
                };
                let mut grid = CellGrid::new(10, 1);
                let area = Rect {
                    x: 0,
                    y: 0,
                    w: 10,
                    h: 1,
                };
                paint_image_fallback(&mut grid, &source, &area);
                let c = grid.get(0, 0).unwrap();
                assert_eq!(c.grapheme.as_str(), "[");
                let c1 = grid.get(1, 0).unwrap();
                assert_eq!(c1.grapheme.as_str(), "S");
            }
        }
    }
}
