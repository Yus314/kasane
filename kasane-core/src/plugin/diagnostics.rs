use std::borrow::Cow;
use std::time::{Duration, Instant};

use crate::protocol::{Attributes, Color, Face, NamedColor};
use crate::surface::SurfaceRegistrationError;

use super::{PluginDescriptor, PluginId};

pub const DEFAULT_PLUGIN_DIAGNOSTIC_OVERLAY_LINES: usize = 3;
pub const PLUGIN_DIAGNOSTIC_OVERLAY_TITLE: &str = "plugin diagnostics";
pub const PLUGIN_ACTIVATION_OVERLAY_TITLE: &str = "plugin activation";
pub const PLUGIN_DISCOVERY_OVERLAY_TITLE: &str = "plugin discovery";
const ERROR_PLUGIN_DIAGNOSTIC_OVERLAY_LINES: usize = 4;
const PLUGIN_ERROR_PLUGIN_DIAGNOSTIC_OVERLAY_LINES: usize = 5;
const MIN_PLUGIN_DIAGNOSTIC_OVERLAY_COLS: u16 = 8;
const MIN_PLUGIN_DIAGNOSTIC_OVERLAY_ROWS: u16 = 3;
const PLUGIN_DIAGNOSTIC_OVERLAY_COALESCE_WINDOW: Duration = Duration::from_millis(750);
const WARNING_PLUGIN_DIAGNOSTIC_OVERLAY_DURATION: Duration = Duration::from_secs(4);
const ERROR_PLUGIN_DIAGNOSTIC_OVERLAY_DURATION: Duration = Duration::from_secs(8);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PluginDiagnosticSeverity {
    Warning,
    Error,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ProviderArtifactStage {
    Read,
    Load,
    Instantiate,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PluginDiagnosticTarget {
    Plugin(PluginId),
    Provider(String),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PluginDiagnosticKind {
    SurfaceRegistrationFailed {
        reason: SurfaceRegistrationError,
    },
    InstantiationFailed,
    ProviderCollectFailed,
    ProviderArtifactFailed {
        artifact: String,
        stage: ProviderArtifactStage,
    },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PluginDiagnostic {
    pub target: PluginDiagnosticTarget,
    pub kind: PluginDiagnosticKind,
    pub message: String,
    pub previous: Option<PluginDescriptor>,
    pub attempted: Option<PluginDescriptor>,
}

impl PluginDiagnostic {
    pub fn surface_registration_failed(
        plugin_id: PluginId,
        reason: SurfaceRegistrationError,
    ) -> Self {
        Self {
            target: PluginDiagnosticTarget::Plugin(plugin_id),
            message: format!("{reason:?}"),
            kind: PluginDiagnosticKind::SurfaceRegistrationFailed { reason },
            previous: None,
            attempted: None,
        }
    }

    pub fn instantiation_failed(plugin_id: PluginId, message: impl Into<String>) -> Self {
        Self {
            target: PluginDiagnosticTarget::Plugin(plugin_id),
            message: message.into(),
            kind: PluginDiagnosticKind::InstantiationFailed,
            previous: None,
            attempted: None,
        }
    }

    pub fn provider_collect_failed(
        provider: impl Into<String>,
        message: impl Into<String>,
    ) -> Self {
        Self {
            target: PluginDiagnosticTarget::Provider(provider.into()),
            message: message.into(),
            kind: PluginDiagnosticKind::ProviderCollectFailed,
            previous: None,
            attempted: None,
        }
    }

    pub fn provider_artifact_failed(
        provider: impl Into<String>,
        artifact: impl Into<String>,
        stage: ProviderArtifactStage,
        message: impl Into<String>,
    ) -> Self {
        Self {
            target: PluginDiagnosticTarget::Provider(provider.into()),
            message: message.into(),
            kind: PluginDiagnosticKind::ProviderArtifactFailed {
                artifact: artifact.into(),
                stage,
            },
            previous: None,
            attempted: None,
        }
    }

    pub fn plugin_id(&self) -> Option<&PluginId> {
        match &self.target {
            PluginDiagnosticTarget::Plugin(plugin_id) => Some(plugin_id),
            PluginDiagnosticTarget::Provider(_) => None,
        }
    }

    pub fn provider_name(&self) -> Option<&str> {
        match &self.target {
            PluginDiagnosticTarget::Plugin(_) => None,
            PluginDiagnosticTarget::Provider(provider) => Some(provider.as_str()),
        }
    }

    pub fn severity(&self) -> PluginDiagnosticSeverity {
        match self.kind {
            PluginDiagnosticKind::SurfaceRegistrationFailed { .. }
            | PluginDiagnosticKind::InstantiationFailed
            | PluginDiagnosticKind::ProviderCollectFailed => PluginDiagnosticSeverity::Error,
            PluginDiagnosticKind::ProviderArtifactFailed { .. } => {
                PluginDiagnosticSeverity::Warning
            }
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PluginDiagnosticOverlayLine {
    pub severity: PluginDiagnosticSeverity,
    pub tag_kind: PluginDiagnosticOverlayTagKind,
    pub text: String,
    pub repeat_count: usize,
}

impl PluginDiagnosticOverlayLine {
    pub fn display_text(&self) -> Cow<'_, str> {
        if self.repeat_count <= 1 {
            Cow::Borrowed(self.text.as_str())
        } else {
            Cow::Owned(format!("{} x{}", self.text, self.repeat_count))
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PluginDiagnosticOverlayTagKind {
    Activation,
    Discovery,
    ArtifactRead,
    ArtifactLoad,
    ArtifactInstantiate,
}

#[derive(Clone, Debug)]
struct OverlayBucket {
    line: PluginDiagnosticOverlayLine,
    priority: (u8, u8, u8, u8),
    target: PluginDiagnosticTarget,
    last_seen: usize,
}

#[derive(Default)]
pub struct PluginDiagnosticOverlayState {
    generation: u64,
    lines: Vec<PluginDiagnosticOverlayLine>,
    hidden_count: usize,
    last_recorded_at: Option<Instant>,
    retained: Vec<OverlayBucket>,
}

impl PluginDiagnosticOverlayState {
    pub fn record(&mut self, diagnostics: &[PluginDiagnostic]) -> Option<u64> {
        self.record_at(diagnostics, Instant::now())
    }

    fn record_at(&mut self, diagnostics: &[PluginDiagnostic], now: Instant) -> Option<u64> {
        if diagnostics.is_empty() {
            return None;
        }

        self.generation = self.generation.saturating_add(1);
        let current = collect_diagnostic_overlay_buckets(diagnostics);
        let all_lines = if self.last_recorded_at.is_some_and(|previous| {
            now.duration_since(previous) <= PLUGIN_DIAGNOSTIC_OVERLAY_COALESCE_WINDOW
        }) {
            merge_overlay_buckets(std::mem::take(&mut self.retained), current)
        } else {
            current
        };
        let line_limit = overlay_line_limit(&all_lines);
        self.retained = all_lines.clone();
        self.last_recorded_at = Some(now);
        self.hidden_count = all_lines.len().saturating_sub(line_limit);
        self.lines = all_lines
            .into_iter()
            .map(|bucket| bucket.line)
            .take(line_limit)
            .collect();
        Some(self.generation)
    }

    pub fn dismiss(&mut self, generation: u64) -> bool {
        if self.lines.is_empty() || self.generation != generation {
            return false;
        }
        self.lines.clear();
        self.hidden_count = 0;
        self.last_recorded_at = None;
        self.retained.clear();
        true
    }

    pub fn is_active(&self) -> bool {
        !self.lines.is_empty()
    }

    pub fn lines(&self) -> &[PluginDiagnosticOverlayLine] {
        &self.lines
    }

    pub fn hidden_count(&self) -> usize {
        self.hidden_count
    }

    pub fn frame(&self, cols: u16, rows: u16) -> Option<PluginDiagnosticOverlayFrame> {
        plugin_diagnostic_overlay_frame_with_title(
            overlay_title(&self.retained),
            &self.lines,
            self.hidden_count,
            cols,
            rows,
        )
    }

    pub fn paint_spec(&self, cols: u16, rows: u16) -> Option<PluginDiagnosticOverlayPaintSpec> {
        plugin_diagnostic_overlay_paint_spec_with_title(
            overlay_title(&self.retained),
            &self.lines,
            self.hidden_count,
            cols,
            rows,
        )
    }

    pub fn paint_with<P: PluginDiagnosticOverlayPainter>(
        &self,
        cols: u16,
        rows: u16,
        painter: &mut P,
    ) -> bool {
        let Some(spec) = self.paint_spec(cols, rows) else {
            return false;
        };
        paint_plugin_diagnostic_overlay(&spec, painter);
        true
    }

    pub fn dismiss_after(&self) -> Option<Duration> {
        let severity = self
            .lines
            .iter()
            .map(|line| line.severity)
            .max_by_key(|severity| match severity {
                PluginDiagnosticSeverity::Warning => 0,
                PluginDiagnosticSeverity::Error => 1,
            })?;

        Some(match severity {
            PluginDiagnosticSeverity::Warning => WARNING_PLUGIN_DIAGNOSTIC_OVERLAY_DURATION,
            PluginDiagnosticSeverity::Error => ERROR_PLUGIN_DIAGNOSTIC_OVERLAY_DURATION,
        })
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PluginDiagnosticOverlayLayout {
    pub header: String,
    pub x: u16,
    pub y: u16,
    pub width: u16,
    pub height: u16,
    pub severity: PluginDiagnosticSeverity,
}

impl PluginDiagnosticOverlayLayout {
    pub fn header_text_width(&self) -> u16 {
        self.width.saturating_sub(2)
    }

    pub fn body_text_width(&self) -> u16 {
        self.width.saturating_sub(4)
    }

    pub fn row_y(&self, index: usize) -> Option<u16> {
        let row = self.y + 1 + index as u16;
        if row >= self.y + self.height.saturating_sub(1) {
            None
        } else {
            Some(row)
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PluginDiagnosticOverlayRow {
    pub y: u16,
    pub severity: PluginDiagnosticSeverity,
    pub tag_kind: PluginDiagnosticOverlayTagKind,
    pub tag: &'static str,
    pub text: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PluginDiagnosticOverlayFrame {
    pub layout: PluginDiagnosticOverlayLayout,
    pub header_text: String,
    pub rows: Vec<PluginDiagnosticOverlayRow>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PluginDiagnosticOverlayTextRun {
    pub x: u16,
    pub y: u16,
    pub text: String,
    pub face: Face,
    pub max_width: u16,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PluginDiagnosticOverlayShadowSpec {
    pub offset: (f32, f32),
    pub blur_radius: f32,
    pub color: [f32; 4],
}

#[derive(Clone, Debug, PartialEq)]
pub struct PluginDiagnosticOverlayPaintSpec {
    pub layout: PluginDiagnosticOverlayLayout,
    pub header_face: Face,
    pub body_face: Face,
    pub border_face: Face,
    pub shadow: Option<PluginDiagnosticOverlayShadowSpec>,
    pub text_runs: Vec<PluginDiagnosticOverlayTextRun>,
}

pub trait PluginDiagnosticOverlayPainter {
    fn fill_region(&mut self, x: u16, y: u16, width: u16, height: u16, face: Face);
    fn draw_border(&mut self, x: u16, y: u16, width: u16, height: u16, face: Face);
    fn draw_text_run(&mut self, run: &PluginDiagnosticOverlayTextRun);
}

pub fn summarize_plugin_diagnostic(diagnostic: &PluginDiagnostic) -> String {
    let target = diagnostic
        .plugin_id()
        .map(|id| id.0.as_str())
        .or_else(|| diagnostic.provider_name())
        .unwrap_or("unknown");

    match &diagnostic.kind {
        PluginDiagnosticKind::SurfaceRegistrationFailed { .. } => {
            format!("{target}: surface registration failed")
        }
        PluginDiagnosticKind::InstantiationFailed => {
            format!("{target}: {}", diagnostic.message)
        }
        PluginDiagnosticKind::ProviderCollectFailed => {
            format!("{target}: {}", diagnostic.message)
        }
        PluginDiagnosticKind::ProviderArtifactFailed { artifact, stage } => {
            format!(
                "{target}: {} {}",
                provider_artifact_stage_summary_label(*stage),
                provider_artifact_summary_name(artifact)
            )
        }
    }
}

pub fn provider_artifact_stage_label(stage: ProviderArtifactStage) -> &'static str {
    match stage {
        ProviderArtifactStage::Read => "read",
        ProviderArtifactStage::Load => "load",
        ProviderArtifactStage::Instantiate => "instantiate",
    }
}

fn provider_artifact_stage_summary_label(stage: ProviderArtifactStage) -> &'static str {
    match stage {
        ProviderArtifactStage::Read => "read",
        ProviderArtifactStage::Load => "load",
        ProviderArtifactStage::Instantiate => "init",
    }
}

fn provider_artifact_summary_name(artifact: &str) -> &str {
    std::path::Path::new(artifact)
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or(artifact)
}

fn overlay_title(buckets: &[OverlayBucket]) -> &'static str {
    match buckets.first().map(|bucket| &bucket.target) {
        Some(PluginDiagnosticTarget::Plugin(_)) => PLUGIN_ACTIVATION_OVERLAY_TITLE,
        Some(PluginDiagnosticTarget::Provider(_)) => PLUGIN_DISCOVERY_OVERLAY_TITLE,
        None => PLUGIN_DIAGNOSTIC_OVERLAY_TITLE,
    }
}

pub fn diagnostic_overlay_lines(
    diagnostics: &[PluginDiagnostic],
    max_lines: usize,
) -> Vec<PluginDiagnosticOverlayLine> {
    collect_diagnostic_overlay_buckets(diagnostics)
        .into_iter()
        .map(|bucket| bucket.line)
        .take(max_lines)
        .collect()
}

fn collect_diagnostic_overlay_buckets(diagnostics: &[PluginDiagnostic]) -> Vec<OverlayBucket> {
    let mut buckets: Vec<OverlayBucket> = Vec::new();

    for (index, diagnostic) in diagnostics.iter().enumerate() {
        let severity = diagnostic.severity();
        let text = summarize_plugin_diagnostic(diagnostic);
        let priority = diagnostic_overlay_priority(diagnostic);
        let tag_kind = diagnostic_overlay_tag_kind(diagnostic);
        if let Some(existing) = buckets.iter_mut().find(|bucket| {
            bucket.line.severity == severity
                && bucket.line.tag_kind == tag_kind
                && bucket.line.text == text
        }) {
            existing.line.repeat_count += 1;
            existing.last_seen = index;
            existing.priority = priority;
            continue;
        }

        buckets.push(OverlayBucket {
            line: PluginDiagnosticOverlayLine {
                severity,
                tag_kind,
                text,
                repeat_count: 1,
            },
            priority,
            target: diagnostic.target.clone(),
            last_seen: index,
        });
    }

    sort_overlay_buckets(&mut buckets);
    buckets
}

fn merge_overlay_buckets(
    previous: Vec<OverlayBucket>,
    current: Vec<OverlayBucket>,
) -> Vec<OverlayBucket> {
    if previous.is_empty() {
        return current;
    }
    if current.is_empty() {
        return previous;
    }

    let previous_len = previous.len();
    let mut buckets: Vec<OverlayBucket> = previous
        .into_iter()
        .enumerate()
        .map(|(index, mut bucket)| {
            bucket.last_seen = index;
            bucket
        })
        .collect();

    for (index, bucket) in current.into_iter().enumerate() {
        if let Some(existing) = buckets.iter_mut().find(|existing| {
            existing.line.severity == bucket.line.severity
                && existing.line.tag_kind == bucket.line.tag_kind
                && existing.line.text == bucket.line.text
        }) {
            existing.line.repeat_count += bucket.line.repeat_count;
            existing.priority = bucket.priority;
            existing.last_seen = previous_len + index;
            continue;
        }
        buckets.push(OverlayBucket {
            last_seen: previous_len + index,
            ..bucket
        });
    }

    sort_overlay_buckets(&mut buckets);
    buckets
}

fn overlay_line_limit(buckets: &[OverlayBucket]) -> usize {
    if buckets.iter().any(|bucket| {
        bucket.line.severity == PluginDiagnosticSeverity::Error
            && matches!(bucket.target, PluginDiagnosticTarget::Plugin(_))
    }) {
        PLUGIN_ERROR_PLUGIN_DIAGNOSTIC_OVERLAY_LINES
    } else if buckets
        .iter()
        .any(|bucket| bucket.line.severity == PluginDiagnosticSeverity::Error)
    {
        ERROR_PLUGIN_DIAGNOSTIC_OVERLAY_LINES
    } else {
        DEFAULT_PLUGIN_DIAGNOSTIC_OVERLAY_LINES
    }
}

fn sort_overlay_buckets(buckets: &mut [OverlayBucket]) {
    buckets.sort_by(|left, right| {
        right
            .priority
            .cmp(&left.priority)
            .then_with(|| right.last_seen.cmp(&left.last_seen))
    });
}

fn diagnostic_overlay_priority(diagnostic: &PluginDiagnostic) -> (u8, u8, u8, u8) {
    let severity = match diagnostic.severity() {
        PluginDiagnosticSeverity::Error => 2,
        PluginDiagnosticSeverity::Warning => 1,
    };
    let target = match diagnostic.target {
        PluginDiagnosticTarget::Plugin(_) => 2,
        PluginDiagnosticTarget::Provider(_) => 1,
    };
    let kind = match diagnostic.kind {
        PluginDiagnosticKind::SurfaceRegistrationFailed { .. } => 3,
        PluginDiagnosticKind::InstantiationFailed => 2,
        PluginDiagnosticKind::ProviderCollectFailed => 1,
        PluginDiagnosticKind::ProviderArtifactFailed { .. } => 0,
    };
    let stage = match diagnostic.kind {
        PluginDiagnosticKind::ProviderArtifactFailed { stage, .. } => {
            provider_artifact_overlay_priority(stage)
        }
        _ => 0,
    };
    (severity, target, kind, stage)
}

fn provider_artifact_overlay_priority(stage: ProviderArtifactStage) -> u8 {
    match stage {
        ProviderArtifactStage::Instantiate => 2,
        ProviderArtifactStage::Load => 1,
        ProviderArtifactStage::Read => 0,
    }
}

fn diagnostic_overlay_tag_kind(diagnostic: &PluginDiagnostic) -> PluginDiagnosticOverlayTagKind {
    match diagnostic.kind {
        PluginDiagnosticKind::SurfaceRegistrationFailed { .. }
        | PluginDiagnosticKind::InstantiationFailed => PluginDiagnosticOverlayTagKind::Activation,
        PluginDiagnosticKind::ProviderCollectFailed => PluginDiagnosticOverlayTagKind::Discovery,
        PluginDiagnosticKind::ProviderArtifactFailed { stage, .. } => match stage {
            ProviderArtifactStage::Read => PluginDiagnosticOverlayTagKind::ArtifactRead,
            ProviderArtifactStage::Load => PluginDiagnosticOverlayTagKind::ArtifactLoad,
            ProviderArtifactStage::Instantiate => {
                PluginDiagnosticOverlayTagKind::ArtifactInstantiate
            }
        },
    }
}

pub fn plugin_diagnostic_overlay_layout(
    lines: &[PluginDiagnosticOverlayLine],
    hidden_count: usize,
    cols: u16,
    rows: u16,
) -> Option<PluginDiagnosticOverlayLayout> {
    plugin_diagnostic_overlay_layout_with_title(
        PLUGIN_DIAGNOSTIC_OVERLAY_TITLE,
        lines,
        hidden_count,
        cols,
        rows,
    )
}

fn plugin_diagnostic_overlay_layout_with_title(
    title: &str,
    lines: &[PluginDiagnosticOverlayLine],
    hidden_count: usize,
    cols: u16,
    rows: u16,
) -> Option<PluginDiagnosticOverlayLayout> {
    if lines.is_empty()
        || cols < MIN_PLUGIN_DIAGNOSTIC_OVERLAY_COLS
        || rows < MIN_PLUGIN_DIAGNOSTIC_OVERLAY_ROWS
    {
        return None;
    }

    let header = if hidden_count == 0 {
        format!(" {title} ({}) ", lines.len())
    } else {
        format!(" {title} ({}/{}) ", lines.len(), lines.len() + hidden_count)
    };
    let body_width = lines
        .iter()
        .map(|line| line.display_text().chars().count() as u16)
        .max()
        .unwrap_or(0);
    let inner_width = body_width
        .max(header.chars().count() as u16)
        .min(cols.saturating_sub(4));
    let width = (inner_width + 2).min(cols);
    let height = ((lines.len() as u16) + 2).min(rows);
    let severity = if lines
        .iter()
        .any(|line| line.severity == PluginDiagnosticSeverity::Error)
    {
        PluginDiagnosticSeverity::Error
    } else {
        PluginDiagnosticSeverity::Warning
    };

    Some(PluginDiagnosticOverlayLayout {
        header,
        x: cols.saturating_sub(width),
        y: 0,
        width,
        height,
        severity,
    })
}

pub fn plugin_diagnostic_overlay_frame(
    lines: &[PluginDiagnosticOverlayLine],
    hidden_count: usize,
    cols: u16,
    rows: u16,
) -> Option<PluginDiagnosticOverlayFrame> {
    plugin_diagnostic_overlay_frame_with_title(
        PLUGIN_DIAGNOSTIC_OVERLAY_TITLE,
        lines,
        hidden_count,
        cols,
        rows,
    )
}

fn plugin_diagnostic_overlay_frame_with_title(
    title: &str,
    lines: &[PluginDiagnosticOverlayLine],
    hidden_count: usize,
    cols: u16,
    rows: u16,
) -> Option<PluginDiagnosticOverlayFrame> {
    let layout =
        plugin_diagnostic_overlay_layout_with_title(title, lines, hidden_count, cols, rows)?;
    let header_text = truncate_to_width(&layout.header, layout.header_text_width());
    let rows = lines
        .iter()
        .enumerate()
        .filter_map(|(idx, line)| {
            let row_y = layout.row_y(idx)?;
            let text = line.display_text();
            Some(PluginDiagnosticOverlayRow {
                y: row_y,
                severity: line.severity,
                tag_kind: line.tag_kind,
                tag: plugin_diagnostic_overlay_tag_text(line.tag_kind),
                text: truncate_to_width(&text, layout.body_text_width()),
            })
        })
        .collect();

    Some(PluginDiagnosticOverlayFrame {
        layout,
        header_text,
        rows,
    })
}

pub fn plugin_diagnostic_overlay_paint_spec(
    lines: &[PluginDiagnosticOverlayLine],
    hidden_count: usize,
    cols: u16,
    rows: u16,
) -> Option<PluginDiagnosticOverlayPaintSpec> {
    plugin_diagnostic_overlay_paint_spec_with_title(
        PLUGIN_DIAGNOSTIC_OVERLAY_TITLE,
        lines,
        hidden_count,
        cols,
        rows,
    )
}

fn plugin_diagnostic_overlay_paint_spec_with_title(
    title: &str,
    lines: &[PluginDiagnosticOverlayLine],
    hidden_count: usize,
    cols: u16,
    rows: u16,
) -> Option<PluginDiagnosticOverlayPaintSpec> {
    let frame = plugin_diagnostic_overlay_frame_with_title(title, lines, hidden_count, cols, rows)?;
    let layout = frame.layout.clone();
    let severity = layout.severity;
    let header_face = plugin_diagnostic_overlay_header_face_for(title, severity);
    let body_face = plugin_diagnostic_overlay_body_face_for(title, severity);
    let border_face = plugin_diagnostic_overlay_border_face(severity);

    let mut text_runs = vec![PluginDiagnosticOverlayTextRun {
        x: layout.x + 1,
        y: layout.y,
        text: frame.header_text,
        face: header_face,
        max_width: layout.header_text_width(),
    }];

    text_runs.extend(frame.rows.into_iter().flat_map(|row| {
        let tag_face = plugin_diagnostic_overlay_tag_face(row.tag_kind, row.severity);
        let text_face = plugin_diagnostic_overlay_text_face(row.tag_kind, row.severity);
        [
            PluginDiagnosticOverlayTextRun {
                x: layout.x + 1,
                y: row.y,
                text: row.tag.to_string(),
                face: tag_face,
                max_width: 1,
            },
            PluginDiagnosticOverlayTextRun {
                x: layout.x + 3,
                y: row.y,
                text: row.text,
                face: text_face,
                max_width: layout.body_text_width(),
            },
        ]
    }));

    Some(PluginDiagnosticOverlayPaintSpec {
        layout,
        header_face,
        body_face,
        border_face,
        shadow: Some(plugin_diagnostic_overlay_shadow_spec_for(title, severity)),
        text_runs,
    })
}

pub fn plugin_diagnostic_overlay_shadow_spec() -> PluginDiagnosticOverlayShadowSpec {
    plugin_diagnostic_overlay_shadow_spec_for(
        PLUGIN_DIAGNOSTIC_OVERLAY_TITLE,
        PluginDiagnosticSeverity::Warning,
    )
}

pub fn plugin_diagnostic_overlay_shadow_spec_for(
    title: &str,
    severity: PluginDiagnosticSeverity,
) -> PluginDiagnosticOverlayShadowSpec {
    match (title, severity) {
        (PLUGIN_ACTIVATION_OVERLAY_TITLE, PluginDiagnosticSeverity::Error) => {
            PluginDiagnosticOverlayShadowSpec {
                offset: (8.0, 8.0),
                blur_radius: 9.0,
                color: [0.16, 0.03, 0.03, 0.38],
            }
        }
        (PLUGIN_DISCOVERY_OVERLAY_TITLE, PluginDiagnosticSeverity::Error) => {
            PluginDiagnosticOverlayShadowSpec {
                offset: (7.0, 7.0),
                blur_radius: 8.0,
                color: [0.12, 0.07, 0.01, 0.34],
            }
        }
        (PLUGIN_DISCOVERY_OVERLAY_TITLE, PluginDiagnosticSeverity::Warning) => {
            PluginDiagnosticOverlayShadowSpec {
                offset: (5.0, 5.0),
                blur_radius: 5.0,
                color: [0.05, 0.04, 0.01, 0.24],
            }
        }
        _ => PluginDiagnosticOverlayShadowSpec {
            offset: (6.0, 6.0),
            blur_radius: 6.0,
            color: [0.0, 0.0, 0.0, 0.30],
        },
    }
}

pub fn paint_plugin_diagnostic_overlay<P: PluginDiagnosticOverlayPainter>(
    spec: &PluginDiagnosticOverlayPaintSpec,
    painter: &mut P,
) {
    painter.fill_region(
        spec.layout.x,
        spec.layout.y,
        spec.layout.width,
        spec.layout.height,
        spec.body_face,
    );
    painter.fill_region(
        spec.layout.x,
        spec.layout.y,
        spec.layout.width,
        1,
        spec.header_face,
    );
    painter.draw_border(
        spec.layout.x,
        spec.layout.y,
        spec.layout.width,
        spec.layout.height,
        spec.border_face,
    );
    for run in &spec.text_runs {
        painter.draw_text_run(run);
    }
}

pub fn plugin_diagnostic_overlay_border_face(severity: PluginDiagnosticSeverity) -> Face {
    Face {
        fg: match severity {
            PluginDiagnosticSeverity::Warning => Color::Named(NamedColor::BrightYellow),
            PluginDiagnosticSeverity::Error => Color::Named(NamedColor::BrightRed),
        },
        bg: Color::Rgb {
            r: 18,
            g: 18,
            b: 18,
        },
        underline: Color::Default,
        attributes: Attributes::BOLD,
    }
}

pub fn plugin_diagnostic_overlay_header_face(severity: PluginDiagnosticSeverity) -> Face {
    plugin_diagnostic_overlay_header_face_for(PLUGIN_DIAGNOSTIC_OVERLAY_TITLE, severity)
}

pub fn plugin_diagnostic_overlay_header_face_for(
    title: &str,
    severity: PluginDiagnosticSeverity,
) -> Face {
    Face {
        fg: Color::Named(NamedColor::BrightWhite),
        bg: match (title, severity) {
            (PLUGIN_ACTIVATION_OVERLAY_TITLE, PluginDiagnosticSeverity::Error) => Color::Rgb {
                r: 128,
                g: 20,
                b: 20,
            },
            (PLUGIN_DISCOVERY_OVERLAY_TITLE, PluginDiagnosticSeverity::Error) => Color::Rgb {
                r: 112,
                g: 60,
                b: 16,
            },
            (PLUGIN_DISCOVERY_OVERLAY_TITLE, PluginDiagnosticSeverity::Warning) => Color::Rgb {
                r: 88,
                g: 68,
                b: 24,
            },
            (_, PluginDiagnosticSeverity::Warning) => Color::Rgb {
                r: 96,
                g: 72,
                b: 12,
            },
            (_, PluginDiagnosticSeverity::Error) => Color::Rgb {
                r: 112,
                g: 24,
                b: 24,
            },
        },
        underline: Color::Default,
        attributes: Attributes::BOLD,
    }
}

pub fn plugin_diagnostic_overlay_body_face() -> Face {
    plugin_diagnostic_overlay_body_face_for(
        PLUGIN_DIAGNOSTIC_OVERLAY_TITLE,
        PluginDiagnosticSeverity::Warning,
    )
}

pub fn plugin_diagnostic_overlay_body_face_for(
    title: &str,
    severity: PluginDiagnosticSeverity,
) -> Face {
    Face {
        fg: Color::Named(NamedColor::BrightWhite),
        bg: match (title, severity) {
            (PLUGIN_ACTIVATION_OVERLAY_TITLE, PluginDiagnosticSeverity::Error) => Color::Rgb {
                r: 28,
                g: 18,
                b: 18,
            },
            (PLUGIN_DISCOVERY_OVERLAY_TITLE, PluginDiagnosticSeverity::Error) => Color::Rgb {
                r: 28,
                g: 23,
                b: 17,
            },
            (PLUGIN_DISCOVERY_OVERLAY_TITLE, PluginDiagnosticSeverity::Warning) => Color::Rgb {
                r: 26,
                g: 24,
                b: 20,
            },
            _ => Color::Rgb {
                r: 24,
                g: 24,
                b: 24,
            },
        },
        underline: Color::Default,
        attributes: Attributes::empty(),
    }
}

pub fn plugin_diagnostic_overlay_text_face(
    kind: PluginDiagnosticOverlayTagKind,
    severity: PluginDiagnosticSeverity,
) -> Face {
    Face {
        fg: match (kind, severity) {
            (PluginDiagnosticOverlayTagKind::Activation, PluginDiagnosticSeverity::Error) => {
                Color::Named(NamedColor::BrightWhite)
            }
            (PluginDiagnosticOverlayTagKind::Activation, PluginDiagnosticSeverity::Warning) => {
                Color::Named(NamedColor::BrightYellow)
            }
            (PluginDiagnosticOverlayTagKind::Discovery, _) => Color::Rgb {
                r: 245,
                g: 214,
                b: 168,
            },
            (PluginDiagnosticOverlayTagKind::ArtifactRead, _) => Color::Rgb {
                r: 171,
                g: 212,
                b: 255,
            },
            (PluginDiagnosticOverlayTagKind::ArtifactLoad, _) => {
                Color::Named(NamedColor::BrightYellow)
            }
            (PluginDiagnosticOverlayTagKind::ArtifactInstantiate, _) => Color::Rgb {
                r: 255,
                g: 194,
                b: 114,
            },
        },
        bg: Color::Rgb {
            r: 24,
            g: 24,
            b: 24,
        },
        underline: Color::Default,
        attributes: match severity {
            PluginDiagnosticSeverity::Error => Attributes::BOLD,
            PluginDiagnosticSeverity::Warning => Attributes::empty(),
        },
    }
}

pub fn plugin_diagnostic_overlay_tag_face(
    kind: PluginDiagnosticOverlayTagKind,
    severity: PluginDiagnosticSeverity,
) -> Face {
    Face {
        fg: match kind {
            PluginDiagnosticOverlayTagKind::Discovery => Color::Named(NamedColor::BrightWhite),
            _ => Color::Named(NamedColor::Black),
        },
        bg: match (kind, severity) {
            (PluginDiagnosticOverlayTagKind::Activation, PluginDiagnosticSeverity::Error) => {
                Color::Named(NamedColor::BrightRed)
            }
            (PluginDiagnosticOverlayTagKind::Activation, PluginDiagnosticSeverity::Warning) => {
                Color::Named(NamedColor::BrightYellow)
            }
            (PluginDiagnosticOverlayTagKind::Discovery, _) => Color::Rgb {
                r: 124,
                g: 54,
                b: 18,
            },
            (PluginDiagnosticOverlayTagKind::ArtifactRead, _) => Color::Rgb {
                r: 78,
                g: 106,
                b: 158,
            },
            (PluginDiagnosticOverlayTagKind::ArtifactLoad, _) => {
                Color::Named(NamedColor::BrightYellow)
            }
            (PluginDiagnosticOverlayTagKind::ArtifactInstantiate, _) => Color::Rgb {
                r: 214,
                g: 126,
                b: 34,
            },
        },
        underline: Color::Default,
        attributes: Attributes::BOLD,
    }
}

pub fn plugin_diagnostic_overlay_tag_text(kind: PluginDiagnosticOverlayTagKind) -> &'static str {
    match kind {
        PluginDiagnosticOverlayTagKind::Activation => "P",
        PluginDiagnosticOverlayTagKind::Discovery => "D",
        PluginDiagnosticOverlayTagKind::ArtifactRead => "R",
        PluginDiagnosticOverlayTagKind::ArtifactLoad => "L",
        PluginDiagnosticOverlayTagKind::ArtifactInstantiate => "I",
    }
}

fn truncate_to_width(text: &str, width: u16) -> String {
    text.chars().take(width as usize).collect()
}

pub fn report_plugin_diagnostics(diagnostics: &[PluginDiagnostic]) {
    for diagnostic in diagnostics {
        let plugin_id = diagnostic.plugin_id().map(|plugin_id| plugin_id.0.as_str());
        let provider = diagnostic.provider_name();
        let severity = diagnostic.severity();
        match diagnostic.kind {
            PluginDiagnosticKind::SurfaceRegistrationFailed { ref reason } => {
                match severity {
                    PluginDiagnosticSeverity::Warning => tracing::warn!(
                        plugin_id = plugin_id.unwrap_or("none"),
                        provider = provider.unwrap_or("none"),
                        kind = "surface_registration_failed",
                        reason = ?reason,
                        message = %diagnostic.message,
                        previous_source = ?diagnostic.previous.as_ref().map(|descriptor| &descriptor.source),
                        previous_revision = diagnostic.previous.as_ref().map(|descriptor| descriptor.revision.0.as_str()).unwrap_or("none"),
                        attempted_source = ?diagnostic.attempted.as_ref().map(|descriptor| &descriptor.source),
                        attempted_revision = diagnostic.attempted.as_ref().map(|descriptor| descriptor.revision.0.as_str()).unwrap_or("none"),
                        "plugin activation failed"
                    ),
                    PluginDiagnosticSeverity::Error => tracing::error!(
                        plugin_id = plugin_id.unwrap_or("none"),
                        provider = provider.unwrap_or("none"),
                        kind = "surface_registration_failed",
                        reason = ?reason,
                        message = %diagnostic.message,
                        previous_source = ?diagnostic.previous.as_ref().map(|descriptor| &descriptor.source),
                        previous_revision = diagnostic.previous.as_ref().map(|descriptor| descriptor.revision.0.as_str()).unwrap_or("none"),
                        attempted_source = ?diagnostic.attempted.as_ref().map(|descriptor| &descriptor.source),
                        attempted_revision = diagnostic.attempted.as_ref().map(|descriptor| descriptor.revision.0.as_str()).unwrap_or("none"),
                        "plugin activation failed"
                    ),
                };
            }
            PluginDiagnosticKind::InstantiationFailed => {
                match severity {
                    PluginDiagnosticSeverity::Warning => tracing::warn!(
                        plugin_id = plugin_id.unwrap_or("none"),
                        provider = provider.unwrap_or("none"),
                        kind = "instantiation_failed",
                        message = %diagnostic.message,
                        previous_source = ?diagnostic.previous.as_ref().map(|descriptor| &descriptor.source),
                        previous_revision = diagnostic.previous.as_ref().map(|descriptor| descriptor.revision.0.as_str()).unwrap_or("none"),
                        attempted_source = ?diagnostic.attempted.as_ref().map(|descriptor| &descriptor.source),
                        attempted_revision = diagnostic.attempted.as_ref().map(|descriptor| descriptor.revision.0.as_str()).unwrap_or("none"),
                        "plugin activation failed"
                    ),
                    PluginDiagnosticSeverity::Error => tracing::error!(
                        plugin_id = plugin_id.unwrap_or("none"),
                        provider = provider.unwrap_or("none"),
                        kind = "instantiation_failed",
                        message = %diagnostic.message,
                        previous_source = ?diagnostic.previous.as_ref().map(|descriptor| &descriptor.source),
                        previous_revision = diagnostic.previous.as_ref().map(|descriptor| descriptor.revision.0.as_str()).unwrap_or("none"),
                        attempted_source = ?diagnostic.attempted.as_ref().map(|descriptor| &descriptor.source),
                        attempted_revision = diagnostic.attempted.as_ref().map(|descriptor| descriptor.revision.0.as_str()).unwrap_or("none"),
                        "plugin activation failed"
                    ),
                };
            }
            PluginDiagnosticKind::ProviderCollectFailed => {
                match severity {
                    PluginDiagnosticSeverity::Warning => tracing::warn!(
                        plugin_id = plugin_id.unwrap_or("none"),
                        provider = provider.unwrap_or("none"),
                        kind = "provider_collect_failed",
                        message = %diagnostic.message,
                        "plugin discovery failed"
                    ),
                    PluginDiagnosticSeverity::Error => tracing::error!(
                        plugin_id = plugin_id.unwrap_or("none"),
                        provider = provider.unwrap_or("none"),
                        kind = "provider_collect_failed",
                        message = %diagnostic.message,
                        "plugin discovery failed"
                    ),
                };
            }
            PluginDiagnosticKind::ProviderArtifactFailed {
                ref artifact,
                stage,
            } => {
                match severity {
                    PluginDiagnosticSeverity::Warning => tracing::warn!(
                        plugin_id = plugin_id.unwrap_or("none"),
                        provider = provider.unwrap_or("none"),
                        kind = "provider_artifact_failed",
                        artifact = %artifact,
                        stage = ?stage,
                        message = %diagnostic.message,
                        "plugin artifact preparation failed"
                    ),
                    PluginDiagnosticSeverity::Error => tracing::error!(
                        plugin_id = plugin_id.unwrap_or("none"),
                        provider = provider.unwrap_or("none"),
                        kind = "provider_artifact_failed",
                        artifact = %artifact,
                        stage = ?stage,
                        message = %diagnostic.message,
                        "plugin artifact preparation failed"
                    ),
                };
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn provider_artifact_failures_are_warnings() {
        let diagnostic = PluginDiagnostic::provider_artifact_failed(
            "test-provider",
            "broken.wasm",
            ProviderArtifactStage::Load,
            "bad artifact",
        );
        assert_eq!(diagnostic.severity(), PluginDiagnosticSeverity::Warning);
    }

    #[test]
    fn winner_activation_failures_are_errors() {
        let diagnostic =
            PluginDiagnostic::instantiation_failed(PluginId("test.plugin".to_string()), "boom");
        assert_eq!(diagnostic.severity(), PluginDiagnosticSeverity::Error);
    }

    #[test]
    fn provider_collect_failures_are_errors() {
        let diagnostic = PluginDiagnostic::provider_collect_failed("test-provider", "boom");
        assert_eq!(diagnostic.severity(), PluginDiagnosticSeverity::Error);
    }

    #[test]
    fn overlay_lines_keep_last_entries_in_order() {
        let diagnostics = vec![
            PluginDiagnostic::provider_collect_failed("provider", "first"),
            PluginDiagnostic::provider_collect_failed("provider", "second"),
            PluginDiagnostic::provider_collect_failed("provider", "third"),
            PluginDiagnostic::provider_collect_failed("provider", "fourth"),
        ];

        let lines = diagnostic_overlay_lines(&diagnostics, 3);
        assert_eq!(lines.len(), 3);
        assert_eq!(lines[0].text, "provider: fourth");
        assert_eq!(lines[1].text, "provider: third");
        assert_eq!(lines[2].text, "provider: second");
    }

    #[test]
    fn overlay_lines_collapse_duplicates_across_batch() {
        let diagnostics = vec![
            PluginDiagnostic::provider_collect_failed("provider", "boom"),
            PluginDiagnostic::provider_collect_failed("provider", "other"),
            PluginDiagnostic::provider_collect_failed("provider", "boom"),
        ];

        let lines = diagnostic_overlay_lines(&diagnostics, 3);
        assert_eq!(lines.len(), 2);
        assert_eq!(lines[0].text, "provider: boom");
        assert_eq!(lines[0].repeat_count, 2);
        assert_eq!(lines[0].display_text(), "provider: boom x2");
        assert_eq!(lines[1].text, "provider: other");
        assert_eq!(lines[1].repeat_count, 1);
    }

    #[test]
    fn overlay_lines_prioritize_errors_and_plugin_targets() {
        let diagnostics = vec![
            PluginDiagnostic::provider_artifact_failed(
                "provider",
                "warn-1.wasm",
                ProviderArtifactStage::Load,
                "warn 1",
            ),
            PluginDiagnostic::provider_artifact_failed(
                "provider",
                "warn-2.wasm",
                ProviderArtifactStage::Load,
                "warn 2",
            ),
            PluginDiagnostic::provider_collect_failed("provider", "collect"),
            PluginDiagnostic::provider_artifact_failed(
                "provider",
                "warn-3.wasm",
                ProviderArtifactStage::Load,
                "warn 3",
            ),
            PluginDiagnostic::instantiation_failed(
                PluginId("plugin.target".to_string()),
                "instantiate failed",
            ),
        ];

        let lines = diagnostic_overlay_lines(&diagnostics, 3);
        assert_eq!(lines.len(), 3);
        assert_eq!(lines[0].text, "plugin.target: instantiate failed");
        assert_eq!(lines[1].text, "provider: collect");
        assert_eq!(lines[2].text, "provider: load warn-3.wasm");
    }

    #[test]
    fn overlay_lines_prioritize_artifact_stage_severity() {
        let diagnostics = vec![
            PluginDiagnostic::provider_artifact_failed(
                "provider",
                "read.wasm",
                ProviderArtifactStage::Read,
                "read failed",
            ),
            PluginDiagnostic::provider_artifact_failed(
                "provider",
                "load.wasm",
                ProviderArtifactStage::Load,
                "load failed",
            ),
            PluginDiagnostic::provider_artifact_failed(
                "provider",
                "init.wasm",
                ProviderArtifactStage::Instantiate,
                "init failed",
            ),
        ];

        let lines = diagnostic_overlay_lines(&diagnostics, 3);
        assert_eq!(lines.len(), 3);
        assert_eq!(lines[0].text, "provider: init init.wasm");
        assert_eq!(lines[1].text, "provider: load load.wasm");
        assert_eq!(lines[2].text, "provider: read read.wasm");
    }

    #[test]
    fn provider_artifact_stage_labels_are_stable() {
        assert_eq!(
            provider_artifact_stage_label(ProviderArtifactStage::Read),
            "read"
        );
        assert_eq!(
            provider_artifact_stage_label(ProviderArtifactStage::Load),
            "load"
        );
        assert_eq!(
            provider_artifact_stage_label(ProviderArtifactStage::Instantiate),
            "instantiate"
        );
    }

    #[test]
    fn provider_artifact_overlay_summary_uses_basename_and_short_stage() {
        let diagnostic = PluginDiagnostic::provider_artifact_failed(
            "provider",
            "/tmp/cache/plugins/instantiate-trap.wasm",
            ProviderArtifactStage::Instantiate,
            "trap",
        );

        assert_eq!(
            summarize_plugin_diagnostic(&diagnostic),
            "provider: init instantiate-trap.wasm"
        );
    }

    #[test]
    fn overlay_palette_is_stable() {
        assert_eq!(PLUGIN_DIAGNOSTIC_OVERLAY_TITLE, "plugin diagnostics");
        assert_eq!(PLUGIN_ACTIVATION_OVERLAY_TITLE, "plugin activation");
        assert_eq!(PLUGIN_DISCOVERY_OVERLAY_TITLE, "plugin discovery");
        assert_eq!(
            plugin_diagnostic_overlay_border_face(PluginDiagnosticSeverity::Warning).fg,
            Color::Named(NamedColor::BrightYellow)
        );
        assert_eq!(
            plugin_diagnostic_overlay_header_face_for(
                PLUGIN_DISCOVERY_OVERLAY_TITLE,
                PluginDiagnosticSeverity::Warning,
            )
            .bg,
            Color::Rgb {
                r: 88,
                g: 68,
                b: 24,
            }
        );
        assert_eq!(
            plugin_diagnostic_overlay_body_face_for(
                PLUGIN_ACTIVATION_OVERLAY_TITLE,
                PluginDiagnosticSeverity::Error,
            )
            .bg,
            Color::Rgb {
                r: 28,
                g: 18,
                b: 18,
            }
        );
        assert_eq!(
            plugin_diagnostic_overlay_tag_face(
                PluginDiagnosticOverlayTagKind::Activation,
                PluginDiagnosticSeverity::Error,
            )
            .bg,
            Color::Named(NamedColor::BrightRed)
        );
        assert_eq!(
            plugin_diagnostic_overlay_tag_face(
                PluginDiagnosticOverlayTagKind::Discovery,
                PluginDiagnosticSeverity::Error,
            )
            .fg,
            Color::Named(NamedColor::BrightWhite)
        );
        assert_eq!(
            plugin_diagnostic_overlay_text_face(
                PluginDiagnosticOverlayTagKind::ArtifactRead,
                PluginDiagnosticSeverity::Warning,
            )
            .fg,
            Color::Rgb {
                r: 171,
                g: 212,
                b: 255,
            }
        );
        assert_eq!(
            plugin_diagnostic_overlay_text_face(
                PluginDiagnosticOverlayTagKind::Activation,
                PluginDiagnosticSeverity::Error,
            )
            .attributes,
            Attributes::BOLD
        );
    }

    #[test]
    fn overlay_layout_is_top_right_and_severity_aware() {
        let lines = vec![
            PluginDiagnosticOverlayLine {
                severity: PluginDiagnosticSeverity::Warning,
                tag_kind: PluginDiagnosticOverlayTagKind::ArtifactLoad,
                text: "provider: one".to_string(),
                repeat_count: 1,
            },
            PluginDiagnosticOverlayLine {
                severity: PluginDiagnosticSeverity::Error,
                tag_kind: PluginDiagnosticOverlayTagKind::Activation,
                text: "provider: two".to_string(),
                repeat_count: 1,
            },
        ];

        let layout = plugin_diagnostic_overlay_layout(&lines, 0, 40, 8).expect("layout");
        assert_eq!(layout.y, 0);
        assert_eq!(layout.x + layout.width, 40);
        assert_eq!(layout.height, 4);
        assert_eq!(layout.severity, PluginDiagnosticSeverity::Error);
        assert_eq!(layout.row_y(0), Some(1));
        assert_eq!(layout.row_y(1), Some(2));
        assert_eq!(layout.row_y(2), None);
    }

    #[test]
    fn overlay_state_is_generation_guarded() {
        let mut overlay = PluginDiagnosticOverlayState::default();
        let generation = overlay
            .record(&[PluginDiagnostic::provider_collect_failed(
                "provider", "boom",
            )])
            .expect("generation");
        assert!(overlay.is_active());
        assert_eq!(overlay.lines().len(), 1);
        assert_eq!(overlay.hidden_count(), 0);
        assert!(!overlay.dismiss(generation + 1));
        assert!(overlay.is_active());
        assert!(overlay.dismiss(generation));
        assert!(!overlay.is_active());
    }

    #[test]
    fn overlay_state_tracks_hidden_count() {
        let diagnostics = vec![
            PluginDiagnostic::provider_artifact_failed(
                "provider",
                "one.wasm",
                ProviderArtifactStage::Load,
                "one",
            ),
            PluginDiagnostic::provider_artifact_failed(
                "provider",
                "two.wasm",
                ProviderArtifactStage::Load,
                "two",
            ),
            PluginDiagnostic::provider_artifact_failed(
                "provider",
                "three.wasm",
                ProviderArtifactStage::Load,
                "three",
            ),
            PluginDiagnostic::provider_artifact_failed(
                "provider",
                "four.wasm",
                ProviderArtifactStage::Load,
                "four",
            ),
        ];

        let mut overlay = PluginDiagnosticOverlayState::default();
        overlay.record(&diagnostics).expect("generation");
        assert_eq!(overlay.lines().len(), 3);
        assert_eq!(overlay.hidden_count(), 1);
    }

    #[test]
    fn overlay_state_expands_for_error_batches() {
        let diagnostics = vec![
            PluginDiagnostic::provider_collect_failed("provider", "one"),
            PluginDiagnostic::provider_collect_failed("provider", "two"),
            PluginDiagnostic::provider_collect_failed("provider", "three"),
            PluginDiagnostic::provider_collect_failed("provider", "four"),
            PluginDiagnostic::provider_collect_failed("provider", "five"),
        ];

        let mut overlay = PluginDiagnosticOverlayState::default();
        overlay.record(&diagnostics).expect("generation");
        assert_eq!(overlay.lines().len(), 4);
        assert_eq!(overlay.hidden_count(), 1);
    }

    #[test]
    fn overlay_state_expands_further_for_plugin_targeted_errors() {
        let diagnostics = vec![
            PluginDiagnostic::instantiation_failed(PluginId("plugin.one".to_string()), "one"),
            PluginDiagnostic::instantiation_failed(PluginId("plugin.two".to_string()), "two"),
            PluginDiagnostic::instantiation_failed(PluginId("plugin.three".to_string()), "three"),
            PluginDiagnostic::instantiation_failed(PluginId("plugin.four".to_string()), "four"),
            PluginDiagnostic::instantiation_failed(PluginId("plugin.five".to_string()), "five"),
            PluginDiagnostic::instantiation_failed(PluginId("plugin.six".to_string()), "six"),
        ];

        let mut overlay = PluginDiagnosticOverlayState::default();
        overlay.record(&diagnostics).expect("generation");
        assert_eq!(overlay.lines().len(), 5);
        assert_eq!(overlay.hidden_count(), 1);
    }

    #[test]
    fn overlay_frame_uses_discovery_title_for_provider_diagnostics() {
        let mut overlay = PluginDiagnosticOverlayState::default();
        overlay
            .record(&[PluginDiagnostic::provider_collect_failed(
                "provider",
                "collect failed",
            )])
            .expect("generation");

        let frame = overlay.frame(40, 8).expect("frame");
        assert!(frame.header_text.contains(PLUGIN_DISCOVERY_OVERLAY_TITLE));
    }

    #[test]
    fn overlay_frame_uses_activation_title_for_plugin_diagnostics() {
        let mut overlay = PluginDiagnosticOverlayState::default();
        overlay
            .record(&[PluginDiagnostic::instantiation_failed(
                PluginId("plugin.target".to_string()),
                "hard failure",
            )])
            .expect("generation");

        let frame = overlay.frame(40, 8).expect("frame");
        assert!(frame.header_text.contains(PLUGIN_ACTIVATION_OVERLAY_TITLE));
    }

    #[test]
    fn overlay_state_uses_longer_dismiss_for_errors() {
        let mut overlay = PluginDiagnosticOverlayState::default();
        overlay
            .record(&[PluginDiagnostic::provider_artifact_failed(
                "provider",
                "warn.wasm",
                ProviderArtifactStage::Load,
                "warn",
            )])
            .expect("generation");
        assert_eq!(
            overlay.dismiss_after(),
            Some(WARNING_PLUGIN_DIAGNOSTIC_OVERLAY_DURATION)
        );

        overlay
            .record(&[PluginDiagnostic::instantiation_failed(
                PluginId("plugin.target".to_string()),
                "hard failure",
            )])
            .expect("generation");
        assert_eq!(
            overlay.dismiss_after(),
            Some(ERROR_PLUGIN_DIAGNOSTIC_OVERLAY_DURATION)
        );
    }

    #[test]
    fn overlay_layout_header_shows_total_when_lines_are_hidden() {
        let lines = vec![
            PluginDiagnosticOverlayLine {
                severity: PluginDiagnosticSeverity::Error,
                tag_kind: PluginDiagnosticOverlayTagKind::Activation,
                text: "provider: a".to_string(),
                repeat_count: 1,
            },
            PluginDiagnosticOverlayLine {
                severity: PluginDiagnosticSeverity::Warning,
                tag_kind: PluginDiagnosticOverlayTagKind::ArtifactLoad,
                text: "provider: b".to_string(),
                repeat_count: 1,
            },
            PluginDiagnosticOverlayLine {
                severity: PluginDiagnosticSeverity::Warning,
                tag_kind: PluginDiagnosticOverlayTagKind::ArtifactRead,
                text: "provider: c".to_string(),
                repeat_count: 1,
            },
        ];

        let layout = plugin_diagnostic_overlay_layout(&lines, 2, 60, 8).expect("layout");
        assert!(layout.header.contains("(3/5)"));
    }

    #[test]
    fn overlay_state_coalesces_within_window() {
        let mut overlay = PluginDiagnosticOverlayState::default();
        let start = Instant::now();
        overlay
            .record_at(
                &[PluginDiagnostic::instantiation_failed(
                    PluginId("plugin.target".to_string()),
                    "hard failure",
                )],
                start,
            )
            .expect("generation");
        overlay
            .record_at(
                &[
                    PluginDiagnostic::provider_artifact_failed(
                        "provider",
                        "warn-1.wasm",
                        ProviderArtifactStage::Load,
                        "warn 1",
                    ),
                    PluginDiagnostic::provider_artifact_failed(
                        "provider",
                        "warn-2.wasm",
                        ProviderArtifactStage::Load,
                        "warn 2",
                    ),
                    PluginDiagnostic::provider_artifact_failed(
                        "provider",
                        "warn-3.wasm",
                        ProviderArtifactStage::Load,
                        "warn 3",
                    ),
                ],
                start + Duration::from_millis(200),
            )
            .expect("generation");

        assert_eq!(overlay.lines().len(), 4);
        assert_eq!(overlay.lines()[0].text, "plugin.target: hard failure");
        assert_eq!(overlay.hidden_count(), 0);
    }

    #[test]
    fn overlay_state_resets_after_coalesce_window() {
        let mut overlay = PluginDiagnosticOverlayState::default();
        let start = Instant::now();
        overlay
            .record_at(
                &[PluginDiagnostic::instantiation_failed(
                    PluginId("plugin.target".to_string()),
                    "hard failure",
                )],
                start,
            )
            .expect("generation");
        overlay
            .record_at(
                &[PluginDiagnostic::provider_artifact_failed(
                    "provider",
                    "warn-1.wasm",
                    ProviderArtifactStage::Load,
                    "warn 1",
                )],
                start + PLUGIN_DIAGNOSTIC_OVERLAY_COALESCE_WINDOW + Duration::from_millis(1),
            )
            .expect("generation");

        assert_eq!(overlay.lines().len(), 1);
        assert_eq!(overlay.lines()[0].text, "provider: load warn-1.wasm");
        assert_eq!(overlay.hidden_count(), 0);
    }

    #[test]
    fn overlay_frame_precomputes_header_rows_and_tags() {
        let diagnostics = vec![
            PluginDiagnostic::provider_collect_failed("provider", "collect failed"),
            PluginDiagnostic::instantiation_failed(
                PluginId("plugin.target".to_string()),
                "instantiation failed",
            ),
        ];

        let mut overlay = PluginDiagnosticOverlayState::default();
        overlay.record(&diagnostics).expect("generation");
        let frame = overlay.frame(32, 8).expect("frame");

        assert!(frame.header_text.contains(PLUGIN_ACTIVATION_OVERLAY_TITLE));
        assert_eq!(frame.rows.len(), 2);
        assert_eq!(frame.rows[0].tag, "P");
        assert_eq!(frame.rows[1].tag, "D");
        assert!(frame.rows[0].text.chars().count() as u16 <= frame.layout.body_text_width());
        assert!(frame.rows[1].y > frame.rows[0].y);
    }

    #[test]
    fn overlay_paint_spec_emits_header_and_row_text_runs() {
        let diagnostics = vec![
            PluginDiagnostic::provider_collect_failed("provider", "collect failed"),
            PluginDiagnostic::provider_artifact_failed(
                "provider",
                "load.wasm",
                ProviderArtifactStage::Load,
                "load failed",
            ),
        ];

        let mut overlay = PluginDiagnosticOverlayState::default();
        overlay.record(&diagnostics).expect("generation");
        let spec = overlay.paint_spec(40, 8).expect("paint spec");

        assert_eq!(spec.text_runs[0].x, spec.layout.x + 1);
        assert_eq!(
            spec.shadow,
            Some(plugin_diagnostic_overlay_shadow_spec_for(
                PLUGIN_DISCOVERY_OVERLAY_TITLE,
                PluginDiagnosticSeverity::Error,
            ))
        );
        assert!(
            spec.text_runs[0]
                .text
                .contains(PLUGIN_DISCOVERY_OVERLAY_TITLE)
        );
        assert!(
            spec.text_runs
                .iter()
                .any(|run| run.text == "D" || run.text == "L")
        );
        assert!(
            spec.text_runs
                .iter()
                .any(|run| run.text.contains("collect"))
        );
    }

    #[test]
    fn overlay_shadow_varies_by_title_and_severity() {
        assert_eq!(
            plugin_diagnostic_overlay_shadow_spec(),
            PluginDiagnosticOverlayShadowSpec {
                offset: (6.0, 6.0),
                blur_radius: 6.0,
                color: [0.0, 0.0, 0.0, 0.30],
            }
        );
        assert_eq!(
            plugin_diagnostic_overlay_shadow_spec_for(
                PLUGIN_DISCOVERY_OVERLAY_TITLE,
                PluginDiagnosticSeverity::Warning,
            ),
            PluginDiagnosticOverlayShadowSpec {
                offset: (5.0, 5.0),
                blur_radius: 5.0,
                color: [0.05, 0.04, 0.01, 0.24],
            }
        );
        assert_eq!(
            plugin_diagnostic_overlay_shadow_spec_for(
                PLUGIN_DISCOVERY_OVERLAY_TITLE,
                PluginDiagnosticSeverity::Error,
            ),
            PluginDiagnosticOverlayShadowSpec {
                offset: (7.0, 7.0),
                blur_radius: 8.0,
                color: [0.12, 0.07, 0.01, 0.34],
            }
        );
        assert_eq!(
            plugin_diagnostic_overlay_shadow_spec_for(
                PLUGIN_ACTIVATION_OVERLAY_TITLE,
                PluginDiagnosticSeverity::Error,
            ),
            PluginDiagnosticOverlayShadowSpec {
                offset: (8.0, 8.0),
                blur_radius: 9.0,
                color: [0.16, 0.03, 0.03, 0.38],
            }
        );
    }

    #[test]
    fn paint_overlay_issues_fill_border_and_text_primitives() {
        #[derive(Default)]
        struct MockPainter {
            fills: Vec<(u16, u16, u16, u16, Face)>,
            borders: Vec<(u16, u16, u16, u16, Face)>,
            texts: Vec<(u16, u16, String, Face, u16)>,
        }

        impl PluginDiagnosticOverlayPainter for MockPainter {
            fn fill_region(&mut self, x: u16, y: u16, width: u16, height: u16, face: Face) {
                self.fills.push((x, y, width, height, face));
            }

            fn draw_border(&mut self, x: u16, y: u16, width: u16, height: u16, face: Face) {
                self.borders.push((x, y, width, height, face));
            }

            fn draw_text_run(&mut self, run: &PluginDiagnosticOverlayTextRun) {
                self.texts
                    .push((run.x, run.y, run.text.clone(), run.face, run.max_width));
            }
        }

        let diagnostics = vec![PluginDiagnostic::provider_artifact_failed(
            "provider",
            "load.wasm",
            ProviderArtifactStage::Load,
            "load failed",
        )];

        let mut overlay = PluginDiagnosticOverlayState::default();
        overlay.record(&diagnostics).expect("generation");
        let spec = overlay.paint_spec(40, 8).expect("paint spec");

        let mut painter = MockPainter::default();
        assert!(overlay.paint_with(40, 8, &mut painter));

        assert_eq!(painter.fills.len(), 2);
        assert_eq!(
            painter.fills[0],
            (
                spec.layout.x,
                spec.layout.y,
                spec.layout.width,
                spec.layout.height,
                spec.body_face,
            )
        );
        assert_eq!(
            painter.fills[1],
            (
                spec.layout.x,
                spec.layout.y,
                spec.layout.width,
                1,
                spec.header_face,
            )
        );
        assert_eq!(
            painter.borders,
            vec![(
                spec.layout.x,
                spec.layout.y,
                spec.layout.width,
                spec.layout.height,
                spec.border_face,
            )]
        );
        assert_eq!(painter.texts.len(), spec.text_runs.len());
        assert_eq!(painter.texts[0].2, spec.text_runs[0].text);
    }

    #[test]
    fn overlay_tag_texts_are_stable() {
        assert_eq!(
            plugin_diagnostic_overlay_tag_text(PluginDiagnosticOverlayTagKind::Activation),
            "P"
        );
        assert_eq!(
            plugin_diagnostic_overlay_tag_text(PluginDiagnosticOverlayTagKind::Discovery),
            "D"
        );
        assert_eq!(
            plugin_diagnostic_overlay_tag_text(PluginDiagnosticOverlayTagKind::ArtifactRead),
            "R"
        );
        assert_eq!(
            plugin_diagnostic_overlay_tag_text(PluginDiagnosticOverlayTagKind::ArtifactLoad),
            "L"
        );
        assert_eq!(
            plugin_diagnostic_overlay_tag_text(PluginDiagnosticOverlayTagKind::ArtifactInstantiate),
            "I"
        );
    }
}
