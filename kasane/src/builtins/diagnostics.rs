//! Built-in diagnostics overlay plugin.
//!
//! Renders plugin diagnostic information (load errors, activation failures)
//! as an overlay in the top-right corner. Registered as the highest z-index
//! overlay so it's always visible above other content.

use kasane_core::element::{BorderConfig, BorderLineStyle, Element, ElementStyle, FlexChild};
use kasane_core::plugin::diagnostics::PluginDiagnosticOverlayState;
use kasane_core::plugin::{
    AppView, OverlayContext, OverlayContribution, PluginBackend, PluginCapabilities, PluginId,
};
use kasane_core::protocol::{Atom, Face, Style};

/// Built-in plugin for diagnostics overlay rendering.
///
/// Reads diagnostic overlay state from AppView and builds an Element-based
/// overlay when diagnostics are active.
pub struct BuiltinDiagnosticsPlugin;

impl PluginBackend for BuiltinDiagnosticsPlugin {
    fn id(&self) -> PluginId {
        PluginId("kasane.builtin.diagnostics".into())
    }

    fn capabilities(&self) -> PluginCapabilities {
        PluginCapabilities::OVERLAY
    }

    fn contribute_overlay_with_ctx(
        &self,
        state: &AppView<'_>,
        _ctx: &OverlayContext,
    ) -> Option<OverlayContribution> {
        let overlay_state = state.diagnostic_overlay();
        if !overlay_state.is_active() {
            return None;
        }

        let cols = state.cols();
        let rows = state.rows();
        let (element, anchor) = build_diagnostic_element(overlay_state, cols, rows)?;

        Some(OverlayContribution {
            element,
            anchor,
            z_index: 100,
            plugin_id: PluginId("kasane.builtin.diagnostics".into()),
        })
    }
}

fn build_diagnostic_element(
    overlay_state: &PluginDiagnosticOverlayState,
    cols: u16,
    rows: u16,
) -> Option<(Element, kasane_core::element::OverlayAnchor)> {
    let spec = overlay_state.paint_spec(cols, rows)?;
    let layout = &spec.layout;

    // Build body rows from text runs.
    // First run is the header; subsequent runs come in pairs (tag + text).
    let mut body_children = Vec::new();

    // Header line (first text run)
    if let Some(header_run) = spec.text_runs.first() {
        body_children.push(FlexChild::fixed(Element::StyledLine(vec![
            Atom::with_style(header_run.text.clone(), Style::from_face(&spec.header_face)),
        ])));
    }

    // Body lines: tag + text pairs
    let body_runs = &spec.text_runs[1..];
    for chunk in body_runs.chunks(2) {
        let mut atoms = Vec::new();
        for run in chunk {
            atoms.push(Atom::with_style(
                run.text.clone(),
                Style::from_face(&run.face),
            ));
            // Space between tag and text
            if atoms.len() == 1 {
                atoms.push(Atom::with_style(" ", Style::from_face(&spec.body_face)));
            }
        }
        body_children.push(FlexChild::fixed(Element::StyledLine(atoms)));
    }

    let body = Element::column(body_children);

    let border_face = Face {
        fg: spec.border_face.fg,
        bg: spec.body_face.bg,
        ..Face::default()
    };

    let container = Element::Container {
        child: Box::new(body),
        border: Some(BorderConfig {
            line_style: BorderLineStyle::Single,
            style: Some(ElementStyle::from(border_face)),
        }),
        shadow: spec.shadow.is_some(),
        padding: kasane_core::element::Edges::ZERO,
        style: ElementStyle::from(spec.body_face),
        title: None,
    };

    let anchor = kasane_core::element::OverlayAnchor::Absolute {
        x: layout.x,
        y: layout.y,
        w: layout.width,
        h: layout.height,
    };

    Some((container, anchor))
}
