use std::time::{Duration, Instant};

use crate::protocol::WireFace;

use super::scoring::{
    OverlayBucket, collect_diagnostic_overlay_buckets, merge_overlay_buckets,
    overlay_backdrop_tone, overlay_title, select_visible_overlay_buckets,
};
use super::{
    PluginDiagnostic, PluginDiagnosticOverlayLine, PluginDiagnosticOverlayTagKind,
    PluginDiagnosticSeverity,
};

/// Suppress a new overlay generation if the previous one was shown within this window.
pub(super) const PLUGIN_DIAGNOSTIC_OVERLAY_COALESCE_WINDOW: Duration = Duration::from_millis(750);
pub(super) const WARNING_PLUGIN_DIAGNOSTIC_OVERLAY_DURATION: Duration = Duration::from_secs(4);
pub(super) const ERROR_PLUGIN_DIAGNOSTIC_OVERLAY_DURATION: Duration = Duration::from_secs(8);

#[derive(Clone, Default)]
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

    pub(super) fn record_at(
        &mut self,
        diagnostics: &[PluginDiagnostic],
        now: Instant,
    ) -> Option<u64> {
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
        let line_limit = super::scoring::overlay_line_limit(&all_lines);
        self.retained = all_lines.clone();
        self.last_recorded_at = Some(now);
        self.hidden_count = all_lines.len().saturating_sub(line_limit);
        self.lines = select_visible_overlay_buckets(&all_lines, line_limit)
            .into_iter()
            .map(|bucket| bucket.line)
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
        super::painter::plugin_diagnostic_overlay_frame_with_title(
            overlay_title(&self.retained),
            &self.lines,
            self.hidden_count,
            cols,
            rows,
        )
    }

    pub fn paint_spec(&self, cols: u16, rows: u16) -> Option<PluginDiagnosticOverlayPaintSpec> {
        super::painter::plugin_diagnostic_overlay_paint_spec_with_style(
            overlay_title(&self.retained),
            overlay_backdrop_tone(&self.retained),
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
        super::painter::paint_plugin_diagnostic_overlay(&spec, painter);
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
    pub face: WireFace,
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
    pub header_face: WireFace,
    pub body_face: WireFace,
    pub border_face: WireFace,
    pub shadow: Option<PluginDiagnosticOverlayShadowSpec>,
    pub text_runs: Vec<PluginDiagnosticOverlayTextRun>,
}

pub trait PluginDiagnosticOverlayPainter {
    fn fill_region(&mut self, x: u16, y: u16, width: u16, height: u16, face: WireFace);
    fn draw_border(&mut self, x: u16, y: u16, width: u16, height: u16, face: WireFace);
    fn draw_text_run(&mut self, run: &PluginDiagnosticOverlayTextRun);
}
