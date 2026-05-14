use unicode_segmentation::UnicodeSegmentation;
use unicode_width::UnicodeWidthStr;

use crate::protocol::{Brush, FontWeight, NamedColor, Style};

use super::scoring::{OverlayBackdropTone, overlay_backdrop_tone_for_title};
use super::types::{
    PluginDiagnosticOverlayFrame, PluginDiagnosticOverlayLayout, PluginDiagnosticOverlayPaintSpec,
    PluginDiagnosticOverlayPainter, PluginDiagnosticOverlayRow, PluginDiagnosticOverlayShadowSpec,
    PluginDiagnosticOverlayTextRun,
};
use super::{
    MIN_PLUGIN_DIAGNOSTIC_OVERLAY_COLS, MIN_PLUGIN_DIAGNOSTIC_OVERLAY_ROWS,
    PLUGIN_DIAGNOSTIC_OVERLAY_TITLE, PluginDiagnosticOverlayLine, PluginDiagnosticOverlayTagKind,
    PluginDiagnosticSeverity,
};

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
        .map(|line| UnicodeWidthStr::width(line.display_text().as_ref()) as u16)
        .max()
        .unwrap_or(0);
    let inner_width = (body_width + 2) // +2 for tag (1) + space (1) between tag and text
        .max(UnicodeWidthStr::width(header.as_str()) as u16)
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

pub(super) fn plugin_diagnostic_overlay_frame_with_title(
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
    plugin_diagnostic_overlay_paint_spec_with_style(
        PLUGIN_DIAGNOSTIC_OVERLAY_TITLE,
        overlay_backdrop_tone_for_title(PLUGIN_DIAGNOSTIC_OVERLAY_TITLE),
        lines,
        hidden_count,
        cols,
        rows,
    )
}

pub(super) fn plugin_diagnostic_overlay_paint_spec_with_style(
    title: &str,
    tone: OverlayBackdropTone,
    lines: &[PluginDiagnosticOverlayLine],
    hidden_count: usize,
    cols: u16,
    rows: u16,
) -> Option<PluginDiagnosticOverlayPaintSpec> {
    let frame = plugin_diagnostic_overlay_frame_with_title(title, lines, hidden_count, cols, rows)?;
    let layout = frame.layout.clone();
    let severity = layout.severity;
    let header_style = plugin_diagnostic_overlay_header_style_with_tone(title, tone, severity);
    let body_style = plugin_diagnostic_overlay_body_style_with_tone(title, tone, severity);
    let border_style = plugin_diagnostic_overlay_border_style(severity);

    let mut text_runs = vec![PluginDiagnosticOverlayTextRun {
        x: layout.x + 1,
        y: layout.y,
        text: frame.header_text,
        style: header_style.clone(),
        max_width: layout.header_text_width(),
    }];

    text_runs.extend(frame.rows.into_iter().flat_map(|row| {
        let tag_style = plugin_diagnostic_overlay_tag_style(row.tag_kind, row.severity);
        let text_style = plugin_diagnostic_overlay_text_style(row.tag_kind, row.severity);
        [
            PluginDiagnosticOverlayTextRun {
                x: layout.x + 1,
                y: row.y,
                text: row.tag.to_string(),
                style: tag_style,
                max_width: 1,
            },
            PluginDiagnosticOverlayTextRun {
                x: layout.x + 3,
                y: row.y,
                text: row.text,
                style: text_style,
                max_width: layout.body_text_width(),
            },
        ]
    }));

    Some(PluginDiagnosticOverlayPaintSpec {
        layout,
        header_style,
        body_style,
        border_style,
        shadow: Some(plugin_diagnostic_overlay_shadow_spec_with_tone(
            title, tone, severity,
        )),
        text_runs,
    })
}

pub fn plugin_diagnostic_overlay_shadow_spec() -> PluginDiagnosticOverlayShadowSpec {
    plugin_diagnostic_overlay_shadow_spec_with_tone(
        PLUGIN_DIAGNOSTIC_OVERLAY_TITLE,
        overlay_backdrop_tone_for_title(PLUGIN_DIAGNOSTIC_OVERLAY_TITLE),
        PluginDiagnosticSeverity::Warning,
    )
}

pub fn plugin_diagnostic_overlay_shadow_spec_for(
    title: &str,
    severity: PluginDiagnosticSeverity,
) -> PluginDiagnosticOverlayShadowSpec {
    plugin_diagnostic_overlay_shadow_spec_with_tone(
        title,
        overlay_backdrop_tone_for_title(title),
        severity,
    )
}

pub(super) fn plugin_diagnostic_overlay_shadow_spec_with_tone(
    _title: &str,
    tone: OverlayBackdropTone,
    severity: PluginDiagnosticSeverity,
) -> PluginDiagnosticOverlayShadowSpec {
    match (tone, severity) {
        (OverlayBackdropTone::Activation, PluginDiagnosticSeverity::Error) => {
            PluginDiagnosticOverlayShadowSpec {
                offset: (8.0, 8.0),
                blur_radius: 9.0,
                color: [0.16, 0.03, 0.03, 0.38],
            }
        }
        (OverlayBackdropTone::Activation, PluginDiagnosticSeverity::Warning) => {
            PluginDiagnosticOverlayShadowSpec {
                offset: (6.0, 6.0),
                blur_radius: 6.5,
                color: [0.12, 0.03, 0.03, 0.30],
            }
        }
        (OverlayBackdropTone::Discovery, PluginDiagnosticSeverity::Error) => {
            PluginDiagnosticOverlayShadowSpec {
                offset: (7.0, 7.0),
                blur_radius: 8.0,
                color: [0.12, 0.07, 0.01, 0.34],
            }
        }
        (OverlayBackdropTone::Discovery, PluginDiagnosticSeverity::Warning) => {
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
        &spec.body_style,
    );
    painter.fill_region(
        spec.layout.x,
        spec.layout.y,
        spec.layout.width,
        1,
        &spec.header_style,
    );
    painter.draw_border(
        spec.layout.x,
        spec.layout.y,
        spec.layout.width,
        spec.layout.height,
        &spec.border_style,
    );
    for run in &spec.text_runs {
        painter.draw_text_run(run);
    }
}

pub fn plugin_diagnostic_overlay_border_style(severity: PluginDiagnosticSeverity) -> Style {
    Style {
        fg: match severity {
            PluginDiagnosticSeverity::Info => Brush::Named(NamedColor::BrightCyan),
            PluginDiagnosticSeverity::Warning => Brush::Named(NamedColor::BrightYellow),
            PluginDiagnosticSeverity::Error => Brush::Named(NamedColor::BrightRed),
        },
        bg: Brush::rgb(18, 18, 18),
        font_weight: FontWeight::BOLD,
        ..Style::default()
    }
}

pub fn plugin_diagnostic_overlay_header_style(severity: PluginDiagnosticSeverity) -> Style {
    plugin_diagnostic_overlay_header_style_for(PLUGIN_DIAGNOSTIC_OVERLAY_TITLE, severity)
}

pub fn plugin_diagnostic_overlay_header_style_for(
    title: &str,
    severity: PluginDiagnosticSeverity,
) -> Style {
    plugin_diagnostic_overlay_header_style_with_tone(
        title,
        overlay_backdrop_tone_for_title(title),
        severity,
    )
}

pub(super) fn plugin_diagnostic_overlay_header_style_with_tone(
    _title: &str,
    tone: OverlayBackdropTone,
    severity: PluginDiagnosticSeverity,
) -> Style {
    Style {
        fg: Brush::Named(NamedColor::BrightWhite),
        bg: match (tone, severity) {
            (OverlayBackdropTone::Activation, PluginDiagnosticSeverity::Error) => {
                Brush::rgb(128, 20, 20)
            }
            (OverlayBackdropTone::Activation, PluginDiagnosticSeverity::Warning) => {
                Brush::rgb(104, 72, 24)
            }
            (OverlayBackdropTone::Discovery, PluginDiagnosticSeverity::Error) => {
                Brush::rgb(112, 60, 16)
            }
            (OverlayBackdropTone::Discovery, PluginDiagnosticSeverity::Warning) => {
                Brush::rgb(88, 68, 24)
            }
            (_, PluginDiagnosticSeverity::Warning) => Brush::rgb(96, 72, 12),
            (_, PluginDiagnosticSeverity::Error) => Brush::rgb(112, 24, 24),
            (_, PluginDiagnosticSeverity::Info) => Brush::rgb(20, 60, 96),
        },
        font_weight: FontWeight::BOLD,
        ..Style::default()
    }
}

pub fn plugin_diagnostic_overlay_body_style() -> Style {
    plugin_diagnostic_overlay_body_style_for(
        PLUGIN_DIAGNOSTIC_OVERLAY_TITLE,
        PluginDiagnosticSeverity::Warning,
    )
}

pub fn plugin_diagnostic_overlay_body_style_for(
    title: &str,
    severity: PluginDiagnosticSeverity,
) -> Style {
    plugin_diagnostic_overlay_body_style_with_tone(
        title,
        overlay_backdrop_tone_for_title(title),
        severity,
    )
}

pub(super) fn plugin_diagnostic_overlay_body_style_with_tone(
    _title: &str,
    tone: OverlayBackdropTone,
    severity: PluginDiagnosticSeverity,
) -> Style {
    Style {
        fg: Brush::Named(NamedColor::BrightWhite),
        bg: match (tone, severity) {
            (OverlayBackdropTone::Activation, PluginDiagnosticSeverity::Error) => {
                Brush::rgb(36, 18, 18)
            }
            (OverlayBackdropTone::Activation, PluginDiagnosticSeverity::Warning) => {
                Brush::rgb(32, 26, 18)
            }
            (OverlayBackdropTone::Discovery, PluginDiagnosticSeverity::Error) => {
                Brush::rgb(34, 24, 17)
            }
            (OverlayBackdropTone::Discovery, PluginDiagnosticSeverity::Warning) => {
                Brush::rgb(30, 28, 20)
            }
            _ => Brush::rgb(24, 24, 24),
        },
        ..Style::default()
    }
}

pub fn plugin_diagnostic_overlay_text_style(
    kind: PluginDiagnosticOverlayTagKind,
    severity: PluginDiagnosticSeverity,
) -> Style {
    Style {
        fg: match (kind, severity) {
            (PluginDiagnosticOverlayTagKind::Activation, PluginDiagnosticSeverity::Error) => {
                Brush::Named(NamedColor::BrightWhite)
            }
            (PluginDiagnosticOverlayTagKind::Activation, PluginDiagnosticSeverity::Warning) => {
                Brush::Named(NamedColor::BrightYellow)
            }
            (PluginDiagnosticOverlayTagKind::Activation, PluginDiagnosticSeverity::Info) => {
                Brush::Named(NamedColor::BrightCyan)
            }
            (PluginDiagnosticOverlayTagKind::Discovery, _) => Brush::rgb(245, 214, 168),
            (PluginDiagnosticOverlayTagKind::ArtifactManifest, _) => Brush::rgb(160, 180, 200),
            (PluginDiagnosticOverlayTagKind::ArtifactRead, _) => Brush::rgb(171, 212, 255),
            (PluginDiagnosticOverlayTagKind::ArtifactLoad, _) => Brush::rgb(200, 160, 60),
            (PluginDiagnosticOverlayTagKind::ArtifactInstantiate, _) => Brush::rgb(255, 194, 114),
            (PluginDiagnosticOverlayTagKind::Runtime, _) => Brush::Named(NamedColor::BrightRed),
            (PluginDiagnosticOverlayTagKind::Config, _) => Brush::rgb(140, 200, 220),
            (PluginDiagnosticOverlayTagKind::PluginEmitted, _) => Brush::rgb(140, 200, 220),
        },
        bg: Brush::rgb(24, 24, 24),
        font_weight: match severity {
            PluginDiagnosticSeverity::Error => FontWeight::BOLD,
            PluginDiagnosticSeverity::Warning => FontWeight::NORMAL,
            PluginDiagnosticSeverity::Info => FontWeight::NORMAL,
        },
        ..Style::default()
    }
}

pub fn plugin_diagnostic_overlay_tag_style(
    kind: PluginDiagnosticOverlayTagKind,
    severity: PluginDiagnosticSeverity,
) -> Style {
    Style {
        fg: match kind {
            PluginDiagnosticOverlayTagKind::Discovery => Brush::Named(NamedColor::BrightWhite),
            _ => Brush::Named(NamedColor::Black),
        },
        bg: match (kind, severity) {
            (PluginDiagnosticOverlayTagKind::Activation, PluginDiagnosticSeverity::Error) => {
                Brush::Named(NamedColor::BrightRed)
            }
            (PluginDiagnosticOverlayTagKind::Activation, PluginDiagnosticSeverity::Warning) => {
                Brush::Named(NamedColor::BrightYellow)
            }
            (PluginDiagnosticOverlayTagKind::Activation, PluginDiagnosticSeverity::Info) => {
                Brush::Named(NamedColor::BrightCyan)
            }
            (PluginDiagnosticOverlayTagKind::Discovery, _) => Brush::rgb(124, 54, 18),
            (PluginDiagnosticOverlayTagKind::ArtifactManifest, _) => Brush::rgb(90, 110, 130),
            (PluginDiagnosticOverlayTagKind::ArtifactRead, _) => Brush::rgb(78, 106, 158),
            (PluginDiagnosticOverlayTagKind::ArtifactLoad, _) => Brush::rgb(180, 140, 40),
            (PluginDiagnosticOverlayTagKind::ArtifactInstantiate, _) => Brush::rgb(214, 126, 34),
            (PluginDiagnosticOverlayTagKind::Runtime, _) => Brush::Named(NamedColor::BrightRed),
            (PluginDiagnosticOverlayTagKind::Config, _) => Brush::rgb(60, 140, 160),
            (PluginDiagnosticOverlayTagKind::PluginEmitted, _) => Brush::rgb(60, 140, 160),
        },
        font_weight: FontWeight::BOLD,
        ..Style::default()
    }
}

pub fn plugin_diagnostic_overlay_tag_text(kind: PluginDiagnosticOverlayTagKind) -> &'static str {
    match kind {
        PluginDiagnosticOverlayTagKind::Activation => "P",
        PluginDiagnosticOverlayTagKind::Discovery => "D",
        PluginDiagnosticOverlayTagKind::ArtifactManifest => "M",
        PluginDiagnosticOverlayTagKind::ArtifactRead => "R",
        PluginDiagnosticOverlayTagKind::ArtifactLoad => "L",
        PluginDiagnosticOverlayTagKind::ArtifactInstantiate => "I",
        PluginDiagnosticOverlayTagKind::Runtime => "R!",
        PluginDiagnosticOverlayTagKind::PluginEmitted => "E",
        PluginDiagnosticOverlayTagKind::Config => "C",
    }
}

fn truncate_to_width(text: &str, width: u16) -> String {
    let max = width as usize;
    let mut used = 0usize;
    let mut result = String::new();
    for grapheme in text.graphemes(true) {
        let w = UnicodeWidthStr::width(grapheme);
        if used + w > max {
            break;
        }
        result.push_str(grapheme);
        used += w;
    }
    result
}
