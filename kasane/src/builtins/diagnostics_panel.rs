//! Built-in diagnostics panel plugin.
//!
//! A togglable in-editor view of the persistent diagnostic history
//! (see `kasane_core::plugin::diagnostics::DiagnosticHistory`). Renders
//! as a centered modal overlay; opens on `<c-?>` and closes on
//! `Esc` / `q` / `<c-?>`. While open, all keys are intercepted via
//! `handle_key_pre_dispatch` so the editor below stays inert.
//!
//! Phase 2 MVP: skeleton + open/close toggle. Rendering and navigation
//! are added in subsequent commits (2.3 / 2.4).

use std::time::Instant;

use kasane_core::element::{
    BorderConfig, BorderLineStyle, Edges, Element, ElementStyle, FlexChild, OverlayAnchor,
};
use kasane_core::input::{Key, KeyEvent, Modifiers};
use kasane_core::plugin::diagnostics::{
    DiagnosticHistory, DiagnosticHistoryEntry, PluginDiagnostic, PluginDiagnosticSeverity,
    summarize_plugin_diagnostic,
};
use kasane_core::plugin::{
    AppView, KeyPreDispatchResult, OverlayContext, OverlayContribution, PluginBackend,
    PluginCapabilities, PluginId, StateUpdates,
};
use kasane_core::protocol::{Atom, Attributes, Color, NamedColor, Style, WireFace};
use kasane_core::state::DirtyFlags;
use unicode_width::UnicodeWidthStr;

const PANEL_TITLE: &str = " Plugin Diagnostics ";
const PANEL_Z_INDEX: i16 = 200;
/// Fixed border + header + footer rows reserved outside the entry list.
const NON_ENTRY_ROWS: u16 = 5;
const MIN_PANEL_W: u16 = 40;
const MIN_PANEL_H: u16 = 8;

/// Panel state. Stored on the plugin instance because diagnostics are
/// framework-internal and we don't want to expose history navigation
/// state on `AppState`/`RuntimeState`.
#[derive(Default)]
pub struct BuiltinDiagnosticsPanelPlugin {
    is_open: bool,
    selected: usize,
    scroll_offset: usize,
}

kasane_core::impl_migrated_caps_default!(BuiltinDiagnosticsPanelPlugin);

impl BuiltinDiagnosticsPanelPlugin {
    fn open(&mut self) {
        self.is_open = true;
        self.selected = 0;
        self.scroll_offset = 0;
    }

    fn close(&mut self) {
        self.is_open = false;
    }

    /// Recognize the toggle key (`<c-?>`).
    fn is_toggle_key(key: &KeyEvent) -> bool {
        key.modifiers == Modifiers::CTRL && matches!(key.key, Key::Char('?'))
    }

    /// Recognize a "close panel" key while open.
    fn is_close_key(key: &KeyEvent) -> bool {
        if Self::is_toggle_key(key) {
            return true;
        }
        if !key.modifiers.is_empty() {
            return false;
        }
        matches!(key.key, Key::Escape | Key::Char('q'))
    }

    /// Apply a navigation action to (selected, scroll_offset). Pure: returns
    /// the new pair so logic is unit-testable without an `AppView`.
    fn navigate(
        action: NavAction,
        total: usize,
        viewport: usize,
        selected: usize,
        scroll: usize,
    ) -> (usize, usize) {
        if total == 0 {
            return (0, 0);
        }
        let last = total.saturating_sub(1);
        let new_selected = match action {
            NavAction::Down(n) => selected.saturating_add(n).min(last),
            NavAction::Up(n) => selected.saturating_sub(n),
            NavAction::Top => 0,
            NavAction::Bottom => last,
        };
        // Keep selected within viewport [scroll, scroll + viewport).
        let new_scroll = if viewport == 0 {
            0
        } else if new_selected < scroll {
            new_selected
        } else if new_selected >= scroll + viewport {
            new_selected + 1 - viewport
        } else {
            scroll
        };
        (new_selected, new_scroll)
    }

    fn nav_action_for(key: &KeyEvent) -> Option<NavAction> {
        if !key.modifiers.is_empty() && key.modifiers != Modifiers::CTRL {
            return None;
        }
        match (key.modifiers, &key.key) {
            (m, Key::Down) | (m, Key::Char('j')) if m.is_empty() => Some(NavAction::Down(1)),
            (m, Key::Up) | (m, Key::Char('k')) if m.is_empty() => Some(NavAction::Up(1)),
            (m, Key::Char('g')) if m.is_empty() => Some(NavAction::Top),
            (m, Key::Char('G')) if m.is_empty() => Some(NavAction::Bottom),
            (m, Key::PageDown) if m.is_empty() => Some(NavAction::Down(10)),
            (m, Key::PageUp) if m.is_empty() => Some(NavAction::Up(10)),
            (Modifiers::CTRL, Key::Char('d')) => Some(NavAction::Down(10)),
            (Modifiers::CTRL, Key::Char('u')) => Some(NavAction::Up(10)),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum NavAction {
    Up(usize),
    Down(usize),
    Top,
    Bottom,
}

impl PluginBackend for BuiltinDiagnosticsPanelPlugin {
    fn id(&self) -> PluginId {
        PluginId("kasane.builtin.diagnostics_panel".into())
    }

    fn capabilities(&self) -> PluginCapabilities {
        PluginCapabilities::KEY_PRE_DISPATCH | PluginCapabilities::OVERLAY
    }

    fn handle_key_pre_dispatch(
        &mut self,
        key: &KeyEvent,
        state: &AppView<'_>,
    ) -> KeyPreDispatchResult {
        if !self.is_open {
            if Self::is_toggle_key(key) {
                self.open();
                return KeyPreDispatchResult::Consumed {
                    flags: DirtyFlags::ALL,
                    commands: vec![],
                    state_updates: StateUpdates::default(),
                    pending_buffer_edit: None,
                };
            }
            return KeyPreDispatchResult::Pass {
                commands: vec![],
                state_updates: StateUpdates::default(),
            };
        }

        if Self::is_close_key(key) {
            self.close();
            return KeyPreDispatchResult::Consumed {
                flags: DirtyFlags::ALL,
                commands: vec![],
                state_updates: StateUpdates::default(),
                pending_buffer_edit: None,
            };
        }

        if let Some(action) = Self::nav_action_for(key) {
            let total = state.diagnostic_history().len();
            let viewport = panel_layout(state.cols(), state.rows())
                .map(|l| l.entry_rows as usize)
                .unwrap_or(0);
            let (sel, off) =
                Self::navigate(action, total, viewport, self.selected, self.scroll_offset);
            self.selected = sel;
            self.scroll_offset = off;
        }
        // All other keys are swallowed to keep the modal modal.
        KeyPreDispatchResult::Consumed {
            flags: DirtyFlags::ALL,
            commands: vec![],
            state_updates: StateUpdates::default(),
            pending_buffer_edit: None,
        }
    }

    fn contribute_overlay_with_ctx(
        &self,
        state: &AppView<'_>,
        _ctx: &OverlayContext,
    ) -> Option<OverlayContribution> {
        if !self.is_open {
            return None;
        }
        let cols = state.cols();
        let rows = state.rows();
        let history = state.diagnostic_history();
        let log_path = state.log_path().and_then(|p| p.to_str().map(str::to_owned));
        let layout = panel_layout(cols, rows)?;
        let element = build_panel_element(
            history,
            self.selected,
            self.scroll_offset,
            log_path.as_deref(),
            &layout,
            Instant::now(),
        );
        Some(OverlayContribution {
            element,
            anchor: OverlayAnchor::Absolute {
                x: layout.x,
                y: layout.y,
                w: layout.w,
                h: layout.h,
            },
            z_index: PANEL_Z_INDEX,
            plugin_id: PluginId("kasane.builtin.diagnostics_panel".into()),
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct PanelLayout {
    x: u16,
    y: u16,
    w: u16,
    h: u16,
    /// Visible entry rows (excluding header + footer + border).
    entry_rows: u16,
}

fn panel_layout(cols: u16, rows: u16) -> Option<PanelLayout> {
    if cols < MIN_PANEL_W || rows < MIN_PANEL_H {
        return None;
    }
    let w = ((cols as f32 * 0.8) as u16).max(MIN_PANEL_W).min(cols);
    let h = ((rows as f32 * 0.8) as u16).max(MIN_PANEL_H).min(rows);
    let x = (cols.saturating_sub(w)) / 2;
    let y = (rows.saturating_sub(h)) / 2;
    let entry_rows = h.saturating_sub(NON_ENTRY_ROWS);
    Some(PanelLayout {
        x,
        y,
        w,
        h,
        entry_rows,
    })
}

fn build_panel_element(
    history: &DiagnosticHistory,
    selected: usize,
    scroll_offset: usize,
    log_path: Option<&str>,
    layout: &PanelLayout,
    now: Instant,
) -> Element {
    let inner_w = layout.w.saturating_sub(2);
    let body_face = panel_body_face();
    let header_face = panel_header_face();
    let footer_face = panel_footer_face();
    let selected_face = panel_selected_face();

    let mut children: Vec<FlexChild> = Vec::with_capacity(layout.h as usize);

    children.push(FlexChild::fixed(Element::StyledLine(vec![header_atom(
        history,
        inner_w,
        &header_face,
    )])));

    let entries = collect_visible_entries(history, scroll_offset, layout.entry_rows);
    let absolute_selected = selected;
    for (relative_idx, entry) in entries.iter().enumerate() {
        let absolute_idx = scroll_offset + relative_idx;
        let is_selected = absolute_idx == absolute_selected;
        let face = if is_selected {
            selected_face
        } else {
            body_face
        };
        let line = format_entry_line(entry, inner_w, now);
        children.push(FlexChild::fixed(Element::StyledLine(vec![
            Atom::with_style(line, Style::from_face(&face)),
        ])));
    }
    // Pad the entry area so the footer always lands at the bottom.
    let used = entries.len() as u16;
    for _ in used..layout.entry_rows {
        children.push(FlexChild::fixed(Element::StyledLine(vec![
            Atom::with_style("", Style::from_face(&body_face)),
        ])));
    }

    children.push(FlexChild::fixed(Element::StyledLine(vec![
        Atom::with_style(footer_keys_text(inner_w), Style::from_face(&footer_face)),
    ])));

    children.push(FlexChild::fixed(Element::StyledLine(vec![
        Atom::with_style(
            footer_log_text(log_path, inner_w),
            Style::from_face(&footer_face),
        ),
    ])));

    let body = Element::column(children);

    let border_face = panel_border_face();
    Element::Container {
        child: Box::new(body),
        border: Some(BorderConfig {
            line_style: BorderLineStyle::Single,
            style: Some(ElementStyle::from(border_face)),
        }),
        shadow: true,
        padding: Edges::ZERO,
        style: ElementStyle::from(body_face),
        title: Some(vec![Atom::with_style(
            PANEL_TITLE,
            Style::from_face(&header_face),
        )]),
    }
}

fn collect_visible_entries(
    history: &DiagnosticHistory,
    scroll_offset: usize,
    entry_rows: u16,
) -> Vec<DiagnosticHistoryEntry> {
    history
        .entries()
        .rev()
        .skip(scroll_offset)
        .take(entry_rows as usize)
        .cloned()
        .collect()
}

fn header_atom(history: &DiagnosticHistory, inner_w: u16, face: &WireFace) -> Atom {
    let total = history.len();
    let errors = history
        .entries()
        .filter(|e| e.diagnostic.severity() == PluginDiagnosticSeverity::Error)
        .count();
    let truncated = history.truncated_count();
    let extra = if truncated > 0 {
        format!(" (+{truncated} older in log)")
    } else {
        String::new()
    };
    let raw =
        format!(" Plugin Diagnostics — {total} entries, {errors} errors{extra}    <c-?> close ",);
    Atom::with_style(truncate_to_width(&raw, inner_w), Style::from_face(face))
}

fn footer_keys_text(inner_w: u16) -> String {
    let s = " ↑↓/jk navigate │ g/G top/bottom │ q/esc close ";
    truncate_to_width(s, inner_w)
}

fn footer_log_text(log_path: Option<&str>, inner_w: u16) -> String {
    let s = match log_path {
        Some(p) => format!(" log: {p}"),
        None => " log: (stderr only)".to_string(),
    };
    truncate_to_width(&s, inner_w)
}

fn format_entry_line(entry: &DiagnosticHistoryEntry, inner_w: u16, now: Instant) -> String {
    let sev = severity_glyph(&entry.diagnostic);
    let age = format_age(now.saturating_duration_since(entry.recorded_at));
    let summary = summarize_plugin_diagnostic(&entry.diagnostic);
    let raw = format!(" {sev} {age:>5}  {summary}");
    truncate_to_width(&raw, inner_w)
}

fn severity_glyph(d: &PluginDiagnostic) -> &'static str {
    match d.severity() {
        PluginDiagnosticSeverity::Error => "E",
        PluginDiagnosticSeverity::Warning => "w",
    }
}

fn format_age(dur: std::time::Duration) -> String {
    let secs = dur.as_secs();
    if secs < 60 {
        format!("{secs}s")
    } else if secs < 3600 {
        format!("{}m", secs / 60)
    } else {
        format!("{}h", secs / 3600)
    }
}

fn truncate_to_width(text: &str, max: u16) -> String {
    let max = max as usize;
    if UnicodeWidthStr::width(text) <= max {
        return text.to_string();
    }
    let mut out = String::new();
    let mut used = 0usize;
    for ch in text.chars() {
        let w = UnicodeWidthStr::width(ch.to_string().as_str());
        if used + w > max.saturating_sub(1) {
            break;
        }
        out.push(ch);
        used += w;
    }
    out.push('…');
    out
}

fn panel_body_face() -> WireFace {
    WireFace {
        fg: Color::Named(NamedColor::BrightWhite),
        bg: Color::Rgb {
            r: 22,
            g: 22,
            b: 28,
        },
        underline: Color::Default,
        attributes: Attributes::empty(),
    }
}

fn panel_header_face() -> WireFace {
    WireFace {
        fg: Color::Named(NamedColor::BrightWhite),
        bg: Color::Rgb {
            r: 60,
            g: 32,
            b: 80,
        },
        underline: Color::Default,
        attributes: Attributes::BOLD,
    }
}

fn panel_footer_face() -> WireFace {
    WireFace {
        fg: Color::Rgb {
            r: 170,
            g: 170,
            b: 170,
        },
        bg: Color::Rgb {
            r: 22,
            g: 22,
            b: 28,
        },
        underline: Color::Default,
        attributes: Attributes::empty(),
    }
}

fn panel_selected_face() -> WireFace {
    WireFace {
        fg: Color::Named(NamedColor::Black),
        bg: Color::Named(NamedColor::BrightYellow),
        underline: Color::Default,
        attributes: Attributes::BOLD,
    }
}

fn panel_border_face() -> WireFace {
    WireFace {
        fg: Color::Named(NamedColor::BrightMagenta),
        bg: Color::Rgb {
            r: 22,
            g: 22,
            b: 28,
        },
        underline: Color::Default,
        attributes: Attributes::BOLD,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ev(key: Key, m: Modifiers) -> KeyEvent {
        KeyEvent { key, modifiers: m }
    }

    fn passes(result: &KeyPreDispatchResult) -> bool {
        matches!(result, KeyPreDispatchResult::Pass { .. })
    }

    fn consumed(result: &KeyPreDispatchResult) -> bool {
        matches!(result, KeyPreDispatchResult::Consumed { .. })
    }

    #[test]
    fn closed_passes_unrelated_keys() {
        let mut plugin = BuiltinDiagnosticsPanelPlugin::default();
        let state = kasane_core::state::AppState::default();
        let view = AppView::new(&state);
        let result = plugin.handle_key_pre_dispatch(&ev(Key::Char('a'), Modifiers::empty()), &view);
        assert!(passes(&result));
        assert!(!plugin.is_open);
    }

    #[test]
    fn closed_consumes_toggle_key_and_opens() {
        let mut plugin = BuiltinDiagnosticsPanelPlugin::default();
        let state = kasane_core::state::AppState::default();
        let view = AppView::new(&state);
        let result = plugin.handle_key_pre_dispatch(&ev(Key::Char('?'), Modifiers::CTRL), &view);
        assert!(consumed(&result));
        assert!(plugin.is_open);
        assert_eq!(plugin.selected, 0);
        assert_eq!(plugin.scroll_offset, 0);
    }

    #[test]
    fn open_consumes_all_keys() {
        let mut plugin = BuiltinDiagnosticsPanelPlugin::default();
        plugin.is_open = true;
        let state = kasane_core::state::AppState::default();
        let view = AppView::new(&state);
        let result = plugin.handle_key_pre_dispatch(&ev(Key::Char('a'), Modifiers::empty()), &view);
        assert!(consumed(&result));
        assert!(plugin.is_open);
    }

    #[test]
    fn open_esc_closes() {
        let mut plugin = BuiltinDiagnosticsPanelPlugin::default();
        plugin.is_open = true;
        let state = kasane_core::state::AppState::default();
        let view = AppView::new(&state);
        let result = plugin.handle_key_pre_dispatch(&ev(Key::Escape, Modifiers::empty()), &view);
        assert!(consumed(&result));
        assert!(!plugin.is_open);
    }

    #[test]
    fn open_q_closes() {
        let mut plugin = BuiltinDiagnosticsPanelPlugin::default();
        plugin.is_open = true;
        let state = kasane_core::state::AppState::default();
        let view = AppView::new(&state);
        let result = plugin.handle_key_pre_dispatch(&ev(Key::Char('q'), Modifiers::empty()), &view);
        assert!(consumed(&result));
        assert!(!plugin.is_open);
    }

    #[test]
    fn open_toggle_closes() {
        let mut plugin = BuiltinDiagnosticsPanelPlugin::default();
        plugin.is_open = true;
        let state = kasane_core::state::AppState::default();
        let view = AppView::new(&state);
        let result = plugin.handle_key_pre_dispatch(&ev(Key::Char('?'), Modifiers::CTRL), &view);
        assert!(consumed(&result));
        assert!(!plugin.is_open);
    }

    #[test]
    fn closed_does_not_consume_q() {
        let mut plugin = BuiltinDiagnosticsPanelPlugin::default();
        let state = kasane_core::state::AppState::default();
        let view = AppView::new(&state);
        let result = plugin.handle_key_pre_dispatch(&ev(Key::Char('q'), Modifiers::empty()), &view);
        assert!(passes(&result));
    }

    // -------------------------------------------------------------------
    // Navigation pure-function tests
    // -------------------------------------------------------------------

    #[test]
    fn navigate_empty_history_is_noop() {
        let (s, o) = BuiltinDiagnosticsPanelPlugin::navigate(NavAction::Down(1), 0, 5, 0, 0);
        assert_eq!((s, o), (0, 0));
    }

    #[test]
    fn navigate_down_within_viewport_keeps_scroll() {
        let (s, o) = BuiltinDiagnosticsPanelPlugin::navigate(NavAction::Down(1), 10, 5, 0, 0);
        assert_eq!((s, o), (1, 0));
    }

    #[test]
    fn navigate_down_past_viewport_advances_scroll() {
        let (s, o) = BuiltinDiagnosticsPanelPlugin::navigate(NavAction::Down(1), 10, 5, 4, 0);
        assert_eq!((s, o), (5, 1));
    }

    #[test]
    fn navigate_up_into_scroll_pulls_scroll_back() {
        let (s, o) = BuiltinDiagnosticsPanelPlugin::navigate(NavAction::Up(1), 10, 5, 5, 5);
        assert_eq!((s, o), (4, 4));
    }

    #[test]
    fn navigate_clamps_to_bounds() {
        let (s, _) = BuiltinDiagnosticsPanelPlugin::navigate(NavAction::Down(100), 10, 5, 0, 0);
        assert_eq!(s, 9);
        let (s2, _) = BuiltinDiagnosticsPanelPlugin::navigate(NavAction::Up(100), 10, 5, 9, 5);
        assert_eq!(s2, 0);
    }

    #[test]
    fn navigate_top_and_bottom() {
        let (s, o) = BuiltinDiagnosticsPanelPlugin::navigate(NavAction::Top, 10, 5, 9, 5);
        assert_eq!((s, o), (0, 0));
        let (s2, o2) = BuiltinDiagnosticsPanelPlugin::navigate(NavAction::Bottom, 10, 5, 0, 0);
        assert_eq!((s2, o2), (9, 5));
    }

    #[test]
    fn nav_action_recognized_for_arrows_and_letters() {
        assert_eq!(
            BuiltinDiagnosticsPanelPlugin::nav_action_for(&ev(Key::Down, Modifiers::empty())),
            Some(NavAction::Down(1))
        );
        assert_eq!(
            BuiltinDiagnosticsPanelPlugin::nav_action_for(&ev(Key::Char('j'), Modifiers::empty())),
            Some(NavAction::Down(1))
        );
        assert_eq!(
            BuiltinDiagnosticsPanelPlugin::nav_action_for(&ev(Key::Char('k'), Modifiers::empty())),
            Some(NavAction::Up(1))
        );
        assert_eq!(
            BuiltinDiagnosticsPanelPlugin::nav_action_for(&ev(Key::Char('G'), Modifiers::empty())),
            Some(NavAction::Bottom)
        );
        assert_eq!(
            BuiltinDiagnosticsPanelPlugin::nav_action_for(&ev(Key::Char('d'), Modifiers::CTRL)),
            Some(NavAction::Down(10))
        );
        assert_eq!(
            BuiltinDiagnosticsPanelPlugin::nav_action_for(&ev(Key::Char('a'), Modifiers::empty())),
            None
        );
    }
}
