use kasane_core::render::{CellDiff, CursorStyle, RenderBackend};

use crate::gpu::CellMetrics;

/// GUI backend implementing `RenderBackend`.
///
/// Unlike the TUI backend, actual GPU rendering is done in `App::render_frame()`
/// via `CellRenderer::render()` which needs the full `CellGrid`. This backend
/// provides the trait interface for `size()`, cursor, and clipboard operations.
pub struct GuiBackend {
    metrics: CellMetrics,
    cursor: Option<(u16, u16, CursorStyle)>,
    clipboard: Option<arboard::Clipboard>,
}

impl GuiBackend {
    pub fn new(metrics: CellMetrics) -> Self {
        let clipboard = arboard::Clipboard::new().ok();
        GuiBackend {
            metrics,
            cursor: None,
            clipboard,
        }
    }

    pub fn update_metrics(&mut self, metrics: CellMetrics) {
        self.metrics = metrics;
    }

    #[cfg(test)]
    pub fn cursor(&self) -> Option<(u16, u16, CursorStyle)> {
        self.cursor
    }
}

impl RenderBackend for GuiBackend {
    fn size(&self) -> (u16, u16) {
        (self.metrics.cols, self.metrics.rows)
    }

    fn draw(&mut self, _diffs: &[CellDiff]) -> anyhow::Result<()> {
        // GPU rendering is done in App::render_frame() directly.
        Ok(())
    }

    fn flush(&mut self) -> anyhow::Result<()> {
        Ok(())
    }

    fn show_cursor(&mut self, x: u16, y: u16, style: CursorStyle) -> anyhow::Result<()> {
        self.cursor = Some((x, y, style));
        Ok(())
    }

    fn hide_cursor(&mut self) -> anyhow::Result<()> {
        self.cursor = None;
        Ok(())
    }

    fn clipboard_get(&mut self) -> Option<String> {
        self.clipboard.as_mut()?.get_text().ok()
    }

    fn clipboard_set(&mut self, text: &str) -> bool {
        self.clipboard
            .as_mut()
            .is_some_and(|cb| cb.set_text(text.to_string()).is_ok())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_gui_backend_size() {
        let metrics = CellMetrics {
            cell_width: 10.0,
            cell_height: 20.0,
            baseline: 15.0,
            cols: 80,
            rows: 24,
        };
        let backend = GuiBackend::new(metrics);
        assert_eq!(backend.size(), (80, 24));
    }

    #[test]
    fn test_gui_backend_cursor() {
        let metrics = CellMetrics {
            cell_width: 10.0,
            cell_height: 20.0,
            baseline: 15.0,
            cols: 80,
            rows: 24,
        };
        let mut backend = GuiBackend::new(metrics);
        assert!(backend.cursor().is_none());
        backend
            .show_cursor(5, 10, CursorStyle::Block)
            .unwrap();
        assert_eq!(backend.cursor(), Some((5, 10, CursorStyle::Block)));
        backend.hide_cursor().unwrap();
        assert!(backend.cursor().is_none());
    }
}
