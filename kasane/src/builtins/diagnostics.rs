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
use kasane_core::protocol::{Atom, Brush, Style};
use unicode_width::UnicodeWidthStr;

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
        let log_path = state.log_path().and_then(|p| p.to_str().map(str::to_owned));
        let (element, anchor) =
            build_diagnostic_element(overlay_state, cols, rows, log_path.as_deref())?;

        Some(OverlayContribution {
            element,
            anchor,
            z_index: 100,
            plugin_id: PluginId("kasane.builtin.diagnostics".into()),
        })
    }
}

/// Build the footer hint line shown below the diagnostic body.
///
/// Returns `None` when `inner_width` is too small to show even the
/// hotkey hint legibly.
fn build_footer_atoms(
    inner_width: u16,
    log_path: Option<&str>,
    body_style: &Style,
) -> Option<Vec<Atom>> {
    if inner_width < 8 {
        return None;
    }
    let hint_style = Style {
        fg: Brush::rgb(160, 160, 160),
        bg: body_style.bg,
        ..Style::default()
    };
    let mut atoms = Vec::new();
    if let Some(path) = log_path {
        let prefix = "log: ";
        let prefix_w = prefix.len() as u16;
        let path_room = inner_width.saturating_sub(prefix_w);
        let displayed = if (UnicodeWidthStr::width(path) as u16) <= path_room {
            path.to_string()
        } else {
            shorten_path_to_width(path, path_room)
        };
        atoms.push(Atom::with_style(prefix, hint_style.clone()));
        atoms.push(Atom::with_style(displayed, hint_style));
    } else {
        atoms.push(Atom::with_style(
            "see log (KASANE_LOG_STDERR=1)",
            hint_style,
        ));
    }
    Some(atoms)
}

/// Truncate a path string from the left so the trailing filename is preserved.
/// Keeps the last component visible; replaces stripped middle with "…/".
fn shorten_path_to_width(path: &str, max_width: u16) -> String {
    let max = max_width as usize;
    if max == 0 {
        return String::new();
    }
    if UnicodeWidthStr::width(path) <= max {
        return path.to_string();
    }
    // Fall back: keep tail. Reserve 2 cells for "…/".
    let tail_room = max.saturating_sub(2);
    let mut tail = String::new();
    let mut used = 0usize;
    for ch in path.chars().rev() {
        let w = UnicodeWidthStr::width(ch.to_string().as_str());
        if used + w > tail_room {
            break;
        }
        tail.insert(0, ch);
        used += w;
    }
    format!("…/{tail}")
}

fn build_diagnostic_element(
    overlay_state: &PluginDiagnosticOverlayState,
    cols: u16,
    rows: u16,
    log_path: Option<&str>,
) -> Option<(Element, kasane_core::element::OverlayAnchor)> {
    let spec = overlay_state.paint_spec(cols, rows)?;
    let layout = &spec.layout;

    // Build body rows from text runs.
    // First run is the header; subsequent runs come in pairs (tag + text).
    let mut body_children = Vec::new();

    // Header line (first text run)
    if let Some(header_run) = spec.text_runs.first() {
        body_children.push(FlexChild::fixed(Element::StyledLine(vec![
            Atom::with_style(header_run.text.clone(), header_run.style.clone()),
        ])));
    }

    // Body lines: tag + text pairs
    let body_runs = &spec.text_runs[1..];
    for chunk in body_runs.chunks(2) {
        let mut atoms = Vec::new();
        for run in chunk {
            atoms.push(Atom::with_style(run.text.clone(), run.style.clone()));
            // Space between tag and text
            if atoms.len() == 1 {
                atoms.push(Atom::with_style(" ", spec.body_style.clone()));
            }
        }
        body_children.push(FlexChild::fixed(Element::StyledLine(atoms)));
    }

    // Footer hint line (log path / hotkey).
    // Adds one extra row to the container if the layout has vertical room.
    let inner_width = layout.width.saturating_sub(2);
    let extra_height = if layout.y + layout.height < rows {
        if let Some(footer_atoms) = build_footer_atoms(inner_width, log_path, &spec.body_style) {
            body_children.push(FlexChild::fixed(Element::StyledLine(footer_atoms)));
            1
        } else {
            0
        }
    } else {
        0
    };

    let body = Element::column(body_children);

    let border_style = Style {
        fg: spec.border_style.fg,
        bg: spec.body_style.bg,
        ..Style::default()
    };

    let container = Element::Container {
        child: Box::new(body),
        border: Some(BorderConfig {
            line_style: BorderLineStyle::Single,
            style: Some(ElementStyle::from(border_style)),
        }),
        shadow: spec.shadow.is_some(),
        padding: kasane_core::element::Edges::ZERO,
        style: ElementStyle::from(spec.body_style.clone()),
        title: None,
    };

    let anchor = kasane_core::element::OverlayAnchor::Absolute {
        x: layout.x,
        y: layout.y,
        w: layout.width,
        h: layout.height + extra_height,
    };

    Some((container, anchor))
}
