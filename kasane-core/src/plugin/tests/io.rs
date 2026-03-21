use super::*;
use crate::plugin::RuntimeEffects;
use crate::scroll::{ScrollAccumulationMode, ScrollCurve, ScrollPlan};

// --- I/O event construction tests ---

#[test]
fn test_io_event_process_stdout_construction() {
    let event = IoEvent::Process(ProcessEvent::Stdout {
        job_id: 42,
        data: b"hello world".to_vec(),
    });
    match &event {
        IoEvent::Process(ProcessEvent::Stdout { job_id, data }) => {
            assert_eq!(*job_id, 42);
            assert_eq!(data, b"hello world");
        }
        _ => panic!("expected Process::Stdout"),
    }
}

#[test]
fn test_io_event_process_stderr_construction() {
    let event = IoEvent::Process(ProcessEvent::Stderr {
        job_id: 7,
        data: b"error msg".to_vec(),
    });
    match &event {
        IoEvent::Process(ProcessEvent::Stderr { job_id, data }) => {
            assert_eq!(*job_id, 7);
            assert_eq!(data, b"error msg");
        }
        _ => panic!("expected Process::Stderr"),
    }
}

#[test]
fn test_io_event_process_exited_construction() {
    let event = IoEvent::Process(ProcessEvent::Exited {
        job_id: 1,
        exit_code: 0,
    });
    match &event {
        IoEvent::Process(ProcessEvent::Exited { job_id, exit_code }) => {
            assert_eq!(*job_id, 1);
            assert_eq!(*exit_code, 0);
        }
        _ => panic!("expected Process::Exited"),
    }
}

#[test]
fn test_io_event_process_spawn_failed_construction() {
    let event = IoEvent::Process(ProcessEvent::SpawnFailed {
        job_id: 99,
        error: "not found".to_string(),
    });
    match &event {
        IoEvent::Process(ProcessEvent::SpawnFailed { job_id, error }) => {
            assert_eq!(*job_id, 99);
            assert_eq!(error, "not found");
        }
        _ => panic!("expected Process::SpawnFailed"),
    }
}

// --- deliver_io_event tests ---

struct IoHandlerPlugin {
    received_events: Vec<String>,
}

impl IoHandlerPlugin {
    fn new() -> Self {
        IoHandlerPlugin {
            received_events: Vec::new(),
        }
    }
}

impl PluginBackend for IoHandlerPlugin {
    fn id(&self) -> PluginId {
        PluginId("io_handler".to_string())
    }

    fn capabilities(&self) -> PluginCapabilities {
        PluginCapabilities::IO_HANDLER
    }

    fn on_io_event_effects(&mut self, event: &IoEvent, _state: &AppState) -> RuntimeEffects {
        match event {
            IoEvent::Process(pe) => match pe {
                ProcessEvent::Stdout { job_id, data } => {
                    self.received_events
                        .push(format!("stdout:{}:{}", job_id, data.len()));
                    RuntimeEffects {
                        redraw: DirtyFlags::BUFFER,
                        commands: vec![],
                        scroll_plans: vec![ScrollPlan {
                            total_amount: 1,
                            line: 1,
                            column: 1,
                            frame_interval_ms: 16,
                            curve: ScrollCurve::Linear,
                            accumulation: ScrollAccumulationMode::Add,
                        }],
                    }
                }
                ProcessEvent::Stderr { job_id, data } => {
                    self.received_events
                        .push(format!("stderr:{}:{}", job_id, data.len()));
                    RuntimeEffects::default()
                }
                ProcessEvent::Exited { job_id, exit_code } => {
                    self.received_events
                        .push(format!("exited:{}:{}", job_id, exit_code));
                    RuntimeEffects::default()
                }
                ProcessEvent::SpawnFailed { job_id, error } => {
                    self.received_events
                        .push(format!("failed:{}:{}", job_id, error));
                    RuntimeEffects::default()
                }
            },
        }
    }
}

#[test]
fn test_deliver_io_event_dispatches_to_plugin() {
    let mut registry = PluginRuntime::new();
    registry.register_backend(Box::new(IoHandlerPlugin::new()));
    let state = AppState::default();

    let event = IoEvent::Process(ProcessEvent::Stdout {
        job_id: 1,
        data: b"output".to_vec(),
    });
    let batch =
        registry.deliver_io_event_batch(&PluginId("io_handler".to_string()), &event, &state);
    assert!(batch.effects.redraw.contains(DirtyFlags::BUFFER));
    assert!(batch.effects.commands.is_empty());
    assert_eq!(batch.effects.scroll_plans.len(), 1);
}

#[test]
fn test_deliver_io_event_unknown_target() {
    let mut registry = PluginRuntime::new();
    registry.register_backend(Box::new(IoHandlerPlugin::new()));
    let state = AppState::default();

    let event = IoEvent::Process(ProcessEvent::Stdout {
        job_id: 1,
        data: vec![],
    });
    let batch =
        registry.deliver_io_event_batch(&PluginId("nonexistent".to_string()), &event, &state);
    assert!(batch.effects.redraw.is_empty());
    assert!(batch.effects.commands.is_empty());
    assert!(batch.effects.scroll_plans.is_empty());
}

// --- ProcessDispatcher tests ---

#[test]
fn test_null_process_dispatcher() {
    let mut dispatcher = NullProcessDispatcher;
    let plugin_id = PluginId("test".into());
    // All methods should be no-ops (no panic)
    dispatcher.spawn(&plugin_id, 1, "echo", &["hello".into()], StdinMode::Null);
    dispatcher.write(&plugin_id, 1, b"data");
    dispatcher.close_stdin(&plugin_id, 1);
    dispatcher.kill(&plugin_id, 1);
    dispatcher.remove_finished_job(&plugin_id, 1);
}

struct RecordingDispatcher {
    spawns: Vec<(String, u64, String)>,
    writes: Vec<(u64, Vec<u8>)>,
    close_stdins: Vec<u64>,
    kills: Vec<u64>,
}

impl RecordingDispatcher {
    fn new() -> Self {
        RecordingDispatcher {
            spawns: Vec::new(),
            writes: Vec::new(),
            close_stdins: Vec::new(),
            kills: Vec::new(),
        }
    }
}

impl ProcessDispatcher for RecordingDispatcher {
    fn spawn(
        &mut self,
        plugin_id: &PluginId,
        job_id: u64,
        program: &str,
        _args: &[String],
        _stdin_mode: StdinMode,
    ) {
        self.spawns
            .push((plugin_id.0.clone(), job_id, program.to_string()));
    }
    fn write(&mut self, _plugin_id: &PluginId, job_id: u64, data: &[u8]) {
        self.writes.push((job_id, data.to_vec()));
    }
    fn close_stdin(&mut self, _plugin_id: &PluginId, job_id: u64) {
        self.close_stdins.push(job_id);
    }
    fn kill(&mut self, _plugin_id: &PluginId, job_id: u64) {
        self.kills.push(job_id);
    }
    fn resize_pty(&mut self, _plugin_id: &PluginId, _job_id: u64, _rows: u16, _cols: u16) {}
    fn remove_finished_job(&mut self, _plugin_id: &PluginId, _job_id: u64) {}
}

#[test]
fn test_recording_dispatcher_tracks_operations() {
    let mut dispatcher = RecordingDispatcher::new();
    let plugin_id = PluginId("my_plugin".into());

    dispatcher.spawn(&plugin_id, 1, "grep", &["foo".into()], StdinMode::Piped);
    dispatcher.write(&plugin_id, 1, b"search input");
    dispatcher.close_stdin(&plugin_id, 1);
    dispatcher.kill(&plugin_id, 1);

    assert_eq!(dispatcher.spawns.len(), 1);
    assert_eq!(dispatcher.spawns[0].0, "my_plugin");
    assert_eq!(dispatcher.spawns[0].1, 1);
    assert_eq!(dispatcher.spawns[0].2, "grep");

    assert_eq!(dispatcher.writes.len(), 1);
    assert_eq!(dispatcher.writes[0].0, 1);
    assert_eq!(dispatcher.writes[0].1, b"search input");

    assert_eq!(dispatcher.close_stdins, vec![1]);
    assert_eq!(dispatcher.kills, vec![1]);
}

// --- plugin_allows_process_spawn tests ---

#[test]
fn test_plugin_allows_process_spawn_default_true() {
    // TestPlugin uses default allows_process_spawn() which returns true
    let mut registry = PluginRuntime::new();
    registry.register_backend(Box::new(TestPlugin));
    assert!(registry.plugin_allows_process_spawn(&PluginId("test".to_string())));
}

#[test]
fn test_plugin_allows_process_spawn_denied() {
    struct DenySpawnPlugin;
    impl PluginBackend for DenySpawnPlugin {
        fn id(&self) -> PluginId {
            PluginId("deny_spawn".to_string())
        }
        fn allows_process_spawn(&self) -> bool {
            false
        }
    }

    let mut registry = PluginRuntime::new();
    registry.register_backend(Box::new(DenySpawnPlugin));
    assert!(!registry.plugin_allows_process_spawn(&PluginId("deny_spawn".to_string())));
}

#[test]
fn test_plugin_allows_process_spawn_unknown_plugin() {
    let registry = PluginRuntime::new();
    // Unknown plugin should return false (is_some_and fails on None)
    assert!(!registry.plugin_allows_process_spawn(&PluginId("unknown".to_string())));
}

#[test]
fn test_stdin_mode_eq() {
    assert_eq!(StdinMode::Null, StdinMode::Null);
    assert_eq!(StdinMode::Piped, StdinMode::Piped);
    assert_ne!(StdinMode::Null, StdinMode::Piped);
}
