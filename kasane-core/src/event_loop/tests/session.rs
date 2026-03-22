use super::*;

use crate::plugin::PluginRuntime;
use crate::surface::{SurfaceId, SurfaceRegistry};

use super::super::context::DeferredContext;
use super::super::dispatch::handle_deferred_commands;
use super::super::session::{SessionMutContext, SessionReadyGate, handle_pane_death};
use super::super::surface::register_builtin_surfaces;

#[derive(Default)]
struct RecordingSessionHost {
    writer: Vec<u8>,
    spawned: Vec<(SessionSpec, bool)>,
    closed: Vec<Option<String>>,
    switched: Vec<String>,
    close_returns_quit: bool,
}

impl super::super::SessionRuntime for RecordingSessionHost {
    fn spawn_session(
        &mut self,
        spec: SessionSpec,
        activate: bool,
        _state: &mut AppState,
        _dirty: &mut DirtyFlags,
        _initial_resize_sent: &mut bool,
    ) {
        self.spawned.push((spec, activate));
    }

    fn close_session(
        &mut self,
        key: Option<&str>,
        _state: &mut AppState,
        _dirty: &mut DirtyFlags,
        _initial_resize_sent: &mut bool,
    ) -> bool {
        self.closed.push(key.map(ToOwned::to_owned));
        self.close_returns_quit
    }

    fn switch_session(
        &mut self,
        key: &str,
        _state: &mut AppState,
        _dirty: &mut DirtyFlags,
        _initial_resize_sent: &mut bool,
    ) {
        self.switched.push(key.to_owned());
    }
}

impl super::super::SessionHost for RecordingSessionHost {
    fn active_writer(&mut self) -> &mut dyn Write {
        &mut self.writer
    }
}

#[test]
fn deferred_session_spawn_is_routed_to_session_host() {
    let mut state = AppState::default();
    let mut registry = PluginRuntime::new();
    let mut surface_registry = SurfaceRegistry::new();

    let mut dirty = DirtyFlags::empty();
    let timer = NoopTimer;
    let mut sessions = RecordingSessionHost::default();
    let mut initial_resize_sent = true;
    let mut dispatcher = RecordingDispatcher::default();
    let mut workspace_changed = false;

    let quit = handle_deferred_commands(
        vec![Command::Session(crate::session::SessionCommand::Spawn {
            key: Some("work".to_string()),
            session: Some("project".to_string()),
            args: vec!["file.txt".to_string()],
            activate: true,
        })],
        &mut DeferredContext {
            state: &mut state,
            registry: &mut registry,
            surface_registry: &mut surface_registry,

            clipboard: &mut crate::clipboard::SystemClipboard::noop(),
            dirty: &mut dirty,
            timer: &timer,
            session_host: &mut sessions,
            initial_resize_sent: &mut initial_resize_sent,
            session_ready_gate: None,
            scroll_plan_sink: &mut |_| {},
            process_dispatcher: &mut dispatcher,
            workspace_changed: &mut workspace_changed,
            scroll_amount: 3,
        },
        None,
    );

    assert!(!quit);
    assert_eq!(sessions.spawned.len(), 1);
    assert_eq!(sessions.spawned[0].0.key, "work");
    assert_eq!(sessions.spawned[0].0.session.as_deref(), Some("project"));
    assert_eq!(sessions.spawned[0].0.args, vec!["file.txt".to_string()]);
    assert!(sessions.spawned[0].1);
}

#[test]
fn deferred_session_close_is_routed_to_session_host() {
    let mut state = AppState::default();
    let mut registry = PluginRuntime::new();
    let mut surface_registry = SurfaceRegistry::new();

    let mut dirty = DirtyFlags::empty();
    let timer = NoopTimer;
    let mut sessions = RecordingSessionHost::default();
    let mut initial_resize_sent = false;
    let mut dispatcher = RecordingDispatcher::default();
    let mut workspace_changed = false;

    let quit = handle_deferred_commands(
        vec![Command::Session(crate::session::SessionCommand::Close {
            key: Some("work".to_string()),
        })],
        &mut DeferredContext {
            state: &mut state,
            registry: &mut registry,
            surface_registry: &mut surface_registry,

            clipboard: &mut crate::clipboard::SystemClipboard::noop(),
            dirty: &mut dirty,
            timer: &timer,
            session_host: &mut sessions,
            initial_resize_sent: &mut initial_resize_sent,
            session_ready_gate: None,
            scroll_plan_sink: &mut |_| {},
            process_dispatcher: &mut dispatcher,
            workspace_changed: &mut workspace_changed,
            scroll_amount: 3,
        },
        None,
    );

    assert!(!quit);
    assert_eq!(sessions.closed, vec![Some("work".to_string())]);
}

#[test]
fn deferred_session_close_returns_quit_when_host_requests_shutdown() {
    let mut state = AppState::default();
    let mut registry = PluginRuntime::new();
    let mut surface_registry = SurfaceRegistry::new();

    let mut dirty = DirtyFlags::empty();
    let timer = NoopTimer;
    let mut sessions = RecordingSessionHost {
        close_returns_quit: true,
        ..RecordingSessionHost::default()
    };
    let mut initial_resize_sent = false;
    let mut dispatcher = RecordingDispatcher::default();
    let mut workspace_changed = false;

    let quit = handle_deferred_commands(
        vec![Command::Session(crate::session::SessionCommand::Close {
            key: None,
        })],
        &mut DeferredContext {
            state: &mut state,
            registry: &mut registry,
            surface_registry: &mut surface_registry,

            clipboard: &mut crate::clipboard::SystemClipboard::noop(),
            dirty: &mut dirty,
            timer: &timer,
            session_host: &mut sessions,
            initial_resize_sent: &mut initial_resize_sent,
            session_ready_gate: None,
            scroll_plan_sink: &mut |_| {},
            process_dispatcher: &mut dispatcher,
            workspace_changed: &mut workspace_changed,
            scroll_amount: 3,
        },
        None,
    );

    assert!(quit);
    assert_eq!(sessions.closed, vec![None]);
}

#[test]
fn deferred_session_switch_is_routed() {
    let mut state = AppState::default();
    let mut registry = PluginRuntime::new();
    let mut surface_registry = SurfaceRegistry::new();

    let mut dirty = DirtyFlags::empty();
    let timer = NoopTimer;
    let mut sessions = RecordingSessionHost::default();
    let mut initial_resize_sent = false;
    let mut dispatcher = RecordingDispatcher::default();
    let mut workspace_changed = false;

    let quit = handle_deferred_commands(
        vec![Command::Session(crate::session::SessionCommand::Switch {
            key: "work".to_string(),
        })],
        &mut DeferredContext {
            state: &mut state,
            registry: &mut registry,
            surface_registry: &mut surface_registry,

            clipboard: &mut crate::clipboard::SystemClipboard::noop(),
            dirty: &mut dirty,
            timer: &timer,
            session_host: &mut sessions,
            initial_resize_sent: &mut initial_resize_sent,
            session_ready_gate: None,
            scroll_plan_sink: &mut |_| {},
            process_dispatcher: &mut dispatcher,
            workspace_changed: &mut workspace_changed,
            scroll_amount: 3,
        },
        None,
    );

    assert!(!quit);
    assert_eq!(sessions.switched, vec!["work".to_string()]);
}

#[test]
fn session_ready_gate_requires_active_session_and_initial_resize() {
    let mut gate = SessionReadyGate::default();
    assert!(!gate.should_notify_ready());

    gate.sync_active_session(Some("alpha"));
    assert!(!gate.should_notify_ready());

    gate.mark_initial_resize_sent();
    assert!(gate.should_notify_ready());
    gate.mark_ready_notified();
    assert!(!gate.should_notify_ready());
}

#[test]
fn session_ready_gate_rearms_on_session_switch() {
    let mut gate = SessionReadyGate::default();
    gate.sync_active_session(Some("alpha"));
    gate.mark_initial_resize_sent();
    gate.mark_ready_notified();
    assert!(!gate.should_notify_ready());

    gate.sync_active_session(Some("beta"));
    assert!(!gate.should_notify_ready());
    gate.mark_initial_resize_sent();
    assert!(gate.should_notify_ready());
}

#[test]
fn session_ready_gate_can_rearm_current_generation() {
    let mut gate = SessionReadyGate::default();
    gate.sync_active_session(Some("alpha"));
    gate.mark_initial_resize_sent();
    gate.mark_ready_notified();
    assert!(!gate.should_notify_ready());

    gate.rearm_ready_notification();
    assert!(gate.should_notify_ready());
}

// ── handle_pane_death tests ──────────────────────────────────────

type TestSessionManager = crate::session::SessionManager<(), Vec<u8>, ()>;

fn setup_pane_death_env() -> (
    AppState,
    TestSessionManager,
    crate::session::SessionStateStore,
    SurfaceRegistry,
) {
    let state = AppState::default();
    let mgr = TestSessionManager::new();
    let store = crate::session::SessionStateStore::new();
    let mut sr = SurfaceRegistry::new();
    register_builtin_surfaces(&mut sr);
    (state, mgr, store, sr)
}

#[test]
fn test_buffer_pane_death_with_remaining_pane() {
    let (mut state, mut mgr, mut store, mut sr) = setup_pane_death_env();
    let mut dirty = DirtyFlags::empty();
    let mut initial_resize_sent = true;

    // Create primary (BUFFER) session
    let primary = mgr
        .insert(
            crate::session::SessionSpec::new("primary", None, vec![]),
            (),
            vec![],
            (),
        )
        .unwrap();
    store.ensure_session(primary, &state);
    sr.bind_session(SurfaceId::BUFFER, primary);

    // Create secondary pane session
    let pane_surface = SurfaceId(100);
    sr.register(TestSurfaceBuilder::new(pane_surface).build());
    sr.workspace_mut().root_mut().split(
        SurfaceId::BUFFER,
        crate::layout::SplitDirection::Vertical,
        0.5,
        pane_surface,
    );
    let pane = mgr
        .insert(
            crate::session::SessionSpec::new("pane", None, vec![]),
            (),
            vec![],
            (),
        )
        .unwrap();
    store.ensure_session(pane, &state);
    sr.bind_session(pane_surface, pane);

    // Kill primary (BUFFER) session
    let quit = handle_pane_death(
        primary,
        &mut sr,
        &mut SessionMutContext {
            session_manager: &mut mgr,
            session_states: &mut store,
            state: &mut state,
            dirty: &mut dirty,
            initial_resize_sent: &mut initial_resize_sent,
        },
    );

    assert!(!quit);
    // BUFFER surface stays in registry but is removed from workspace
    assert!(sr.get(SurfaceId::BUFFER).is_some());
    assert!(
        !sr.workspace()
            .root()
            .collect_ids()
            .contains(&SurfaceId::BUFFER)
    );
    // Pane surface remains in both
    assert!(sr.get(pane_surface).is_some());
    assert!(sr.workspace().root().collect_ids().contains(&pane_surface));
    // Pane session promoted to active
    assert_eq!(mgr.active_session_id(), Some(pane));
}

#[test]
fn test_secondary_pane_death() {
    let (mut state, mut mgr, mut store, mut sr) = setup_pane_death_env();
    let mut dirty = DirtyFlags::empty();
    let mut initial_resize_sent = true;

    // Create primary session
    let primary = mgr
        .insert(
            crate::session::SessionSpec::new("primary", None, vec![]),
            (),
            vec![],
            (),
        )
        .unwrap();
    store.ensure_session(primary, &state);
    sr.bind_session(SurfaceId::BUFFER, primary);

    // Create secondary pane
    let pane_surface = SurfaceId(100);
    sr.register(TestSurfaceBuilder::new(pane_surface).build());
    sr.workspace_mut().root_mut().split(
        SurfaceId::BUFFER,
        crate::layout::SplitDirection::Vertical,
        0.5,
        pane_surface,
    );
    let pane = mgr
        .insert(
            crate::session::SessionSpec::new("pane", None, vec![]),
            (),
            vec![],
            (),
        )
        .unwrap();
    store.ensure_session(pane, &state);
    sr.bind_session(pane_surface, pane);

    // Kill secondary pane
    let quit = handle_pane_death(
        pane,
        &mut sr,
        &mut SessionMutContext {
            session_manager: &mut mgr,
            session_states: &mut store,
            state: &mut state,
            dirty: &mut dirty,
            initial_resize_sent: &mut initial_resize_sent,
        },
    );

    assert!(!quit);
    // Pane surface removed from both registry and workspace
    assert!(sr.get(pane_surface).is_none());
    assert!(!sr.workspace().root().collect_ids().contains(&pane_surface));
    // BUFFER stays
    assert!(sr.get(SurfaceId::BUFFER).is_some());
    assert_eq!(mgr.active_session_id(), Some(primary));
}

#[test]
fn test_last_session_death_quits() {
    let (mut state, mut mgr, mut store, mut sr) = setup_pane_death_env();
    let mut dirty = DirtyFlags::empty();
    let mut initial_resize_sent = true;

    let only = mgr
        .insert(
            crate::session::SessionSpec::new("only", None, vec![]),
            (),
            vec![],
            (),
        )
        .unwrap();
    store.ensure_session(only, &state);
    sr.bind_session(SurfaceId::BUFFER, only);

    let quit = handle_pane_death(
        only,
        &mut sr,
        &mut SessionMutContext {
            session_manager: &mut mgr,
            session_states: &mut store,
            state: &mut state,
            dirty: &mut dirty,
            initial_resize_sent: &mut initial_resize_sent,
        },
    );

    assert!(quit);
}

#[test]
fn test_idempotent_double_death() {
    let (mut state, mut mgr, mut store, mut sr) = setup_pane_death_env();
    let mut dirty = DirtyFlags::empty();
    let mut initial_resize_sent = true;

    // Two sessions so first death doesn't quit
    let s1 = mgr
        .insert(
            crate::session::SessionSpec::new("s1", None, vec![]),
            (),
            vec![],
            (),
        )
        .unwrap();
    store.ensure_session(s1, &state);
    sr.bind_session(SurfaceId::BUFFER, s1);

    let pane_surface = SurfaceId(100);
    sr.register(TestSurfaceBuilder::new(pane_surface).build());
    sr.workspace_mut().root_mut().split(
        SurfaceId::BUFFER,
        crate::layout::SplitDirection::Vertical,
        0.5,
        pane_surface,
    );
    let s2 = mgr
        .insert(
            crate::session::SessionSpec::new("s2", None, vec![]),
            (),
            vec![],
            (),
        )
        .unwrap();
    store.ensure_session(s2, &state);
    sr.bind_session(pane_surface, s2);

    // First death
    let quit1 = handle_pane_death(
        s2,
        &mut sr,
        &mut SessionMutContext {
            session_manager: &mut mgr,
            session_states: &mut store,
            state: &mut state,
            dirty: &mut dirty,
            initial_resize_sent: &mut initial_resize_sent,
        },
    );
    assert!(!quit1);

    // Second death of same session — idempotent no-op
    dirty = DirtyFlags::empty();
    let quit2 = handle_pane_death(
        s2,
        &mut sr,
        &mut SessionMutContext {
            session_manager: &mut mgr,
            session_states: &mut store,
            state: &mut state,
            dirty: &mut dirty,
            initial_resize_sent: &mut initial_resize_sent,
        },
    );
    assert!(!quit2);
}
