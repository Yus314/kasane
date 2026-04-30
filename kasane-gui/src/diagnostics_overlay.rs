use kasane_core::element::BorderLineStyle;
use kasane_core::plugin::PluginDiagnosticOverlayState;
use kasane_core::plugin::diagnostics::{
    PluginDiagnosticOverlayPainter, PluginDiagnosticOverlayTextRun, paint_plugin_diagnostic_overlay,
};
use kasane_core::protocol::WireFace;
use kasane_core::render::scene::{PixelPos, PixelRect};
use kasane_core::render::{CellSize, DrawCommand};

pub(crate) fn build_diagnostic_overlay_commands(
    state: &PluginDiagnosticOverlayState,
    cell_size: CellSize,
    cols: u16,
    rows: u16,
) -> Vec<DrawCommand> {
    let Some(spec) = state.paint_spec(cols, rows) else {
        return vec![];
    };
    let mut commands = vec![DrawCommand::BeginOverlay];
    if let Some(shadow) = spec.shadow {
        commands.push(DrawCommand::DrawShadow {
            rect: pixel_rect(
                spec.layout.x,
                spec.layout.y,
                spec.layout.width,
                spec.layout.height,
                cell_size,
            ),
            offset: shadow.offset,
            blur_radius: shadow.blur_radius,
            color: shadow.color,
        });
    }

    let mut painter = SceneOverlayPainter {
        commands: &mut commands,
        cell_size,
    };
    paint_plugin_diagnostic_overlay(&spec, &mut painter);
    commands
}

struct SceneOverlayPainter<'a> {
    commands: &'a mut Vec<DrawCommand>,
    cell_size: CellSize,
}

impl PluginDiagnosticOverlayPainter for SceneOverlayPainter<'_> {
    fn fill_region(&mut self, x: u16, y: u16, width: u16, height: u16, face: WireFace) {
        self.commands.push(DrawCommand::FillRect {
            rect: pixel_rect(x, y, width, height, self.cell_size),
            face: face.into(),
            elevated: true,
        });
    }

    fn draw_border(&mut self, x: u16, y: u16, width: u16, height: u16, face: WireFace) {
        self.commands.push(DrawCommand::DrawBorder {
            rect: pixel_rect(x, y, width, height, self.cell_size),
            line_style: BorderLineStyle::Single,
            face: face.into(),
            fill_face: None,
        });
    }

    fn draw_text_run(&mut self, run: &PluginDiagnosticOverlayTextRun) {
        self.commands.push(DrawCommand::DrawText {
            pos: pixel_pos(run.x, run.y, self.cell_size),
            text: run.text.clone(),
            face: run.face.into(),
            max_width: run.max_width as f32 * self.cell_size.width,
        });
    }
}

fn pixel_rect(x: u16, y: u16, w: u16, h: u16, cell_size: CellSize) -> PixelRect {
    PixelRect {
        x: x as f32 * cell_size.width,
        y: y as f32 * cell_size.height,
        w: w as f32 * cell_size.width,
        h: h as f32 * cell_size.height,
    }
}

fn pixel_pos(x: u16, y: u16, cell_size: CellSize) -> PixelPos {
    PixelPos {
        x: x as f32 * cell_size.width,
        y: y as f32 * cell_size.height,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use kasane_core::plugin::{
        PluginDiagnostic, PluginDiagnosticOverlayState, ProviderArtifactStage,
    };

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
    fn build_commands_contains_overlay_boundary_and_text() {
        let mut overlay = PluginDiagnosticOverlayState::default();
        overlay.record(&[PluginDiagnostic::provider_artifact_failed(
            "provider",
            "broken.wasm",
            ProviderArtifactStage::Load,
            "bad wasm",
        )]);

        let commands = build_diagnostic_overlay_commands(
            &overlay,
            CellSize {
                width: 10.0,
                height: 20.0,
            },
            40,
            8,
        );

        assert!(matches!(commands.first(), Some(DrawCommand::BeginOverlay)));
        assert!(
            commands
                .iter()
                .any(|command| { matches!(command, DrawCommand::DrawShadow { .. }) })
        );
        assert!(commands.iter().any(|command| {
            matches!(
                command,
                DrawCommand::DrawText { text, .. } if text == "L"
            )
        }));
    }
}
