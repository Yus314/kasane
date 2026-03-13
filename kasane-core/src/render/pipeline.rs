use super::cache::{LayoutCache, ViewCache};
use super::cursor::{
    apply_secondary_cursor_faces, clear_block_cursor_face, cursor_position, cursor_style,
    find_buffer_x_offset,
};
use super::grid::CellGrid;
use super::scene::{self, DrawCommand, SceneCache};
use super::theme::Theme;
use super::{RenderResult, paint, patch, view};
use crate::layout::Rect;
use crate::layout::flex;
use crate::layout::line_display_width;
use crate::plugin::PaintHook;
use crate::plugin::PluginRegistry;
use crate::protocol::CursorMode;
use crate::state::{AppState, DirtyFlags};
use crate::surface::SurfaceRegistry;

/// Apply paint hooks that match the current dirty flags.
pub fn apply_paint_hooks(
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

/// GUI 用シーンレンダリングパイプライン (backward-compatible).
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

/// GUI 用シーンレンダリングパイプライン (cached variant — ViewCache only).
pub fn scene_render_pipeline_cached(
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

/// GUI 用シーンレンダリングパイプライン with DrawCommand-level caching.
///
/// Returns a slice into the SceneCache's composed buffer and the RenderResult.
/// Per-section invalidation: only dirty sections are re-rendered.
pub fn scene_render_pipeline_scene_cached<'a>(
    state: &AppState,
    registry: &PluginRegistry,
    cell_size: scene::CellSize,
    dirty: DirtyFlags,
    view_cache: &mut ViewCache,
    scene_cache: &'a mut SceneCache,
) -> (&'a [DrawCommand], RenderResult) {
    crate::perf::perf_span!("scene_render_pipeline_scene_cached");

    // Invalidate both caches
    view_cache.invalidate(dirty);
    scene_cache.invalidate(dirty, cell_size, state.cols, state.rows);

    // Get view sections (uses ViewCache) — needed for buffer_x_offset even on fast path
    let sections = view::view_sections_cached(state, registry, view_cache);

    let root_area = Rect {
        x: 0,
        y: 0,
        w: state.cols,
        h: state.rows,
    };

    // Compute buffer_x_offset from the base layout
    let base_layout = flex::place(&sections.base, root_area, state);
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
        let cmds = scene::scene_paint_section(
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
            scene::scene_paint_section(
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
            let overlay_cmds = scene::scene_paint_section(
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

/// 宣言的レンダリングパイプライン (backward-compatible).
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

/// 宣言的レンダリングパイプライン (cached variant).
pub fn render_pipeline_cached(
    state: &AppState,
    registry: &PluginRegistry,
    grid: &mut CellGrid,
    dirty: DirtyFlags,
    cache: &mut ViewCache,
) -> RenderResult {
    render_pipeline_cached_with_hooks(state, registry, grid, dirty, cache, &[])
}

/// 宣言的レンダリングパイプライン (cached variant with paint hooks).
pub fn render_pipeline_cached_with_hooks(
    state: &AppState,
    registry: &PluginRegistry,
    grid: &mut CellGrid,
    dirty: DirtyFlags,
    cache: &mut ViewCache,
    paint_hooks: &[Box<dyn PaintHook>],
) -> RenderResult {
    crate::perf::perf_span!("render_pipeline");

    cache.invalidate(dirty);
    let element = view::view_cached(state, registry, cache);
    let root_area = Rect {
        x: 0,
        y: 0,
        w: state.cols,
        h: state.rows,
    };
    let layout_result = flex::place(&element, root_area, state);

    // Line-level dirty optimization: when only BUFFER is dirty and some lines
    // are clean, skip grid.clear() and let paint_buffer_ref() skip those lines.
    // The grid retains valid content from the previous frame for clean rows.
    let use_line_dirty = dirty == DirtyFlags::BUFFER
        && !state.lines_dirty.is_empty()
        && state.lines_dirty.iter().any(|d| !d);

    if !use_line_dirty {
        grid.clear(&state.default_face);
    }
    paint::paint(&element, &layout_result, grid, state);

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

/// Section-aware rendering pipeline (S1).
///
/// When only one section is dirty, repaints only that section's region
/// instead of clearing and repainting the entire grid.
/// Falls back to full `render_pipeline_cached` when multiple sections
/// are dirty or the layout cache is cold.
pub fn render_pipeline_sectioned(
    state: &AppState,
    registry: &PluginRegistry,
    grid: &mut CellGrid,
    dirty: DirtyFlags,
    view_cache: &mut ViewCache,
    layout_cache: &mut LayoutCache,
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
        // We need the status row position. If we have it cached, use it.
        // Otherwise, do a full layout to find it.
        let status_y = layout_cache.status_row.unwrap_or_else(|| {
            if state.status_at_top {
                0
            } else {
                state.rows.saturating_sub(1)
            }
        });

        // Rebuild only the view sections that changed
        view_cache.invalidate(dirty);
        let sections = view::view_sections_cached(state, registry, view_cache);
        let element = sections.into_element();
        let layout_result = flex::place(&element, root_area, state);

        // Cache layout info for next time
        layout_cache.status_row = Some(status_y);
        layout_cache.root_area = Some(root_area);
        layout_cache.base_layout = Some(layout_result.clone());

        // Clear and repaint only the status bar row
        let status_rect = Rect {
            x: 0,
            y: status_y,
            w: state.cols,
            h: 1,
        };
        grid.clear_region(&status_rect, &state.status_default_face);
        paint::paint(&element, &layout_result, grid, state);

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
        view_cache.invalidate(dirty);
        let sections = view::view_sections_cached(state, registry, view_cache);

        // Compute the overlay rect before consuming sections
        let menu_rect = sections
            .menu_overlay
            .as_ref()
            .map(|overlay| crate::layout::layout_single_overlay(overlay, root_area, state).area);

        if let Some(menu_rect) = menu_rect {
            let element = sections.into_element();
            let layout_result = flex::place(&element, root_area, state);

            // Clear and repaint the menu region
            grid.clear_region(&menu_rect, &state.default_face);
            paint::paint(&element, &layout_result, grid, state);

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
    let result = render_pipeline_cached(state, registry, grid, dirty, view_cache);

    // Update layout cache from the full render
    let element = view::view_cached(state, registry, view_cache);
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

/// Patched rendering pipeline (S3).
///
/// Tries compiled paint patches first (direct cell writes), then falls through
/// to section-level paint (S1), then to the full cached pipeline.
///
/// In debug builds, after applying a patch, runs the full interpreter pipeline
/// and asserts CellGrid equivalence (correctness invariant).
pub fn render_pipeline_patched(
    state: &AppState,
    registry: &PluginRegistry,
    grid: &mut CellGrid,
    dirty: DirtyFlags,
    view_cache: &mut ViewCache,
    layout_cache: &mut LayoutCache,
    patches: &[&dyn patch::PaintPatch],
) -> RenderResult {
    crate::perf::perf_span!("render_pipeline_patched");

    // Try each patch
    let plugins_changed = registry.any_plugin_state_changed();
    if patch::try_apply_grid_patch(patches, grid, state, dirty, layout_cache, plugins_changed) {
        // Compute buffer_x_offset from cached layout if available
        let buffer_x_offset = if let Some(ref base_layout) = layout_cache.base_layout {
            let element = view::view_cached(state, registry, view_cache);
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
            render_pipeline_cached(
                state,
                registry,
                &mut ref_grid,
                DirtyFlags::ALL,
                &mut ref_cache,
            );
            debug_assert_grid_equivalent(grid, &ref_grid, state);
        }

        // Still invalidate view cache for future renders
        view_cache.invalidate(dirty);
        return result;
    }

    // Fall through to section-level paint (S1)
    render_pipeline_sectioned(state, registry, grid, dirty, view_cache, layout_cache)
}

/// Debug assertion: check that two grids produce equivalent content.
/// Only checks cells in rows that are dirty in the patched grid.
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

// ---------------------------------------------------------------------------
// Surface-based rendering pipeline (cached)
// ---------------------------------------------------------------------------

/// Surface-based cached rendering pipeline (TUI).
///
/// Uses `SurfaceRegistry` as the element source while maintaining all
/// caching optimizations (ViewCache, line-dirty, paint hooks).
pub fn render_pipeline_surfaces_cached(
    state: &AppState,
    plugin_registry: &PluginRegistry,
    surface_registry: &SurfaceRegistry,
    grid: &mut CellGrid,
    dirty: DirtyFlags,
    cache: &mut ViewCache,
    paint_hooks: &[Box<dyn PaintHook>],
) -> RenderResult {
    crate::perf::perf_span!("render_pipeline_surfaces_cached");

    cache.invalidate(dirty);
    let sections =
        view::surface_view_sections_cached(state, plugin_registry, surface_registry, cache);
    let element = sections.into_element();
    let root_area = Rect {
        x: 0,
        y: 0,
        w: state.cols,
        h: state.rows,
    };
    let layout_result = flex::place(&element, root_area, state);

    let use_line_dirty = dirty == DirtyFlags::BUFFER
        && !state.lines_dirty.is_empty()
        && state.lines_dirty.iter().any(|d| !d);

    if !use_line_dirty {
        grid.clear(&state.default_face);
    }
    paint::paint(&element, &layout_result, grid, state);

    if !paint_hooks.is_empty() {
        apply_paint_hooks(paint_hooks, grid, &root_area, state, dirty);
    }

    let buffer_x_offset = find_buffer_x_offset(&element, &layout_result);
    apply_secondary_cursor_faces(state, grid, buffer_x_offset);

    let style = cursor_style(state, plugin_registry);
    clear_block_cursor_face(state, grid, style, buffer_x_offset);
    let (cx, cy) = cursor_position(state, grid, buffer_x_offset);

    RenderResult {
        cursor_x: cx,
        cursor_y: cy,
        cursor_style: style,
    }
}

/// Surface-based section-aware rendering pipeline (TUI).
///
/// Mirrors `render_pipeline_sectioned` but uses SurfaceRegistry.
#[allow(clippy::too_many_arguments)]
pub fn render_pipeline_surfaces_sectioned(
    state: &AppState,
    plugin_registry: &PluginRegistry,
    surface_registry: &SurfaceRegistry,
    grid: &mut CellGrid,
    dirty: DirtyFlags,
    view_cache: &mut ViewCache,
    layout_cache: &mut LayoutCache,
    paint_hooks: &[Box<dyn PaintHook>],
) -> RenderResult {
    crate::perf::perf_span!("render_pipeline_surfaces_sectioned");

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

        view_cache.invalidate(dirty);
        let sections = view::surface_view_sections_cached(
            state,
            plugin_registry,
            surface_registry,
            view_cache,
        );
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
        paint::paint(&element, &layout_result, grid, state);

        let buffer_x_offset = find_buffer_x_offset(&element, &layout_result);
        let style = cursor_style(state, plugin_registry);
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
        view_cache.invalidate(dirty);
        let sections = view::surface_view_sections_cached(
            state,
            plugin_registry,
            surface_registry,
            view_cache,
        );
        let menu_rect = sections
            .menu_overlay
            .as_ref()
            .map(|overlay| crate::layout::layout_single_overlay(overlay, root_area, state).area);

        if let Some(menu_rect) = menu_rect {
            let element = sections.into_element();
            let layout_result = flex::place(&element, root_area, state);

            grid.clear_region(&menu_rect, &state.default_face);
            paint::paint(&element, &layout_result, grid, state);

            layout_cache.root_area = Some(root_area);
            layout_cache.base_layout = Some(layout_result.clone());

            let buffer_x_offset = find_buffer_x_offset(&element, &layout_result);
            let style = cursor_style(state, plugin_registry);
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
    let result = render_pipeline_surfaces_cached(
        state,
        plugin_registry,
        surface_registry,
        grid,
        dirty,
        view_cache,
        paint_hooks,
    );

    let element =
        view::surface_view_sections_cached(state, plugin_registry, surface_registry, view_cache)
            .into_element();
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

/// Surface-based patched rendering pipeline (TUI).
///
/// Mirrors `render_pipeline_patched` but uses SurfaceRegistry.
/// Tries compiled paint patches first, then section-level, then full pipeline.
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
    crate::perf::perf_span!("render_pipeline_surfaces_patched");

    // Try each patch
    let plugins_changed = plugin_registry.any_plugin_state_changed();
    if patch::try_apply_grid_patch(patches, grid, state, dirty, layout_cache, plugins_changed) {
        let buffer_x_offset = if let Some(ref base_layout) = layout_cache.base_layout {
            let element = view::surface_view_sections_cached(
                state,
                plugin_registry,
                surface_registry,
                view_cache,
            )
            .into_element();
            find_buffer_x_offset(&element, base_layout)
        } else {
            0
        };
        let style = cursor_style(state, plugin_registry);
        clear_block_cursor_face(state, grid, style, buffer_x_offset);
        let (cx, cy) = cursor_position(state, grid, buffer_x_offset);

        let result = RenderResult {
            cursor_x: cx,
            cursor_y: cy,
            cursor_style: style,
        };

        #[cfg(debug_assertions)]
        {
            let mut ref_grid = CellGrid::new(grid.width(), grid.height());
            let mut ref_cache = ViewCache::new();
            render_pipeline_surfaces_cached(
                state,
                plugin_registry,
                surface_registry,
                &mut ref_grid,
                DirtyFlags::ALL,
                &mut ref_cache,
                &[],
            );
            debug_assert_grid_equivalent(grid, &ref_grid, state);
        }

        view_cache.invalidate(dirty);
        return result;
    }

    // Fall through to section-level
    render_pipeline_surfaces_sectioned(
        state,
        plugin_registry,
        surface_registry,
        grid,
        dirty,
        view_cache,
        layout_cache,
        paint_hooks,
    )
}

/// Surface-based rendering pipeline (GPU).
///
/// Uses `SurfaceRegistry` via cached view sections for the GPU backend.
pub fn scene_render_pipeline_surfaces_cached<'a>(
    state: &AppState,
    plugin_registry: &PluginRegistry,
    surface_registry: &SurfaceRegistry,
    cell_size: scene::CellSize,
    dirty: DirtyFlags,
    view_cache: &mut ViewCache,
    scene_cache: &'a mut SceneCache,
) -> (&'a [DrawCommand], RenderResult) {
    crate::perf::perf_span!("scene_render_pipeline_surfaces_cached");

    view_cache.invalidate(dirty);
    scene_cache.invalidate(dirty, cell_size, state.cols, state.rows);

    let sections =
        view::surface_view_sections_cached(state, plugin_registry, surface_registry, view_cache);

    let root_area = Rect {
        x: 0,
        y: 0,
        w: state.cols,
        h: state.rows,
    };

    let base_layout = flex::place(&sections.base, root_area, state);
    let buffer_x_offset = find_buffer_x_offset(&sections.base, &base_layout);
    let result = compute_render_result(state, plugin_registry, buffer_x_offset);

    if scene_cache.is_fully_cached() {
        scene_cache.compose();
        return (scene_cache.composed_ref(), result);
    }

    let theme = Theme::default_theme();

    if scene_cache.base_commands.is_none() {
        let cmds = scene::scene_paint_section(
            &sections.base,
            &base_layout,
            state,
            &theme,
            cell_size,
            result.cursor_style,
        );
        scene_cache.base_commands = Some(cmds);
    }

    if scene_cache.menu_commands.is_none() {
        let cmds = if let Some(ref overlay) = sections.menu_overlay {
            let overlay_layout = crate::layout::layout_single_overlay(overlay, root_area, state);
            scene::scene_paint_section(
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

    if scene_cache.info_commands.is_none() {
        let mut cmds = Vec::new();
        for overlay in sections
            .info_overlays
            .iter()
            .chain(sections.plugin_overlays.iter())
        {
            cmds.push(DrawCommand::BeginOverlay);
            let overlay_layout = crate::layout::layout_single_overlay(overlay, root_area, state);
            let overlay_cmds = scene::scene_paint_section(
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
