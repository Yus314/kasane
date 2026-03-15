use super::cache::{LayoutCache, ViewCache};
use super::grid::CellGrid;
use super::pipeline::{
    ViewSource, render_cached_core, render_patched_core, render_sectioned_core, scene_render_core,
};
use super::scene::{self, DrawCommand, SceneCache};
use super::{RenderResult, patch};
use crate::plugin::{PaintHook, PluginRegistry};
use crate::state::{AppState, DirtyFlags};
use crate::surface::SurfaceRegistry;

use super::view;

/// Builds view sections using the SurfaceRegistry for workspace-aware layouts.
struct SurfaceViewSource<'a> {
    surface_registry: &'a SurfaceRegistry,
}

impl ViewSource for SurfaceViewSource<'_> {
    fn invalidate_view_cache(
        &self,
        dirty: DirtyFlags,
        registry: &PluginRegistry,
        cache: &mut ViewCache,
    ) {
        let deps = view::effective_surface_section_deps(
            cache.base.value.as_ref(),
            registry,
            self.surface_registry,
        );
        cache.invalidate_with_deps(dirty, &deps);
    }

    fn view_sections(
        &self,
        state: &AppState,
        registry: &PluginRegistry,
        cache: &mut ViewCache,
    ) -> view::ViewSections {
        view::surface_view_sections_cached(state, registry, self.surface_registry, cache)
    }
}

/// Surface-based cached rendering pipeline (TUI).
pub fn render_pipeline_surfaces_cached(
    state: &AppState,
    plugin_registry: &PluginRegistry,
    surface_registry: &SurfaceRegistry,
    grid: &mut CellGrid,
    dirty: DirtyFlags,
    cache: &mut ViewCache,
    paint_hooks: &[Box<dyn PaintHook>],
) -> RenderResult {
    let source = SurfaceViewSource { surface_registry };
    render_cached_core(
        &source,
        state,
        plugin_registry,
        grid,
        dirty,
        cache,
        paint_hooks,
    )
}

/// Surface-based section-aware rendering pipeline (TUI).
#[allow(dead_code, clippy::too_many_arguments)]
pub(crate) fn render_pipeline_surfaces_sectioned(
    state: &AppState,
    plugin_registry: &PluginRegistry,
    surface_registry: &SurfaceRegistry,
    grid: &mut CellGrid,
    dirty: DirtyFlags,
    view_cache: &mut ViewCache,
    layout_cache: &mut LayoutCache,
    paint_hooks: &[Box<dyn PaintHook>],
) -> RenderResult {
    let source = SurfaceViewSource { surface_registry };
    render_sectioned_core(
        &source,
        state,
        plugin_registry,
        grid,
        dirty,
        view_cache,
        layout_cache,
        paint_hooks,
    )
}

/// Surface-based patched rendering pipeline (TUI).
#[allow(clippy::too_many_arguments)]
pub fn render_pipeline_surfaces_patched(
    state: &AppState,
    plugin_registry: &PluginRegistry,
    surface_registry: &SurfaceRegistry,
    grid: &mut CellGrid,
    dirty: DirtyFlags,
    view_cache: &mut ViewCache,
    layout_cache: &mut LayoutCache,
    patches: &[&dyn patch::PaintPatch],
    paint_hooks: &[Box<dyn PaintHook>],
) -> RenderResult {
    let source = SurfaceViewSource { surface_registry };
    render_patched_core(
        &source,
        state,
        plugin_registry,
        grid,
        dirty,
        view_cache,
        layout_cache,
        patches,
        paint_hooks,
    )
}

/// Surface-based scene rendering pipeline (GPU).
pub fn scene_render_pipeline_surfaces_cached<'a>(
    state: &AppState,
    plugin_registry: &PluginRegistry,
    surface_registry: &SurfaceRegistry,
    cell_size: scene::CellSize,
    dirty: DirtyFlags,
    view_cache: &mut ViewCache,
    scene_cache: &'a mut SceneCache,
) -> (&'a [DrawCommand], RenderResult) {
    let source = SurfaceViewSource { surface_registry };
    scene_render_core(
        &source,
        state,
        plugin_registry,
        cell_size,
        dirty,
        view_cache,
        scene_cache,
    )
}
