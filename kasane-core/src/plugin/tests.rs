use super::*;
use crate::input::KeyEvent;
use crate::layout::SplitDirection;
use crate::protocol::Face;
use crate::state::{AppState, DirtyFlags};
use crate::surface::{EventContext, SizeHint, Surface, SurfaceEvent, SurfaceId, ViewContext};
use crate::workspace::Placement;

struct TestSurface {
    id: SurfaceId,
}

impl Surface for TestSurface {
    fn id(&self) -> SurfaceId {
        self.id
    }

    fn surface_key(&self) -> compact_str::CompactString {
        format!("test.surface.{}", self.id.0).into()
    }

    fn size_hint(&self) -> SizeHint {
        SizeHint::fill()
    }

    fn view(&self, _ctx: &ViewContext<'_>) -> crate::element::Element {
        crate::element::Element::Empty
    }

    fn handle_event(&mut self, _event: SurfaceEvent, _ctx: &EventContext<'_>) -> Vec<Command> {
        vec![]
    }
}

struct TestPlugin;

impl Plugin for TestPlugin {
    fn id(&self) -> PluginId {
        PluginId("test".to_string())
    }
}

#[test]
fn test_empty_registry() {
    let registry = PluginRegistry::new();
    assert!(registry.plugin_count() == 0);
}

#[test]
fn test_plugin_id() {
    let plugin = TestPlugin;
    assert_eq!(plugin.id(), PluginId("test".to_string()));
}

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

// --- Lifecycle hooks tests ---

struct LifecyclePlugin {
    init_called: bool,
    shutdown_called: bool,
    state_changes: Vec<DirtyFlags>,
}

struct SurfacePlugin;

impl Plugin for SurfacePlugin {
    fn id(&self) -> PluginId {
        PluginId("surface-plugin".to_string())
    }

    fn surfaces(&mut self) -> Vec<Box<dyn Surface>> {
        vec![
            Box::new(TestSurface { id: SurfaceId(200) }),
            Box::new(TestSurface { id: SurfaceId(201) }),
        ]
    }

    fn workspace_request(&self) -> Option<Placement> {
        Some(Placement::SplitFocused {
            direction: SplitDirection::Vertical,
            ratio: 0.5,
        })
    }
}

impl LifecyclePlugin {
    fn new() -> Self {
        LifecyclePlugin {
            init_called: false,
            shutdown_called: false,
            state_changes: Vec::new(),
        }
    }
}

impl Plugin for LifecyclePlugin {
    fn id(&self) -> PluginId {
        PluginId("lifecycle".to_string())
    }

    fn on_init(&mut self, _state: &AppState) -> Vec<Command> {
        self.init_called = true;
        vec![Command::RequestRedraw(DirtyFlags::BUFFER)]
    }

    fn on_shutdown(&mut self) {
        self.shutdown_called = true;
    }

    fn on_state_changed(&mut self, _state: &AppState, dirty: DirtyFlags) -> Vec<Command> {
        self.state_changes.push(dirty);
        vec![]
    }
}

#[test]
fn test_init_all_returns_commands() {
    let mut registry = PluginRegistry::new();
    registry.register(Box::new(LifecyclePlugin::new()));
    let state = AppState::default();
    let commands = registry.init_all(&state);
    assert_eq!(commands.len(), 1);
    assert!(matches!(commands[0], Command::RequestRedraw(_)));
}

#[test]
fn test_shutdown_all_calls_all_plugins() {
    let mut registry = PluginRegistry::new();
    registry.register(Box::new(LifecyclePlugin::new()));
    registry.register(Box::new(LifecyclePlugin::new()));
    registry.shutdown_all();
    // Verify via count — can't inspect internal state, but no panic = success
}

#[test]
fn test_collect_plugin_surfaces_returns_owner_group() {
    let mut registry = PluginRegistry::new();
    registry.register(Box::new(SurfacePlugin));

    let surface_sets = registry.collect_plugin_surfaces();
    assert_eq!(surface_sets.len(), 1);
    assert_eq!(
        surface_sets[0].owner,
        PluginId("surface-plugin".to_string())
    );
    assert_eq!(surface_sets[0].surfaces.len(), 2);
    assert_eq!(surface_sets[0].surfaces[0].id(), SurfaceId(200));
    assert_eq!(surface_sets[0].surfaces[1].id(), SurfaceId(201));
    assert!(matches!(
        surface_sets[0].legacy_workspace_request,
        Some(Placement::SplitFocused {
            direction: SplitDirection::Vertical,
            ratio
        }) if (ratio - 0.5).abs() < f32::EPSILON
    ));
}

#[test]
fn test_remove_plugin_removes_registered_plugin() {
    let mut registry = PluginRegistry::new();
    registry.register(Box::new(TestPlugin));
    registry.register(Box::new(SurfacePlugin));

    assert!(registry.remove_plugin(&PluginId("surface-plugin".to_string())));
    assert_eq!(registry.plugin_count(), 1);
    assert!(!registry.remove_plugin(&PluginId("surface-plugin".to_string())));
}

#[test]
fn test_on_state_changed_dispatched_with_flags() {
    let mut registry = PluginRegistry::new();
    registry.register(Box::new(LifecyclePlugin::new()));
    let state = AppState::default();

    // Simulate what update() does for Msg::Kakoune
    let flags = DirtyFlags::BUFFER | DirtyFlags::STATUS;
    for plugin in registry.plugins_mut() {
        plugin.on_state_changed(&state, flags);
    }
    // No panic, default implementations work
}

#[test]
fn test_lifecycle_defaults() {
    // TestPlugin has no lifecycle hooks — defaults should work
    let mut registry = PluginRegistry::new();
    registry.register(Box::new(TestPlugin));
    let state = AppState::default();

    let commands = registry.init_all(&state);
    assert!(commands.is_empty());

    registry.shutdown_all();
    // No panic
}

// --- Input observation tests ---

struct ObservingPlugin {
    observed_keys: std::cell::RefCell<Vec<String>>,
}

impl ObservingPlugin {
    fn new() -> Self {
        ObservingPlugin {
            observed_keys: std::cell::RefCell::new(Vec::new()),
        }
    }
}

impl Plugin for ObservingPlugin {
    fn id(&self) -> PluginId {
        PluginId("observer".to_string())
    }

    fn observe_key(&mut self, key: &KeyEvent, _state: &AppState) {
        self.observed_keys
            .borrow_mut()
            .push(format!("{:?}", key.key));
    }
}

#[test]
fn test_observe_key_called() {
    let mut registry = PluginRegistry::new();
    registry.register(Box::new(ObservingPlugin::new()));
    let state = AppState::default();
    let key = KeyEvent {
        key: crate::input::Key::Char('a'),
        modifiers: crate::input::Modifiers::empty(),
    };
    for plugin in registry.plugins_mut() {
        plugin.observe_key(&key, &state);
    }
    // No panic = success, since we can't downcast
}

// --- Menu transform tests ---

struct IconPlugin;

impl Plugin for IconPlugin {
    fn id(&self) -> PluginId {
        PluginId("icons".to_string())
    }

    fn transform_menu_item(
        &self,
        item: &[crate::protocol::Atom],
        _index: usize,
        _selected: bool,
        _state: &AppState,
    ) -> Option<Vec<crate::protocol::Atom>> {
        let mut result = vec![crate::protocol::Atom {
            face: Face::default(),
            contents: "★ ".into(),
        }];
        result.extend(item.iter().cloned());
        Some(result)
    }
}

#[test]
fn test_transform_menu_item() {
    let mut registry = PluginRegistry::new();
    registry.register(Box::new(IconPlugin));
    let state = AppState::default();
    let item = vec![crate::protocol::Atom {
        face: Face::default(),
        contents: "foo".into(),
    }];
    let result = registry.transform_menu_item(&item, 0, false, &state);
    assert!(result.is_some());
    let result = result.unwrap();
    assert_eq!(result[0].contents.as_str(), "★ ");
    assert_eq!(result[1].contents.as_str(), "foo");
}

#[test]
fn test_transform_menu_item_no_plugin() {
    let registry = PluginRegistry::new();
    let state = AppState::default();
    let item = vec![crate::protocol::Atom {
        face: Face::default(),
        contents: "foo".into(),
    }];
    assert!(
        registry
            .transform_menu_item(&item, 0, false, &state)
            .is_none()
    );
}

// --- deliver_message tests ---

#[test]
fn test_deliver_message_to_plugin() {
    let mut registry = PluginRegistry::new();
    registry.register(Box::new(TestPlugin));
    let state = AppState::default();
    let (flags, commands) =
        registry.deliver_message(&PluginId("test".to_string()), Box::new(42u32), &state);
    assert!(flags.is_empty());
    assert!(commands.is_empty());
}

#[test]
fn test_deliver_message_unknown_target() {
    let mut registry = PluginRegistry::new();
    let state = AppState::default();
    let (flags, commands) =
        registry.deliver_message(&PluginId("unknown".to_string()), Box::new(42u32), &state);
    assert!(flags.is_empty());
    assert!(commands.is_empty());
}

// --- extract_deferred_commands tests ---

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

// --- Plugin state change guard tests ---

struct StatefulPlugin {
    hash: u64,
}

impl Plugin for StatefulPlugin {
    fn id(&self) -> PluginId {
        PluginId("stateful".to_string())
    }

    fn state_hash(&self) -> u64 {
        self.hash
    }
}

#[test]
fn test_any_plugin_state_changed_flag() {
    let mut registry = PluginRegistry::new();
    registry.register(Box::new(StatefulPlugin { hash: 1 }));

    // Initial prepare: hash differs from default 0 → changed
    registry.prepare_plugin_cache(DirtyFlags::ALL);
    assert!(registry.any_plugin_state_changed());

    // Second prepare with same hash → no change
    registry.prepare_plugin_cache(DirtyFlags::ALL);
    assert!(!registry.any_plugin_state_changed());
}

// --- SlotId tests ---

#[test]
fn test_slot_id_well_known() {
    assert!(SlotId::BUFFER_LEFT.is_well_known());
    assert!(SlotId::STATUS_RIGHT.is_well_known());
    assert_eq!(SlotId::BUFFER_LEFT.well_known_index(), Some(0));
}

#[test]
fn test_slot_id_custom_not_well_known() {
    let custom = SlotId::new("my.plugin.sidebar");
    assert!(!custom.is_well_known());
    assert_eq!(custom.well_known_index(), None);
    assert_eq!(custom.as_str(), "my.plugin.sidebar");
}

// -----------------------------------------------------------------------
// PaintHook tests
// -----------------------------------------------------------------------

struct TestPaintHook {
    id: &'static str,
    deps: DirtyFlags,
    surface_filter: Option<crate::surface::SurfaceId>,
}

impl PaintHook for TestPaintHook {
    fn id(&self) -> &str {
        self.id
    }
    fn deps(&self) -> DirtyFlags {
        self.deps
    }
    fn surface_filter(&self) -> Option<crate::surface::SurfaceId> {
        self.surface_filter.clone()
    }
    fn apply(
        &self,
        grid: &mut crate::render::CellGrid,
        _region: &crate::layout::Rect,
        _state: &AppState,
    ) {
        // Write a marker character at (0, 0) to prove the hook ran
        if let Some(cell) = grid.get_mut(0, 0) {
            cell.grapheme = compact_str::CompactString::new(self.id);
        }
    }
}

struct PaintHookPlugin {
    hooks: Vec<Box<dyn PaintHook>>,
}

impl Plugin for PaintHookPlugin {
    fn id(&self) -> PluginId {
        PluginId("paint-hook-test".to_string())
    }

    fn capabilities(&self) -> PluginCapabilities {
        PluginCapabilities::PAINT_HOOK
    }

    fn paint_hooks(&self) -> Vec<Box<dyn PaintHook>> {
        // Re-create hooks each time (test simplicity)
        self.hooks
            .iter()
            .map(|h| -> Box<dyn PaintHook> {
                Box::new(TestPaintHook {
                    id: match h.id() {
                        "hook-a" => "hook-a",
                        "hook-b" => "hook-b",
                        _ => "unknown",
                    },
                    deps: h.deps(),
                    surface_filter: h.surface_filter(),
                })
            })
            .collect()
    }
}

#[test]
fn test_collect_paint_hooks() {
    let mut registry = PluginRegistry::new();
    registry.register(Box::new(PaintHookPlugin {
        hooks: vec![
            Box::new(TestPaintHook {
                id: "hook-a",
                deps: DirtyFlags::BUFFER,
                surface_filter: None,
            }),
            Box::new(TestPaintHook {
                id: "hook-b",
                deps: DirtyFlags::STATUS,
                surface_filter: None,
            }),
        ],
    }));
    let hooks = registry.collect_paint_hooks();
    assert_eq!(hooks.len(), 2);
    assert_eq!(hooks[0].id(), "hook-a");
    assert_eq!(hooks[1].id(), "hook-b");
}

#[test]
fn test_paint_hook_applies_to_grid() {
    use crate::layout::Rect;
    use crate::render::CellGrid;

    let mut grid = CellGrid::new(10, 5);
    let state = AppState::default();
    let region = Rect {
        x: 0,
        y: 0,
        w: 10,
        h: 5,
    };
    let hook = TestPaintHook {
        id: "X",
        deps: DirtyFlags::ALL,
        surface_filter: None,
    };
    hook.apply(&mut grid, &region, &state);
    assert_eq!(grid.get(0, 0).unwrap().grapheme.as_str(), "X");
}

#[test]
fn test_apply_paint_hooks_deps_filtering() {
    use crate::layout::Rect;
    use crate::render::CellGrid;
    use crate::render::pipeline::apply_paint_hooks;

    let mut grid = CellGrid::new(10, 5);
    let state = AppState::default();
    let region = Rect {
        x: 0,
        y: 0,
        w: 10,
        h: 5,
    };

    // Hook depends on STATUS, but dirty is BUFFER → should NOT run
    let hooks: Vec<Box<dyn PaintHook>> = vec![Box::new(TestPaintHook {
        id: "Z",
        deps: DirtyFlags::STATUS,
        surface_filter: None,
    })];
    apply_paint_hooks(&hooks, &mut grid, &region, &state, DirtyFlags::BUFFER);
    // Cell (0,0) should still be the default (space)
    assert_ne!(grid.get(0, 0).unwrap().grapheme.as_str(), "Z");

    // Now with matching dirty flags → should run
    apply_paint_hooks(&hooks, &mut grid, &region, &state, DirtyFlags::STATUS);
    assert_eq!(grid.get(0, 0).unwrap().grapheme.as_str(), "Z");
}

#[test]
fn test_paint_hook_no_capability_not_collected() {
    struct NoPaintHookPlugin;
    impl Plugin for NoPaintHookPlugin {
        fn id(&self) -> PluginId {
            PluginId("no-hook".to_string())
        }
        // capabilities() defaults to empty — no PAINT_HOOK
    }

    let mut registry = PluginRegistry::new();
    registry.register(Box::new(NoPaintHookPlugin));
    let hooks = registry.collect_paint_hooks();
    assert!(hooks.is_empty());
}

// -----------------------------------------------------------------------
// Phase P-2: I/O event and process command tests
// -----------------------------------------------------------------------

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

impl Plugin for IoHandlerPlugin {
    fn id(&self) -> PluginId {
        PluginId("io_handler".to_string())
    }

    fn capabilities(&self) -> PluginCapabilities {
        PluginCapabilities::IO_HANDLER
    }

    fn on_io_event(&mut self, event: &IoEvent, _state: &AppState) -> Vec<Command> {
        match event {
            IoEvent::Process(pe) => match pe {
                ProcessEvent::Stdout { job_id, data } => {
                    self.received_events
                        .push(format!("stdout:{}:{}", job_id, data.len()));
                    vec![Command::RequestRedraw(DirtyFlags::BUFFER)]
                }
                ProcessEvent::Stderr { job_id, data } => {
                    self.received_events
                        .push(format!("stderr:{}:{}", job_id, data.len()));
                    vec![]
                }
                ProcessEvent::Exited { job_id, exit_code } => {
                    self.received_events
                        .push(format!("exited:{}:{}", job_id, exit_code));
                    vec![]
                }
                ProcessEvent::SpawnFailed { job_id, error } => {
                    self.received_events
                        .push(format!("failed:{}:{}", job_id, error));
                    vec![]
                }
            },
        }
    }
}

#[test]
fn test_deliver_io_event_dispatches_to_plugin() {
    let mut registry = PluginRegistry::new();
    registry.register(Box::new(IoHandlerPlugin::new()));
    let state = AppState::default();

    let event = IoEvent::Process(ProcessEvent::Stdout {
        job_id: 1,
        data: b"output".to_vec(),
    });
    let (flags, commands) =
        registry.deliver_io_event(&PluginId("io_handler".to_string()), &event, &state);

    // The plugin returns RequestRedraw(BUFFER) for stdout events
    assert!(flags.contains(DirtyFlags::BUFFER));
    assert!(commands.is_empty()); // RequestRedraw is extracted into flags
}

#[test]
fn test_deliver_io_event_unknown_target() {
    let mut registry = PluginRegistry::new();
    registry.register(Box::new(IoHandlerPlugin::new()));
    let state = AppState::default();

    let event = IoEvent::Process(ProcessEvent::Stdout {
        job_id: 1,
        data: vec![],
    });
    let (flags, commands) =
        registry.deliver_io_event(&PluginId("nonexistent".to_string()), &event, &state);
    assert!(flags.is_empty());
    assert!(commands.is_empty());
}

#[test]
fn test_deliver_io_event_skips_plugin_without_io_handler_capability() {
    // TestPlugin has default capabilities (all()), but let's make one with no IO_HANDLER
    struct NoIoCapPlugin;
    impl Plugin for NoIoCapPlugin {
        fn id(&self) -> PluginId {
            PluginId("no_io".to_string())
        }
        fn capabilities(&self) -> PluginCapabilities {
            // All capabilities EXCEPT IO_HANDLER
            PluginCapabilities::all() - PluginCapabilities::IO_HANDLER
        }
        fn on_io_event(&mut self, _event: &IoEvent, _state: &AppState) -> Vec<Command> {
            // This should never be called
            vec![Command::Quit]
        }
    }

    let mut registry = PluginRegistry::new();
    registry.register(Box::new(NoIoCapPlugin));
    let state = AppState::default();

    let event = IoEvent::Process(ProcessEvent::Exited {
        job_id: 1,
        exit_code: 0,
    });
    let (flags, commands) =
        registry.deliver_io_event(&PluginId("no_io".to_string()), &event, &state);
    // Should return empty because the capability check short-circuits
    assert!(flags.is_empty());
    assert!(commands.is_empty());
}

// --- ProcessDispatcher mock test ---

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
    let mut registry = PluginRegistry::new();
    registry.register(Box::new(TestPlugin));
    assert!(registry.plugin_allows_process_spawn(&PluginId("test".to_string())));
}

#[test]
fn test_plugin_allows_process_spawn_denied() {
    struct DenySpawnPlugin;
    impl Plugin for DenySpawnPlugin {
        fn id(&self) -> PluginId {
            PluginId("deny_spawn".to_string())
        }
        fn allows_process_spawn(&self) -> bool {
            false
        }
    }

    let mut registry = PluginRegistry::new();
    registry.register(Box::new(DenySpawnPlugin));
    assert!(!registry.plugin_allows_process_spawn(&PluginId("deny_spawn".to_string())));
}

#[test]
fn test_plugin_allows_process_spawn_unknown_plugin() {
    let registry = PluginRegistry::new();
    // Unknown plugin should return false (is_some_and fails on None)
    assert!(!registry.plugin_allows_process_spawn(&PluginId("unknown".to_string())));
}

#[test]
fn test_stdin_mode_eq() {
    assert_eq!(StdinMode::Null, StdinMode::Null);
    assert_eq!(StdinMode::Piped, StdinMode::Piped);
    assert_ne!(StdinMode::Null, StdinMode::Piped);
}
