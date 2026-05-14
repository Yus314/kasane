//! RFC-106a — plugin-emitted diagnostic tests.
//!
//! Pin the `PluginDiagnosticKind::PluginEmitted` factory + the
//! `Command::EmitDiagnostic` classification + the overlay / history
//! integration. Full event-loop dispatch is exercised indirectly: tests
//! here verify the lossless path from `plugin_emitted()` → overlay /
//! history record, which is what the dispatcher invokes (see
//! `kasane-core/src/event_loop/dispatch.rs::handle_inter_plugin_command`
//! `Command::EmitDiagnostic` arm).

use std::time::Duration;

use kasane_core::plugin::PluginId;
use kasane_core::plugin::diagnostics::{
    DiagnosticHistory, DiagnosticSourceRange, PluginDiagnostic, PluginDiagnosticKind,
    PluginDiagnosticOverlayState, PluginDiagnosticSeverity, PluginDiagnosticTarget,
};
use kasane_core::plugin::effect::command::{Command, EffectCategory};

// ---------------------------------------------------------------------------
// Factory
// ---------------------------------------------------------------------------

#[test]
fn plugin_emitted_factory_sets_target_to_plugin() {
    let diag = PluginDiagnostic::plugin_emitted(
        PluginId::from("md.rich"),
        "parse failed",
        "unclosed code block",
        PluginDiagnosticSeverity::Warning,
        None,
        None,
        None,
    );
    match diag.target {
        PluginDiagnosticTarget::Plugin(ref id) => assert_eq!(id.as_str(), "md.rich"),
        other => panic!("expected Plugin target, got {other:?}"),
    }
}

#[test]
fn plugin_emitted_factory_routes_severity() {
    for s in [
        PluginDiagnosticSeverity::Info,
        PluginDiagnosticSeverity::Warning,
        PluginDiagnosticSeverity::Error,
    ] {
        let diag = PluginDiagnostic::plugin_emitted(
            PluginId::from("p"),
            "title",
            "body",
            s,
            None,
            None,
            None,
        );
        assert_eq!(diag.severity(), s);
    }
}

#[test]
fn plugin_emitted_factory_composes_title_and_body_into_message() {
    let diag = PluginDiagnostic::plugin_emitted(
        PluginId::from("p"),
        "T",
        "B",
        PluginDiagnosticSeverity::Info,
        None,
        None,
        None,
    );
    assert_eq!(diag.message, "T: B");
}

#[test]
fn plugin_emitted_factory_drops_separator_when_body_empty() {
    let diag = PluginDiagnostic::plugin_emitted(
        PluginId::from("p"),
        "title-only",
        "",
        PluginDiagnosticSeverity::Info,
        None,
        None,
        None,
    );
    assert_eq!(diag.message, "title-only");
}

#[test]
fn plugin_emitted_kind_carries_all_fields() {
    let diag = PluginDiagnostic::plugin_emitted(
        PluginId::from("p"),
        "T",
        "B",
        PluginDiagnosticSeverity::Error,
        Some(DiagnosticSourceRange {
            line: 5,
            byte_start: 3,
            byte_end: 8,
        }),
        Some("k".into()),
        Some(Duration::from_secs(10)),
    );
    if let PluginDiagnosticKind::PluginEmitted {
        ref title,
        ref body,
        severity,
        ref range,
        ref dedup_key,
        ttl_override,
    } = diag.kind
    {
        assert_eq!(title, "T");
        assert_eq!(body, "B");
        assert_eq!(severity, PluginDiagnosticSeverity::Error);
        assert_eq!(range.as_ref().map(|r| r.line), Some(5));
        assert_eq!(dedup_key.as_deref(), Some("k"));
        assert_eq!(ttl_override, Some(Duration::from_secs(10)));
    } else {
        panic!("expected PluginEmitted kind");
    }
}

// ---------------------------------------------------------------------------
// Overlay / history integration
// ---------------------------------------------------------------------------

#[test]
fn plugin_emitted_diagnostic_records_into_history() {
    let mut history = DiagnosticHistory::default();
    let diag = PluginDiagnostic::plugin_emitted(
        PluginId::from("p"),
        "T",
        "B",
        PluginDiagnosticSeverity::Info,
        None,
        None,
        None,
    );
    history.record(std::slice::from_ref(&diag));
    let snapshot: Vec<_> = history.entries().collect();
    assert_eq!(snapshot.len(), 1);
    assert!(
        matches!(
            snapshot[0].diagnostic.kind,
            PluginDiagnosticKind::PluginEmitted { .. }
        ),
        "history must record PluginEmitted kind verbatim"
    );
}

#[test]
fn plugin_emitted_overlay_records_visible_line() {
    let mut overlay = PluginDiagnosticOverlayState::default();
    let diag = PluginDiagnostic::plugin_emitted(
        PluginId::from("p"),
        "title",
        "body",
        PluginDiagnosticSeverity::Warning,
        None,
        None,
        None,
    );
    let generation = overlay.record(std::slice::from_ref(&diag));
    assert!(generation.is_some(), "overlay must record a generation");
}

#[test]
fn plugin_emitted_overlay_dismiss_after_info() {
    let mut overlay = PluginDiagnosticOverlayState::default();
    let diag = PluginDiagnostic::plugin_emitted(
        PluginId::from("p"),
        "t",
        "b",
        PluginDiagnosticSeverity::Info,
        None,
        None,
        None,
    );
    overlay.record(std::slice::from_ref(&diag));
    assert!(
        overlay.dismiss_after().is_some(),
        "Info severity must have a finite auto-dismiss duration"
    );
}

#[test]
fn plugin_emitted_overlay_dismiss_after_error_persists() {
    let mut overlay = PluginDiagnosticOverlayState::default();
    let diag = PluginDiagnostic::plugin_emitted(
        PluginId::from("p"),
        "t",
        "b",
        PluginDiagnosticSeverity::Error,
        None,
        None,
        None,
    );
    overlay.record(std::slice::from_ref(&diag));
    assert!(
        overlay.dismiss_after().is_none(),
        "Error severity must persist until explicit dismiss"
    );
}

// ---------------------------------------------------------------------------
// Command classification
// ---------------------------------------------------------------------------

fn sample_emit_command() -> Command {
    Command::EmitDiagnostic {
        severity: PluginDiagnosticSeverity::Info,
        title: "t".into(),
        body: "b".into(),
        range: None,
        dedup_key: None,
        ttl_override: None,
    }
}

#[test]
fn emit_diagnostic_is_deferred() {
    assert!(sample_emit_command().is_deferred());
}

#[test]
fn emit_diagnostic_is_not_kakoune_writing() {
    assert!(!sample_emit_command().is_kakoune_writing());
}

#[test]
fn emit_diagnostic_is_not_process_command() {
    assert!(!sample_emit_command().is_process_command());
}

#[test]
fn emit_diagnostic_effect_category_is_redraw() {
    assert_eq!(
        sample_emit_command().effect_category(),
        EffectCategory::REDRAW
    );
}
