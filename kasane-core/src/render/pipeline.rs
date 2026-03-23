use super::cursor::{
    apply_secondary_cursor_faces, clear_block_cursor_face, cursor_position, cursor_style,
    find_buffer_origin_in_rect, find_buffer_x_offset, neutralize_unfocused_cursors,
};
use super::grid::CellGrid;
use super::scene::{self, DrawCommand, SceneCache};
use super::walk;
use super::{RenderResult, view};
use crate::display::{DisplayMap, DisplayMapRef};
use crate::layout::Rect;
use crate::layout::flex;
use crate::layout::line_display_width;
use crate::plugin::PaintHook;
use crate::plugin::PluginView;
use crate::protocol::CursorMode;
use crate::state::{AppState, DirtyFlags};

// ---------------------------------------------------------------------------
// ViewSource: abstracts where view sections come from
// ---------------------------------------------------------------------------

/// Trait that abstracts the source of `ViewSections` for the rendering pipeline.
///
/// Two implementations exist:
/// - `DirectViewSource`: builds sections from `PluginRuntime` without caching
/// - `SalsaViewSource`: reads from Salsa tracked functions (production path)
pub(crate) trait ViewSource {
    /// Prepare for a new frame: invalidate internal caches if needed.
    fn prepare(&mut self, dirty: DirtyFlags, registry: &PluginView<'_>);

    /// Build the decomposed view sections.
    fn view_sections(&mut self, state: &AppState, registry: &PluginView<'_>) -> view::ViewSections;
}

/// Builds view sections directly from PluginRuntime without any memoization.
pub(crate) struct DirectViewSource;

impl ViewSource for DirectViewSource {
    fn prepare(&mut self, _dirty: DirtyFlags, _registry: &PluginView<'_>) {
        // No cache to invalidate
    }

    fn view_sections(&mut self, state: &AppState, registry: &PluginView<'_>) -> view::ViewSections {
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
    registry: &PluginView<'_>,
    buffer_x_offset: u16,
    display_map: Option<&DisplayMap>,
    buffer_y_offset: u16,
    display_scroll_offset: u16,
) -> RenderResult {
    let style = cursor_style(state, registry);
    let (cx, cy) = match state.cursor_mode {
        CursorMode::Buffer => {
            let cx = state.cursor_pos.column as u16 + buffer_x_offset;
            let cy = display_map
                .filter(|dm| !dm.is_identity())
                .and_then(|dm| dm.buffer_to_display(state.cursor_pos.line as usize))
                .map(|y| y as u16)
                .unwrap_or(state.cursor_pos.line as u16)
                .saturating_sub(display_scroll_offset)
                + buffer_y_offset;
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
        display_scroll_offset: display_scroll_offset as usize,
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
// PreparedFrame: shared pipeline orchestration
// ---------------------------------------------------------------------------

/// Pre-computed frame data shared between TUI and GPU pipelines.
///
/// Captures the common orchestration: source.prepare → view_sections → display_map
/// extraction → backfill_surface_report_areas → buffer offset computation.
pub(crate) struct PreparedFrame {
    pub sections: view::ViewSections,
    pub base_layout: flex::LayoutResult,
    pub display_map: DisplayMapRef,
    pub root_area: Rect,
    pub buffer_x_offset: u16,
    pub buffer_y_offset: u16,
    pub display_scroll_offset: usize,
    pub focused_pane_rect: Option<Rect>,
    pub focused_pane_state: Option<Box<AppState>>,
}

/// Run the shared pipeline orchestration, returning a `PreparedFrame`.
///
/// Both `render_cached_core` (TUI) and `scene_render_core` (GPU) call this
/// to avoid duplicating ~70% of their setup code.
pub(crate) fn prepare_frame(
    source: &mut impl ViewSource,
    state: &AppState,
    registry: &PluginView<'_>,
    dirty: DirtyFlags,
) -> PreparedFrame {
    source.prepare(dirty, registry);
    let mut sections = source.view_sections(state, registry);
    let display_map = std::sync::Arc::clone(&sections.display_map);
    let display_scroll_offset = sections.display_scroll_offset;
    let root_area = Rect {
        x: 0,
        y: 0,
        w: state.cols,
        h: state.rows,
    };
    let focused_pane_rect = sections.focused_pane_rect;
    let focused_pane_state = sections.focused_pane_state.take();
    let base_layout = backfill_surface_report_areas(&mut sections, root_area, state);

    // Compute buffer offset from the base element+layout.
    // This is correct regardless of overlays (which don't shift buffer position).
    let (buffer_x_offset, buffer_y_offset) = match focused_pane_rect {
        Some(ref focus_rect) => {
            find_buffer_origin_in_rect(&sections.base, &base_layout, focus_rect)
                .unwrap_or((find_buffer_x_offset(&sections.base, &base_layout), 0))
        }
        None => (find_buffer_x_offset(&sections.base, &base_layout), 0),
    };

    PreparedFrame {
        sections,
        base_layout,
        display_map,
        root_area,
        buffer_x_offset,
        buffer_y_offset,
        display_scroll_offset,
        focused_pane_rect,
        focused_pane_state,
    }
}

// ---------------------------------------------------------------------------
// Generic core pipeline functions
// ---------------------------------------------------------------------------

/// Core cached rendering pipeline, generic over the view section source.
pub(crate) fn render_cached_core(
    source: &mut impl ViewSource,
    state: &AppState,
    registry: &PluginView<'_>,
    grid: &mut CellGrid,
    dirty: DirtyFlags,
    paint_hooks: &[Box<dyn PaintHook>],
) -> RenderResult {
    crate::perf::perf_span!("render_pipeline");

    let frame = prepare_frame(source, state, registry, dirty);
    let dm = dm_ref(&frame.display_map);
    let dso = frame.display_scroll_offset as u16;
    let root_area = frame.root_area;

    let (element, layout_result) =
        frame
            .sections
            .into_element_and_layout(frame.base_layout, root_area, state);

    // Line-level dirty optimization: when BUFFER is dirty and some lines
    // are clean, skip full grid.clear() and only clear non-buffer sections.
    // paint_buffer_ref() skips clean lines, reusing previous frame content.
    selective_clear(grid, state, dirty);
    let theme = &state.theme;
    walk::walk_paint_grid(&element, &layout_result, grid, state, theme);

    // Apply plugin paint hooks after standard paint
    if !paint_hooks.is_empty() {
        apply_paint_hooks(paint_hooks, grid, &root_area, state, dirty);
    }

    // Use focused pane state for cursor operations in multi-pane mode
    let cursor_state = frame.focused_pane_state.as_deref().unwrap_or(state);

    // In multi-pane mode, remove cursor highlighting from unfocused panes
    if let Some(ref focus_rect) = frame.focused_pane_rect {
        neutralize_unfocused_cursors(
            cursor_state,
            &element,
            &layout_result,
            grid,
            focus_rect,
            dm,
            dso,
        );
    }

    // Differentiate secondary cursor faces before clearing primary cursor
    apply_secondary_cursor_faces(
        cursor_state,
        grid,
        frame.buffer_x_offset,
        dm,
        frame.buffer_y_offset,
        dso,
    );

    let style = cursor_style(cursor_state, registry);
    clear_block_cursor_face(
        cursor_state,
        grid,
        style,
        frame.buffer_x_offset,
        dm,
        frame.buffer_y_offset,
        dso,
    );
    let (cx, cy) = cursor_position(
        cursor_state,
        grid,
        frame.buffer_x_offset,
        dm,
        frame.buffer_y_offset,
        dso,
    );

    RenderResult {
        cursor_x: cx,
        cursor_y: cy,
        cursor_style: style,
        display_scroll_offset: frame.display_scroll_offset,
    }
}

/// Core scene rendering pipeline, generic over the view section source.
///
/// Returns a slice into the SceneCache's composed buffer and the RenderResult.
/// Per-section invalidation: only dirty sections are re-rendered.
pub(crate) fn scene_render_core<'a>(
    source: &mut impl ViewSource,
    state: &AppState,
    registry: &PluginView<'_>,
    cell_size: scene::CellSize,
    dirty: DirtyFlags,
    scene_cache: &'a mut SceneCache,
) -> (&'a [DrawCommand], RenderResult) {
    crate::perf::perf_span!("scene_render_pipeline");

    scene_cache.invalidate(dirty, cell_size, state.cols, state.rows);

    let frame = prepare_frame(source, state, registry, dirty);
    let dm = dm_ref(&frame.display_map);
    let dso = frame.display_scroll_offset as u16;
    let root_area = frame.root_area;

    // Use focused pane state for cursor computation in multi-pane mode
    let cursor_state = frame.focused_pane_state.as_deref().unwrap_or(state);
    let result = compute_render_result(
        cursor_state,
        registry,
        frame.buffer_x_offset,
        dm,
        frame.buffer_y_offset,
        dso,
    );

    // Fast path: all sections cached
    if scene_cache.is_fully_cached() {
        scene_cache.compose();
        return (scene_cache.composed_ref(), result);
    }

    let theme = &state.theme;

    // Base section
    if scene_cache.base_commands.is_none() {
        let cmds = walk::walk_paint_scene_section(
            &frame.sections.base,
            &frame.base_layout,
            state,
            theme,
            cell_size,
            result.cursor_style,
        );
        scene_cache.base_commands = Some(cmds);
    }

    // Menu section
    if scene_cache.menu_commands.is_none() {
        let cmds = if let Some(ref overlay) = frame.sections.menu_overlay {
            let overlay_layout = crate::layout::layout_single_overlay(overlay, root_area, state);
            walk::walk_paint_scene_section(
                &overlay.element,
                &overlay_layout,
                state,
                theme,
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
        for overlay in frame
            .sections
            .info_overlays
            .iter()
            .chain(frame.sections.plugin_overlays.iter())
        {
            cmds.push(DrawCommand::BeginOverlay);
            let overlay_layout = crate::layout::layout_single_overlay(overlay, root_area, state);
            let overlay_cmds = walk::walk_paint_scene_section(
                &overlay.element,
                &overlay_layout,
                state,
                theme,
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
    registry: &PluginView<'_>,
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
    registry: &PluginView<'_>,
    grid: &mut CellGrid,
) -> RenderResult {
    let mut source = DirectViewSource;
    render_cached_core(&mut source, state, registry, grid, DirtyFlags::ALL, &[])
}

/// Declarative rendering pipeline with explicit dirty flags for incremental rendering.
/// Uses `DirectViewSource` (no Salsa memoization).
pub fn render_pipeline_direct(
    state: &AppState,
    registry: &PluginView<'_>,
    grid: &mut CellGrid,
    dirty: DirtyFlags,
) -> RenderResult {
    let mut source = DirectViewSource;
    render_cached_core(&mut source, state, registry, grid, dirty, &[])
}
