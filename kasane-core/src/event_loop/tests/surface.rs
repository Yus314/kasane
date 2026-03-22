use super::*;

use crate::plugin::PluginRuntime;
use crate::surface::{SurfaceId, SurfaceRegistry};

use super::super::surface::{
    rebuild_plugin_surface_registry, reconcile_plugin_surfaces, register_builtin_surfaces,
    setup_plugin_surfaces,
};

#[test]
fn rebuild_plugin_surface_registry_removes_stale_plugin_surfaces() {
    let state = AppState::default();
    let mut registry = PluginRuntime::new();
    registry.register_backend(Box::new(SurfacePlugin));

    let mut surface_registry = SurfaceRegistry::new();
    register_builtin_surfaces(&mut surface_registry);
    setup_plugin_surfaces(&mut registry, &mut surface_registry, &state);

    assert!(surface_registry.get(SurfaceId(200)).is_some());
    assert!(
        surface_registry
            .workspace()
            .root()
            .collect_ids()
            .contains(&SurfaceId(200))
    );

    assert!(registry.unload_plugin(&PluginId("surface-plugin".to_string())));
    rebuild_plugin_surface_registry(&mut registry, &mut surface_registry, &state);

    assert!(surface_registry.get(SurfaceId(200)).is_none());
    assert!(
        !surface_registry
            .workspace()
            .root()
            .collect_ids()
            .contains(&SurfaceId(200))
    );
    assert!(surface_registry.get(SurfaceId::BUFFER).is_some());
    assert!(surface_registry.get(SurfaceId::STATUS).is_some());
}

#[test]
fn reconcile_plugin_surfaces_removes_stale_plugin_surfaces() {
    let state = AppState::default();
    let mut registry = PluginRuntime::new();
    registry.register_backend(Box::new(SurfacePlugin));

    let mut surface_registry = SurfaceRegistry::new();
    register_builtin_surfaces(&mut surface_registry);
    let disabled_plugins = setup_plugin_surfaces(&mut registry, &mut surface_registry, &state);
    assert!(disabled_plugins.is_empty());

    assert!(surface_registry.get(SurfaceId(200)).is_some());
    assert!(
        surface_registry
            .workspace()
            .root()
            .collect_ids()
            .contains(&SurfaceId(200))
    );

    assert!(registry.unload_plugin(&PluginId("surface-plugin".to_string())));
    let disabled_plugins = reconcile_plugin_surfaces(
        &mut registry,
        &mut surface_registry,
        &state,
        &[owner_delta(Some("r0"), None)],
    );

    assert!(disabled_plugins.is_empty());
    assert!(surface_registry.get(SurfaceId(200)).is_none());
    assert!(
        !surface_registry
            .workspace()
            .root()
            .collect_ids()
            .contains(&SurfaceId(200))
    );
}

#[test]
fn reconcile_plugin_surfaces_preserves_same_id_workspace_placement() {
    let state = AppState::default();
    let mut registry = PluginRuntime::new();
    registry.register_backend(Box::new(SurfacePlugin));

    let mut surface_registry = SurfaceRegistry::new();
    register_builtin_surfaces(&mut surface_registry);
    let disabled_plugins = setup_plugin_surfaces(&mut registry, &mut surface_registry, &state);
    assert!(disabled_plugins.is_empty());
    assert_eq!(
        surface_registry
            .workspace()
            .root()
            .collect_ids()
            .into_iter()
            .filter(|surface_id| *surface_id == SurfaceId(200))
            .count(),
        1
    );

    let _ = registry.reload_plugin_batch(Box::new(ReplacementSurfacePlugin), &AppView::new(&state));
    let disabled_plugins = reconcile_plugin_surfaces(
        &mut registry,
        &mut surface_registry,
        &state,
        &[owner_delta(Some("r1"), Some("r2"))],
    );

    assert!(disabled_plugins.is_empty());
    assert!(surface_registry.get(SurfaceId(200)).is_some());
    assert_eq!(
        surface_registry
            .workspace()
            .root()
            .collect_ids()
            .into_iter()
            .filter(|surface_id| *surface_id == SurfaceId(200))
            .count(),
        1
    );
}

#[test]
fn setup_plugin_surfaces_returns_diagnostic_for_invalid_surface_contract() {
    let state = AppState::default();
    let mut registry = PluginRuntime::new();
    registry.register_backend(Box::new(InvalidSurfacePlugin));

    let mut surface_registry = SurfaceRegistry::new();
    register_builtin_surfaces(&mut surface_registry);
    let diagnostics = setup_plugin_surfaces(&mut registry, &mut surface_registry, &state);

    assert_eq!(diagnostics.len(), 1);
    assert_eq!(
        diagnostics[0].plugin_id(),
        Some(&PluginId("invalid-surface-plugin".to_string()))
    );
    assert!(matches!(
        diagnostics[0].kind,
        crate::plugin::PluginDiagnosticKind::SurfaceRegistrationFailed {
            reason: SurfaceRegistrationError::DuplicateSurfaceId { .. }
        }
    ));
    assert!(!registry.contains_plugin(&PluginId("invalid-surface-plugin".to_string())));
}

#[test]
fn reconcile_plugin_surfaces_returns_diagnostic_for_invalid_replacement() {
    let state = AppState::default();
    let mut registry = PluginRuntime::new();
    registry.register_backend(Box::new(SurfacePlugin));

    let mut surface_registry = SurfaceRegistry::new();
    register_builtin_surfaces(&mut surface_registry);
    let diagnostics = setup_plugin_surfaces(&mut registry, &mut surface_registry, &state);
    assert!(diagnostics.is_empty());

    let _ = registry.reload_plugin_batch(Box::new(InvalidSurfacePlugin), &AppView::new(&state));
    let diagnostics = reconcile_plugin_surfaces(
        &mut registry,
        &mut surface_registry,
        &state,
        &[AppliedWinnerDelta {
            id: PluginId("invalid-surface-plugin".to_string()),
            old: None,
            new: Some(PluginDescriptor {
                id: PluginId("invalid-surface-plugin".to_string()),
                source: PluginSource::Host {
                    provider: "test".to_string(),
                },
                revision: PluginRevision("r1".to_string()),
                rank: PluginRank::HOST,
            }),
        }],
    );

    assert_eq!(diagnostics.len(), 1);
    assert_eq!(
        diagnostics[0].plugin_id(),
        Some(&PluginId("invalid-surface-plugin".to_string()))
    );
    assert!(matches!(
        diagnostics[0].kind,
        crate::plugin::PluginDiagnosticKind::SurfaceRegistrationFailed {
            reason: SurfaceRegistrationError::DuplicateSurfaceId { .. }
        }
    ));
    assert!(!registry.contains_plugin(&PluginId("invalid-surface-plugin".to_string())));
}
