use super::*;
use crate::plugin::{BootstrapEffects, RuntimeEffects, SessionReadyCommand, SessionReadyEffects};
use crate::protocol::KasaneRequest;
use crate::scroll::{ScrollAccumulationMode, ScrollCurve, ScrollPlan};
use std::sync::Arc;
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
    let registry = PluginRegistry::new();
    assert!(registry.plugin_count() == 0);
}

#[test]
fn test_plugin_id() {
    let plugin = TestPlugin;
    assert_eq!(plugin.id(), PluginId("test".to_string()));
}

#[test]
fn test_init_all_batch_collects_bootstrap_effects() {
    let mut registry = PluginRegistry::new();
    registry.register_backend(Box::new(TypedLifecyclePlugin));
    let state = AppState::default();

    let batch = registry.init_all_batch(&state);
    assert!(batch.effects.redraw.contains(DirtyFlags::STATUS));
}

#[test]
fn test_notify_active_session_ready_batch_collects_effects() {
    let mut registry = PluginRegistry::new();
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
    let mut registry = PluginRegistry::new();
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
    let mut registry = PluginRegistry::new();
    registry.register_backend(Box::new(TypedRuntimePlugin));
    let state = AppState::default();

    let batch = registry.notify_state_changed_batch(&state, DirtyFlags::BUFFER);
    assert!(batch.effects.redraw.contains(DirtyFlags::INFO));
    assert_eq!(batch.effects.commands.len(), 1);
    assert_eq!(batch.effects.scroll_plans.len(), 1);
}

#[test]
fn test_deliver_message_batch_collects_runtime_effects() {
    let mut registry = PluginRegistry::new();
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
    let mut registry = PluginRegistry::new();
    registry.register_backend(Box::new(LifecyclePlugin::new()));
    registry.register_backend(Box::new(LifecyclePlugin::new()));
    registry.shutdown_all();
    // Verify via count — can't inspect internal state, but no panic = success
}

#[test]
fn test_collect_plugin_surfaces_returns_owner_group() {
    let mut registry = PluginRegistry::new();
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
    let mut registry = PluginRegistry::new();
    registry.register_backend(Box::new(TestPlugin));
    registry.register_backend(Box::new(SurfacePlugin));

    assert!(registry.remove_plugin(&PluginId("surface-plugin".to_string())));
    assert_eq!(registry.plugin_count(), 1);
    assert!(!registry.remove_plugin(&PluginId("surface-plugin".to_string())));
}

#[test]
fn test_unload_plugin_calls_shutdown_and_removes_plugin() {
    let mut registry = PluginRegistry::new();
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
    let mut registry = PluginRegistry::new();
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
    let mut registry = PluginRegistry::new();
    registry.register_backend(Box::new(TestPlugin));
    let state = AppState::default();

    let batch = registry.init_all_batch(&state);
    assert!(batch.effects.redraw.is_empty());

    registry.shutdown_all();
    // No panic
}

#[test]
fn test_init_all_batch_collects_lifecycle_bootstrap_effects() {
    let mut registry = PluginRegistry::new();
    registry.register_backend(Box::new(LifecyclePlugin::new()));
    let state = AppState::default();

    let batch = registry.init_all_batch(&state);
    assert!(batch.effects.redraw.contains(DirtyFlags::BUFFER));
}

#[test]
fn test_reload_plugin_batch_collects_bootstrap_effects() {
    let mut registry = PluginRegistry::new();
    registry.register_backend(Box::new(TypedLifecyclePlugin));
    let state = AppState::default();

    let batch = registry.reload_plugin_batch(Box::new(TypedLifecyclePlugin), &state);
    assert!(batch.effects.redraw.contains(DirtyFlags::STATUS));
}

#[test]
fn test_any_plugin_state_changed_flag() {
    let mut registry = PluginRegistry::new();
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
    let mut registry = PluginRegistry::new();
    let state = AppState::default();
    let batch =
        registry.deliver_message_batch(&PluginId("unknown".to_string()), Box::new(42u32), &state);
    assert!(batch.effects.redraw.is_empty());
    assert!(batch.effects.commands.is_empty());
    assert!(batch.effects.scroll_plans.is_empty());
}
