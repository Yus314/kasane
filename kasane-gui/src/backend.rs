//! GUI backend for cell metrics tracking.

use crate::gpu::CellMetrics;

/// GUI backend providing cell metrics for layout.
///
/// Actual GPU rendering is done in `App::render_frame()` via
/// `SceneRenderer::render()`. This type only tracks cell metrics.
pub struct GuiBackend {
    metrics: CellMetrics,
}

impl GuiBackend {
    pub fn new(metrics: CellMetrics) -> Self {
        GuiBackend { metrics }
    }

    pub fn update_metrics(&mut self, metrics: CellMetrics) {
        self.metrics = metrics;
    }
}
