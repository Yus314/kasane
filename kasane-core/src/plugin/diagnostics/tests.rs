use std::time::{Duration, Instant};

use crate::protocol::{Attributes, Color, Face, NamedColor};
use crate::surface::SurfaceRegistrationError;

use super::painter::{
    plugin_diagnostic_overlay_body_face_for, plugin_diagnostic_overlay_body_face_with_tone,
    plugin_diagnostic_overlay_border_face, plugin_diagnostic_overlay_header_face_for,
    plugin_diagnostic_overlay_header_face_with_tone, plugin_diagnostic_overlay_layout,
    plugin_diagnostic_overlay_shadow_spec, plugin_diagnostic_overlay_shadow_spec_for,
    plugin_diagnostic_overlay_shadow_spec_with_tone, plugin_diagnostic_overlay_tag_face,
    plugin_diagnostic_overlay_tag_text, plugin_diagnostic_overlay_text_face,
};
use super::scoring::{OverlayBackdropTone, diagnostic_overlay_lines};
use super::types::{
    ERROR_PLUGIN_DIAGNOSTIC_OVERLAY_DURATION, PLUGIN_DIAGNOSTIC_OVERLAY_COALESCE_WINDOW,
    PluginDiagnosticOverlayPainter, PluginDiagnosticOverlayShadowSpec,
    PluginDiagnosticOverlayState, PluginDiagnosticOverlayTextRun,
    WARNING_PLUGIN_DIAGNOSTIC_OVERLAY_DURATION,
};
use super::{
    PLUGIN_ACTIVATION_OVERLAY_TITLE, PLUGIN_DIAGNOSTIC_OVERLAY_TITLE,
    PLUGIN_DISCOVERY_OVERLAY_TITLE, PluginDiagnostic, PluginDiagnosticOverlayLine,
    PluginDiagnosticOverlayTagKind, PluginDiagnosticSeverity, ProviderArtifactStage,
    provider_artifact_stage_label, summarize_plugin_diagnostic,
};
use crate::plugin::PluginId;

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
fn overlay_state_keeps_provider_artifact_context_in_provider_only_batches() {
    let diagnostics = vec![
        PluginDiagnostic::provider_collect_failed("provider", "one"),
        PluginDiagnostic::provider_collect_failed("provider", "two"),
        PluginDiagnostic::provider_collect_failed("provider", "three"),
        PluginDiagnostic::provider_collect_failed("provider", "four"),
        PluginDiagnostic::provider_artifact_failed(
            "provider",
            "warn.wasm",
            ProviderArtifactStage::Load,
            "warn",
        ),
    ];

    let mut overlay = PluginDiagnosticOverlayState::default();
    overlay.record(&diagnostics).expect("generation");

    assert_eq!(overlay.lines().len(), 4);
    assert!(overlay.lines().iter().any(|line| {
        line.tag_kind == PluginDiagnosticOverlayTagKind::ArtifactLoad
            && line.severity == PluginDiagnosticSeverity::Warning
    }));
    assert_eq!(overlay.hidden_count(), 1);
}

#[test]
fn overlay_state_shows_distinct_provider_artifact_stages_in_warning_batches() {
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
        PluginDiagnostic::provider_artifact_failed(
            "provider",
            "extra-load.wasm",
            ProviderArtifactStage::Load,
            "load failed again",
        ),
    ];

    let mut overlay = PluginDiagnosticOverlayState::default();
    overlay.record(&diagnostics).expect("generation");

    assert_eq!(overlay.lines().len(), 3);
    assert_eq!(overlay.hidden_count(), 1);
    assert!(
        overlay
            .lines()
            .iter()
            .any(|line| { line.tag_kind == PluginDiagnosticOverlayTagKind::ArtifactInstantiate })
    );
    assert!(
        overlay
            .lines()
            .iter()
            .any(|line| { line.tag_kind == PluginDiagnosticOverlayTagKind::ArtifactLoad })
    );
    assert!(
        overlay
            .lines()
            .iter()
            .any(|line| { line.tag_kind == PluginDiagnosticOverlayTagKind::ArtifactRead })
    );
}

#[test]
fn overlay_state_shows_discovery_and_multiple_artifact_stages_in_provider_error_batches() {
    let diagnostics = vec![
        PluginDiagnostic::provider_collect_failed("provider", "collect failed"),
        PluginDiagnostic::provider_collect_failed("provider", "collect failed again"),
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

    let mut overlay = PluginDiagnosticOverlayState::default();
    overlay.record(&diagnostics).expect("generation");

    assert_eq!(overlay.lines().len(), 4);
    assert_eq!(overlay.hidden_count(), 1);
    assert!(
        overlay
            .lines()
            .iter()
            .any(|line| { line.tag_kind == PluginDiagnosticOverlayTagKind::Discovery })
    );
    assert!(
        overlay
            .lines()
            .iter()
            .any(|line| { line.tag_kind == PluginDiagnosticOverlayTagKind::ArtifactInstantiate })
    );
    assert!(
        overlay
            .lines()
            .iter()
            .any(|line| { line.tag_kind == PluginDiagnosticOverlayTagKind::ArtifactLoad })
    );
    assert!(
        overlay
            .lines()
            .iter()
            .any(|line| { line.tag_kind == PluginDiagnosticOverlayTagKind::ArtifactRead })
    );
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
fn overlay_state_reserves_provider_line_in_mixed_batches() {
    let diagnostics = vec![
        PluginDiagnostic::instantiation_failed(PluginId("plugin.one".to_string()), "one"),
        PluginDiagnostic::instantiation_failed(PluginId("plugin.two".to_string()), "two"),
        PluginDiagnostic::instantiation_failed(PluginId("plugin.three".to_string()), "three"),
        PluginDiagnostic::instantiation_failed(PluginId("plugin.four".to_string()), "four"),
        PluginDiagnostic::instantiation_failed(PluginId("plugin.five".to_string()), "five"),
        PluginDiagnostic::provider_collect_failed("provider", "collect failed"),
    ];

    let mut overlay = PluginDiagnosticOverlayState::default();
    overlay.record(&diagnostics).expect("generation");

    assert_eq!(overlay.lines().len(), 5);
    assert!(
        overlay
            .lines()
            .iter()
            .any(|line| matches!(line.tag_kind, PluginDiagnosticOverlayTagKind::Discovery))
    );
    assert!(
        overlay
            .lines()
            .iter()
            .any(|line| matches!(line.tag_kind, PluginDiagnosticOverlayTagKind::Activation))
    );
    assert_eq!(overlay.hidden_count(), 1);
}

#[test]
fn overlay_state_keeps_provider_warning_context_alongside_errors() {
    let diagnostics = vec![
        PluginDiagnostic::instantiation_failed(PluginId("plugin.one".to_string()), "one"),
        PluginDiagnostic::instantiation_failed(PluginId("plugin.two".to_string()), "two"),
        PluginDiagnostic::instantiation_failed(PluginId("plugin.three".to_string()), "three"),
        PluginDiagnostic::provider_collect_failed("provider", "collect failed"),
        PluginDiagnostic::provider_artifact_failed(
            "provider",
            "warn.wasm",
            ProviderArtifactStage::Load,
            "warn",
        ),
    ];

    let mut overlay = PluginDiagnosticOverlayState::default();
    overlay.record(&diagnostics).expect("generation");

    assert!(overlay.lines().iter().any(|line| line.tag_kind
        == PluginDiagnosticOverlayTagKind::Discovery
        && line.severity == PluginDiagnosticSeverity::Error));
    assert!(overlay.lines().iter().any(|line| line.tag_kind
        == PluginDiagnosticOverlayTagKind::ArtifactLoad
        && line.severity == PluginDiagnosticSeverity::Warning));
}

#[test]
fn overlay_state_prefers_plugin_lines_when_plugin_score_dominates_mixed_error_batches() {
    let diagnostics = vec![
        PluginDiagnostic::instantiation_failed(PluginId("plugin.one".to_string()), "one"),
        PluginDiagnostic::instantiation_failed(PluginId("plugin.two".to_string()), "two"),
        PluginDiagnostic::instantiation_failed(PluginId("plugin.three".to_string()), "three"),
        PluginDiagnostic::provider_collect_failed("provider", "collect failed"),
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

    let mut overlay = PluginDiagnosticOverlayState::default();
    overlay.record(&diagnostics).expect("generation");

    assert_eq!(overlay.lines().len(), 5);
    assert_eq!(overlay.hidden_count(), 2);
    assert_eq!(
        overlay
            .lines()
            .iter()
            .filter(|line| {
                line.tag_kind == PluginDiagnosticOverlayTagKind::Activation
                    && line.severity == PluginDiagnosticSeverity::Error
            })
            .count(),
        3
    );
    assert!(overlay.lines().iter().any(|line| {
        line.tag_kind == PluginDiagnosticOverlayTagKind::Discovery
            && line.severity == PluginDiagnosticSeverity::Error
    }));
    assert!(overlay.lines().iter().any(|line| {
        line.tag_kind == PluginDiagnosticOverlayTagKind::ArtifactInstantiate
            && line.severity == PluginDiagnosticSeverity::Warning
    }));
    assert_eq!(
        overlay
            .lines()
            .iter()
            .filter(|line| {
                matches!(
                    line.tag_kind,
                    PluginDiagnosticOverlayTagKind::ArtifactInstantiate
                        | PluginDiagnosticOverlayTagKind::ArtifactLoad
                        | PluginDiagnosticOverlayTagKind::ArtifactRead
                ) && line.severity == PluginDiagnosticSeverity::Warning
            })
            .count(),
        1
    );
}

#[test]
fn overlay_state_keeps_two_provider_artifact_stages_when_provider_score_dominates() {
    let diagnostics = vec![
        PluginDiagnostic::instantiation_failed(PluginId("plugin.one".to_string()), "one"),
        PluginDiagnostic::instantiation_failed(PluginId("plugin.two".to_string()), "two"),
        PluginDiagnostic::provider_collect_failed("provider", "collect failed"),
        PluginDiagnostic::provider_collect_failed("provider", "collect failed"),
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

    let mut overlay = PluginDiagnosticOverlayState::default();
    overlay.record(&diagnostics).expect("generation");

    assert_eq!(overlay.lines().len(), 5);
    assert_eq!(overlay.hidden_count(), 1);
    assert_eq!(
        overlay
            .lines()
            .iter()
            .filter(|line| {
                matches!(
                    line.tag_kind,
                    PluginDiagnosticOverlayTagKind::ArtifactInstantiate
                        | PluginDiagnosticOverlayTagKind::ArtifactLoad
                        | PluginDiagnosticOverlayTagKind::ArtifactRead
                ) && line.severity == PluginDiagnosticSeverity::Warning
            })
            .count(),
        2
    );
}

#[test]
fn overlay_state_shows_three_provider_artifact_stages_when_provider_strongly_dominates() {
    let diagnostics = vec![
        PluginDiagnostic::instantiation_failed(PluginId("plugin.one".to_string()), "one"),
        PluginDiagnostic::provider_collect_failed("provider", "collect failed"),
        PluginDiagnostic::provider_collect_failed("provider", "collect failed"),
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

    let mut overlay = PluginDiagnosticOverlayState::default();
    overlay.record(&diagnostics).expect("generation");

    assert_eq!(overlay.lines().len(), 5);
    assert_eq!(overlay.hidden_count(), 0);
    assert_eq!(
        overlay
            .lines()
            .iter()
            .filter(|line| {
                matches!(
                    line.tag_kind,
                    PluginDiagnosticOverlayTagKind::ArtifactInstantiate
                        | PluginDiagnosticOverlayTagKind::ArtifactLoad
                        | PluginDiagnosticOverlayTagKind::ArtifactRead
                ) && line.severity == PluginDiagnosticSeverity::Warning
            })
            .count(),
        3
    );
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
fn overlay_frame_precomputes_rows_and_tags_for_mixed_batches() {
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

    assert!(frame.header_text.contains(PLUGIN_DIAGNOSTIC_OVERLAY_TITLE));
    assert_eq!(frame.rows.len(), 2);
    assert_eq!(frame.rows[0].tag, "P");
    assert_eq!(frame.rows[1].tag, "D");
    assert!(frame.rows[0].text.chars().count() as u16 <= frame.layout.body_text_width());
    assert!(frame.rows[1].y > frame.rows[0].y);
}

#[test]
fn overlay_frame_uses_neutral_title_for_mixed_batches() {
    let diagnostics = vec![
        PluginDiagnostic::provider_collect_failed("provider", "collect failed"),
        PluginDiagnostic::instantiation_failed(
            PluginId("plugin.target".to_string()),
            "instantiation failed",
        ),
    ];

    let mut overlay = PluginDiagnosticOverlayState::default();
    overlay.record(&diagnostics).expect("generation");
    let frame = overlay.frame(40, 8).expect("frame");
    let spec = overlay.paint_spec(40, 8).expect("paint spec");

    assert!(frame.header_text.contains(PLUGIN_DIAGNOSTIC_OVERLAY_TITLE));
    assert_eq!(
        spec.shadow,
        Some(plugin_diagnostic_overlay_shadow_spec_for(
            PLUGIN_DIAGNOSTIC_OVERLAY_TITLE,
            PluginDiagnosticSeverity::Error,
        ))
    );
}

#[test]
fn overlay_paint_spec_uses_activation_tone_for_plugin_error_with_provider_warning() {
    let diagnostics = vec![
        PluginDiagnostic::provider_artifact_failed(
            "provider",
            "warn.wasm",
            ProviderArtifactStage::Load,
            "load failed",
        ),
        PluginDiagnostic::instantiation_failed(
            PluginId("plugin.target".to_string()),
            "instantiation failed",
        ),
    ];

    let mut overlay = PluginDiagnosticOverlayState::default();
    overlay.record(&diagnostics).expect("generation");
    let frame = overlay.frame(40, 8).expect("frame");
    let spec = overlay.paint_spec(40, 8).expect("paint spec");

    assert!(frame.header_text.contains(PLUGIN_DIAGNOSTIC_OVERLAY_TITLE));
    assert_eq!(
        spec.shadow,
        Some(plugin_diagnostic_overlay_shadow_spec_with_tone(
            PLUGIN_DIAGNOSTIC_OVERLAY_TITLE,
            OverlayBackdropTone::Activation,
            PluginDiagnosticSeverity::Error,
        ))
    );
    assert_eq!(
        spec.header_face,
        plugin_diagnostic_overlay_header_face_with_tone(
            PLUGIN_DIAGNOSTIC_OVERLAY_TITLE,
            OverlayBackdropTone::Activation,
            PluginDiagnosticSeverity::Error,
        )
    );
    assert_eq!(
        spec.body_face,
        plugin_diagnostic_overlay_body_face_with_tone(
            PLUGIN_DIAGNOSTIC_OVERLAY_TITLE,
            OverlayBackdropTone::Activation,
            PluginDiagnosticSeverity::Error,
        )
    );
}

#[test]
fn overlay_paint_spec_uses_neutral_tone_for_plugin_error_with_provider_init_warning() {
    let diagnostics = vec![
        PluginDiagnostic::provider_artifact_failed(
            "provider",
            "init.wasm",
            ProviderArtifactStage::Instantiate,
            "init failed",
        ),
        PluginDiagnostic::instantiation_failed(
            PluginId("plugin.target".to_string()),
            "instantiation failed",
        ),
    ];

    let mut overlay = PluginDiagnosticOverlayState::default();
    overlay.record(&diagnostics).expect("generation");
    let frame = overlay.frame(40, 8).expect("frame");
    let spec = overlay.paint_spec(40, 8).expect("paint spec");

    assert!(frame.header_text.contains(PLUGIN_DIAGNOSTIC_OVERLAY_TITLE));
    assert_eq!(
        spec.shadow,
        Some(plugin_diagnostic_overlay_shadow_spec_with_tone(
            PLUGIN_DIAGNOSTIC_OVERLAY_TITLE,
            OverlayBackdropTone::Neutral,
            PluginDiagnosticSeverity::Error,
        ))
    );
    assert_eq!(
        spec.header_face,
        plugin_diagnostic_overlay_header_face_with_tone(
            PLUGIN_DIAGNOSTIC_OVERLAY_TITLE,
            OverlayBackdropTone::Neutral,
            PluginDiagnosticSeverity::Error,
        )
    );
}

#[test]
fn overlay_paint_spec_keeps_neutral_tone_for_balanced_mixed_errors() {
    let diagnostics = vec![
        PluginDiagnostic::provider_collect_failed("provider", "collect failed"),
        PluginDiagnostic::surface_registration_failed(
            PluginId("plugin.target".to_string()),
            SurfaceRegistrationError::DuplicateSurfaceId {
                surface_id: crate::surface::SurfaceId(12),
                existing_surface_key: "existing".into(),
                new_surface_key: "new".into(),
            },
        ),
    ];

    let mut overlay = PluginDiagnosticOverlayState::default();
    overlay.record(&diagnostics).expect("generation");
    let frame = overlay.frame(40, 8).expect("frame");
    let spec = overlay.paint_spec(40, 8).expect("paint spec");

    assert!(frame.header_text.contains(PLUGIN_DIAGNOSTIC_OVERLAY_TITLE));
    assert_eq!(
        spec.shadow,
        Some(plugin_diagnostic_overlay_shadow_spec_with_tone(
            PLUGIN_DIAGNOSTIC_OVERLAY_TITLE,
            OverlayBackdropTone::Neutral,
            PluginDiagnosticSeverity::Error,
        ))
    );
    assert_eq!(
        spec.header_face,
        plugin_diagnostic_overlay_header_face_with_tone(
            PLUGIN_DIAGNOSTIC_OVERLAY_TITLE,
            OverlayBackdropTone::Neutral,
            PluginDiagnosticSeverity::Error,
        )
    );
    assert_eq!(
        spec.body_face,
        plugin_diagnostic_overlay_body_face_with_tone(
            PLUGIN_DIAGNOSTIC_OVERLAY_TITLE,
            OverlayBackdropTone::Neutral,
            PluginDiagnosticSeverity::Error,
        )
    );
}

#[test]
fn overlay_paint_spec_uses_activation_tone_when_plugin_score_dominates_mixed_batch() {
    let diagnostics = vec![
        PluginDiagnostic::instantiation_failed(PluginId("plugin.one".to_string()), "one"),
        PluginDiagnostic::instantiation_failed(PluginId("plugin.two".to_string()), "two"),
        PluginDiagnostic::provider_collect_failed("provider", "collect failed"),
    ];

    let mut overlay = PluginDiagnosticOverlayState::default();
    overlay.record(&diagnostics).expect("generation");
    let frame = overlay.frame(40, 8).expect("frame");
    let spec = overlay.paint_spec(40, 8).expect("paint spec");

    assert!(frame.header_text.contains(PLUGIN_DIAGNOSTIC_OVERLAY_TITLE));
    assert_eq!(
        spec.shadow,
        Some(plugin_diagnostic_overlay_shadow_spec_with_tone(
            PLUGIN_DIAGNOSTIC_OVERLAY_TITLE,
            OverlayBackdropTone::Activation,
            PluginDiagnosticSeverity::Error,
        ))
    );
    assert_eq!(
        spec.header_face,
        plugin_diagnostic_overlay_header_face_with_tone(
            PLUGIN_DIAGNOSTIC_OVERLAY_TITLE,
            OverlayBackdropTone::Activation,
            PluginDiagnosticSeverity::Error,
        )
    );
}

#[test]
fn overlay_paint_spec_uses_discovery_tone_when_provider_score_dominates_mixed_batch() {
    let diagnostics = vec![
        PluginDiagnostic::provider_collect_failed("provider", "collect failed"),
        PluginDiagnostic::provider_collect_failed("provider", "collect failed again"),
        PluginDiagnostic::instantiation_failed(PluginId("plugin.one".to_string()), "one"),
    ];

    let mut overlay = PluginDiagnosticOverlayState::default();
    overlay.record(&diagnostics).expect("generation");
    let frame = overlay.frame(40, 8).expect("frame");
    let spec = overlay.paint_spec(40, 8).expect("paint spec");

    assert!(frame.header_text.contains(PLUGIN_DIAGNOSTIC_OVERLAY_TITLE));
    assert_eq!(
        spec.shadow,
        Some(plugin_diagnostic_overlay_shadow_spec_with_tone(
            PLUGIN_DIAGNOSTIC_OVERLAY_TITLE,
            OverlayBackdropTone::Discovery,
            PluginDiagnosticSeverity::Error,
        ))
    );
    assert_eq!(
        spec.header_face,
        plugin_diagnostic_overlay_header_face_with_tone(
            PLUGIN_DIAGNOSTIC_OVERLAY_TITLE,
            OverlayBackdropTone::Discovery,
            PluginDiagnosticSeverity::Error,
        )
    );
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
