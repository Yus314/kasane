//! Shared `CellMetrics` type. The Parley implementation in
//! [`text::metrics::calculate_with_parley`](super::text::metrics) is the
//! only producer; the cosmic-text-based `calculate` constructor was
//! removed when its backing dependency retired.

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
    /// Distance from baseline to top of the underline decoration, in
    /// physical pixels. Positive = below the baseline. `0.0` means the
    /// font's own value was unavailable; the decoration emitter falls
    /// back to a `cell_h × ratio` heuristic in that case.
    pub underline_offset: f32,
    /// Underline stroke thickness in physical pixels. `0.0` → fallback.
    pub underline_thickness: f32,
    /// Strikethrough offset (positive = above baseline, font convention)
    /// and thickness in physical pixels. Same `0.0`-fallback contract.
    pub strikethrough_offset: f32,
    pub strikethrough_thickness: f32,
}
