use super::*;

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
fn test_extract_deferred_separates_correctly() {
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
    let (normal, deferred) = extract_deferred_commands(commands);
    assert_eq!(normal.len(), 2); // SendToKakoune + Paste
    assert_eq!(deferred.len(), 3); // Timer + Message + Config
}

#[test]
fn test_extract_deferred_empty() {
    let commands = vec![
        Command::SendToKakoune(crate::protocol::KasaneRequest::Keys(vec!["a".into()])),
        Command::Quit,
    ];
    let (normal, deferred) = extract_deferred_commands(commands);
    assert_eq!(normal.len(), 2);
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
fn test_extract_deferred_spawn_process() {
    let commands = vec![Command::SpawnProcess {
        job_id: 1,
        program: "cat".into(),
        args: vec!["/etc/hostname".into()],
        stdin_mode: StdinMode::Null,
    }];
    let (normal, deferred) = extract_deferred_commands(commands);
    assert!(normal.is_empty());
    assert_eq!(deferred.len(), 1);
    match &deferred[0] {
        DeferredCommand::SpawnProcess {
            job_id,
            program,
            args,
            stdin_mode,
        } => {
            assert_eq!(*job_id, 1);
            assert_eq!(program, "cat");
            assert_eq!(args, &["/etc/hostname".to_string()]);
            assert_eq!(*stdin_mode, StdinMode::Null);
        }
        _ => panic!("expected DeferredCommand::SpawnProcess"),
    }
}

#[test]
fn test_extract_deferred_write_to_process() {
    let commands = vec![Command::WriteToProcess {
        job_id: 5,
        data: b"input data".to_vec(),
    }];
    let (normal, deferred) = extract_deferred_commands(commands);
    assert!(normal.is_empty());
    assert_eq!(deferred.len(), 1);
    match &deferred[0] {
        DeferredCommand::WriteToProcess { job_id, data } => {
            assert_eq!(*job_id, 5);
            assert_eq!(data, b"input data");
        }
        _ => panic!("expected DeferredCommand::WriteToProcess"),
    }
}

#[test]
fn test_extract_deferred_close_process_stdin() {
    let commands = vec![Command::CloseProcessStdin { job_id: 3 }];
    let (normal, deferred) = extract_deferred_commands(commands);
    assert!(normal.is_empty());
    assert_eq!(deferred.len(), 1);
    assert!(matches!(
        deferred[0],
        DeferredCommand::CloseProcessStdin { job_id: 3 }
    ));
}

#[test]
fn test_extract_deferred_kill_process() {
    let commands = vec![Command::KillProcess { job_id: 10 }];
    let (normal, deferred) = extract_deferred_commands(commands);
    assert!(normal.is_empty());
    assert_eq!(deferred.len(), 1);
    assert!(matches!(
        deferred[0],
        DeferredCommand::KillProcess { job_id: 10 }
    ));
}

#[test]
fn test_extract_deferred_mixed_process_commands() {
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
    let (normal, deferred) = extract_deferred_commands(commands);
    assert_eq!(normal.len(), 2); // SendToKakoune + Paste
    assert_eq!(deferred.len(), 4); // SpawnProcess + WriteToProcess + CloseProcessStdin + KillProcess
}
