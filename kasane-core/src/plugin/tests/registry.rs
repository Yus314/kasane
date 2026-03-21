use super::*;
use crate::display::DisplayDirective;
use crate::input::{Key, KeyEvent, Modifiers};
use crate::layout::Rect;
use crate::plugin::{
    BootstrapEffects, KeyDispatchResult, KeyHandleResult, RuntimeEffects, SessionReadyCommand,
    SessionReadyEffects,
};
use crate::protocol::KasaneRequest;
use crate::scroll::{ScrollAccumulationMode, ScrollCurve, ScrollPlan};
use std::sync::Arc;
use std::sync::Mutex;
use std::sync::atomic::{AtomicUsize, Ordering};

struct TypedLifecyclePlugin;

impl PluginBackend for TypedLifecyclePlugin {
    fn id(&self) -> PluginId {
        PluginId("typed-lifecycle".to_string())
    }

    fn on_init_effects(&mut self, _state: &AppState) -> BootstrapEffects {
        BootstrapEffects {
            redraw: DirtyFlags::STATUS,
        }
    }

    fn on_active_session_ready_effects(&mut self, _state: &AppState) -> SessionReadyEffects {
        SessionReadyEffects {
            redraw: DirtyFlags::BUFFER,
            commands: vec![SessionReadyCommand::SendToKakoune(KasaneRequest::Scroll {
                amount: 3,
                line: 1,
                column: 1,
            })],
            scroll_plans: vec![],
        }
    }
}

struct TypedRuntimePlugin;

impl PluginBackend for TypedRuntimePlugin {
    fn id(&self) -> PluginId {
        PluginId("typed-runtime".to_string())
    }

    fn on_state_changed_effects(&mut self, _state: &AppState, dirty: DirtyFlags) -> RuntimeEffects {
        if !dirty.contains(DirtyFlags::BUFFER) {
            return RuntimeEffects::default();
        }
        RuntimeEffects {
            redraw: DirtyFlags::INFO,
            commands: vec![Command::RequestRedraw(DirtyFlags::STATUS)],
            scroll_plans: vec![ScrollPlan {
                total_amount: 3,
                line: 2,
                column: 4,
                frame_interval_ms: 8,
                curve: ScrollCurve::Linear,
                accumulation: ScrollAccumulationMode::Add,
            }],
        }
    }

    fn update_effects(&mut self, msg: &mut dyn std::any::Any, _state: &AppState) -> RuntimeEffects {
        if msg.downcast_ref::<u32>() != Some(&7) {
            return RuntimeEffects::default();
        }
        RuntimeEffects {
            redraw: DirtyFlags::BUFFER,
            commands: vec![Command::RequestRedraw(DirtyFlags::STATUS)],
            scroll_plans: vec![ScrollPlan {
                total_amount: -2,
                line: 1,
                column: 1,
                frame_interval_ms: 16,
                curve: ScrollCurve::Linear,
                accumulation: ScrollAccumulationMode::Add,
            }],
        }
    }
}

struct ShutdownProbePlugin {
    id: &'static str,
    shutdowns: Arc<AtomicUsize>,
}

impl PluginBackend for ShutdownProbePlugin {
    fn id(&self) -> PluginId {
        PluginId(self.id.to_string())
    }

    fn on_shutdown(&mut self) {
        self.shutdowns.fetch_add(1, Ordering::SeqCst);
    }
}

struct AuthorityPlugin {
    id: &'static str,
    authorities: PluginAuthorities,
}

struct DisplayTransformPlugin {
    id: &'static str,
    directives: Vec<DisplayDirective>,
}

impl PluginBackend for DisplayTransformPlugin {
    fn id(&self) -> PluginId {
        PluginId(self.id.to_string())
    }

    fn capabilities(&self) -> PluginCapabilities {
        PluginCapabilities::DISPLAY_TRANSFORM
    }

    fn display_directives(&self, _state: &AppState) -> Vec<DisplayDirective> {
        self.directives.clone()
    }
}

struct WorkspaceObserverPlugin {
    id: &'static str,
    hits: Arc<AtomicUsize>,
}

impl PluginBackend for WorkspaceObserverPlugin {
    fn id(&self) -> PluginId {
        PluginId(self.id.to_string())
    }

    fn capabilities(&self) -> PluginCapabilities {
        PluginCapabilities::WORKSPACE_OBSERVER
    }

    fn on_workspace_changed(&mut self, _query: &crate::workspace::WorkspaceQuery<'_>) {
        self.hits.fetch_add(1, Ordering::SeqCst);
    }
}

enum MiddlewareBehavior {
    Passthrough,
    Transform(KeyEvent),
    Consume(String),
}

struct KeyMiddlewarePlugin {
    id: &'static str,
    seen: Arc<Mutex<Vec<KeyEvent>>>,
    behavior: MiddlewareBehavior,
}

impl PluginBackend for KeyMiddlewarePlugin {
    fn id(&self) -> PluginId {
        PluginId(self.id.to_string())
    }

    fn handle_key_middleware(&mut self, key: &KeyEvent, _state: &AppState) -> KeyHandleResult {
        self.seen.lock().unwrap().push(key.clone());
        match &self.behavior {
            MiddlewareBehavior::Passthrough => KeyHandleResult::Passthrough,
            MiddlewareBehavior::Transform(next_key) => {
                KeyHandleResult::Transformed(next_key.clone())
            }
            MiddlewareBehavior::Consume(keyspec) => KeyHandleResult::Consumed(vec![
                Command::SendToKakoune(KasaneRequest::Keys(vec![keyspec.clone()])),
            ]),
        }
    }
}

impl PluginBackend for AuthorityPlugin {
    fn id(&self) -> PluginId {
        PluginId(self.id.to_string())
    }

    fn authorities(&self) -> PluginAuthorities {
        self.authorities
    }
}

struct TargetedReadyPlugin {
    id: &'static str,
    redraw: DirtyFlags,
}

impl PluginBackend for TargetedReadyPlugin {
    fn id(&self) -> PluginId {
        PluginId(self.id.to_string())
    }

    fn on_active_session_ready_effects(&mut self, _state: &AppState) -> SessionReadyEffects {
        SessionReadyEffects {
            redraw: self.redraw,
            commands: vec![],
            scroll_plans: vec![],
        }
    }
}

#[test]
fn test_empty_registry() {
    let registry = PluginRuntime::new();
    assert!(registry.plugin_count() == 0);
}

#[test]
fn test_plugin_id() {
    let plugin = TestPlugin;
    assert_eq!(plugin.id(), PluginId("test".to_string()));
}

#[test]
fn test_init_all_batch_collects_bootstrap_effects() {
    let mut registry = PluginRuntime::new();
    registry.register_backend(Box::new(TypedLifecyclePlugin));
    let state = AppState::default();

    let batch = registry.init_all_batch(&state);
    assert!(batch.effects.redraw.contains(DirtyFlags::STATUS));
}

#[test]
fn test_notify_active_session_ready_batch_collects_effects() {
    let mut registry = PluginRuntime::new();
    registry.register_backend(Box::new(TypedLifecyclePlugin));
    let state = AppState::default();

    let batch = registry.notify_active_session_ready_batch(&state);
    assert!(batch.effects.redraw.contains(DirtyFlags::BUFFER));
    assert_eq!(batch.effects.commands.len(), 1);
    assert!(matches!(
        batch.effects.commands.into_iter().next(),
        Some(SessionReadyCommand::SendToKakoune(
            KasaneRequest::Scroll { .. }
        ))
    ));
}

#[test]
fn test_notify_plugin_active_session_ready_batch_targets_only_requested_plugin() {
    let mut registry = PluginRuntime::new();
    registry.register_backend(Box::new(TargetedReadyPlugin {
        id: "alpha",
        redraw: DirtyFlags::STATUS,
    }));
    registry.register_backend(Box::new(TargetedReadyPlugin {
        id: "beta",
        redraw: DirtyFlags::BUFFER,
    }));
    let state = AppState::default();

    let batch =
        registry.notify_plugin_active_session_ready_batch(&PluginId("beta".to_string()), &state);
    assert!(batch.effects.redraw.contains(DirtyFlags::BUFFER));
    assert!(!batch.effects.redraw.contains(DirtyFlags::STATUS));
}

#[test]
fn test_notify_state_changed_batch_collects_runtime_effects() {
    let mut registry = PluginRuntime::new();
    registry.register_backend(Box::new(TypedRuntimePlugin));
    let state = AppState::default();

    let batch = registry.notify_state_changed_batch(&state, DirtyFlags::BUFFER);
    assert!(batch.effects.redraw.contains(DirtyFlags::INFO));
    assert_eq!(batch.effects.commands.len(), 1);
    assert_eq!(batch.effects.scroll_plans.len(), 1);
}

#[test]
fn test_deliver_message_batch_collects_runtime_effects() {
    let mut registry = PluginRuntime::new();
    registry.register_backend(Box::new(TypedRuntimePlugin));
    let state = AppState::default();

    let batch = registry.deliver_message_batch(
        &PluginId("typed-runtime".to_string()),
        Box::new(7u32),
        &state,
    );
    assert!(batch.effects.redraw.contains(DirtyFlags::BUFFER));
    assert_eq!(batch.effects.commands.len(), 1);
    assert_eq!(batch.effects.scroll_plans.len(), 1);
}

#[test]
fn test_shutdown_all_calls_all_plugins() {
    let mut registry = PluginRuntime::new();
    registry.register_backend(Box::new(LifecyclePlugin::new()));
    registry.register_backend(Box::new(LifecyclePlugin::new()));
    registry.shutdown_all();
    // Verify via count — can't inspect internal state, but no panic = success
}

#[test]
fn test_collect_plugin_surfaces_returns_owner_group() {
    let mut registry = PluginRuntime::new();
    registry.register_backend(Box::new(SurfacePlugin));

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
    let mut registry = PluginRuntime::new();
    registry.register_backend(Box::new(TestPlugin));
    registry.register_backend(Box::new(SurfacePlugin));

    assert!(registry.remove_plugin(&PluginId("surface-plugin".to_string())));
    assert_eq!(registry.plugin_count(), 1);
    assert!(!registry.remove_plugin(&PluginId("surface-plugin".to_string())));
}

#[test]
fn test_plugin_has_authority_uses_declared_authorities() {
    let mut registry = PluginRuntime::new();
    let plugin_id = PluginId("authority-probe".to_string());
    registry.register_backend(Box::new(AuthorityPlugin {
        id: "authority-probe",
        authorities: PluginAuthorities::DYNAMIC_SURFACE,
    }));

    assert!(registry.plugin_has_authority(&plugin_id, PluginAuthorities::DYNAMIC_SURFACE));
    assert!(!registry.plugin_has_authority(&plugin_id, PluginAuthorities::PTY_PROCESS));
}

#[test]
fn test_register_backend_replacement_updates_authorities() {
    let mut registry = PluginRuntime::new();
    let plugin_id = PluginId("authority-probe".to_string());
    registry.register_backend(Box::new(AuthorityPlugin {
        id: "authority-probe",
        authorities: PluginAuthorities::DYNAMIC_SURFACE,
    }));
    registry.register_backend(Box::new(AuthorityPlugin {
        id: "authority-probe",
        authorities: PluginAuthorities::PTY_PROCESS,
    }));

    assert!(!registry.plugin_has_authority(&plugin_id, PluginAuthorities::DYNAMIC_SURFACE));
    assert!(registry.plugin_has_authority(&plugin_id, PluginAuthorities::PTY_PROCESS));
}

#[test]
fn test_collect_display_directives_uses_first_non_empty_contributor() {
    let mut registry = PluginRuntime::new();
    registry.register_backend(Box::new(DisplayTransformPlugin {
        id: "first",
        directives: vec![DisplayDirective::Hide { range: 1..3 }],
    }));
    registry.register_backend(Box::new(DisplayTransformPlugin {
        id: "second",
        directives: vec![DisplayDirective::InsertAfter {
            after: 0,
            content: "ignored".to_string(),
            face: Face::default(),
        }],
    }));

    let mut state = AppState::default();
    state.lines = vec![vec![], vec![], vec![], vec![]];

    assert_eq!(
        registry.collect_display_directives(&state),
        vec![DisplayDirective::Hide { range: 1..3 }]
    );
}

#[test]
fn test_collect_display_map_ignores_later_display_transform_plugins() {
    let mut registry = PluginRuntime::new();
    registry.register_backend(Box::new(DisplayTransformPlugin {
        id: "first",
        directives: vec![DisplayDirective::Hide { range: 1..3 }],
    }));
    registry.register_backend(Box::new(DisplayTransformPlugin {
        id: "second",
        directives: vec![DisplayDirective::InsertAfter {
            after: 0,
            content: "ignored".to_string(),
            face: Face::default(),
        }],
    }));

    let mut state = AppState::default();
    state.lines = vec![vec![], vec![], vec![], vec![]];

    let display_map = registry.collect_display_map(&state);
    assert_eq!(display_map.display_line_count(), 2);
    assert_eq!(display_map.buffer_to_display(0), Some(0));
    assert_eq!(display_map.buffer_to_display(1), None);
    assert_eq!(display_map.buffer_to_display(2), None);
    assert_eq!(display_map.buffer_to_display(3), Some(1));
}

#[test]
fn test_notify_workspace_changed_dispatches_only_to_observers() {
    let hits = Arc::new(AtomicUsize::new(0));
    let mut registry = PluginRuntime::new();
    registry.register_backend(Box::new(WorkspaceObserverPlugin {
        id: "observer",
        hits: hits.clone(),
    }));
    registry.register_backend(Box::new(TestPlugin));

    let workspace = crate::workspace::Workspace::default();
    let query = workspace.query(Rect {
        x: 0,
        y: 0,
        w: 80,
        h: 24,
    });
    registry.notify_workspace_changed(&query);

    assert_eq!(hits.load(Ordering::SeqCst), 1);
}

#[test]
fn test_dispatch_key_middleware_passes_transformed_key_to_next_plugin() {
    let first_seen = Arc::new(Mutex::new(Vec::new()));
    let second_seen = Arc::new(Mutex::new(Vec::new()));
    let mut registry = PluginRuntime::new();
    registry.register_backend(Box::new(KeyMiddlewarePlugin {
        id: "transformer",
        seen: first_seen.clone(),
        behavior: MiddlewareBehavior::Transform(KeyEvent {
            key: Key::Char('b'),
            modifiers: Modifiers::SHIFT,
        }),
    }));
    registry.register_backend(Box::new(KeyMiddlewarePlugin {
        id: "consumer",
        seen: second_seen.clone(),
        behavior: MiddlewareBehavior::Consume("<esc>".to_string()),
    }));

    let state = AppState::default();
    let result = registry.dispatch_key_middleware(
        &KeyEvent {
            key: Key::Char('a'),
            modifiers: Modifiers::empty(),
        },
        &state,
    );

    let first_keys = first_seen.lock().unwrap().clone();
    let second_keys = second_seen.lock().unwrap().clone();
    assert_eq!(first_keys.len(), 1);
    assert_eq!(first_keys[0].key, Key::Char('a'));
    assert_eq!(second_keys.len(), 1);
    assert_eq!(second_keys[0].key, Key::Char('b'));
    assert_eq!(second_keys[0].modifiers, Modifiers::SHIFT);
    match result {
        KeyDispatchResult::Consumed {
            source_plugin,
            commands,
        } => {
            assert_eq!(source_plugin, PluginId("consumer".to_string()));
            assert_eq!(commands.len(), 1);
            assert!(matches!(
                &commands[0],
                Command::SendToKakoune(KasaneRequest::Keys(keys)) if keys == &vec!["<esc>".to_string()]
            ));
        }
        KeyDispatchResult::Passthrough(_) => panic!("expected middleware consumer"),
    }
}

#[test]
fn test_dispatch_key_middleware_returns_final_passthrough_key() {
    let mut registry = PluginRuntime::new();
    registry.register_backend(Box::new(KeyMiddlewarePlugin {
        id: "transformer",
        seen: Arc::new(Mutex::new(Vec::new())),
        behavior: MiddlewareBehavior::Transform(KeyEvent {
            key: Key::PageDown,
            modifiers: Modifiers::CTRL,
        }),
    }));
    registry.register_backend(Box::new(KeyMiddlewarePlugin {
        id: "passthrough",
        seen: Arc::new(Mutex::new(Vec::new())),
        behavior: MiddlewareBehavior::Passthrough,
    }));

    let state = AppState::default();
    match registry.dispatch_key_middleware(
        &KeyEvent {
            key: Key::Char('x'),
            modifiers: Modifiers::empty(),
        },
        &state,
    ) {
        KeyDispatchResult::Consumed { .. } => panic!("expected passthrough"),
        KeyDispatchResult::Passthrough(key) => {
            assert_eq!(key.key, Key::PageDown);
            assert_eq!(key.modifiers, Modifiers::CTRL);
        }
    }
}

#[test]
fn test_unload_plugin_calls_shutdown_and_removes_plugin() {
    let mut registry = PluginRuntime::new();
    let shutdowns = Arc::new(AtomicUsize::new(0));
    registry.register_backend(Box::new(ShutdownProbePlugin {
        id: "shutdown-probe",
        shutdowns: shutdowns.clone(),
    }));

    assert!(registry.contains_plugin(&PluginId("shutdown-probe".to_string())));
    assert!(registry.unload_plugin(&PluginId("shutdown-probe".to_string())));
    assert_eq!(shutdowns.load(Ordering::SeqCst), 1);
    assert!(!registry.contains_plugin(&PluginId("shutdown-probe".to_string())));
    assert!(!registry.unload_plugin(&PluginId("shutdown-probe".to_string())));
    assert_eq!(shutdowns.load(Ordering::SeqCst), 1);
}

#[test]
fn test_on_state_changed_dispatched_with_flags() {
    let mut registry = PluginRuntime::new();
    registry.register_backend(Box::new(LifecyclePlugin::new()));
    let state = AppState::default();

    // Simulate what update() does for Msg::Kakoune
    let flags = DirtyFlags::BUFFER | DirtyFlags::STATUS;
    for plugin in registry.plugins_mut() {
        let _ = plugin.on_state_changed_effects(&state, flags);
    }
    // No panic, default implementations work
}

#[test]
fn test_lifecycle_defaults() {
    // TestPlugin has no lifecycle hooks — defaults should work
    let mut registry = PluginRuntime::new();
    registry.register_backend(Box::new(TestPlugin));
    let state = AppState::default();

    let batch = registry.init_all_batch(&state);
    assert!(batch.effects.redraw.is_empty());

    registry.shutdown_all();
    // No panic
}

#[test]
fn test_init_all_batch_collects_lifecycle_bootstrap_effects() {
    let mut registry = PluginRuntime::new();
    registry.register_backend(Box::new(LifecyclePlugin::new()));
    let state = AppState::default();

    let batch = registry.init_all_batch(&state);
    assert!(batch.effects.redraw.contains(DirtyFlags::BUFFER));
}

#[test]
fn test_reload_plugin_batch_collects_bootstrap_effects() {
    let mut registry = PluginRuntime::new();
    registry.register_backend(Box::new(TypedLifecyclePlugin));
    let state = AppState::default();

    let batch = registry.reload_plugin_batch(Box::new(TypedLifecyclePlugin), &state);
    assert!(batch.effects.redraw.contains(DirtyFlags::STATUS));
}

#[test]
fn test_any_plugin_state_changed_flag() {
    let mut registry = PluginRuntime::new();
    registry.register_backend(Box::new(StatefulPlugin { hash: 1 }));

    // Initial prepare: hash differs from default 0 → changed
    registry.prepare_plugin_cache(DirtyFlags::ALL);
    assert!(registry.any_plugin_state_changed());

    // Second prepare with same hash → no change
    registry.prepare_plugin_cache(DirtyFlags::ALL);
    assert!(!registry.any_plugin_state_changed());
}

// --- deliver_message tests ---

#[test]
fn test_deliver_message_unknown_target() {
    let mut registry = PluginRuntime::new();
    let state = AppState::default();
    let batch =
        registry.deliver_message_batch(&PluginId("unknown".to_string()), Box::new(42u32), &state);
    assert!(batch.effects.redraw.is_empty());
    assert!(batch.effects.commands.is_empty());
    assert!(batch.effects.scroll_plans.is_empty());
}
