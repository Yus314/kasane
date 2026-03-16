use super::*;

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
fn test_init_all_returns_commands() {
    let mut registry = PluginRegistry::new();
    registry.register_backend(Box::new(LifecyclePlugin::new()));
    let state = AppState::default();
    let commands = registry.init_all(&state);
    assert_eq!(commands.len(), 1);
    assert!(matches!(commands[0], Command::RequestRedraw(_)));
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
fn test_on_state_changed_dispatched_with_flags() {
    let mut registry = PluginRegistry::new();
    registry.register_backend(Box::new(LifecyclePlugin::new()));
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
    registry.register_backend(Box::new(TestPlugin));
    let state = AppState::default();

    let commands = registry.init_all(&state);
    assert!(commands.is_empty());

    registry.shutdown_all();
    // No panic
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
fn test_deliver_message_to_plugin() {
    let mut registry = PluginRegistry::new();
    registry.register_backend(Box::new(TestPlugin));
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
