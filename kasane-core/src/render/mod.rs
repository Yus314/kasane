pub mod cache;
mod grid;
pub mod markup;
pub(crate) mod menu;
pub mod paint;
pub mod patch;
pub mod scene;
#[cfg(test)]
pub(crate) mod test_helpers;
pub(crate) mod theme;
pub mod view;

#[cfg(test)]
#[allow(clippy::field_reassign_with_default)]
mod tests;

pub use cache::{ComponentCache, LayoutCache, ViewCache};
pub use grid::{Cell, CellDiff, CellGrid};
pub use patch::{CursorPatch, MenuSelectionPatch, PaintPatch, StatusBarPatch};
pub use scene::{CellSize, DrawCommand, PixelPos, PixelRect, ResolvedAtom};
pub use theme::Theme;

use crate::element::Overlay;
use crate::layout::Rect;
use crate::layout::flex;
use crate::plugin::PluginRegistry;
use crate::protocol::CursorMode;
use crate::state::AppState;
use crate::state::DirtyFlags;

// ---------------------------------------------------------------------------
// RenderBackend trait
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CursorStyle {
    Block,
    Bar,
    Underline,
    Outline,
}

pub trait RenderBackend {
    fn size(&self) -> (u16, u16);
    fn begin_frame(&mut self) -> anyhow::Result<()> {
        Ok(())
    }
    fn end_frame(&mut self) -> anyhow::Result<()> {
        Ok(())
    }
    fn draw(&mut self, diffs: &[CellDiff]) -> anyhow::Result<()>;
    fn flush(&mut self) -> anyhow::Result<()>;
    fn show_cursor(&mut self, x: u16, y: u16, style: CursorStyle) -> anyhow::Result<()>;
    fn hide_cursor(&mut self) -> anyhow::Result<()>;

    /// Read text from the system clipboard. Returns None if unavailable.
    fn clipboard_get(&mut self) -> Option<String> {
        None
    }
    /// Write text to the system clipboard. Returns true on success.
    fn clipboard_set(&mut self, _text: &str) -> bool {
        false
    }
}

// ---------------------------------------------------------------------------
// Cursor utilities
// ---------------------------------------------------------------------------

/// Compute the terminal cursor position from the application state.
/// Returns (x, y) coordinates for the terminal cursor.
pub fn cursor_position(state: &AppState, grid: &CellGrid) -> (u16, u16) {
    let cx = state.cursor_pos.column as u16;
    let cy = match state.cursor_mode {
        CursorMode::Prompt => grid.height().saturating_sub(1),
        CursorMode::Buffer => state.cursor_pos.line as u16,
    };
    (cx, cy)
}

/// Determine the cursor style from the application state.
///
/// Priority: ui_option `kasane_cursor_style` > prompt mode > mode_line heuristic > Block.
pub fn cursor_style(state: &AppState) -> CursorStyle {
    if let Some(style) = state.ui_options.get("kasane_cursor_style") {
        return match style.as_str() {
            "bar" => CursorStyle::Bar,
            "underline" => CursorStyle::Underline,
            _ => CursorStyle::Block,
        };
    }
    if !state.focused {
        return CursorStyle::Outline;
    }
    if state.cursor_mode == CursorMode::Prompt {
        return CursorStyle::Bar;
    }
    let mode = state
        .status_mode_line
        .iter()
        .find_map(|atom| match atom.contents.as_str() {
            "insert" => Some(CursorStyle::Bar),
            "replace" => Some(CursorStyle::Underline),
            _ => None,
        });
    mode.unwrap_or(CursorStyle::Block)
}

/// In non-block cursor modes (insert/replace), clear the PrimaryCursor face
/// highlight from the cursor cell so the terminal cursor shape is visible.
pub fn clear_block_cursor_face(state: &AppState, grid: &mut CellGrid, style: CursorStyle) {
    if style == CursorStyle::Block || style == CursorStyle::Outline {
        return;
    }
    let cx = state.cursor_pos.column as u16;
    let cy = match state.cursor_mode {
        CursorMode::Prompt => grid.height().saturating_sub(1),
        CursorMode::Buffer => state.cursor_pos.line as u16,
    };
    let base_face = match state.cursor_mode {
        CursorMode::Buffer => &state.default_face,
        CursorMode::Prompt => &state.status_default_face,
    };
    if let Some(cell) = grid.get_mut(cx, cy) {
        cell.face = *base_face;
    }
}

// ---------------------------------------------------------------------------
// Overlay layout helper
// ---------------------------------------------------------------------------

/// Lay out a single overlay element against a root area.
/// Extracts the per-overlay logic from `place_stack` for use by `SceneCache`.
pub(crate) fn layout_overlay(
    overlay: &Overlay,
    root_area: Rect,
    state: &AppState,
) -> flex::LayoutResult {
    let (ox, oy, ow, oh) = match &overlay.anchor {
        crate::element::OverlayAnchor::Absolute { x, y, w, h } => {
            (root_area.x + *x, root_area.y + *y, *w, *h)
        }
        crate::element::OverlayAnchor::AnchorPoint {
            coord,
            prefer_above,
            avoid,
        } => {
            let overlay_size = flex::measure(
                &overlay.element,
                flex::Constraints::loose(root_area.w, root_area.h),
                state,
            );
            let (y, x) = crate::layout::compute_pos(
                (coord.line, coord.column),
                (overlay_size.height, overlay_size.width),
                root_area,
                avoid,
                *prefer_above,
            );
            (x, y, overlay_size.width, overlay_size.height)
        }
    };

    let overlay_area = Rect {
        x: ox,
        y: oy,
        w: ow,
        h: oh,
    };

    flex::place(&overlay.element, overlay_area, state)
}

// ---------------------------------------------------------------------------
// Declarative render pipeline
// ---------------------------------------------------------------------------

/// レンダリングパイプラインの結果。バックエンド固有の描画に必要な情報を返す。
#[derive(Debug, Clone, Copy)]
pub struct RenderResult {
    pub cursor_x: u16,
    pub cursor_y: u16,
    pub cursor_style: CursorStyle,
}

// ---------------------------------------------------------------------------
// SceneCache — DrawCommand-level caching per section
// ---------------------------------------------------------------------------

/// Cache for memoized `DrawCommand` lists per view section.
/// Mirrors `ViewCache` invalidation: each DirtyFlag clears only its section.
#[derive(Debug, Default)]
pub struct SceneCache {
    base_commands: Option<Vec<DrawCommand>>,
    menu_commands: Option<Vec<DrawCommand>>,
    info_commands: Option<Vec<DrawCommand>>,
    composed: Vec<DrawCommand>,
    cached_cell_size: Option<(u32, u32)>,
    cached_dims: Option<(u16, u16)>,
}

impl SceneCache {
    pub fn new() -> Self {
        Self::default()
    }

    /// Invalidate cached sections based on dirty flags and cell size / dims changes.
    pub fn invalidate(
        &mut self,
        dirty: DirtyFlags,
        cell_size: scene::CellSize,
        cols: u16,
        rows: u16,
    ) {
        let cs_key = (cell_size.width.to_bits(), cell_size.height.to_bits());
        let dims_key = (cols, rows);

        if self.cached_cell_size != Some(cs_key) || self.cached_dims != Some(dims_key) {
            self.base_commands = None;
            self.menu_commands = None;
            self.info_commands = None;
            self.cached_cell_size = Some(cs_key);
            self.cached_dims = Some(dims_key);
            return;
        }

        if dirty.intersects(DirtyFlags::BUFFER | DirtyFlags::STATUS | DirtyFlags::OPTIONS) {
            self.base_commands = None;
        }
        if dirty.intersects(DirtyFlags::MENU) {
            self.menu_commands = None;
        }
        if dirty.intersects(DirtyFlags::INFO) {
            self.info_commands = None;
        }
    }

    /// Returns true if all sections are cached.
    pub fn is_fully_cached(&self) -> bool {
        self.base_commands.is_some() && self.menu_commands.is_some() && self.info_commands.is_some()
    }

    /// Assemble the composed output from cached sections.
    pub fn compose(&mut self) {
        self.composed.clear();
        if let Some(ref base) = self.base_commands {
            self.composed.extend_from_slice(base);
        }
        if let Some(ref menu) = self.menu_commands
            && !menu.is_empty()
        {
            self.composed.push(DrawCommand::BeginOverlay);
            self.composed.extend_from_slice(menu);
        }
        if let Some(ref info) = self.info_commands {
            self.composed.extend_from_slice(info);
        }
    }

    /// Get a reference to the composed output.
    pub fn composed_ref(&self) -> &[DrawCommand] {
        &self.composed
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

/// Compute the RenderResult (cursor position + style) from AppState.
fn compute_render_result(state: &AppState) -> RenderResult {
    let style = cursor_style(state);
    let cx = state.cursor_pos.column as u16;
    let cy = match state.cursor_mode {
        CursorMode::Prompt => state.rows.saturating_sub(1),
        CursorMode::Buffer => state.cursor_pos.line as u16,
    };
    RenderResult {
        cursor_x: cx,
        cursor_y: cy,
        cursor_style: style,
    }
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

    let result = compute_render_result(state);

    // Invalidate both caches
    view_cache.invalidate(dirty);
    scene_cache.invalidate(dirty, cell_size, state.cols, state.rows);

    // Fast path: all sections cached
    if scene_cache.is_fully_cached() {
        scene_cache.compose();
        return (scene_cache.composed_ref(), result);
    }

    // Get view sections (uses ViewCache)
    let sections = view::view_sections_cached(state, registry, view_cache);

    let root_area = Rect {
        x: 0,
        y: 0,
        w: state.cols,
        h: state.rows,
    };
    let theme = Theme::default_theme();

    // Base section
    if scene_cache.base_commands.is_none() {
        let base_layout = flex::place(&sections.base, root_area, state);
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
            let overlay_layout = layout_overlay(overlay, root_area, state);
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
            let overlay_layout = layout_overlay(overlay, root_area, state);
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

    let style = cursor_style(state);
    clear_block_cursor_face(state, grid, style);
    let (cx, cy) = cursor_position(state, grid);

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

        let style = cursor_style(state);
        clear_block_cursor_face(state, grid, style);
        let (cx, cy) = cursor_position(state, grid);
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
            .map(|overlay| layout_overlay(overlay, root_area, state).area);

        if let Some(menu_rect) = menu_rect {
            let element = sections.into_element();
            let layout_result = flex::place(&element, root_area, state);

            // Clear and repaint the menu region
            grid.clear_region(&menu_rect, &state.default_face);
            paint::paint(&element, &layout_result, grid, state);

            layout_cache.root_area = Some(root_area);
            layout_cache.base_layout = Some(layout_result);

            let style = cursor_style(state);
            clear_block_cursor_face(state, grid, style);
            let (cx, cy) = cursor_position(state, grid);
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
    if patch::try_apply_grid_patch(patches, grid, state, dirty, layout_cache) {
        let style = cursor_style(state);
        clear_block_cursor_face(state, grid, style);
        let (cx, cy) = cursor_position(state, grid);

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
