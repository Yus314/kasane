//! Shared `CellMetrics` type. Phase 11 dropped the cosmic-text-based
//! `calculate` constructor along with the dependency; the Parley
//! implementation in `parley_text::metrics::calculate_with_parley` is
//! now the only producer.

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
