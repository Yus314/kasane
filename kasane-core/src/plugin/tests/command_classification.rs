//! Structural witness tests for Command classification (ADR-030 Level 3).

use std::collections::BTreeSet;
use std::time::Duration;

use crate::input::{InputEvent, Key, KeyEvent, Modifiers};
use crate::plugin::command::Command;
use crate::plugin::io::StdinMode;
use crate::plugin::kakoune_safe_command::KakouneSafeCommand;
use crate::plugin::{BufferEdit, BufferPosition, PluginId};
use crate::protocol::KasaneRequest;
use crate::session::SessionCommand;
use crate::state::DirtyFlags;
use crate::surface::SurfaceId;
use crate::workspace::WorkspaceCommand;

/// Construct one instance of each Command variant with dummy values.
pub(super) fn make_all_command_instances() -> Vec<Command> {
    vec![
        Command::SendToKakoune(KasaneRequest::Keys(vec![])),
        Command::InsertText(String::new()),
        Command::PasteClipboard,
        Command::Quit,
        Command::RequestRedraw(DirtyFlags::empty()),
        Command::ScheduleTimer {
            timer_id: 0,
            delay: Duration::ZERO,
            target: PluginId("test".into()),
            payload: Box::new(()),
        },
        Command::CancelTimer { timer_id: 0 },
        Command::PluginMessage {
            target: PluginId("test".into()),
            payload: Box::new(()),
        },
        Command::SetConfig {
            key: String::new(),
            value: String::new(),
        },
        Command::SetSetting {
            plugin_id: PluginId("test".into()),
            key: String::new(),
            value: crate::plugin::setting::SettingValue::Bool(false),
        },
        Command::Workspace(WorkspaceCommand::RemoveSurface(SurfaceId(0))),
        Command::RegisterSurface {
            surface: crate::test_support::TestSurfaceBuilder::new(SurfaceId(999)).build(),
            placement: crate::workspace::Placement::Tab,
        },
        Command::RegisterSurfaceRequested {
            surface: crate::test_support::TestSurfaceBuilder::new(SurfaceId(998)).build(),
            placement: crate::surface::SurfacePlacementRequest::Tab,
        },
        Command::UnregisterSurface {
            surface_id: SurfaceId(0),
        },
        Command::UnregisterSurfaceKey {
            surface_key: String::new(),
        },
        Command::RegisterThemeTokens(vec![]),
        Command::SpawnProcess {
            job_id: 0,
            program: String::new(),
            args: vec![],
            stdin_mode: StdinMode::Null,
        },
        Command::Session(SessionCommand::Close { key: None }),
        Command::WriteToProcess {
            job_id: 0,
            data: vec![],
        },
        Command::CloseProcessStdin { job_id: 0 },
        Command::KillProcess { job_id: 0 },
        Command::ResizePty {
            job_id: 0,
            rows: 24,
            cols: 80,
        },
        Command::EditBuffer {
            edits: vec![BufferEdit {
                start: BufferPosition { line: 1, column: 1 },
                end: BufferPosition { line: 1, column: 1 },
                replacement: String::new(),
            }],
        },
        Command::InjectInput(InputEvent::Key(KeyEvent {
            key: Key::Char('a'),
            modifiers: Modifiers::empty(),
        })),
        Command::SpawnPaneClient {
            pane_key: String::new(),
            placement: crate::workspace::Placement::Tab,
        },
        Command::ClosePaneClient {
            pane_key: String::new(),
        },
        Command::BindSurfaceSession {
            surface_id: SurfaceId(0),
            session_id: crate::session::SessionId(0),
        },
        Command::UnbindSurfaceSession {
            surface_id: SurfaceId(0),
        },
        Command::StartProcessTask {
            task_name: String::new(),
        },
        Command::ExposeVariable {
            name: String::new(),
            value: crate::widget::types::Value::Empty,
        },
        Command::HttpRequest {
            job_id: 0,
            config: crate::plugin::HttpRequestConfig {
                url: String::new(),
                method: crate::plugin::HttpMethod::Get,
                headers: vec![],
                body: None,
                timeout_ms: 30_000,
                idle_timeout_ms: 10_000,
                streaming: crate::plugin::StreamingMode::Buffered,
            },
        },
        Command::CancelHttpRequest { job_id: 0 },
        Command::SetStructuralProjection(None),
        Command::ToggleAdditiveProjection(crate::display::ProjectionId::new("test")),
        Command::ProjectionOff,
        Command::UpdateShadowCursor(None),
        Command::UpdateDragState(crate::state::DragState::None),
    ]
}

#[test]
fn all_variant_names_count_matches_enum() {
    let all_instances = make_all_command_instances();
    assert_eq!(
        all_instances.len(),
        Command::ALL_VARIANT_NAMES.len(),
        "make_all_command_instances() must produce exactly one instance per variant"
    );
}

#[test]
fn all_variant_names_are_unique() {
    let set: BTreeSet<_> = Command::ALL_VARIANT_NAMES.iter().copied().collect();
    assert_eq!(
        set.len(),
        Command::ALL_VARIANT_NAMES.len(),
        "ALL_VARIANT_NAMES must not contain duplicates"
    );
}

#[test]
fn variant_name_covers_all() {
    let from_instances: BTreeSet<_> = make_all_command_instances()
        .iter()
        .map(|cmd| cmd.variant_name())
        .collect();
    let from_const: BTreeSet<_> = Command::ALL_VARIANT_NAMES.iter().copied().collect();
    assert_eq!(
        from_instances, from_const,
        "variant_name() must match ALL_VARIANT_NAMES exactly"
    );
}

#[test]
fn writing_set_matches_semantics() {
    assert_eq!(
        Command::KAKOUNE_WRITING_VARIANTS,
        &["SendToKakoune", "InsertText", "EditBuffer"],
    );
}

#[test]
fn transparent_covers_exactly_non_writing() {
    let transparent: BTreeSet<_> = KakouneSafeCommand::VARIANT_NAMES.iter().copied().collect();
    let writing: BTreeSet<_> = Command::KAKOUNE_WRITING_VARIANTS.iter().copied().collect();
    let all: BTreeSet<_> = Command::ALL_VARIANT_NAMES.iter().copied().collect();
    assert_eq!(transparent, &all - &writing);
}

#[test]
fn is_kakoune_writing_matches_constants() {
    for cmd in make_all_command_instances() {
        let name = cmd.variant_name();
        assert_eq!(
            cmd.is_kakoune_writing(),
            Command::KAKOUNE_WRITING_VARIANTS.contains(&name),
            "classification mismatch for {name}"
        );
    }
}

#[test]
fn kakoune_writing_are_never_commutative() {
    for cmd in make_all_command_instances() {
        if cmd.is_kakoune_writing() {
            assert!(
                !cmd.is_commutative(),
                "writing + commutative conflict for {}",
                cmd.variant_name()
            );
        }
    }
}

#[test]
fn kakoune_writing_are_all_immediate() {
    for cmd in make_all_command_instances() {
        if cmd.is_kakoune_writing() {
            assert!(
                !cmd.is_deferred(),
                "writing commands must be immediate, but {} is deferred",
                cmd.variant_name()
            );
        }
    }
}
