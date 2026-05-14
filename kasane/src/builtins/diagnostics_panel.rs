//! Built-in diagnostics panel plugin.
//!
//! A togglable in-editor view of the persistent diagnostic history
//! (see `kasane_core::plugin::diagnostics::DiagnosticHistory`). Renders
//! as a centered modal overlay; opens on `<c-?>` and closes on
//! `Esc` / `q` / `<c-?>`. While open, all keys are intercepted via
//! `on_key_pre_dispatch` so the editor below stays inert.
//!
//! Migrated to the `Plugin + HandlerRegistry` pattern: the panel's
//! mutable state (open flag, selection cursor, scroll offset) lives on
//! [`DiagnosticsPanelState`] instead of on the plugin instance, and
//! handlers are registered declaratively via [`Plugin::register`].

use std::path::{Path, PathBuf};
use std::time::Instant;

use kasane_core::element::{
    BorderConfig, BorderLineStyle, Edges, Element, ElementStyle, FlexChild, OverlayAnchor,
};
use kasane_core::input::{Key, KeyEvent, Modifiers};
use kasane_core::plugin::diagnostics::{
    DiagnosticHistory, DiagnosticHistoryEntry, PluginDiagnostic, PluginDiagnosticKind,
    PluginDiagnosticSeverity, PluginDiagnosticTarget, provider_artifact_stage_label,
    summarize_plugin_diagnostic,
};
use kasane_core::plugin::{
    AppView, Command, HandlerRegistry, KeyPreDispatchResult, OverlayContribution, Plugin, PluginId,
    StateUpdates,
};
use kasane_core::plugin::{PluginDescriptor, PluginSource};
use kasane_core::protocol::{Atom, Attributes, Color, NamedColor, Style, WireFace};
use kasane_core::state::DirtyFlags;
use unicode_width::UnicodeWidthStr;

const PANEL_TITLE: &str = " Plugin Diagnostics ";
const PANEL_Z_INDEX: i16 = 200;
/// Border (2) + header (1) + footer (2) rows reserved outside the entry list.
const NON_ENTRY_ROWS: u16 = 5;
/// Detail block lines: separator + message + previous + attempted.
const DETAIL_ROWS: u16 = 4;
const MIN_PANEL_W: u16 = 40;
const MIN_PANEL_H: u16 = 12;

/// Panel state. Lives in the plugin's `Plugin::State` slot rather than on
/// the plugin instance because `register()` handlers receive an
/// immutable `&State` and return the next state — moving these fields off
/// the struct keeps the new contract honest.
#[derive(Clone, Default, Debug, PartialEq)]
pub struct DiagnosticsPanelState {
    is_open: bool,
    selected: usize,
    scroll_offset: usize,
}

impl DiagnosticsPanelState {
    fn opened() -> Self {
        Self {
            is_open: true,
            selected: 0,
            scroll_offset: 0,
        }
    }
}

/// Recognize the toggle key (`<c-?>`).
fn is_toggle_key(key: &KeyEvent) -> bool {
    key.modifiers == Modifiers::CTRL && matches!(key.key, Key::Char('?'))
}

/// Recognize a "close panel" key while open.
fn is_close_key(key: &KeyEvent) -> bool {
    if is_toggle_key(key) {
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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum NavAction {
    Up(usize),
    Down(usize),
    Top,
    Bottom,
}

/// Pure key-handling logic shared between the registered handler and tests.
fn handle_key(
    state: &DiagnosticsPanelState,
    key: &KeyEvent,
    app: &AppView<'_>,
) -> (DiagnosticsPanelState, KeyPreDispatchResult) {
    if !state.is_open {
        if is_toggle_key(key) {
            return (
                DiagnosticsPanelState::opened(),
                KeyPreDispatchResult::Consumed {
                    flags: DirtyFlags::ALL,
                    // Clear any active popup so the modal owns the screen
                    // exclusively and an error popup that pre-dates the
                    // panel doesn't keep painting underneath.
                    commands: vec![Command::DismissDiagnosticOverlay],
                    state_updates: StateUpdates::default(),
                    pending_buffer_edit: None,
                },
            );
        }
        return (
            state.clone(),
            KeyPreDispatchResult::Pass {
                commands: vec![],
                state_updates: StateUpdates::default(),
            },
        );
    }

    if is_close_key(key) {
        let mut next = state.clone();
        next.is_open = false;
        return (
            next,
            KeyPreDispatchResult::Consumed {
                flags: DirtyFlags::ALL,
                commands: vec![],
                state_updates: StateUpdates::default(),
                pending_buffer_edit: None,
            },
        );
    }

    // r: trigger a plugin reload (whole-set; per-plugin reload is on
    // the roadmap). Closes the panel because the live plugin
    // instances will be torn down and re-instantiated, rendering
    // the in-flight panel state stale.
    if key.modifiers.is_empty() && key.key == Key::Char('r') {
        let mut next = state.clone();
        next.is_open = false;
        return (
            next,
            KeyPreDispatchResult::Consumed {
                flags: DirtyFlags::ALL,
                commands: vec![Command::TriggerPluginReload],
                state_updates: StateUpdates::default(),
                pending_buffer_edit: None,
            },
        );
    }

    // y: copy a structured representation of the selected diagnostic
    // to the system clipboard for inclusion in bug reports.
    if key.modifiers.is_empty() && key.key == Key::Char('y') {
        let history = app.diagnostic_history();
        let mut commands = Vec::new();
        if let Some(entry) = history.entries().rev().nth(state.selected) {
            commands.push(Command::SetClipboard(format_diagnostic_for_yank(
                &entry.diagnostic,
                app.log_path(),
            )));
        }
        return (
            state.clone(),
            KeyPreDispatchResult::Consumed {
                flags: DirtyFlags::empty(),
                commands,
                state_updates: StateUpdates::default(),
                pending_buffer_edit: None,
            },
        );
    }

    // Enter: hand off to Kakoune to open the log file. We close the
    // panel first so the user lands on the freshly-opened buffer.
    if key.modifiers.is_empty() && key.key == Key::Enter {
        let mut commands = Vec::new();
        if let Some(path) = app.log_path() {
            let resolved = resolve_active_log_file(path);
            if let Some(path_str) = resolved.to_str() {
                commands.push(Command::kakoune_command(&format!("edit {path_str}")));
            }
        }
        let mut next = state.clone();
        next.is_open = false;
        return (
            next,
            KeyPreDispatchResult::Consumed {
                flags: DirtyFlags::ALL,
                commands,
                state_updates: StateUpdates::default(),
                pending_buffer_edit: None,
            },
        );
    }

    let mut next = state.clone();
    if let Some(action) = nav_action_for(key) {
        let total = app.diagnostic_history().len();
        let viewport = panel_layout(app.cols(), app.rows())
            .map(|l| l.entry_rows as usize)
            .unwrap_or(0);
        let (sel, off) = navigate(action, total, viewport, state.selected, state.scroll_offset);
        next.selected = sel;
        next.scroll_offset = off;
    }
    // All other keys are swallowed to keep the modal modal.
    (
        next,
        KeyPreDispatchResult::Consumed {
            flags: DirtyFlags::ALL,
            commands: vec![],
            state_updates: StateUpdates::default(),
            pending_buffer_edit: None,
        },
    )
}

/// Pure overlay-building logic shared between the registered handler and tests.
fn build_overlay(state: &DiagnosticsPanelState, app: &AppView<'_>) -> Option<OverlayContribution> {
    if !state.is_open {
        return None;
    }
    let cols = app.cols();
    let rows = app.rows();
    let history = app.diagnostic_history();
    let log_path = app.log_path().and_then(|p| p.to_str().map(str::to_owned));
    let layout = panel_layout(cols, rows)?;
    let element = build_panel_element(
        history,
        state.selected,
        state.scroll_offset,
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
        plugin_id: PluginId::from("kasane.builtin.diagnostics_panel"),
    })
}

#[derive(Default)]
pub struct BuiltinDiagnosticsPanelPlugin;

impl Plugin for BuiltinDiagnosticsPanelPlugin {
    type State = DiagnosticsPanelState;

    fn id(&self) -> PluginId {
        PluginId::from("kasane.builtin.diagnostics_panel")
    }

    fn register(&self, r: &mut HandlerRegistry<DiagnosticsPanelState>) {
        r.on_key_pre_dispatch(handle_key);
        r.on_overlay(|state, app, _ctx| build_overlay(state, app));
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
    // Reserve detail rows up front; the entry list shrinks accordingly so
    // the detail block has a stable position regardless of selection.
    let entry_rows = h.saturating_sub(NON_ENTRY_ROWS + DETAIL_ROWS);
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

    // Detail block: separator + 3 stable rows. We always emit DETAIL_ROWS
    // rows so the footer position never shifts; rows render blank when
    // the corresponding field is absent.
    let detail_face = panel_detail_face();
    let detail_separator_face = panel_detail_separator_face();
    let selected_entry = entries
        .iter()
        .enumerate()
        .find(|(rel, _)| scroll_offset + rel == absolute_selected)
        .map(|(_, e)| e);
    let detail_lines = format_detail_lines(selected_entry.map(|e| &e.diagnostic), inner_w);

    children.push(FlexChild::fixed(Element::StyledLine(vec![
        Atom::with_style(
            detail_separator_text(inner_w),
            Style::from_face(&detail_separator_face),
        ),
    ])));
    for line in detail_lines {
        children.push(FlexChild::fixed(Element::StyledLine(vec![
            Atom::with_style(line, Style::from_face(&detail_face)),
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
    let s = " ↑↓/jk nav │ g/G top/bot │ r reload │ enter open log │ y yank │ q close ";
    truncate_to_width(s, inner_w)
}

fn detail_separator_text(inner_w: u16) -> String {
    let max = inner_w as usize;
    let label = " details ";
    if max <= label.len() + 2 {
        return "─".repeat(max);
    }
    let dashes = max.saturating_sub(label.len() + 1);
    format!(" {label}{}", "─".repeat(dashes))
}

/// Build the 3 detail lines (always 3 rows, blank when absent) for the
/// row block under the separator.
fn format_detail_lines(diag: Option<&PluginDiagnostic>, inner_w: u16) -> [String; 3] {
    let Some(d) = diag else {
        return [String::new(), String::new(), String::new()];
    };
    let kind_label = kind_short_label(&d.kind);
    let target = match &d.target {
        PluginDiagnosticTarget::Plugin(id) => format!("plugin {}", id.0),
        PluginDiagnosticTarget::Provider(p) => format!("provider {p}"),
    };
    let line1 = truncate_to_width(&format!(" [{kind_label}] {target}"), inner_w);
    let line2 = truncate_to_width(&format!(" message: {}", d.message), inner_w);
    let line3 = format_descriptors_line(d, inner_w);
    [line1, line2, line3]
}

fn kind_short_label(kind: &PluginDiagnosticKind) -> &'static str {
    match kind {
        PluginDiagnosticKind::SurfaceRegistrationFailed { .. } => "surface",
        PluginDiagnosticKind::InstantiationFailed => "init",
        PluginDiagnosticKind::AbiVersionMismatch { .. } => "abi",
        PluginDiagnosticKind::ProviderCollectFailed => "discovery",
        PluginDiagnosticKind::ProviderArtifactFailed { .. } => "artifact",
        PluginDiagnosticKind::RuntimeError { .. } => "runtime",
        PluginDiagnosticKind::ConfigError { .. } => "config",
        PluginDiagnosticKind::BackendCapabilityRejected { .. } => "backend",
        PluginDiagnosticKind::PluginEmitted { .. } => "emit",
    }
}

/// Third detail row: kind-specific extras (artifact stage, runtime method,
/// previous→attempted descriptor pair). Always returns one truncated line.
fn format_descriptors_line(d: &PluginDiagnostic, inner_w: u16) -> String {
    let mut parts: Vec<String> = Vec::new();
    match &d.kind {
        PluginDiagnosticKind::ProviderArtifactFailed { artifact, stage } => {
            parts.push(format!(
                "stage: {} artifact: {artifact}",
                provider_artifact_stage_label(*stage)
            ));
        }
        PluginDiagnosticKind::RuntimeError { method } => {
            parts.push(format!("method: {method}"));
        }
        PluginDiagnosticKind::ConfigError { key } => {
            parts.push(format!("key: {key}"));
        }
        PluginDiagnosticKind::BackendCapabilityRejected {
            primitive_kind,
            backend,
        } => {
            parts.push(format!("backend {backend} rejected {primitive_kind}"));
        }
        PluginDiagnosticKind::AbiVersionMismatch { required, host } => {
            parts.push(format!("required @{required} host @{host}"));
        }
        _ => {}
    }
    if let Some(prev) = d.previous.as_ref() {
        parts.push(format!("prev {}", short_descriptor(prev)));
    }
    if let Some(att) = d.attempted.as_ref() {
        parts.push(format!("attempted {}", short_descriptor(att)));
    }
    let raw = if parts.is_empty() {
        String::new()
    } else {
        format!(" {}", parts.join(" │ "))
    };
    truncate_to_width(&raw, inner_w)
}

/// Format a diagnostic for clipboard yank. The output is a multi-line
/// block suited for pasting into a bug report: severity tag, target,
/// kind label, full message, descriptors, and the active log path so
/// the reader can correlate with the structured trace.
fn format_diagnostic_for_yank(d: &PluginDiagnostic, log_path: Option<&Path>) -> String {
    let sev = match d.severity() {
        PluginDiagnosticSeverity::Error => "ERROR",
        PluginDiagnosticSeverity::Warning => "warn",
        PluginDiagnosticSeverity::Info => "info",
    };
    let target = match &d.target {
        PluginDiagnosticTarget::Plugin(id) => format!("plugin {}", id.0),
        PluginDiagnosticTarget::Provider(p) => format!("provider {p}"),
    };
    let kind = kind_short_label(&d.kind);
    let mut out = String::new();
    out.push_str(&format!("[{sev}] {target} ({kind})\n"));
    out.push_str(&format!("  message: {}\n", d.message));
    match &d.kind {
        PluginDiagnosticKind::ProviderArtifactFailed { artifact, stage } => {
            out.push_str(&format!(
                "  stage: {}\n  artifact: {artifact}\n",
                provider_artifact_stage_label(*stage)
            ));
        }
        PluginDiagnosticKind::RuntimeError { method } => {
            out.push_str(&format!("  method: {method}\n"));
        }
        PluginDiagnosticKind::ConfigError { key } => {
            out.push_str(&format!("  key: {key}\n"));
        }
        PluginDiagnosticKind::BackendCapabilityRejected {
            primitive_kind,
            backend,
        } => {
            out.push_str(&format!(
                "  backend: {backend}\n  primitive_kind: {primitive_kind}\n"
            ));
        }
        PluginDiagnosticKind::AbiVersionMismatch { required, host } => {
            out.push_str(&format!("  required: @{required}\n  host: @{host}\n"));
        }
        _ => {}
    }
    if let Some(prev) = d.previous.as_ref() {
        out.push_str(&format!("  previous: {}\n", short_descriptor(prev)));
    }
    if let Some(att) = d.attempted.as_ref() {
        out.push_str(&format!("  attempted: {}\n", short_descriptor(att)));
    }
    if let Some(p) = log_path {
        out.push_str(&format!("  log: {}\n", p.display()));
    }
    out
}

/// Resolve the active rotated log file from the configured base path.
///
/// `tracing_appender::rolling::daily` writes to `<prefix>.YYYY-MM-DD`;
/// the path stored in `RuntimeState::log_path` is the un-suffixed base
/// (`<dir>/kasane.log`). This walks the directory and returns the
/// most recently modified `kasane.log*` entry, falling back to the
/// configured base path when nothing is found (so Kakoune still opens
/// *something* the user can see).
fn resolve_active_log_file(configured: &Path) -> PathBuf {
    let Some(dir) = configured.parent() else {
        return configured.to_path_buf();
    };
    let Some(stem) = configured.file_name().and_then(|n| n.to_str()) else {
        return configured.to_path_buf();
    };
    let read = match std::fs::read_dir(dir) {
        Ok(r) => r,
        Err(_) => return configured.to_path_buf(),
    };
    let mut best: Option<(std::time::SystemTime, PathBuf)> = None;
    for entry in read.flatten() {
        let name = entry.file_name();
        let Some(name_str) = name.to_str() else {
            continue;
        };
        if !name_str.starts_with(stem) {
            continue;
        }
        let mtime = match entry.metadata().and_then(|m| m.modified()) {
            Ok(t) => t,
            Err(_) => continue,
        };
        match &best {
            Some((bt, _)) if *bt >= mtime => {}
            _ => best = Some((mtime, entry.path())),
        }
    }
    best.map(|(_, p)| p)
        .unwrap_or_else(|| configured.to_path_buf())
}

fn short_descriptor(d: &PluginDescriptor) -> String {
    let src = match &d.source {
        PluginSource::BundledWasm { name } => format!("bundled:{name}"),
        PluginSource::FilesystemWasm { path } => path
            .file_name()
            .and_then(|n| n.to_str())
            .map(|n| format!("file:{n}"))
            .unwrap_or_else(|| "file:?".to_string()),
        PluginSource::Host { provider } => format!("host:{provider}"),
        PluginSource::Builtin { name } => format!("builtin:{name}"),
    };
    format!("{}@{}", src, d.revision.0)
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
        PluginDiagnosticSeverity::Info => "i",
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

fn panel_detail_face() -> WireFace {
    WireFace {
        fg: Color::Rgb {
            r: 200,
            g: 200,
            b: 220,
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

fn panel_detail_separator_face() -> WireFace {
    WireFace {
        fg: Color::Rgb {
            r: 110,
            g: 110,
            b: 130,
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

    fn closed_state() -> DiagnosticsPanelState {
        DiagnosticsPanelState::default()
    }

    fn open_state() -> DiagnosticsPanelState {
        DiagnosticsPanelState::opened()
    }

    #[test]
    fn closed_passes_unrelated_keys() {
        let state = closed_state();
        let app_state = kasane_core::state::AppState::default();
        let view = AppView::new(&app_state);
        let (next, result) = handle_key(&state, &ev(Key::Char('a'), Modifiers::empty()), &view);
        assert!(passes(&result));
        assert!(!next.is_open);
    }

    #[test]
    fn closed_consumes_toggle_key_and_opens() {
        let state = closed_state();
        let app_state = kasane_core::state::AppState::default();
        let view = AppView::new(&app_state);
        let (next, result) = handle_key(&state, &ev(Key::Char('?'), Modifiers::CTRL), &view);
        assert!(consumed(&result));
        assert!(next.is_open);
        assert_eq!(next.selected, 0);
        assert_eq!(next.scroll_offset, 0);
    }

    #[test]
    fn open_consumes_all_keys() {
        let state = open_state();
        let app_state = kasane_core::state::AppState::default();
        let view = AppView::new(&app_state);
        let (next, result) = handle_key(&state, &ev(Key::Char('a'), Modifiers::empty()), &view);
        assert!(consumed(&result));
        assert!(next.is_open);
    }

    #[test]
    fn open_esc_closes() {
        let state = open_state();
        let app_state = kasane_core::state::AppState::default();
        let view = AppView::new(&app_state);
        let (next, result) = handle_key(&state, &ev(Key::Escape, Modifiers::empty()), &view);
        assert!(consumed(&result));
        assert!(!next.is_open);
    }

    #[test]
    fn open_q_closes() {
        let state = open_state();
        let app_state = kasane_core::state::AppState::default();
        let view = AppView::new(&app_state);
        let (next, result) = handle_key(&state, &ev(Key::Char('q'), Modifiers::empty()), &view);
        assert!(consumed(&result));
        assert!(!next.is_open);
    }

    #[test]
    fn open_toggle_closes() {
        let state = open_state();
        let app_state = kasane_core::state::AppState::default();
        let view = AppView::new(&app_state);
        let (next, result) = handle_key(&state, &ev(Key::Char('?'), Modifiers::CTRL), &view);
        assert!(consumed(&result));
        assert!(!next.is_open);
    }

    #[test]
    fn closed_does_not_consume_q() {
        let state = closed_state();
        let app_state = kasane_core::state::AppState::default();
        let view = AppView::new(&app_state);
        let (_, result) = handle_key(&state, &ev(Key::Char('q'), Modifiers::empty()), &view);
        assert!(passes(&result));
    }

    #[test]
    fn navigate_empty_history_is_noop() {
        let (s, o) = navigate(NavAction::Down(1), 0, 5, 0, 0);
        assert_eq!((s, o), (0, 0));
    }

    #[test]
    fn navigate_down_within_viewport_keeps_scroll() {
        let (s, o) = navigate(NavAction::Down(1), 10, 5, 0, 0);
        assert_eq!((s, o), (1, 0));
    }

    #[test]
    fn navigate_down_past_viewport_advances_scroll() {
        let (s, o) = navigate(NavAction::Down(1), 10, 5, 4, 0);
        assert_eq!((s, o), (5, 1));
    }

    #[test]
    fn navigate_up_into_scroll_pulls_scroll_back() {
        let (s, o) = navigate(NavAction::Up(1), 10, 5, 5, 5);
        assert_eq!((s, o), (4, 4));
    }

    #[test]
    fn navigate_clamps_to_bounds() {
        let (s, _) = navigate(NavAction::Down(100), 10, 5, 0, 0);
        assert_eq!(s, 9);
        let (s2, _) = navigate(NavAction::Up(100), 10, 5, 9, 5);
        assert_eq!(s2, 0);
    }

    #[test]
    fn navigate_top_and_bottom() {
        let (s, o) = navigate(NavAction::Top, 10, 5, 9, 5);
        assert_eq!((s, o), (0, 0));
        let (s2, o2) = navigate(NavAction::Bottom, 10, 5, 0, 0);
        assert_eq!((s2, o2), (9, 5));
    }

    #[test]
    fn enter_emits_no_commands_when_log_path_unset() {
        let state = open_state();
        let app_state = kasane_core::state::AppState::default();
        let view = AppView::new(&app_state);
        let (next, result) = handle_key(&state, &ev(Key::Enter, Modifiers::empty()), &view);
        match result {
            KeyPreDispatchResult::Consumed { commands, .. } => {
                assert!(commands.is_empty());
            }
            _ => panic!("expected Consumed"),
        }
        assert!(!next.is_open, "enter should close the panel");
    }

    #[test]
    fn enter_with_log_path_emits_kakoune_command_and_closes() {
        let state = open_state();
        let mut app_state = kasane_core::state::AppState::default();
        app_state.runtime.log_path = Some(std::path::PathBuf::from("/nonexistent/path/kasane.log"));
        let view = AppView::new(&app_state);
        let (next, result) = handle_key(&state, &ev(Key::Enter, Modifiers::empty()), &view);
        match result {
            KeyPreDispatchResult::Consumed { commands, .. } => {
                assert_eq!(commands.len(), 1, "should emit one Kakoune command");
            }
            _ => panic!("expected Consumed"),
        }
        assert!(!next.is_open);
    }

    #[test]
    fn yank_with_no_history_emits_no_command() {
        let state = open_state();
        let app_state = kasane_core::state::AppState::default();
        let view = AppView::new(&app_state);
        let (next, result) = handle_key(&state, &ev(Key::Char('y'), Modifiers::empty()), &view);
        match result {
            KeyPreDispatchResult::Consumed { commands, .. } => {
                assert!(commands.is_empty());
            }
            _ => panic!("expected Consumed"),
        }
        assert!(next.is_open, "y must not close the panel");
    }

    #[test]
    fn yank_with_history_emits_set_clipboard_command() {
        let state = open_state();
        let mut app_state = kasane_core::state::AppState::default();
        app_state.runtime.diagnostic_history.record(&[
            kasane_core::plugin::diagnostics::PluginDiagnostic::instantiation_failed(
                kasane_core::plugin::PluginId::from("session-ui"),
                "wasm trap: unreachable",
            ),
        ]);
        let view = AppView::new(&app_state);
        let (next, result) = handle_key(&state, &ev(Key::Char('y'), Modifiers::empty()), &view);
        match result {
            KeyPreDispatchResult::Consumed { commands, .. } => {
                assert_eq!(commands.len(), 1);
                match &commands[0] {
                    kasane_core::plugin::Command::SetClipboard(text) => {
                        assert!(text.contains("ERROR"));
                        assert!(text.contains("session-ui"));
                        assert!(text.contains("wasm trap: unreachable"));
                    }
                    other => panic!("expected SetClipboard, got {:?}", other.variant_name()),
                }
            }
            _ => panic!("expected Consumed"),
        }
        assert!(next.is_open);
    }

    #[test]
    fn resolve_active_log_file_returns_configured_when_dir_missing() {
        let p = std::path::PathBuf::from("/definitely/not/a/dir/kasane.log");
        assert_eq!(resolve_active_log_file(&p), p);
    }

    #[test]
    fn r_emits_trigger_plugin_reload_and_closes() {
        let state = open_state();
        let app_state = kasane_core::state::AppState::default();
        let view = AppView::new(&app_state);
        let (next, result) = handle_key(&state, &ev(Key::Char('r'), Modifiers::empty()), &view);
        match result {
            KeyPreDispatchResult::Consumed { commands, .. } => {
                assert_eq!(commands.len(), 1);
                assert_eq!(commands[0].variant_name(), "TriggerPluginReload");
            }
            _ => panic!("expected Consumed"),
        }
        assert!(!next.is_open, "r should close the panel");
    }

    #[test]
    fn open_emits_dismiss_diagnostic_overlay() {
        let state = DiagnosticsPanelState::default();
        let app_state = kasane_core::state::AppState::default();
        let view = AppView::new(&app_state);
        let (next, result) = handle_key(&state, &ev(Key::Char('?'), Modifiers::CTRL), &view);
        match result {
            KeyPreDispatchResult::Consumed { commands, .. } => {
                assert_eq!(commands.len(), 1);
                assert_eq!(commands[0].variant_name(), "DismissDiagnosticOverlay");
            }
            _ => panic!("expected Consumed"),
        }
        assert!(next.is_open);
    }

    #[test]
    fn nav_action_recognized_for_arrows_and_letters() {
        assert_eq!(
            nav_action_for(&ev(Key::Down, Modifiers::empty())),
            Some(NavAction::Down(1))
        );
        assert_eq!(
            nav_action_for(&ev(Key::Char('j'), Modifiers::empty())),
            Some(NavAction::Down(1))
        );
        assert_eq!(
            nav_action_for(&ev(Key::Char('k'), Modifiers::empty())),
            Some(NavAction::Up(1))
        );
        assert_eq!(
            nav_action_for(&ev(Key::Char('G'), Modifiers::empty())),
            Some(NavAction::Bottom)
        );
        assert_eq!(
            nav_action_for(&ev(Key::Char('d'), Modifiers::CTRL)),
            Some(NavAction::Down(10))
        );
        assert_eq!(
            nav_action_for(&ev(Key::Char('a'), Modifiers::empty())),
            None
        );
    }
}
