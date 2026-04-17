//! Surface lifecycle management.
//!
//! Registration, reconciliation, and teardown of plugin-owned surfaces.

use crate::layout::Rect;
use crate::plugin::{AppliedWinnerDelta, PluginDiagnostic, PluginRuntime};
use crate::state::{AppState, DirtyFlags};
use crate::surface::buffer::KakouneBufferSurface;
use crate::surface::{SurfaceId, SurfaceRegistry};
use crate::workspace::{Placement, Workspace};

/// Register plugin-owned surfaces in the surface registry.
///
/// Iterates over all plugin surface sets, registers each surface, and applies
/// initial placements. If registration fails for any surface in a set, all
/// previously-registered surfaces from that set are rolled back and the plugin
/// is removed from the registry.
pub fn setup_plugin_surfaces(
    registry: &mut PluginRuntime,
    surface_registry: &mut SurfaceRegistry,
    state: &AppState,
) -> Vec<PluginDiagnostic> {
    let mut diagnostics = Vec::new();
    for surface_set in registry.collect_plugin_surfaces() {
        let mut registered_ids = Vec::new();
        let mut registration_error = None;
        for surface in surface_set.surfaces {
            let surface_id = surface.id();
            match surface_registry.try_register_for_owner(surface, Some(surface_set.owner.clone()))
            {
                Ok(()) => registered_ids.push(surface_id),
                Err(err) => {
                    registration_error = Some(err);
                    break;
                }
            }
        }
        if let Some(err) = registration_error {
            for surface_id in registered_ids {
                surface_registry.remove(surface_id);
            }
            registry.unload_plugin(&surface_set.owner);
            diagnostics.push(PluginDiagnostic::surface_registration_failed(
                surface_set.owner.clone(),
                err.clone(),
            ));
        } else {
            apply_surface_initial_placements(
                surface_registry,
                &registered_ids,
                surface_set.legacy_workspace_request.as_ref(),
                state,
            );
        }
    }
    diagnostics
}

pub fn register_builtin_surfaces(surface_registry: &mut SurfaceRegistry) {
    surface_registry
        .try_register(Box::new(KakouneBufferSurface::new()))
        .expect("failed to register built-in surface kasane.buffer");
    surface_registry
        .try_register(Box::new(crate::surface::status::StatusBarSurface::new()))
        .expect("failed to register built-in surface kasane.status");
}

/// Apply initial surface placements and log any unresolved ones.
fn apply_surface_initial_placements(
    surface_registry: &mut SurfaceRegistry,
    surface_ids: &[SurfaceId],
    legacy_request: Option<&Placement>,
    state: &AppState,
) {
    let mut bootstrap_dirty = DirtyFlags::empty();
    for (surface_id, request) in surface_registry.apply_initial_placements_with_total(
        surface_ids,
        legacy_request,
        &mut bootstrap_dirty,
        Some(Rect {
            x: 0,
            y: 0,
            w: state.runtime.cols,
            h: state.runtime.rows,
        }),
    ) {
        tracing::warn!(
            "skipping unresolved initial placement for surface {surface_id:?}: {request:?}"
        );
    }
}

pub fn rebuild_plugin_surface_registry(
    registry: &mut PluginRuntime,
    surface_registry: &mut SurfaceRegistry,
    state: &AppState,
) -> Vec<PluginDiagnostic> {
    let workspace = std::mem::take(surface_registry.workspace_mut());
    let mut rebuilt = SurfaceRegistry::with_workspace(workspace);
    register_builtin_surfaces(&mut rebuilt);
    let disabled_plugins = setup_plugin_surfaces(registry, &mut rebuilt, state);
    prune_missing_workspace_surfaces(&mut rebuilt);
    *surface_registry = rebuilt;
    disabled_plugins
}

/// Reconcile plugin-owned surfaces for the specific set of changed winners.
///
/// Unchanged owners keep their surface instances and workspace placement.
/// Changed owners are removed first, then re-registered from the current
/// registry. Missing surfaces are pruned from the workspace afterwards.
pub fn reconcile_plugin_surfaces(
    registry: &mut PluginRuntime,
    surface_registry: &mut SurfaceRegistry,
    state: &AppState,
    deltas: &[AppliedWinnerDelta],
) -> Vec<PluginDiagnostic> {
    if deltas.is_empty() {
        return vec![];
    }

    for delta in deltas {
        if delta.old.is_some() {
            surface_registry.remove_owned_surfaces(&delta.id);
        }
    }

    let mut diagnostics = Vec::new();
    for delta in deltas {
        if delta.new.is_none() {
            continue;
        }
        let Some(surface_set) = registry.collect_plugin_surfaces_for_owner(&delta.id) else {
            continue;
        };

        let mut registered_ids = Vec::new();
        let mut new_workspace_surfaces = Vec::new();
        let mut registration_error = None;

        for surface in surface_set.surfaces {
            let surface_id = surface.id();
            let already_in_workspace = surface_registry.workspace_contains(surface_id);
            match surface_registry.try_register_for_owner(surface, Some(surface_set.owner.clone()))
            {
                Ok(()) => {
                    registered_ids.push(surface_id);
                    if !already_in_workspace {
                        new_workspace_surfaces.push(surface_id);
                    }
                }
                Err(err) => {
                    registration_error = Some(err);
                    break;
                }
            }
        }

        if let Some(err) = registration_error {
            for surface_id in registered_ids {
                surface_registry.remove(surface_id);
            }
            registry.unload_plugin(&surface_set.owner);
            diagnostics.push(PluginDiagnostic::surface_registration_failed(
                surface_set.owner.clone(),
                err.clone(),
            ));
            continue;
        }

        if !new_workspace_surfaces.is_empty() {
            apply_surface_initial_placements(
                surface_registry,
                &new_workspace_surfaces,
                surface_set.legacy_workspace_request.as_ref(),
                state,
            );
        }
    }

    prune_missing_workspace_surfaces(surface_registry);
    diagnostics
}

fn prune_missing_workspace_surfaces(surface_registry: &mut SurfaceRegistry) {
    let stale_ids: Vec<_> = surface_registry
        .workspace()
        .root()
        .collect_ids()
        .into_iter()
        .filter(|surface_id| surface_registry.get(*surface_id).is_none())
        .collect();

    if stale_ids.is_empty() {
        return;
    }

    for stale_id in stale_ids {
        let _ = surface_registry.workspace_mut().close(stale_id);
    }

    let still_stale = surface_registry
        .workspace()
        .root()
        .collect_ids()
        .into_iter()
        .any(|surface_id| surface_registry.get(surface_id).is_none());
    if still_stale {
        *surface_registry.workspace_mut() = Workspace::default();
    }
}
