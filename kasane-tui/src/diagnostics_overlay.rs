// Legacy painter-based diagnostics overlay.
// Production rendering is now handled by BuiltinDiagnosticsPlugin via on_overlay().
// This module is retained for backward-compatibility tests of the painting infrastructure.

#[cfg(test)]
mod tests {
    use kasane_core::plugin::diagnostics::{
        PluginDiagnosticOverlayPainter, PluginDiagnosticOverlayTextRun,
    };
    use kasane_core::plugin::{
        PluginDiagnostic, PluginDiagnosticOverlayState, ProviderArtifactStage,
    };
    use kasane_core::protocol::Face;
    use kasane_core::render::CellGrid;

    fn paint_diagnostic_overlay(state: &PluginDiagnosticOverlayState, grid: &mut CellGrid) {
        let cols = grid.width();
        let rows = grid.height();
        let mut painter = CellGridOverlayPainter { grid };
        let _ = state.paint_with(cols, rows, &mut painter);
    }

    struct CellGridOverlayPainter<'a> {
        grid: &'a mut CellGrid,
    }

    impl PluginDiagnosticOverlayPainter for CellGridOverlayPainter<'_> {
        fn fill_region(&mut self, x: u16, y: u16, width: u16, height: u16, face: Face) {
            for row in 0..height {
                self.grid.fill_region(y + row, x, width, &face);
            }
        }

        fn draw_border(&mut self, x: u16, y: u16, width: u16, height: u16, face: Face) {
            draw_text(self.grid, x, y, "┌", &face, 1);
            draw_text(self.grid, x + width.saturating_sub(1), y, "┐", &face, 1);
            for dx in 1..width.saturating_sub(1) {
                draw_text(self.grid, x + dx, y, "─", &face, 1);
            }

            for row in 1..height.saturating_sub(1) {
                draw_text(self.grid, x, y + row, "│", &face, 1);
                draw_text(
                    self.grid,
                    x + width.saturating_sub(1),
                    y + row,
                    "│",
                    &face,
                    1,
                );
            }

            if height >= 2 {
                let bottom = y + height - 1;
                draw_text(self.grid, x, bottom, "└", &face, 1);
                draw_text(
                    self.grid,
                    x + width.saturating_sub(1),
                    bottom,
                    "┘",
                    &face,
                    1,
                );
                for dx in 1..width.saturating_sub(1) {
                    draw_text(self.grid, x + dx, bottom, "─", &face, 1);
                }
            }
        }

        fn draw_text_run(&mut self, run: &PluginDiagnosticOverlayTextRun) {
            draw_text(self.grid, run.x, run.y, &run.text, &run.face, run.max_width);
        }
    }

    fn draw_text(grid: &mut CellGrid, x: u16, y: u16, text: &str, face: &Face, max_width: u16) {
        use kasane_core::protocol::Atom;
        grid.put_line_with_base(y, x, &[Atom::from_face(*face, text)], max_width, None);
    }

    #[test]
    fn record_and_dismiss_are_generation_guarded() {
        let mut overlay = PluginDiagnosticOverlayState::default();
        let generation = overlay
            .record(&[PluginDiagnostic::provider_collect_failed(
                "provider", "boom",
            )])
            .expect("generation");
        assert!(overlay.is_active());
        assert!(!overlay.dismiss(generation + 1));
        assert!(overlay.is_active());
        assert!(overlay.dismiss(generation));
        assert!(!overlay.is_active());
    }

    #[test]
    fn paint_writes_overlay_cells() {
        let mut overlay = PluginDiagnosticOverlayState::default();
        overlay.record(&[PluginDiagnostic::provider_artifact_failed(
            "provider",
            "broken.wasm",
            ProviderArtifactStage::Load,
            "bad wasm",
        )]);
        let mut grid = CellGrid::new(40, 8);
        paint_diagnostic_overlay(&overlay, &mut grid);
        assert_eq!(
            grid.get(39, 0).expect("top right cell").grapheme.as_str(),
            "┐"
        );
        assert!(
            (0..40).any(|x| grid.get(x, 1).expect("cell").grapheme.as_str() == "L"),
            "expected load marker in overlay"
        );
    }
}
