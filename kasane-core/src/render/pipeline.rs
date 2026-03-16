use super::cache::{LayoutCache, ViewCache};
use super::cursor::{
    apply_secondary_cursor_faces, clear_block_cursor_face, cursor_position, cursor_style,
    find_buffer_x_offset,
};
use super::grid::CellGrid;
use super::scene::{self, DrawCommand, SceneCache};
use super::theme::Theme;
use super::walk;
use super::{RenderResult, patch, view};
use crate::layout::Rect;
use crate::layout::flex;
use crate::layout::line_display_width;
use crate::plugin::PaintHook;
use crate::plugin::PluginRegistry;
use crate::protocol::CursorMode;
use crate::state::{AppState, DirtyFlags};

// ---------------------------------------------------------------------------
// ViewSource: abstracts where view sections come from
// ---------------------------------------------------------------------------

/// Trait that abstracts the source of `ViewSections` for the rendering pipeline.
///
/// Two implementations exist:
/// - `PluginViewSource`: builds sections from `PluginRegistry` alone (legacy/test path)
/// - `SurfaceViewSource`: builds sections from `SurfaceRegistry` (workspace-aware path)
pub(crate) trait ViewSource {
    fn invalidate_view_cache(
        &self,
        dirty: DirtyFlags,
        registry: &PluginRegistry,
        cache: &mut ViewCache,
    );

    fn view_sections(
        &self,
        state: &AppState,
        registry: &PluginRegistry,
        cache: &mut ViewCache,
    ) -> view::ViewSections;
}

/// Builds view sections using only the PluginRegistry (no workspace surfaces).
struct PluginViewSource;

impl ViewSource for PluginViewSource {
    fn invalidate_view_cache(
        &self,
        dirty: DirtyFlags,
        registry: &PluginRegistry,
        cache: &mut ViewCache,
    ) {
        cache.invalidate_with_deps(dirty, registry.section_deps());
    }

    fn view_sections(
        &self,
        state: &AppState,
        registry: &PluginRegistry,
        cache: &mut ViewCache,
    ) -> view::ViewSections {
        view::view_sections_cached(state, registry, cache)
    }
}

// ---------------------------------------------------------------------------
// Shared helpers
// ---------------------------------------------------------------------------

/// Apply paint hooks that match the current dirty flags.
pub(crate) fn apply_paint_hooks(
    hooks: &[Box<dyn PaintHook>],
    grid: &mut CellGrid,
    region: &Rect,
    state: &AppState,
    dirty: DirtyFlags,
) {
    for hook in hooks {
        if dirty.intersects(hook.deps()) {
            hook.apply(grid, region, state);
        }
    }
}

/// Selective clear: when BUFFER is dirty and line-level dirty info is available,
/// skip clearing buffer rows (paint_buffer_ref will skip clean lines) and only
/// clear non-buffer sections that are dirty. This extends line-dirty optimization
/// to BUFFER|STATUS and other BUFFER-containing combinations.
fn selective_clear(grid: &mut CellGrid, state: &AppState, dirty: DirtyFlags) {
    let line_dirty_active = dirty.contains(DirtyFlags::BUFFER_CONTENT)
        && !state.lines_dirty.is_empty()
        && state.lines_dirty.iter().any(|d| !d);

    if line_dirty_active {
        // Clear only non-buffer sections; buffer lines handled by paint_buffer_ref
        if dirty.intersects(DirtyFlags::STATUS) {
            let status_y = if state.status_at_top {
                0
            } else {
                state.rows.saturating_sub(1)
            };
            let status_rect = Rect {
                x: 0,
                y: status_y,
                w: state.cols,
                h: 1,
            };
            grid.clear_region(&status_rect, &state.status_default_face);
        }
        // Menu/info overlays paint over buffer anyway — no separate clear needed
    } else {
        grid.clear(&state.default_face);
    }
}

/// Compute the RenderResult (cursor position + style) from AppState.
fn compute_render_result(
    state: &AppState,
    registry: &PluginRegistry,
    buffer_x_offset: u16,
) -> RenderResult {
    let style = cursor_style(state, registry);
    let (cx, cy) = match state.cursor_mode {
        CursorMode::Buffer => {
            let cx = state.cursor_pos.column as u16 + buffer_x_offset;
            let cy = state.cursor_pos.line as u16;
            (cx, cy)
        }
        CursorMode::Prompt => {
            let prompt_width = line_display_width(&state.status_prompt) as u16;
            let cx = prompt_width + (state.status_content_cursor_pos.max(0) as u16);
            let cy = if state.status_at_top {
                0
            } else {
                state.rows.saturating_sub(1)
            };
            (cx, cy)
        }
    };
    RenderResult {
        cursor_x: cx,
        cursor_y: cy,
        cursor_style: style,
    }
}

/// Debug assertion: check that two grids produce equivalent content.
#[cfg(debug_assertions)]
fn debug_assert_grid_equivalent(patched: &CellGrid, reference: &CellGrid, _state: &AppState) {
    assert_eq!(
        patched.width(),
        reference.width(),
        "grid width mismatch in patch correctness check"
    );
    assert_eq!(
        patched.height(),
        reference.height(),
        "grid height mismatch in patch correctness check"
    );
    for y in 0..patched.height() {
        for x in 0..patched.width() {
            let p = patched.get(x, y);
            let r = reference.get(x, y);
            if let (Some(p), Some(r)) = (p, r) {
                debug_assert_eq!(
                    p.grapheme, r.grapheme,
                    "patch correctness: grapheme mismatch at ({x}, {y}): patch={:?} ref={:?}",
                    p.grapheme, r.grapheme
                );
                debug_assert_eq!(
                    p.face, r.face,
                    "patch correctness: face mismatch at ({x}, {y})"
                );
            }
        }
    }
}

fn backfill_surface_report_areas(
    sections: &mut view::ViewSections,
    root_area: Rect,
    state: &AppState,
) -> flex::LayoutResult {
    let base_layout = flex::place(&sections.base, root_area, state);
    crate::surface::resolve::backfill_surface_report_areas(
        &mut sections.surface_reports,
        &sections.base,
        &base_layout,
    );
    base_layout
}

// ---------------------------------------------------------------------------
// Generic core pipeline functions
// ---------------------------------------------------------------------------

/// Core cached rendering pipeline, generic over the view section source.
pub(crate) fn render_cached_core(
    source: &impl ViewSource,
    state: &AppState,
    registry: &PluginRegistry,
    grid: &mut CellGrid,
    dirty: DirtyFlags,
    cache: &mut ViewCache,
    paint_hooks: &[Box<dyn PaintHook>],
) -> RenderResult {
    crate::perf::perf_span!("render_pipeline");

    source.invalidate_view_cache(dirty, registry, cache);
    let mut sections = source.view_sections(state, registry, cache);
    let root_area = Rect {
        x: 0,
        y: 0,
        w: state.cols,
        h: state.rows,
    };
    let _base_layout = backfill_surface_report_areas(&mut sections, root_area, state);
    let element = sections.into_element();
    let layout_result = flex::place(&element, root_area, state);

    // Line-level dirty optimization: when BUFFER is dirty and some lines
    // are clean, skip full grid.clear() and only clear non-buffer sections.
    // paint_buffer_ref() skips clean lines, reusing previous frame content.
    selective_clear(grid, state, dirty);
    let theme = Theme::default_theme();
    walk::walk_paint_grid(&element, &layout_result, grid, state, &theme);

    // Apply plugin paint hooks after standard paint
    if !paint_hooks.is_empty() {
        apply_paint_hooks(paint_hooks, grid, &root_area, state, dirty);
    }

    let buffer_x_offset = find_buffer_x_offset(&element, &layout_result);

    // Differentiate secondary cursor faces before clearing primary cursor
    apply_secondary_cursor_faces(state, grid, buffer_x_offset);

    let style = cursor_style(state, registry);
    clear_block_cursor_face(state, grid, style, buffer_x_offset);
    let (cx, cy) = cursor_position(state, grid, buffer_x_offset);

    RenderResult {
        cursor_x: cx,
        cursor_y: cy,
        cursor_style: style,
    }
}

/// Core section-aware rendering pipeline, generic over the view section source.
///
/// When only one section is dirty, repaints only that section's region
/// instead of clearing and repainting the entire grid.
/// Falls back to `render_cached_core` when multiple sections are dirty
/// or the layout cache is cold.
#[allow(clippy::too_many_arguments)]
pub(crate) fn render_sectioned_core(
    source: &impl ViewSource,
    state: &AppState,
    registry: &PluginRegistry,
    grid: &mut CellGrid,
    dirty: DirtyFlags,
    view_cache: &mut ViewCache,
    layout_cache: &mut LayoutCache,
    paint_hooks: &[Box<dyn PaintHook>],
) -> RenderResult {
    crate::perf::perf_span!("render_pipeline_sectioned");

    layout_cache.invalidate(dirty, state.cols, state.rows);

    let root_area = Rect {
        x: 0,
        y: 0,
        w: state.cols,
        h: state.rows,
    };

    // Only STATUS dirty: repaint just the status bar row
    if dirty == DirtyFlags::STATUS {
        let status_y = layout_cache.status_row.unwrap_or_else(|| {
            if state.status_at_top {
                0
            } else {
                state.rows.saturating_sub(1)
            }
        });

        source.invalidate_view_cache(dirty, registry, view_cache);
        let mut sections = source.view_sections(state, registry, view_cache);
        let _base_layout = backfill_surface_report_areas(&mut sections, root_area, state);
        let element = sections.into_element();
        let layout_result = flex::place(&element, root_area, state);

        layout_cache.status_row = Some(status_y);
        layout_cache.root_area = Some(root_area);
        layout_cache.base_layout = Some(layout_result.clone());

        let status_rect = Rect {
            x: 0,
            y: status_y,
            w: state.cols,
            h: 1,
        };
        grid.clear_region(&status_rect, &state.status_default_face);
        let theme = Theme::default_theme();
        walk::walk_paint_grid(&element, &layout_result, grid, state, &theme);

        let buffer_x_offset = find_buffer_x_offset(&element, &layout_result);
        let style = cursor_style(state, registry);
        clear_block_cursor_face(state, grid, style, buffer_x_offset);
        let (cx, cy) = cursor_position(state, grid, buffer_x_offset);
        return RenderResult {
            cursor_x: cx,
            cursor_y: cy,
            cursor_style: style,
        };
    }

    // Only MENU_SELECTION dirty: repaint just the menu overlay area
    if dirty == DirtyFlags::MENU_SELECTION && state.menu.is_some() {
        source.invalidate_view_cache(dirty, registry, view_cache);
        let mut sections = source.view_sections(state, registry, view_cache);

        let menu_rect = sections
            .menu_overlay
            .as_ref()
            .map(|overlay| crate::layout::layout_single_overlay(overlay, root_area, state).area);

        if let Some(menu_rect) = menu_rect {
            let _base_layout = backfill_surface_report_areas(&mut sections, root_area, state);
            let element = sections.into_element();
            let layout_result = flex::place(&element, root_area, state);

            grid.clear_region(&menu_rect, &state.default_face);
            let theme = Theme::default_theme();
            walk::walk_paint_grid(&element, &layout_result, grid, state, &theme);

            layout_cache.root_area = Some(root_area);
            layout_cache.base_layout = Some(layout_result.clone());

            let buffer_x_offset = find_buffer_x_offset(&element, &layout_result);
            let style = cursor_style(state, registry);
            clear_block_cursor_face(state, grid, style, buffer_x_offset);
            let (cx, cy) = cursor_position(state, grid, buffer_x_offset);
            return RenderResult {
                cursor_x: cx,
                cursor_y: cy,
                cursor_style: style,
            };
        }
    }

    // Fallback: full pipeline
    let result = render_cached_core(
        source,
        state,
        registry,
        grid,
        dirty,
        view_cache,
        paint_hooks,
    );

    // Update layout cache from the full render
    let mut sections = source.view_sections(state, registry, view_cache);
    let _base_layout = backfill_surface_report_areas(&mut sections, root_area, state);
    let element = sections.into_element();
    let layout_result = flex::place(&element, root_area, state);
    let status_y = if state.status_at_top {
        0
    } else {
        state.rows.saturating_sub(1)
    };
    layout_cache.status_row = Some(status_y);
    layout_cache.root_area = Some(root_area);
    layout_cache.base_layout = Some(layout_result);

    result
}

/// Core patched rendering pipeline, generic over the view section source.
///
/// Tries compiled paint patches first (direct cell writes), then falls through
/// to section-level paint, then to the full cached pipeline.
///
/// In debug builds, after applying a patch, runs the full interpreter pipeline
/// and asserts CellGrid equivalence (correctness invariant).
#[allow(clippy::too_many_arguments)]
pub(crate) fn render_patched_core(
    source: &impl ViewSource,
    state: &AppState,
    registry: &PluginRegistry,
    grid: &mut CellGrid,
    dirty: DirtyFlags,
    view_cache: &mut ViewCache,
    layout_cache: &mut LayoutCache,
    patches: &[&dyn patch::PaintPatch],
    paint_hooks: &[Box<dyn PaintHook>],
) -> RenderResult {
    crate::perf::perf_span!("render_pipeline_patched");

    // Try each patch
    let plugins_changed = registry.any_plugin_state_changed();
    if patch::try_apply_grid_patch(patches, grid, state, dirty, layout_cache, plugins_changed) {
        // Compute buffer_x_offset from cached layout if available
        let buffer_x_offset = if let Some(ref base_layout) = layout_cache.base_layout {
            let element = source
                .view_sections(state, registry, view_cache)
                .into_element();
            find_buffer_x_offset(&element, base_layout)
        } else {
            0
        };
        let style = cursor_style(state, registry);
        clear_block_cursor_face(state, grid, style, buffer_x_offset);
        let (cx, cy) = cursor_position(state, grid, buffer_x_offset);

        let result = RenderResult {
            cursor_x: cx,
            cursor_y: cy,
            cursor_style: style,
        };

        // Debug correctness check: verify patch output matches full pipeline
        #[cfg(debug_assertions)]
        {
            let mut ref_grid = CellGrid::new(grid.width(), grid.height());
            let mut ref_cache = ViewCache::new();
            render_cached_core(
                source,
                state,
                registry,
                &mut ref_grid,
                DirtyFlags::ALL,
                &mut ref_cache,
                &[],
            );
            debug_assert_grid_equivalent(grid, &ref_grid, state);
        }

        // Still invalidate view cache for future renders
        source.invalidate_view_cache(dirty, registry, view_cache);
        return result;
    }

    // Fall through to section-level paint
    render_sectioned_core(
        source,
        state,
        registry,
        grid,
        dirty,
        view_cache,
        layout_cache,
        paint_hooks,
    )
}

/// Core scene rendering pipeline, generic over the view section source.
///
/// Returns a slice into the SceneCache's composed buffer and the RenderResult.
/// Per-section invalidation: only dirty sections are re-rendered.
pub(crate) fn scene_render_core<'a>(
    source: &impl ViewSource,
    state: &AppState,
    registry: &PluginRegistry,
    cell_size: scene::CellSize,
    dirty: DirtyFlags,
    view_cache: &mut ViewCache,
    scene_cache: &'a mut SceneCache,
) -> (&'a [DrawCommand], RenderResult) {
    crate::perf::perf_span!("scene_render_pipeline");

    // Invalidate both caches
    source.invalidate_view_cache(dirty, registry, view_cache);
    scene_cache.invalidate(dirty, cell_size, state.cols, state.rows);

    // Get view sections (uses ViewCache) — needed for buffer_x_offset even on fast path
    let mut sections = source.view_sections(state, registry, view_cache);

    let root_area = Rect {
        x: 0,
        y: 0,
        w: state.cols,
        h: state.rows,
    };

    // Compute buffer_x_offset from the base layout
    let base_layout = backfill_surface_report_areas(&mut sections, root_area, state);
    let buffer_x_offset = find_buffer_x_offset(&sections.base, &base_layout);
    let result = compute_render_result(state, registry, buffer_x_offset);

    // Fast path: all sections cached
    if scene_cache.is_fully_cached() {
        scene_cache.compose();
        return (scene_cache.composed_ref(), result);
    }

    let theme = Theme::default_theme();

    // Base section
    if scene_cache.base_commands.is_none() {
        let cmds = walk::walk_paint_scene_section(
            &sections.base,
            &base_layout,
            state,
            &theme,
            cell_size,
            result.cursor_style,
        );
        scene_cache.base_commands = Some(cmds);
    }

    // Menu section
    if scene_cache.menu_commands.is_none() {
        let cmds = if let Some(ref overlay) = sections.menu_overlay {
            let overlay_layout = crate::layout::layout_single_overlay(overlay, root_area, state);
            walk::walk_paint_scene_section(
                &overlay.element,
                &overlay_layout,
                state,
                &theme,
                cell_size,
                result.cursor_style,
            )
        } else {
            Vec::new()
        };
        scene_cache.menu_commands = Some(cmds);
    }

    // Info + plugin overlays section
    if scene_cache.info_commands.is_none() {
        let mut cmds = Vec::new();
        for overlay in sections
            .info_overlays
            .iter()
            .chain(sections.plugin_overlays.iter())
        {
            cmds.push(DrawCommand::BeginOverlay);
            let overlay_layout = crate::layout::layout_single_overlay(overlay, root_area, state);
            let overlay_cmds = walk::walk_paint_scene_section(
                &overlay.element,
                &overlay_layout,
                state,
                &theme,
                cell_size,
                result.cursor_style,
            );
            cmds.extend(overlay_cmds);
        }
        scene_cache.info_commands = Some(cmds);
    }

    scene_cache.compose();
    (scene_cache.composed_ref(), result)
}

// ---------------------------------------------------------------------------
// Public API: backward-compatible wrappers
// ---------------------------------------------------------------------------

/// GUI scene rendering pipeline (backward-compatible).
pub fn scene_render_pipeline(
    state: &AppState,
    registry: &PluginRegistry,
    cell_size: scene::CellSize,
) -> (Vec<DrawCommand>, RenderResult) {
    scene_render_pipeline_cached(
        state,
        registry,
        cell_size,
        DirtyFlags::ALL,
        &mut ViewCache::new(),
    )
}

/// GUI scene rendering pipeline (cached variant — ViewCache only).
pub(crate) fn scene_render_pipeline_cached(
    state: &AppState,
    registry: &PluginRegistry,
    cell_size: scene::CellSize,
    dirty: DirtyFlags,
    view_cache: &mut ViewCache,
) -> (Vec<DrawCommand>, RenderResult) {
    let mut scene_cache = SceneCache::new();
    let (commands, result) = scene_render_pipeline_scene_cached(
        state,
        registry,
        cell_size,
        dirty,
        view_cache,
        &mut scene_cache,
    );
    (commands.to_vec(), result)
}

/// GUI scene rendering pipeline with DrawCommand-level caching.
pub fn scene_render_pipeline_scene_cached<'a>(
    state: &AppState,
    registry: &PluginRegistry,
    cell_size: scene::CellSize,
    dirty: DirtyFlags,
    view_cache: &mut ViewCache,
    scene_cache: &'a mut SceneCache,
) -> (&'a [DrawCommand], RenderResult) {
    scene_render_core(
        &PluginViewSource,
        state,
        registry,
        cell_size,
        dirty,
        view_cache,
        scene_cache,
    )
}

/// Declarative rendering pipeline (backward-compatible).
pub fn render_pipeline(
    state: &AppState,
    registry: &PluginRegistry,
    grid: &mut CellGrid,
) -> RenderResult {
    render_pipeline_cached(
        state,
        registry,
        grid,
        DirtyFlags::ALL,
        &mut ViewCache::new(),
    )
}

/// Declarative rendering pipeline (cached variant).
pub fn render_pipeline_cached(
    state: &AppState,
    registry: &PluginRegistry,
    grid: &mut CellGrid,
    dirty: DirtyFlags,
    cache: &mut ViewCache,
) -> RenderResult {
    render_pipeline_cached_with_hooks(state, registry, grid, dirty, cache, &[])
}

/// Declarative rendering pipeline (cached variant with paint hooks).
pub(crate) fn render_pipeline_cached_with_hooks(
    state: &AppState,
    registry: &PluginRegistry,
    grid: &mut CellGrid,
    dirty: DirtyFlags,
    cache: &mut ViewCache,
    paint_hooks: &[Box<dyn PaintHook>],
) -> RenderResult {
    render_cached_core(
        &PluginViewSource,
        state,
        registry,
        grid,
        dirty,
        cache,
        paint_hooks,
    )
}

/// Section-aware rendering pipeline (S1).
pub fn render_pipeline_sectioned(
    state: &AppState,
    registry: &PluginRegistry,
    grid: &mut CellGrid,
    dirty: DirtyFlags,
    view_cache: &mut ViewCache,
    layout_cache: &mut LayoutCache,
) -> RenderResult {
    render_sectioned_core(
        &PluginViewSource,
        state,
        registry,
        grid,
        dirty,
        view_cache,
        layout_cache,
        &[],
    )
}

/// Patched rendering pipeline (S3).
pub fn render_pipeline_patched(
    state: &AppState,
    registry: &PluginRegistry,
    grid: &mut CellGrid,
    dirty: DirtyFlags,
    view_cache: &mut ViewCache,
    layout_cache: &mut LayoutCache,
    patches: &[&dyn patch::PaintPatch],
) -> RenderResult {
    render_patched_core(
        &PluginViewSource,
        state,
        registry,
        grid,
        dirty,
        view_cache,
        layout_cache,
        patches,
        &[],
    )
}
