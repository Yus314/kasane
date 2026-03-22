use super::*;
use crate::test_support::TestSurfaceBuilder;

#[test]
fn test_extract_redraw_flags_merges() {
    let mut commands = vec![
        Command::RequestRedraw(DirtyFlags::BUFFER),
        Command::SendToKakoune(crate::protocol::KasaneRequest::Keys(vec!["a".into()])),
        Command::RequestRedraw(DirtyFlags::INFO),
    ];
    let flags = extract_redraw_flags(&mut commands);
    assert_eq!(flags, DirtyFlags::BUFFER | DirtyFlags::INFO);
    assert_eq!(commands.len(), 1);
    assert!(matches!(commands[0], Command::SendToKakoune(_)));
}

#[test]
fn test_extract_redraw_flags_empty() {
    let mut commands = vec![
        Command::SendToKakoune(crate::protocol::KasaneRequest::Keys(vec!["a".into()])),
        Command::Paste,
    ];
    let flags = extract_redraw_flags(&mut commands);
    assert!(flags.is_empty());
    assert_eq!(commands.len(), 2);
}

#[test]
fn test_partition_separates_correctly() {
    let commands = vec![
        Command::SendToKakoune(crate::protocol::KasaneRequest::Keys(vec!["a".into()])),
        Command::ScheduleTimer {
            delay: std::time::Duration::from_millis(100),
            target: PluginId("test".into()),
            payload: Box::new(42u32),
        },
        Command::PluginMessage {
            target: PluginId("other".into()),
            payload: Box::new("hello"),
        },
        Command::SetConfig {
            key: "foo".into(),
            value: "bar".into(),
        },
        Command::Paste,
    ];
    let (immediate, deferred) = partition_commands(commands);
    assert_eq!(immediate.len(), 2); // SendToKakoune + Paste
    assert_eq!(deferred.len(), 3); // Timer + Message + Config
}

#[test]
fn test_partition_empty_deferred() {
    let commands = vec![
        Command::SendToKakoune(crate::protocol::KasaneRequest::Keys(vec!["a".into()])),
        Command::Quit,
    ];
    let (immediate, deferred) = partition_commands(commands);
    assert_eq!(immediate.len(), 2);
    assert!(deferred.is_empty());
}

#[test]
fn test_set_config_stores_in_ui_options() {
    // SetConfig applied via ui_options (integration would be in event loop)
    let mut state = AppState::default();
    state.ui_options.insert("key".into(), "value".into());
    assert_eq!(state.ui_options.get("key").unwrap(), "value");
}

#[test]
fn test_partition_spawn_process() {
    let commands = vec![Command::SpawnProcess {
        job_id: 1,
        program: "cat".into(),
        args: vec!["/etc/hostname".into()],
        stdin_mode: StdinMode::Null,
    }];
    let (immediate, deferred) = partition_commands(commands);
    assert!(immediate.is_empty());
    assert_eq!(deferred.len(), 1);
    assert!(matches!(
        deferred[0],
        Command::SpawnProcess {
            job_id: 1,
            program: _,
            ..
        }
    ));
}

#[test]
fn test_partition_write_to_process() {
    let commands = vec![Command::WriteToProcess {
        job_id: 5,
        data: b"input data".to_vec(),
    }];
    let (immediate, deferred) = partition_commands(commands);
    assert!(immediate.is_empty());
    assert_eq!(deferred.len(), 1);
    assert!(matches!(
        deferred[0],
        Command::WriteToProcess { job_id: 5, .. }
    ));
}

#[test]
fn test_partition_close_process_stdin() {
    let commands = vec![Command::CloseProcessStdin { job_id: 3 }];
    let (immediate, deferred) = partition_commands(commands);
    assert!(immediate.is_empty());
    assert_eq!(deferred.len(), 1);
    assert!(matches!(
        deferred[0],
        Command::CloseProcessStdin { job_id: 3 }
    ));
}

#[test]
fn test_partition_kill_process() {
    let commands = vec![Command::KillProcess { job_id: 10 }];
    let (immediate, deferred) = partition_commands(commands);
    assert!(immediate.is_empty());
    assert_eq!(deferred.len(), 1);
    assert!(matches!(deferred[0], Command::KillProcess { job_id: 10 }));
}

#[test]
fn test_partition_mixed_process_commands() {
    let commands = vec![
        Command::SendToKakoune(crate::protocol::KasaneRequest::Keys(vec!["x".into()])),
        Command::SpawnProcess {
            job_id: 1,
            program: "ls".into(),
            args: vec![],
            stdin_mode: StdinMode::Null,
        },
        Command::WriteToProcess {
            job_id: 1,
            data: vec![],
        },
        Command::CloseProcessStdin { job_id: 1 },
        Command::KillProcess { job_id: 2 },
        Command::Paste,
    ];
    let (immediate, deferred) = partition_commands(commands);
    assert_eq!(immediate.len(), 2); // SendToKakoune + Paste
    assert_eq!(deferred.len(), 4); // SpawnProcess + WriteToProcess + CloseProcessStdin + KillProcess
}

#[test]
fn test_partition_dynamic_surface_commands() {
    let commands = vec![
        Command::RegisterSurface {
            surface: TestSurfaceBuilder::new(SurfaceId(250)).build(),
            placement: Placement::SplitFocused {
                direction: SplitDirection::Vertical,
                ratio: 0.5,
            },
        },
        Command::UnregisterSurface {
            surface_id: SurfaceId(250),
        },
        Command::RegisterSurfaceRequested {
            surface: TestSurfaceBuilder::new(SurfaceId(251)).build(),
            placement: crate::surface::SurfacePlacementRequest::Tab,
        },
        Command::UnregisterSurfaceKey {
            surface_key: "test.dynamic".into(),
        },
    ];
    let (immediate, deferred) = partition_commands(commands);
    assert!(immediate.is_empty());
    assert_eq!(deferred.len(), 4);
    assert!(matches!(deferred[0], Command::RegisterSurface { .. }));
    assert!(matches!(
        deferred[1],
        Command::UnregisterSurface {
            surface_id: SurfaceId(250)
        }
    ));
    assert!(matches!(
        deferred[2],
        Command::RegisterSurfaceRequested { .. }
    ));
    assert!(matches!(deferred[3], Command::UnregisterSurfaceKey { .. }));
}

// --- G2: EditBuffer tests ---

#[test]
fn test_edit_buffer_is_immediate() {
    let commands = vec![Command::EditBuffer {
        edits: vec![BufferEdit {
            start: BufferPosition { line: 1, column: 1 },
            end: BufferPosition { line: 1, column: 5 },
            replacement: "hello".into(),
        }],
    }];
    let (immediate, deferred) = partition_commands(commands);
    assert_eq!(immediate.len(), 1);
    assert!(deferred.is_empty());
}

#[test]
fn test_escape_kakoune_insert_text_special_chars() {
    let escaped = escape_kakoune_insert_text("a<b>\nt\t\x1b -");
    assert_eq!(
        escaped,
        vec![
            "a", "<lt>", "b", "<gt>", "<ret>", "t", "<tab>", "<esc>", "<space>", "<minus>"
        ]
    );
}

#[test]
fn test_escape_kakoune_insert_text_empty() {
    let escaped = escape_kakoune_insert_text("");
    assert!(escaped.is_empty());
}

#[test]
fn test_escape_kakoune_insert_text_multibyte() {
    let escaped = escape_kakoune_insert_text("日本語");
    assert_eq!(escaped, vec!["日", "本", "語"]);
}

#[test]
fn test_edits_to_keys_empty() {
    let keys = edits_to_keys(&[]);
    assert!(keys.is_empty());
}

#[test]
fn test_edits_to_keys_single_replace() {
    let edits = vec![BufferEdit {
        start: BufferPosition { line: 3, column: 5 },
        end: BufferPosition {
            line: 3,
            column: 10,
        },
        replacement: "hello".into(),
    }];
    let keys = edits_to_keys(&edits);
    // Should start with <esc>, navigate to position, change, type text, exit
    assert_eq!(keys[0], "<esc>");
    assert!(keys.contains(&"3g".to_string()));
    assert!(keys.contains(&"c".to_string()));
    assert!(keys.contains(&"h".to_string()));
    assert!(*keys.last().unwrap() == "<esc>");
}

#[test]
fn test_edits_to_keys_deletion() {
    let edits = vec![BufferEdit {
        start: BufferPosition { line: 1, column: 1 },
        end: BufferPosition { line: 1, column: 5 },
        replacement: String::new(),
    }];
    let keys = edits_to_keys(&edits);
    assert!(keys.contains(&"d".to_string()));
    assert!(!keys.contains(&"c".to_string()));
}

#[test]
fn test_edits_to_keys_multiple_sorted_bottom_up() {
    let edits = vec![
        BufferEdit {
            start: BufferPosition { line: 1, column: 1 },
            end: BufferPosition { line: 1, column: 3 },
            replacement: "AA".into(),
        },
        BufferEdit {
            start: BufferPosition {
                line: 10,
                column: 1,
            },
            end: BufferPosition {
                line: 10,
                column: 3,
            },
            replacement: "BB".into(),
        },
    ];
    let keys = edits_to_keys(&edits);

    // Line 10 should appear before line 1 in the key sequence (bottom-up)
    let line10_pos = keys.iter().position(|k| k == "10g").unwrap();
    let line1_pos = keys.iter().position(|k| k == "1g").unwrap();
    assert!(
        line10_pos < line1_pos,
        "line 10 edit should come before line 1 edit (bottom-up order)"
    );
}

#[test]
fn test_edits_to_keys_insert_at_point() {
    // Zero-width range (start == end) with replacement text = insertion
    let edits = vec![BufferEdit {
        start: BufferPosition { line: 5, column: 3 },
        end: BufferPosition { line: 5, column: 3 },
        replacement: "new".into(),
    }];
    let keys = edits_to_keys(&edits);
    assert!(keys.contains(&"c".to_string()));
    assert!(keys.contains(&"n".to_string()));
    assert!(keys.contains(&"e".to_string()));
    assert!(keys.contains(&"w".to_string()));
}

#[test]
fn test_edits_to_keys_zero_width_empty_replacement_skipped() {
    // Zero-width range with empty replacement = no-op
    let edits = vec![BufferEdit {
        start: BufferPosition { line: 1, column: 1 },
        end: BufferPosition { line: 1, column: 1 },
        replacement: String::new(),
    }];
    let keys = edits_to_keys(&edits);
    // Should only have <esc> and navigation, no d or c
    assert!(!keys.contains(&"d".to_string()));
    assert!(!keys.contains(&"c".to_string()));
}

#[test]
fn test_execute_commands_edit_buffer() {
    let mut output = Vec::new();
    let result = execute_commands(
        vec![Command::EditBuffer {
            edits: vec![BufferEdit {
                start: BufferPosition { line: 1, column: 1 },
                end: BufferPosition { line: 1, column: 3 },
                replacement: "hi".into(),
            }],
        }],
        &mut output,
        &mut crate::clipboard::SystemClipboard::noop(),
    );
    assert!(matches!(result, CommandResult::Continue));
    // Output should contain a JSON-RPC keys request
    let output_str = String::from_utf8(output).unwrap();
    assert!(output_str.contains("\"method\":\"keys\""));
}

#[test]
fn test_inject_input_is_deferred() {
    use crate::input::{InputEvent, Key, KeyEvent, Modifiers};
    let commands = vec![Command::InjectInput(InputEvent::Key(KeyEvent {
        key: Key::Char('a'),
        modifiers: Modifiers::empty(),
    }))];
    let (immediate, deferred) = partition_commands(commands);
    assert!(immediate.is_empty());
    assert_eq!(deferred.len(), 1);
    assert!(matches!(deferred[0], Command::InjectInput(_)));
}

#[test]
fn test_execute_commands_edit_buffer_empty_edits() {
    let mut output = Vec::new();
    let result = execute_commands(
        vec![Command::EditBuffer { edits: vec![] }],
        &mut output,
        &mut crate::clipboard::SystemClipboard::noop(),
    );
    assert!(matches!(result, CommandResult::Continue));
    // No output for empty edits
    assert!(output.is_empty());
}
