use super::cache::LayoutCache;
use super::cursor::{
    apply_secondary_cursor_faces, clear_block_cursor_face, cursor_position, cursor_style,
    find_buffer_x_offset,
};
use super::grid::CellGrid;
use super::scene::{self, DrawCommand, SceneCache};
use super::theme::Theme;
use super::walk;
use super::{RenderResult, view};
use crate::display::{DisplayMap, DisplayMapRef};
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
/// - `DirectViewSource`: builds sections from `PluginRegistry` without caching
/// - `SalsaViewSource`: reads from Salsa tracked functions (production path)
pub(crate) trait ViewSource {
    /// Prepare for a new frame: invalidate internal caches if needed.
    fn prepare(&mut self, dirty: DirtyFlags, registry: &PluginRegistry);

    /// Build the decomposed view sections.
    fn view_sections(&mut self, state: &AppState, registry: &PluginRegistry) -> view::ViewSections;
}

/// Builds view sections directly from PluginRegistry without any memoization.
pub(crate) struct DirectViewSource;

impl ViewSource for DirectViewSource {
    fn prepare(&mut self, _dirty: DirtyFlags, _registry: &PluginRegistry) {
        // No cache to invalidate
    }

    fn view_sections(&mut self, state: &AppState, registry: &PluginRegistry) -> view::ViewSections {
        view::view_sections(state, registry)
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
    display_map: Option<&DisplayMap>,
) -> RenderResult {
    let style = cursor_style(state, registry);
    let (cx, cy) = match state.cursor_mode {
        CursorMode::Buffer => {
            let cx = state.cursor_pos.column as u16 + buffer_x_offset;
            let cy = display_map
                .filter(|dm| !dm.is_identity())
                .and_then(|dm| dm.buffer_to_display(state.cursor_pos.line as usize))
                .map(|y| y as u16)
                .unwrap_or(state.cursor_pos.line as u16);
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

/// Extract a display_map Option reference from a DisplayMapRef,
/// returning None when the map is identity (optimization).
fn dm_ref(dm: &DisplayMapRef) -> Option<&DisplayMap> {
    if dm.is_identity() { None } else { Some(dm) }
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
    source: &mut impl ViewSource,
    state: &AppState,
    registry: &PluginRegistry,
    grid: &mut CellGrid,
    dirty: DirtyFlags,
    paint_hooks: &[Box<dyn PaintHook>],
) -> RenderResult {
    crate::perf::perf_span!("render_pipeline");

    source.prepare(dirty, registry);
    let mut sections = source.view_sections(state, registry);
    let display_map = std::sync::Arc::clone(&sections.display_map);
    let dm = dm_ref(&display_map);
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
    apply_secondary_cursor_faces(state, grid, buffer_x_offset, dm);

    let style = cursor_style(state, registry);
    clear_block_cursor_face(state, grid, style, buffer_x_offset, dm);
    let (cx, cy) = cursor_position(state, grid, buffer_x_offset, dm);

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
    source: &mut impl ViewSource,
    state: &AppState,
    registry: &PluginRegistry,
    grid: &mut CellGrid,
    dirty: DirtyFlags,
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

        source.prepare(dirty, registry);
        let mut sections = source.view_sections(state, registry);
        let display_map = std::sync::Arc::clone(&sections.display_map);
        let dm = dm_ref(&display_map);
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
        clear_block_cursor_face(state, grid, style, buffer_x_offset, dm);
        let (cx, cy) = cursor_position(state, grid, buffer_x_offset, dm);
        return RenderResult {
            cursor_x: cx,
            cursor_y: cy,
            cursor_style: style,
        };
    }

    // Only MENU_SELECTION dirty: repaint just the menu overlay area
    if dirty == DirtyFlags::MENU_SELECTION && state.menu.is_some() {
        source.prepare(dirty, registry);
        let mut sections = source.view_sections(state, registry);
        let display_map = std::sync::Arc::clone(&sections.display_map);
        let dm = dm_ref(&display_map);

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
            clear_block_cursor_face(state, grid, style, buffer_x_offset, dm);
            let (cx, cy) = cursor_position(state, grid, buffer_x_offset, dm);
            return RenderResult {
                cursor_x: cx,
                cursor_y: cy,
                cursor_style: style,
            };
        }
    }

    // Fallback: full pipeline
    let result = render_cached_core(source, state, registry, grid, dirty, paint_hooks);

    // Update layout cache from the full render
    let mut sections = source.view_sections(state, registry);
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

/// Core scene rendering pipeline, generic over the view section source.
///
/// Returns a slice into the SceneCache's composed buffer and the RenderResult.
/// Per-section invalidation: only dirty sections are re-rendered.
pub(crate) fn scene_render_core<'a>(
    source: &mut impl ViewSource,
    state: &AppState,
    registry: &PluginRegistry,
    cell_size: scene::CellSize,
    dirty: DirtyFlags,
    scene_cache: &'a mut SceneCache,
) -> (&'a [DrawCommand], RenderResult) {
    crate::perf::perf_span!("scene_render_pipeline");

    // Invalidate caches
    source.prepare(dirty, registry);
    scene_cache.invalidate(dirty, cell_size, state.cols, state.rows);

    // Get view sections — needed for buffer_x_offset even on fast path
    let mut sections = source.view_sections(state, registry);

    let root_area = Rect {
        x: 0,
        y: 0,
        w: state.cols,
        h: state.rows,
    };

    // Compute buffer_x_offset from the base layout
    let display_map = std::sync::Arc::clone(&sections.display_map);
    let dm = dm_ref(&display_map);
    let base_layout = backfill_surface_report_areas(&mut sections, root_area, state);
    let buffer_x_offset = find_buffer_x_offset(&sections.base, &base_layout);
    let result = compute_render_result(state, registry, buffer_x_offset, dm);

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
// Public API
// ---------------------------------------------------------------------------

/// GUI scene rendering pipeline.
pub fn scene_render_pipeline(
    state: &AppState,
    registry: &PluginRegistry,
    cell_size: scene::CellSize,
) -> (Vec<DrawCommand>, RenderResult) {
    let mut scene_cache = SceneCache::new();
    let mut source = DirectViewSource;
    let (commands, result) = scene_render_core(
        &mut source,
        state,
        registry,
        cell_size,
        DirtyFlags::ALL,
        &mut scene_cache,
    );
    (commands.to_vec(), result)
}

/// Declarative rendering pipeline.
pub fn render_pipeline(
    state: &AppState,
    registry: &PluginRegistry,
    grid: &mut CellGrid,
) -> RenderResult {
    let mut source = DirectViewSource;
    render_cached_core(&mut source, state, registry, grid, DirtyFlags::ALL, &[])
}

/// Declarative rendering pipeline with explicit dirty flags for incremental rendering.
pub fn render_pipeline_cached(
    state: &AppState,
    registry: &PluginRegistry,
    grid: &mut CellGrid,
    dirty: DirtyFlags,
) -> RenderResult {
    let mut source = DirectViewSource;
    render_cached_core(&mut source, state, registry, grid, dirty, &[])
}
