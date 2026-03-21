use crate::protocol::{Attributes, Color, Face, NamedColor};

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
    let header_face = plugin_diagnostic_overlay_header_face_with_tone(title, tone, severity);
    let body_face = plugin_diagnostic_overlay_body_face_with_tone(title, tone, severity);
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
    plugin_diagnostic_overlay_header_face_with_tone(
        title,
        overlay_backdrop_tone_for_title(title),
        severity,
    )
}

pub(super) fn plugin_diagnostic_overlay_header_face_with_tone(
    _title: &str,
    tone: OverlayBackdropTone,
    severity: PluginDiagnosticSeverity,
) -> Face {
    Face {
        fg: Color::Named(NamedColor::BrightWhite),
        bg: match (tone, severity) {
            (OverlayBackdropTone::Activation, PluginDiagnosticSeverity::Error) => Color::Rgb {
                r: 128,
                g: 20,
                b: 20,
            },
            (OverlayBackdropTone::Activation, PluginDiagnosticSeverity::Warning) => Color::Rgb {
                r: 104,
                g: 24,
                b: 24,
            },
            (OverlayBackdropTone::Discovery, PluginDiagnosticSeverity::Error) => Color::Rgb {
                r: 112,
                g: 60,
                b: 16,
            },
            (OverlayBackdropTone::Discovery, PluginDiagnosticSeverity::Warning) => Color::Rgb {
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
    plugin_diagnostic_overlay_body_face_with_tone(
        title,
        overlay_backdrop_tone_for_title(title),
        severity,
    )
}

pub(super) fn plugin_diagnostic_overlay_body_face_with_tone(
    _title: &str,
    tone: OverlayBackdropTone,
    severity: PluginDiagnosticSeverity,
) -> Face {
    Face {
        fg: Color::Named(NamedColor::BrightWhite),
        bg: match (tone, severity) {
            (OverlayBackdropTone::Activation, PluginDiagnosticSeverity::Error) => Color::Rgb {
                r: 28,
                g: 18,
                b: 18,
            },
            (OverlayBackdropTone::Activation, PluginDiagnosticSeverity::Warning) => Color::Rgb {
                r: 26,
                g: 20,
                b: 20,
            },
            (OverlayBackdropTone::Discovery, PluginDiagnosticSeverity::Error) => Color::Rgb {
                r: 28,
                g: 23,
                b: 17,
            },
            (OverlayBackdropTone::Discovery, PluginDiagnosticSeverity::Warning) => Color::Rgb {
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
