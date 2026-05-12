use super::*;

use crate::plugin::PluginRuntime;
use crate::surface::{SourcedSurfaceCommands, SurfaceId, SurfaceRegistry};
use crate::workspace::Placement;

use super::super::context::DeferredContext;
use super::super::dispatch::{
    handle_deferred_commands, handle_deferred_commands_inner, handle_sourced_surface_commands,
};
use super::super::surface::register_builtin_surfaces;

#[test]
fn sourced_surface_commands_preserve_plugin_for_spawn_process() {
    let plugin_id = PluginId("surface-owner".to_string());
    let mut registry = PluginRuntime::new();
    registry.register(TestPlugin {
        id: plugin_id.clone(),
        allow_spawn: true,
        authorities: PluginAuthorities::empty(),
    });

    let mut state = AppState::default();
    let mut surface_registry = SurfaceRegistry::new();

    let mut dirty = DirtyFlags::empty();
    let timer = NoopTimer;
    let mut sessions = NoopSessionRuntime::default();
    let mut initial_resize_sent = false;
    let mut dispatcher = RecordingDispatcher::default();
    let mut workspace_changed = false;

    let quit = handle_sourced_surface_commands(
        vec![SourcedSurfaceCommands {
            source_plugin: Some(plugin_id.clone()),
            commands: vec![Command::SpawnProcess {
                job_id: 42,
                program: "fd".to_string(),
                args: vec!["foo".to_string()],
                stdin_mode: StdinMode::Null,
            }],
        }],
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
            http_dispatcher: &mut crate::plugin::NullHttpDispatcher,
            workspace_changed: &mut workspace_changed,
            scroll_amount: 3,
        },
    );

    assert!(!quit);
    assert_eq!(dispatcher.spawned.len(), 1);
    assert_eq!(dispatcher.spawned[0].0, plugin_id);
    assert_eq!(dispatcher.spawned[0].1, 42);
    assert_eq!(dispatcher.spawned[0].2, "fd");
    assert_eq!(dispatcher.spawned[0].3, vec!["foo".to_string()]);
    assert_eq!(dispatcher.spawned[0].4, StdinMode::Null);
}

#[test]
fn plugin_message_runtime_effects_update_dirty_and_enqueue_scroll_plans() {
    let mut state = AppState::default();
    let mut registry = PluginRuntime::new();
    registry.register(RuntimeMessagePlugin);
    let mut surface_registry = SurfaceRegistry::new();

    let mut dirty = DirtyFlags::empty();
    let timer = NoopTimer;
    let mut sessions = NoopSessionRuntime::default();
    let mut initial_resize_sent = true;
    let mut dispatcher = RecordingDispatcher::default();
    let mut plans = Vec::new();
    let mut workspace_changed = false;

    let quit = handle_deferred_commands(
        vec![Command::PluginMessage {
            target: PluginId("runtime-message".to_string()),
            payload: Box::new(11u32),
        }],
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
            scroll_plan_sink: &mut |plan| plans.push(plan),
            process_dispatcher: &mut dispatcher,
            http_dispatcher: &mut crate::plugin::NullHttpDispatcher,
            workspace_changed: &mut workspace_changed,
            scroll_amount: 3,
        },
        None,
    );

    assert!(!quit);
    assert!(dirty.contains(DirtyFlags::INFO));
    assert!(dirty.contains(DirtyFlags::STATUS));
    assert_eq!(plans.len(), 1);
    assert_eq!(plans[0].total_amount, 2);
}

#[test]
fn register_surface_requires_dynamic_surface_authority() {
    let plugin_id = PluginId("surface-owner".to_string());
    let mut state = AppState::default();
    let mut registry = PluginRuntime::new();
    registry.register(TestPlugin {
        id: plugin_id.clone(),
        allow_spawn: false,
        authorities: PluginAuthorities::empty(),
    });
    let mut surface_registry = SurfaceRegistry::new();
    register_builtin_surfaces(&mut surface_registry);

    let mut dirty = DirtyFlags::empty();
    let timer = NoopTimer;
    let mut sessions = NoopSessionRuntime::default();
    let mut initial_resize_sent = false;
    let mut dispatcher = RecordingDispatcher::default();
    let mut workspace_changed = false;

    let quit = handle_deferred_commands(
        vec![Command::RegisterSurface {
            surface: TestSurfaceBuilder::new(SurfaceId(300))
                .key("dynamic.surface")
                .build(),
            placement: Placement::SplitFocused {
                direction: SplitDirection::Vertical,
                ratio: 0.5,
            },
        }],
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
            http_dispatcher: &mut crate::plugin::NullHttpDispatcher,
            workspace_changed: &mut workspace_changed,
            scroll_amount: 3,
        },
        Some(&plugin_id),
    );

    assert!(!quit);
    assert!(surface_registry.get(SurfaceId(300)).is_none());
    assert!(!surface_registry.workspace_contains(SurfaceId(300)));
}

#[test]
fn register_surface_adds_plugin_owned_surface_to_workspace() {
    let plugin_id = PluginId("surface-owner".to_string());
    let mut state = AppState::default();
    let mut registry = PluginRuntime::new();
    registry.register(TestPlugin {
        id: plugin_id.clone(),
        allow_spawn: false,
        authorities: PluginAuthorities::DYNAMIC_SURFACE,
    });
    let mut surface_registry = SurfaceRegistry::new();
    register_builtin_surfaces(&mut surface_registry);

    let mut dirty = DirtyFlags::empty();
    let timer = NoopTimer;
    let mut sessions = NoopSessionRuntime::default();
    let mut initial_resize_sent = false;
    let mut dispatcher = RecordingDispatcher::default();
    let mut workspace_changed = false;

    let quit = handle_deferred_commands(
        vec![Command::RegisterSurface {
            surface: TestSurfaceBuilder::new(SurfaceId(301))
                .key("dynamic.surface.authorized")
                .build(),
            placement: Placement::SplitFocused {
                direction: SplitDirection::Vertical,
                ratio: 0.5,
            },
        }],
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
            http_dispatcher: &mut crate::plugin::NullHttpDispatcher,
            workspace_changed: &mut workspace_changed,
            scroll_amount: 3,
        },
        Some(&plugin_id),
    );

    assert!(!quit);
    assert!(surface_registry.get(SurfaceId(301)).is_some());
    assert_eq!(
        surface_registry.surface_owner_plugin(SurfaceId(301)),
        Some(&plugin_id)
    );
    assert!(surface_registry.workspace_contains(SurfaceId(301)));
    assert!(dirty.contains(DirtyFlags::ALL));
}

#[test]
fn register_surface_requested_resolves_keyed_placement() {
    let plugin_id = PluginId("surface-owner".to_string());
    let mut state = AppState::default();
    let mut registry = PluginRuntime::new();
    registry.register(TestPlugin {
        id: plugin_id.clone(),
        allow_spawn: false,
        authorities: PluginAuthorities::DYNAMIC_SURFACE,
    });
    let mut surface_registry = SurfaceRegistry::new();
    register_builtin_surfaces(&mut surface_registry);

    let mut dirty = DirtyFlags::empty();
    let timer = NoopTimer;
    let mut sessions = NoopSessionRuntime::default();
    let mut initial_resize_sent = false;
    let mut dispatcher = RecordingDispatcher::default();
    let mut workspace_changed = false;

    let quit = handle_deferred_commands(
        vec![Command::RegisterSurfaceRequested {
            surface: TestSurfaceBuilder::new(SurfaceId(304))
                .key("dynamic.surface.requested")
                .build(),
            placement: SurfacePlacementRequest::TabIn {
                target_surface_key: "kasane.buffer".into(),
            },
        }],
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
            http_dispatcher: &mut crate::plugin::NullHttpDispatcher,
            workspace_changed: &mut workspace_changed,
            scroll_amount: 3,
        },
        Some(&plugin_id),
    );

    assert!(!quit);
    assert!(surface_registry.get(SurfaceId(304)).is_some());
    assert_eq!(
        surface_registry.surface_owner_plugin(SurfaceId(304)),
        Some(&plugin_id)
    );
    assert!(surface_registry.workspace_contains(SurfaceId(304)));
    assert!(dirty.contains(DirtyFlags::ALL));
}

#[test]
fn unregister_surface_rejects_non_owner_even_with_authority() {
    let owner_id = PluginId("surface-owner".to_string());
    let other_id = PluginId("other-owner".to_string());
    let mut state = AppState::default();
    let mut registry = PluginRuntime::new();
    registry.register(TestPlugin {
        id: owner_id.clone(),
        allow_spawn: false,
        authorities: PluginAuthorities::DYNAMIC_SURFACE,
    });
    registry.register(TestPlugin {
        id: other_id.clone(),
        allow_spawn: false,
        authorities: PluginAuthorities::DYNAMIC_SURFACE,
    });
    let mut surface_registry = SurfaceRegistry::new();
    register_builtin_surfaces(&mut surface_registry);
    surface_registry
        .try_register_for_owner(
            TestSurfaceBuilder::new(SurfaceId(302))
                .key("dynamic.surface.owned")
                .build(),
            Some(owner_id.clone()),
        )
        .unwrap();
    let mut bootstrap_dirty = DirtyFlags::empty();
    crate::workspace::dispatch_workspace_command_with_total(
        &mut surface_registry,
        crate::workspace::WorkspaceCommand::AddSurface {
            surface_id: SurfaceId(302),
            placement: Placement::SplitFocused {
                direction: SplitDirection::Vertical,
                ratio: 0.5,
            },
        },
        &mut bootstrap_dirty,
        Some(crate::layout::Rect {
            x: 0,
            y: 0,
            w: state.runtime.cols,
            h: state.runtime.rows,
        }),
    );

    let mut dirty = DirtyFlags::empty();
    let timer = NoopTimer;
    let mut sessions = NoopSessionRuntime::default();
    let mut initial_resize_sent = false;
    let mut dispatcher = RecordingDispatcher::default();
    let mut workspace_changed = false;

    let quit = handle_deferred_commands(
        vec![Command::UnregisterSurface {
            surface_id: SurfaceId(302),
        }],
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
            http_dispatcher: &mut crate::plugin::NullHttpDispatcher,
            workspace_changed: &mut workspace_changed,
            scroll_amount: 3,
        },
        Some(&other_id),
    );

    assert!(!quit);
    assert!(surface_registry.get(SurfaceId(302)).is_some());
    assert!(surface_registry.workspace_contains(SurfaceId(302)));
}

#[test]
fn unregister_surface_removes_owned_surface() {
    let plugin_id = PluginId("surface-owner".to_string());
    let mut state = AppState::default();
    let mut registry = PluginRuntime::new();
    registry.register(TestPlugin {
        id: plugin_id.clone(),
        allow_spawn: false,
        authorities: PluginAuthorities::DYNAMIC_SURFACE,
    });
    let mut surface_registry = SurfaceRegistry::new();
    register_builtin_surfaces(&mut surface_registry);
    surface_registry
        .try_register_for_owner(
            TestSurfaceBuilder::new(SurfaceId(303))
                .key("dynamic.surface.remove")
                .build(),
            Some(plugin_id.clone()),
        )
        .unwrap();
    let mut bootstrap_dirty = DirtyFlags::empty();
    crate::workspace::dispatch_workspace_command_with_total(
        &mut surface_registry,
        crate::workspace::WorkspaceCommand::AddSurface {
            surface_id: SurfaceId(303),
            placement: Placement::SplitFocused {
                direction: SplitDirection::Vertical,
                ratio: 0.5,
            },
        },
        &mut bootstrap_dirty,
        Some(crate::layout::Rect {
            x: 0,
            y: 0,
            w: state.runtime.cols,
            h: state.runtime.rows,
        }),
    );

    let mut dirty = DirtyFlags::empty();
    let timer = NoopTimer;
    let mut sessions = NoopSessionRuntime::default();
    let mut initial_resize_sent = false;
    let mut dispatcher = RecordingDispatcher::default();
    let mut workspace_changed = false;

    let quit = handle_deferred_commands(
        vec![Command::UnregisterSurface {
            surface_id: SurfaceId(303),
        }],
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
            http_dispatcher: &mut crate::plugin::NullHttpDispatcher,
            workspace_changed: &mut workspace_changed,
            scroll_amount: 3,
        },
        Some(&plugin_id),
    );

    assert!(!quit);
    assert!(surface_registry.get(SurfaceId(303)).is_none());
    assert!(!surface_registry.workspace_contains(SurfaceId(303)));
    assert!(dirty.contains(DirtyFlags::ALL));
}

#[test]
fn unregister_surface_key_removes_owned_surface() {
    let plugin_id = PluginId("surface-owner".to_string());
    let mut state = AppState::default();
    let mut registry = PluginRuntime::new();
    registry.register(TestPlugin {
        id: plugin_id.clone(),
        allow_spawn: false,
        authorities: PluginAuthorities::DYNAMIC_SURFACE,
    });
    let mut surface_registry = SurfaceRegistry::new();
    register_builtin_surfaces(&mut surface_registry);
    surface_registry
        .try_register_for_owner(
            TestSurfaceBuilder::new(SurfaceId(305))
                .key("dynamic.surface.remove.by.key")
                .build(),
            Some(plugin_id.clone()),
        )
        .unwrap();
    let mut bootstrap_dirty = DirtyFlags::empty();
    crate::workspace::dispatch_workspace_command_with_total(
        &mut surface_registry,
        crate::workspace::WorkspaceCommand::AddSurface {
            surface_id: SurfaceId(305),
            placement: Placement::SplitFocused {
                direction: SplitDirection::Vertical,
                ratio: 0.5,
            },
        },
        &mut bootstrap_dirty,
        Some(crate::layout::Rect {
            x: 0,
            y: 0,
            w: state.runtime.cols,
            h: state.runtime.rows,
        }),
    );

    let mut dirty = DirtyFlags::empty();
    let timer = NoopTimer;
    let mut sessions = NoopSessionRuntime::default();
    let mut initial_resize_sent = false;
    let mut dispatcher = RecordingDispatcher::default();
    let mut workspace_changed = false;

    let quit = handle_deferred_commands(
        vec![Command::UnregisterSurfaceKey {
            surface_key: "dynamic.surface.remove.by.key".into(),
        }],
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
            http_dispatcher: &mut crate::plugin::NullHttpDispatcher,
            workspace_changed: &mut workspace_changed,
            scroll_amount: 3,
        },
        Some(&plugin_id),
    );

    assert!(!quit);
    assert!(surface_registry.get(SurfaceId(305)).is_none());
    assert!(!surface_registry.workspace_contains(SurfaceId(305)));
    assert!(dirty.contains(DirtyFlags::ALL));
}

#[test]
fn pty_spawn_requires_pty_process_authority() {
    let plugin_id = PluginId("pty-plugin".to_string());
    let mut state = AppState::default();
    let mut registry = PluginRuntime::new();
    registry.register(TestPlugin {
        id: plugin_id.clone(),
        allow_spawn: true,
        authorities: PluginAuthorities::empty(), // no PTY_PROCESS authority
    });
    let mut surface_registry = SurfaceRegistry::new();
    register_builtin_surfaces(&mut surface_registry);

    let mut dirty = DirtyFlags::empty();
    let timer = NoopTimer;
    let mut sessions = NoopSessionRuntime::default();
    let mut initial_resize_sent = true;
    let mut dispatcher = RecordingDispatcher::default();
    let mut workspace_changed = false;

    let quit = handle_deferred_commands(
        vec![Command::SpawnProcess {
            job_id: 1,
            program: "bash".to_string(),
            args: vec![],
            stdin_mode: StdinMode::Pty { rows: 24, cols: 80 },
        }],
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
            http_dispatcher: &mut crate::plugin::NullHttpDispatcher,
            workspace_changed: &mut workspace_changed,
            scroll_amount: 3,
        },
        Some(&plugin_id),
    );

    assert!(!quit);
    // PTY spawn should be rejected — dispatcher should not receive the spawn
    assert!(dispatcher.spawned.is_empty());
}

#[test]
fn pty_spawn_allowed_with_authority() {
    let plugin_id = PluginId("pty-plugin".to_string());
    let mut state = AppState::default();
    let mut registry = PluginRuntime::new();
    registry.register(TestPlugin {
        id: plugin_id.clone(),
        allow_spawn: true,
        authorities: PluginAuthorities::PTY_PROCESS,
    });
    let mut surface_registry = SurfaceRegistry::new();
    register_builtin_surfaces(&mut surface_registry);

    let mut dirty = DirtyFlags::empty();
    let timer = NoopTimer;
    let mut sessions = NoopSessionRuntime::default();
    let mut initial_resize_sent = true;
    let mut dispatcher = RecordingDispatcher::default();
    let mut workspace_changed = false;

    let quit = handle_deferred_commands(
        vec![Command::SpawnProcess {
            job_id: 1,
            program: "bash".to_string(),
            args: vec![],
            stdin_mode: StdinMode::Pty { rows: 24, cols: 80 },
        }],
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
            http_dispatcher: &mut crate::plugin::NullHttpDispatcher,
            workspace_changed: &mut workspace_changed,
            scroll_amount: 3,
        },
        Some(&plugin_id),
    );

    assert!(!quit);
    assert_eq!(dispatcher.spawned.len(), 1);
    assert_eq!(
        dispatcher.spawned[0].4,
        StdinMode::Pty { rows: 24, cols: 80 }
    );
}

#[test]
fn piped_spawn_does_not_require_pty_authority() {
    let plugin_id = PluginId("piped-plugin".to_string());
    let mut state = AppState::default();
    let mut registry = PluginRuntime::new();
    registry.register(TestPlugin {
        id: plugin_id.clone(),
        allow_spawn: true,
        authorities: PluginAuthorities::empty(), // no PTY_PROCESS authority
    });
    let mut surface_registry = SurfaceRegistry::new();
    register_builtin_surfaces(&mut surface_registry);

    let mut dirty = DirtyFlags::empty();
    let timer = NoopTimer;
    let mut sessions = NoopSessionRuntime::default();
    let mut initial_resize_sent = true;
    let mut dispatcher = RecordingDispatcher::default();
    let mut workspace_changed = false;

    let quit = handle_deferred_commands(
        vec![Command::SpawnProcess {
            job_id: 1,
            program: "echo".to_string(),
            args: vec!["test".to_string()],
            stdin_mode: StdinMode::Piped,
        }],
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
            http_dispatcher: &mut crate::plugin::NullHttpDispatcher,
            workspace_changed: &mut workspace_changed,
            scroll_amount: 3,
        },
        Some(&plugin_id),
    );

    assert!(!quit);
    // Piped spawn should succeed without PTY_PROCESS authority
    assert_eq!(dispatcher.spawned.len(), 1);
    assert_eq!(dispatcher.spawned[0].4, StdinMode::Piped);
}

#[test]
fn inject_input_dispatches_through_update() {
    use crate::input::{InputEvent, Key, KeyEvent, Modifiers};

    let mut state = AppState::default();
    let mut registry = PluginRuntime::new();
    let mut surface_registry = SurfaceRegistry::new();

    let mut dirty = DirtyFlags::empty();
    let timer = NoopTimer;
    let mut sessions = NoopSessionRuntime::default();
    let mut initial_resize_sent = false;
    let mut dispatcher = RecordingDispatcher::default();
    let mut workspace_changed = false;
    let mut scroll_plans = Vec::new();

    let quit = handle_deferred_commands(
        vec![Command::InjectInput(InputEvent::Key(KeyEvent {
            key: Key::Char('a'),
            modifiers: Modifiers::empty(),
        }))],
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
            scroll_plan_sink: &mut |plan| scroll_plans.push(plan),
            process_dispatcher: &mut dispatcher,
            http_dispatcher: &mut crate::plugin::NullHttpDispatcher,
            workspace_changed: &mut workspace_changed,
            scroll_amount: 3,
        },
        None,
    );

    // The injected key should have been processed through update()
    // which sends it to Kakoune via SendToKakoune (immediate command)
    assert!(!quit);
}

#[test]
fn inject_paste_dispatches_through_text_pipeline() {
    use crate::input::InputEvent;

    let mut state = AppState::default();
    let mut registry = PluginRuntime::new();
    registry.register(TextInputPlugin);
    let mut surface_registry = SurfaceRegistry::new();

    let mut dirty = DirtyFlags::empty();
    let timer = NoopTimer;
    let mut sessions = NoopSessionRuntime::default();
    let mut initial_resize_sent = false;
    let mut dispatcher = RecordingDispatcher::default();
    let mut workspace_changed = false;

    let quit = handle_deferred_commands(
        vec![Command::InjectInput(InputEvent::Paste("kana".into()))],
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
            http_dispatcher: &mut crate::plugin::NullHttpDispatcher,
            workspace_changed: &mut workspace_changed,
            scroll_amount: 3,
        },
        None,
    );

    assert!(!quit);
    assert!(dirty.contains(DirtyFlags::INFO));
}

#[test]
fn inject_input_respects_depth_limit() {
    use super::super::context::MAX_INJECT_DEPTH;
    use crate::input::{InputEvent, Key, KeyEvent, Modifiers};

    let mut state = AppState::default();
    let mut registry = PluginRuntime::new();
    let mut surface_registry = SurfaceRegistry::new();

    let mut dirty = DirtyFlags::empty();
    let timer = NoopTimer;
    let mut sessions = NoopSessionRuntime::default();
    let mut initial_resize_sent = false;
    let mut dispatcher = RecordingDispatcher::default();
    let mut workspace_changed = false;

    // Call at MAX depth — should be dropped
    let quit = handle_deferred_commands_inner(
        vec![Command::InjectInput(InputEvent::Key(KeyEvent {
            key: Key::Char('x'),
            modifiers: Modifiers::empty(),
        }))],
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
            http_dispatcher: &mut crate::plugin::NullHttpDispatcher,
            workspace_changed: &mut workspace_changed,
            scroll_amount: 3,
        },
        None,
        MAX_INJECT_DEPTH, // at limit — should be dropped
    );

    assert!(!quit);
}

/// Plugin that responds to every PluginMessage by sending another
/// PluginMessage to itself, creating an infinite cascade.
struct CascadingMessagePlugin;

impl crate::plugin::Plugin for CascadingMessagePlugin {
    type State = ();

    fn id(&self) -> PluginId {
        PluginId("cascading".to_string())
    }

    fn register(&self, r: &mut crate::plugin::HandlerRegistry<()>) {
        // The handler intentionally cascades to itself to exercise the
        // depth limit; the framework cuts it off at
        // MAX_COMMAND_CASCADE_DEPTH.
        r.on_update_tier2(|_state, _msg, _app| {
            let mut effects = crate::plugin::ProcessCapableEffects::none();
            effects
                .base
                .commands
                .push(crate::plugin::KakouneSideCommand::plugin_message(
                    PluginId("cascading".to_string()),
                    Box::new(()),
                ));
            ((), effects)
        });
    }
}

#[test]
fn command_cascade_terminates_at_depth_limit() {
    let mut state = AppState::default();
    let mut registry = PluginRuntime::new();
    registry.register(CascadingMessagePlugin);
    let mut surface_registry = SurfaceRegistry::new();

    let mut dirty = DirtyFlags::empty();
    let timer = NoopTimer;
    let mut sessions = NoopSessionRuntime::default();
    let mut initial_resize_sent = false;
    let mut dispatcher = RecordingDispatcher::default();
    let mut workspace_changed = false;

    // Seed a single PluginMessage — should cascade but terminate
    let quit = handle_deferred_commands(
        vec![Command::PluginMessage {
            target: PluginId("cascading".to_string()),
            payload: Box::new(()),
        }],
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
            http_dispatcher: &mut crate::plugin::NullHttpDispatcher,
            workspace_changed: &mut workspace_changed,
            scroll_amount: 3,
        },
        None,
    );

    assert!(!quit);
    // The cascade should have been cut off at MAX_COMMAND_CASCADE_DEPTH.
    // The test's primary assertion is that it terminates without panic/hang.
}

/// Plugin that handles every key by injecting another key, creating
/// an infinite injection cascade.
struct CascadingInjectPlugin;

impl crate::plugin::Plugin for CascadingInjectPlugin {
    type State = ();

    fn id(&self) -> PluginId {
        PluginId("cascading-inject".to_string())
    }

    fn register(&self, r: &mut crate::plugin::HandlerRegistry<()>) {
        r.on_key(|_state, _key, _app| {
            Some((
                (),
                vec![Command::InjectInput(crate::input::InputEvent::Key(
                    crate::input::KeyEvent {
                        key: crate::input::Key::Char('z'),
                        modifiers: crate::input::Modifiers::empty(),
                    },
                ))],
            ))
        });
    }
}

#[test]
fn inject_cascade_terminates_at_depth_limit() {
    let mut state = AppState::default();
    let mut registry = PluginRuntime::new();
    registry.register(CascadingInjectPlugin);
    let mut surface_registry = SurfaceRegistry::new();

    let mut dirty = DirtyFlags::empty();
    let timer = NoopTimer;
    let mut sessions = NoopSessionRuntime::default();
    let mut initial_resize_sent = false;
    let mut dispatcher = RecordingDispatcher::default();
    let mut workspace_changed = false;

    // Inject a key — the plugin will re-inject on every handle_key,
    // but the depth limit should cut it off.
    let quit = handle_deferred_commands(
        vec![Command::InjectInput(crate::input::InputEvent::Key(
            crate::input::KeyEvent {
                key: crate::input::Key::Char('a'),
                modifiers: crate::input::Modifiers::empty(),
            },
        ))],
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
            http_dispatcher: &mut crate::plugin::NullHttpDispatcher,
            workspace_changed: &mut workspace_changed,
            scroll_amount: 3,
        },
        None,
    );

    assert!(!quit);
}

// ── sync_suppressed_builtins ─────────────────────────────────────

struct SuppressPlugin {
    targets: std::collections::HashSet<crate::plugin::BuiltinTarget>,
}

impl crate::plugin::Plugin for SuppressPlugin {
    type State = ();

    fn id(&self) -> PluginId {
        PluginId("suppress-test".to_string())
    }

    fn register(&self, r: &mut crate::plugin::HandlerRegistry<()>) {
        for target in &self.targets {
            r.suppress_builtin(*target);
        }
    }
}

#[test]
fn sync_suppressed_builtins_copies_from_registry_to_state() {
    use crate::plugin::BuiltinTarget;

    let mut state = AppState::default();
    let mut registry = PluginRuntime::new();

    let mut targets = std::collections::HashSet::new();
    targets.insert(BuiltinTarget::StatusBar);
    targets.insert(BuiltinTarget::ShadowCursor);
    registry.register(SuppressPlugin { targets });

    // Before sync: state has no suppressions
    assert!(state.runtime.suppressed_builtins.is_empty());

    super::super::sync_suppressed_builtins(&mut state, &registry);

    // After sync: state mirrors registry
    assert!(
        state
            .runtime
            .suppressed_builtins
            .contains(&BuiltinTarget::StatusBar)
    );
    assert!(
        state
            .runtime
            .suppressed_builtins
            .contains(&BuiltinTarget::ShadowCursor)
    );
    assert!(
        !state
            .runtime
            .suppressed_builtins
            .contains(&BuiltinTarget::Menu)
    );
}

/// Regression test for #100: process commands without `source_plugin` attribution
/// must surface an error log instead of being silently dropped. The drop itself is
/// preserved (the spawn cannot happen without authority checks); the log lets plugin
/// authors diagnose why their `on_state_changed_effects`-issued spawn never reached
/// the dispatcher.
#[test]
fn process_command_without_source_logs_error() {
    use std::io::Write;
    use std::sync::{Arc, Mutex};
    use tracing_subscriber::fmt::MakeWriter;

    #[derive(Clone, Default)]
    struct LogCapture(Arc<Mutex<Vec<u8>>>);

    struct LogCaptureWriter(Arc<Mutex<Vec<u8>>>);

    impl Write for LogCaptureWriter {
        fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
            self.0.lock().unwrap().extend_from_slice(buf);
            Ok(buf.len())
        }
        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }

    impl<'a> MakeWriter<'a> for LogCapture {
        type Writer = LogCaptureWriter;
        fn make_writer(&'a self) -> Self::Writer {
            LogCaptureWriter(Arc::clone(&self.0))
        }
    }

    let capture = LogCapture::default();
    let subscriber = tracing_subscriber::fmt()
        .with_writer(capture.clone())
        .with_max_level(tracing::Level::ERROR)
        .with_ansi(false)
        .finish();

    let mut state = AppState::default();
    let mut registry = PluginRuntime::new();
    let mut surface_registry = SurfaceRegistry::new();
    let mut dirty = DirtyFlags::empty();
    let timer = NoopTimer;
    let mut sessions = NoopSessionRuntime::default();
    let mut initial_resize_sent = false;
    let mut dispatcher = RecordingDispatcher::default();
    let mut workspace_changed = false;

    let quit = tracing::subscriber::with_default(subscriber, || {
        handle_deferred_commands(
            vec![Command::SpawnProcess {
                job_id: 77,
                program: "fd".to_string(),
                args: vec!["needle".to_string()],
                stdin_mode: StdinMode::Null,
            }],
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
                http_dispatcher: &mut crate::plugin::NullHttpDispatcher,
                workspace_changed: &mut workspace_changed,
                scroll_amount: 3,
            },
            None,
        )
    });

    assert!(!quit);
    assert!(
        dispatcher.spawned.is_empty(),
        "spawn must be dropped when source is missing"
    );

    let logs = String::from_utf8(capture.0.lock().unwrap().clone()).unwrap();
    assert!(
        logs.contains("SpawnProcess"),
        "expected SpawnProcess in logs, got: {logs}"
    );
    assert!(
        logs.contains("source_plugin missing"),
        "expected drop-reason in logs, got: {logs}"
    );
    assert!(
        logs.contains("job_id=77"),
        "expected job_id field in logs, got: {logs}"
    );
}

/// End-to-end regression for issue #101: a plugin emitting `SpawnProcess` from
/// `on_state_changed_effects` must reach the process dispatcher with the
/// plugin's id, not get silently dropped (issue #100) or lose its source in
/// `EffectsBatch.merge`.
///
/// This is the sprout-picker scenario boiled down: bumping a state-bit causes
/// the plugin's state-changed handler to fire, and that handler tries to spawn
/// the picker process. Prior to #101, the spawn never reached `fork+execve`
/// because attribution was lost in the multi-plugin merge.
#[test]
fn state_changed_sourced_commands_reach_dispatcher_with_plugin_id() {
    use crate::plugin::SourcedCommands;

    let plugin_id = PluginId("state-changed-spawner".to_string());
    let mut registry = PluginRuntime::new();
    registry.register(TestPlugin {
        id: plugin_id.clone(),
        allow_spawn: true,
        authorities: PluginAuthorities::empty(),
    });

    let mut state = AppState::default();
    let mut surface_registry = SurfaceRegistry::new();
    let mut dirty = DirtyFlags::empty();
    let timer = NoopTimer;
    let mut sessions = NoopSessionRuntime::default();
    let mut initial_resize_sent = false;
    let mut dispatcher = RecordingDispatcher::default();
    let mut workspace_changed = false;

    let groups = vec![SourcedCommands::new(
        Some(plugin_id.clone()),
        vec![Command::SpawnProcess {
            job_id: 314,
            program: "fd".to_string(),
            args: vec!["pattern".to_string()],
            stdin_mode: StdinMode::Null,
        }],
    )];

    let quit = {
        let mut ctx = DeferredContext {
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
            http_dispatcher: &mut crate::plugin::NullHttpDispatcher,
            workspace_changed: &mut workspace_changed,
            scroll_amount: 3,
        };
        groups.into_iter().any(|group| {
            handle_deferred_commands(group.commands, &mut ctx, group.source_plugin.as_ref())
        })
    };

    assert!(!quit);
    assert_eq!(
        dispatcher.spawned.len(),
        1,
        "spawn from state-changed must reach the dispatcher"
    );
    assert_eq!(dispatcher.spawned[0].0, plugin_id);
    assert_eq!(dispatcher.spawned[0].1, 314);
    assert_eq!(dispatcher.spawned[0].2, "fd");
}
