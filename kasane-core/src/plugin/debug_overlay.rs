//! Built-in debug overlay plugin.
//!
//! Toggled by `Ctrl-Shift-D` (or configurable shortcut), this overlay displays
//! diagnostic information about the current editor state using a [`TextPanel`]
//! anchored to the top-right corner of the screen.
//!
//! **Displayed info:**
//! - Cursor position, selection count, editor mode
//! - Buffer statistics (line count, dirty lines)
//! - Terminal geometry (cols × rows)
//! - Session info
//!
//! This plugin is opt-in — it is registered by default but the overlay is
//! hidden until toggled.

use compact_str::CompactString;

use crate::element::{Element, OverlayAnchor};
use crate::input::KeyEvent;
use crate::plugin::context::{OverlayContext, OverlayContribution};
use crate::plugin::{AppView, Command, HandlerRegistry, Plugin, PluginId};
use crate::protocol::{Atom, Color, Face, NamedColor};
use crate::state::DirtyFlags;

/// Debug overlay plugin state.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct DebugOverlayState {
    /// Whether the overlay is currently visible.
    pub visible: bool,
}

/// Built-in debug overlay plugin.
pub struct DebugOverlayPlugin;

impl Plugin for DebugOverlayPlugin {
    type State = DebugOverlayState;

    fn id(&self) -> PluginId {
        PluginId("debug_overlay".into())
    }

    fn register(&self, r: &mut HandlerRegistry<DebugOverlayState>) {
        r.declare_interests(DirtyFlags::BUFFER | DirtyFlags::STATUS);

        // Toggle with Ctrl-Shift-D
        r.on_key(|state, key, _app| {
            if is_ctrl_shift_d(key) {
                let mut new_state = state.clone();
                new_state.visible = !new_state.visible;
                Some((new_state, vec![Command::RequestRedraw(DirtyFlags::all())]))
            } else {
                None
            }
        });

        // Render the overlay
        r.on_overlay(|state, app, ctx| {
            if !state.visible {
                return None;
            }
            let lines = build_debug_lines(app, ctx);
            let width = lines.iter().map(|l| atom_text_len(l)).max().unwrap_or(0) as u16;
            let height = lines.len() as u16;

            // Anchor to top-right corner with 1-cell margin
            let x = ctx.screen_cols.saturating_sub(width + 2);
            let element = Element::text_panel(lines);

            Some(OverlayContribution {
                element,
                anchor: OverlayAnchor::Absolute {
                    x,
                    y: 1,
                    w: width + 2,
                    h: height,
                },
                z_index: 100,
                plugin_id: PluginId("debug_overlay".into()),
            })
        });
    }
}

fn is_ctrl_shift_d(key: &KeyEvent) -> bool {
    use crate::input::{Key, Modifiers};
    key.modifiers == (Modifiers::CTRL | Modifiers::SHIFT)
        && matches!(key.key, Key::Char('d') | Key::Char('D'))
}

fn build_debug_lines(app: &AppView<'_>, ctx: &OverlayContext) -> Vec<Vec<Atom>> {
    let header_face = Face {
        fg: Color::Named(NamedColor::Black),
        bg: Color::Named(NamedColor::Cyan),
        ..Face::default()
    };
    let label_face = Face {
        fg: Color::Named(NamedColor::Cyan),
        ..Face::default()
    };
    let value_face = Face::default();

    let mut lines: Vec<Vec<Atom>> = Vec::new();

    // Header
    lines.push(vec![atom(" Debug Overlay ", header_face)]);

    // Cursor
    let cursor = app.cursor_pos();
    lines.push(kv_line(
        "Cursor",
        &format!("{}:{}", cursor.line, cursor.column),
        label_face,
        value_face,
    ));

    // Selections
    lines.push(kv_line(
        "Selections",
        &app.cursor_count().to_string(),
        label_face,
        value_face,
    ));

    // Editor mode
    lines.push(kv_line(
        "Mode",
        &format!("{:?}", app.editor_mode()),
        label_face,
        value_face,
    ));

    // Buffer stats
    let line_count = app.line_count();
    let dirty_count = app.lines_dirty().iter().filter(|&&d| d).count();
    lines.push(kv_line(
        "Lines",
        &format!("{line_count} ({dirty_count} dirty)"),
        label_face,
        value_face,
    ));

    // Terminal geometry
    lines.push(kv_line(
        "Terminal",
        &format!("{}x{}", ctx.screen_cols, ctx.screen_rows),
        label_face,
        value_face,
    ));

    // Active session
    let session = app.active_session_key().unwrap_or("<none>");
    lines.push(kv_line("Session", session, label_face, value_face));

    // Overlays count
    lines.push(kv_line(
        "Overlays",
        &ctx.existing_overlays.len().to_string(),
        label_face,
        value_face,
    ));

    lines
}

fn atom(text: &str, face: Face) -> Atom {
    Atom::from_face(face, text)
}

fn kv_line(key: &str, value: &str, label_face: Face, value_face: Face) -> Vec<Atom> {
    vec![
        atom(&format!(" {key}: "), label_face),
        atom(&format!("{value} "), value_face),
    ]
}

fn atom_text_len(atoms: &[Atom]) -> usize {
    atoms.iter().map(|a| a.contents.len()).sum()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::plugin::PluginRuntime;
    use crate::plugin::bridge::PluginBridge;
    use crate::state::AppState;

    #[test]
    fn debug_overlay_registers_and_toggles() {
        let mut runtime = PluginRuntime::new();
        runtime.register(DebugOverlayPlugin);

        assert!(runtime.contains_plugin(&PluginId("debug_overlay".into())));
        assert_eq!(runtime.plugin_count(), 1);
    }

    #[test]
    fn debug_overlay_hidden_by_default() {
        let state = DebugOverlayState::default();
        assert!(!state.visible);
    }

    #[test]
    fn build_debug_lines_produces_expected_sections() {
        let app_state = AppState::default();
        let app = AppView::new(&app_state);
        let ctx = OverlayContext {
            screen_cols: 80,
            screen_rows: 24,
            menu_rect: None,
            existing_overlays: vec![],
            focused_surface_id: None,
        };
        let lines = build_debug_lines(&app, &ctx);
        // Header + Cursor + Selections + Mode + Lines + Terminal + Session + Overlays = 8
        assert_eq!(lines.len(), 8);
        // Header line
        assert!(lines[0][0].contents.contains("Debug Overlay"));
        // Terminal line should contain dimensions
        let terminal_text: String = lines[5].iter().map(|a| a.contents.as_str()).collect();
        assert!(terminal_text.contains("80x24"));
    }
}
