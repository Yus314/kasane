use glyphon::{Attrs, Buffer as GlyphonBuffer, FontSystem, Metrics, Shaping};
use kasane_core::config::FontConfig;
use winit::dpi::PhysicalSize;

/// Pre-computed cell dimensions in physical pixels.
#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct CellMetrics {
    pub cell_width: f32,
    pub cell_height: f32,
    /// Baseline offset from cell top (ascent).
    pub baseline: f32,
    pub cols: u16,
    pub rows: u16,
}

impl CellMetrics {
    pub fn calculate(
        font_system: &mut FontSystem,
        font_config: &FontConfig,
        scale_factor: f64,
        window_size: PhysicalSize<u32>,
    ) -> Self {
        let font_size = font_config.size * scale_factor as f32;
        let line_height_px = font_size * font_config.line_height;

        // Create a temporary buffer to measure the "M" character advance
        let metrics = Metrics::new(font_size, line_height_px);
        let mut buffer = GlyphonBuffer::new(font_system, metrics);
        buffer.set_size(font_system, Some(1000.0), Some(line_height_px));
        buffer.set_text(
            font_system,
            "M",
            &Attrs::new().family(super::to_family(&font_config.family)),
            Shaping::Basic,
            None,
        );
        buffer.shape_until_scroll(font_system, false);

        // Get the advance width of "M"
        let cell_width = buffer
            .layout_runs()
            .next()
            .and_then(|run| run.glyphs.first())
            .map(|g| g.w)
            .unwrap_or(font_size * 0.6)
            + font_config.letter_spacing * scale_factor as f32;

        let cell_height = line_height_px;

        // Compute baseline from font metrics
        let baseline = buffer
            .layout_runs()
            .next()
            .map(|run| run.line_y)
            .unwrap_or(font_size * 0.8);

        let cols = (window_size.width as f32 / cell_width).floor().max(1.0) as u16;
        let rows = (window_size.height as f32 / cell_height).floor().max(1.0) as u16;

        CellMetrics {
            cell_width,
            cell_height,
            baseline,
            cols,
            rows,
        }
    }
}
