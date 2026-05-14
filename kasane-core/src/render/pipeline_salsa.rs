//! SalsaViewSource: ViewSource implementation backed by Salsa tracked functions.
//!
//! Uses Salsa-memoized pure element generation (Stage 1) combined with
//! plugin contributions read from Salsa inputs (Stage 2) and imperative
//! transform application (Stage 3).
//!
//! The flow per section:
//! 1. Salsa tracked function produces the core element (auto-memoized)
//! 2. Plugin contributions (slots, annotations, overlays) read from Salsa inputs
//! 3. Plugin transforms are applied on top (using PluginRuntime)
//!
//! Salsa handles memoization of pure elements,
//! and `sync_plugin_contributions()` pre-computes plugin contributions
//! into Salsa inputs each frame.

use super::CursorStyleHint;
use super::RenderResult;
use super::cell_decoration;
use super::cursor::{
    apply_secondary_cursor_faces, clear_cursor_face_at, cursor_position, cursor_style_default,
    find_buffer_origin_in_rect, find_buffer_x_offset, find_status_left_slot_width,
    neutralize_unfocused_cursors,
};
use super::grid::CellGrid;
use super::ornament::{
    apply_surface_ornaments_tui, lower_surface_ornaments_gui, resolve_surface_ornaments,
};
use super::scene::{self, DrawCommand, SceneCache};
use super::view;
use super::walk;
use crate::display::{DisplayMap, DisplayMapRef};
use crate::element::{
    Direction, Element, ElementStyle, FlexChild, Overlay, ResolvedSlotInstanceId,
};
use crate::layout::Rect;
use crate::layout::flex;
use crate::layout::line_display_width;
use crate::plugin::{
    AppView, OverlayContext, PluginCapabilities, PluginView, TransformSubject, TransformTarget,
};
use crate::protocol::{CursorMode, MenuStyle};
use crate::salsa_db::KasaneDatabase;
use crate::salsa_sync::SalsaInputHandles;
use crate::salsa_views;
use crate::state::{AppState, DirtyFlags};
use crate::surface::SurfaceRegistry;
use crate::surface::pane_map::PaneStates;

/// ViewSource that uses Salsa tracked functions for core element generation
/// and reads plugin contributions from Salsa inputs.
///
/// Stage 1 (pure, Salsa-memoized): `pure_status_element`, `pure_buffer_element`,
/// `pure_menu_overlay`, `pure_info_overlays`.
///
/// Stage 2 (Salsa inputs): slot contributions, annotations, overlays
/// (set by `sync_plugin_contributions()` each frame).
///
/// Stage 3 (imperative): plugin transforms applied via `PluginRuntime`.
pub(crate) struct SalsaViewSource<'a> {
    db: &'a KasaneDatabase,
    handles: &'a SalsaInputHandles,
    surface_registry: Option<&'a SurfaceRegistry>,
    pane_states: Option<&'a PaneStates<'a>>,
}

impl<'a> SalsaViewSource<'a> {
    pub(crate) fn new(
        db: &'a KasaneDatabase,
        handles: &'a SalsaInputHandles,
        surface_registry: Option<&'a SurfaceRegistry>,
        pane_states: Option<&'a PaneStates<'a>>,
    ) -> Self {
        Self {
            db,
            handles,
            surface_registry,
            pane_states,
        }
    }
}

impl SalsaViewSource<'_> {
    /// No-op: Salsa handles invalidation automatically.
    /// Plugin contributions are synced by `sync_plugin_contributions()` before rendering.
    fn prepare(&mut self, _dirty: DirtyFlags, _registry: &PluginView<'_>) {}

    fn view_sections(&mut self, state: &AppState, registry: &PluginView<'_>) -> view::ViewSections {
        let db = self.db;
        let h = self.handles;

        let is_multi_pane = self.surface_registry.is_some_and(|sr| sr.is_multi_pane());

        // --- Base section (buffer + status + slots + annotations) ---
        let (
            base_el,
            surface_reports,
            focused_pane_rect,
            focused_pane_state,
            display_scroll_offset,
        ) = if is_multi_pane {
            let sr = self
                .surface_registry
                .expect("surface_registry present when is_multi_pane");
            let total = Rect {
                x: 0,
                y: 0,
                w: state.runtime.cols,
                h: state.runtime.rows,
            };
            let result = sr.compose_base_result(state, self.pane_states, registry, total);
            let focused = sr.workspace().focused();
            let focused_rect = sr.workspace().compute_rects(total).get(&focused).copied();
            let focused_state = self
                .pane_states
                .and_then(|ps| ps.state_for_surface(focused))
                .map(|s| Box::new(s.clone()));
            // Multi-pane: each pane computes its own offset; use 0 for the top-level
            (
                result.base.unwrap_or(Element::Empty),
                result.surface_reports,
                focused_rect,
                focused_state,
                0usize,
            )
        } else {
            let status_el = salsa_views::pure_status_element(db, h.status);
            let buffer_el = salsa_views::pure_buffer_element(db, h.config);
            let display_map_ref = salsa_views::display_map_query(db, h.display_directives);
            let (base, salsa_display_scroll_offset) = compose_base_from_salsa(
                buffer_el,
                status_el,
                state,
                registry,
                &display_map_ref,
                db,
                h,
            );
            (base, vec![], None, None, salsa_display_scroll_offset)
        };

        // --- Menu overlay ---
        // In multi-pane mode, build menu/info from the focused pane's state
        // instead of the primary state, because Salsa inputs only reflect the
        // primary session. The focused pane's menu_show/info data lives in its
        // SessionStateStore snapshot.
        //
        // The pane's Kakoune produces overlay anchors in pane-local coordinates
        // (since it was Resize'd to the pane rect). We build using pane-local
        // dimensions, then offset absolute anchors to full-screen coordinates.
        let (overlay_state_owned, pane_offset) =
            if let (Some(fps), Some(fr)) = (&focused_pane_state, &focused_pane_rect) {
                let mut s = fps.as_ref().clone();
                s.runtime.cols = fr.w;
                s.runtime.rows = fr.h;
                (Some(s), Some((fr.x, fr.y)))
            } else {
                (None, None)
            };
        let overlay_state: &AppState = overlay_state_owned.as_ref().unwrap_or(state);
        let menu_overlay = if focused_pane_state.is_some() {
            // Multi-pane: bypass Salsa, build directly from focused pane state
            let mut overlay = view::build_menu_section_standalone(overlay_state, registry);
            if let (Some(o), Some((ox, oy))) = (&mut overlay, pane_offset) {
                offset_overlay_anchor(&mut o.anchor, ox, oy);
            }
            overlay
        } else if registry.has_capability(PluginCapabilities::MENU_TRANSFORM) {
            // When menu-item-transform plugins are present, bypass the Salsa
            // cache and build via the non-Salsa path which applies per-item
            // transforms during element construction.
            view::build_menu_section_standalone(state, registry)
        } else {
            let pure = salsa_views::pure_menu_overlay(db, h.menu, h.config);
            pure.map(|overlay| {
                let menu_state = state.observed.menu.as_ref();
                let target = menu_state
                    .map(|m| match m.style {
                        MenuStyle::Prompt => TransformTarget::MENU_PROMPT,
                        MenuStyle::Inline => TransformTarget::MENU_INLINE,
                        MenuStyle::Search => TransformTarget::MENU_SEARCH,
                    })
                    .unwrap_or(TransformTarget::MENU);

                registry
                    .apply_transform_chain_hierarchical(
                        target,
                        TransformSubject::Overlay(overlay),
                        &AppView::new(state),
                    )
                    .into_overlay()
                    .expect("overlay transform preserves variant")
            })
        };

        // --- Info overlays ---
        let info_overlays = if focused_pane_state.is_some() {
            // Multi-pane: bypass Salsa, build directly from focused pane state
            let mut overlays = view::build_info_section_standalone(overlay_state, registry);
            if let Some((ox, oy)) = pane_offset {
                for o in &mut overlays {
                    offset_overlay_anchor(&mut o.anchor, ox, oy);
                }
            }
            overlays
        } else {
            let pure = salsa_views::pure_info_overlays(db, h.info, h.menu, h.buffer, h.config);
            let app_view = AppView::new(state);
            pure.into_iter()
                .map(|(style, overlay)| {
                    let target = match style {
                        crate::protocol::InfoStyle::Prompt => TransformTarget::INFO_PROMPT,
                        crate::protocol::InfoStyle::Modal => TransformTarget::INFO_MODAL,
                        _ => TransformTarget::INFO,
                    };
                    registry
                        .apply_transform_chain_hierarchical(
                            target,
                            TransformSubject::Overlay(overlay),
                            &app_view,
                        )
                        .into_overlay()
                        .expect("overlay transform preserves variant")
                })
                .collect()
        };

        // --- Assemble all overlays in unified vec ---
        let mut overlays = Vec::new();
        if let Some(menu) = menu_overlay {
            overlays.push(menu);
        }
        // Plugin overlays — collected inline (θ-spike: no Salsa intermediate).
        // The previous `PluginOverlaysInput` did two jobs: (1) pipe the value
        // from sync_plugin_contributions to here, and (2) skip recollection
        // when `any_overlay_needs_recollect()` reported stale-free state. No
        // `#[salsa::tracked]` query depends on the value, so the Salsa
        // wrapper provided nothing structural — only a cache. The spike
        // drops the cache entirely (always recollect) to measure whether the
        // cache is justified for the typical overlay workload. If the bench
        // shows meaningful regression, the follow-up adds a non-Salsa cache.
        let overlay_ctx = OverlayContext {
            screen_cols: state.runtime.cols,
            screen_rows: state.runtime.rows,
            menu_rect: crate::layout::get_menu_rect(state),
            existing_overlays: vec![],
            focused_surface_id: None,
        };
        overlays.extend(
            registry
                .collect_overlays_with_ctx(&AppView::new(state), &overlay_ctx)
                .into_iter()
                .map(|oc| Overlay {
                    element: oc.element,
                    anchor: oc.anchor,
                }),
        );
        overlays.extend(info_overlays);

        let display_map = salsa_views::display_map_query(db, h.display_directives);
        view::ViewSections {
            base: base_el,
            overlays,
            surface_reports,
            display_map,
            display_scroll_offset,
            segment_map: None,
            focused_pane_rect,
            focused_pane_state,
        }
    }

    fn surface_registry(&self) -> Option<&SurfaceRegistry> {
        self.surface_registry
    }
}

// ---------------------------------------------------------------------------
// Shared rendering helpers (previously in render::pipeline)
// ---------------------------------------------------------------------------

/// Selective clear: when BUFFER is dirty and line-level dirty info is available,
/// skip clearing buffer rows (paint_buffer_ref will skip clean lines) and only
/// clear non-buffer sections that are dirty. This extends line-dirty optimization
/// to BUFFER|STATUS and other BUFFER-containing combinations.
fn selective_clear(grid: &mut CellGrid, state: &AppState, dirty: DirtyFlags) {
    let line_dirty_active = dirty.contains(DirtyFlags::BUFFER_CONTENT)
        && !state.inference.lines_dirty.is_empty()
        && state.inference.lines_dirty.iter().any(|d| !d);

    if line_dirty_active {
        // Clear only non-buffer sections; buffer lines handled by paint_buffer_ref
        if dirty.intersects(DirtyFlags::STATUS) {
            let status_y = if state.config.status_at_top {
                0
            } else {
                state.runtime.rows.saturating_sub(1)
            };
            let status_rect = Rect {
                x: 0,
                y: status_y,
                w: state.runtime.cols,
                h: 1,
            };
            grid.clear_region(
                &status_rect,
                &crate::render::TerminalStyle::from_style(&state.observed.status_default_style),
            );
        }
        // Menu/info overlays paint over buffer anyway — no separate clear needed
    } else {
        grid.clear(&crate::render::TerminalStyle::from_style(
            &state.observed.default_style,
        ));
    }
}

/// Compute the RenderResult (cursor position + style) from AppState.
#[allow(clippy::too_many_arguments)]
fn compute_render_result(
    state: &AppState,
    hint: CursorStyleHint,
    buffer_x_offset: u16,
    display_map: Option<&DisplayMap>,
    buffer_y_offset: u16,
    display_scroll_offset: u16,
    focused_pane_rect: Option<&Rect>,
    status_content_x_offset: u16,
) -> RenderResult {
    let (cx, cy) = match state.inference.cursor_mode {
        CursorMode::Buffer => {
            let cx = state.observed.cursor_pos.column as u16 + buffer_x_offset;
            let cy = display_map
                .filter(|dm| !dm.is_identity())
                .and_then(|dm| {
                    dm.buffer_to_display(crate::display::BufferLine(
                        state.observed.cursor_pos.line as usize,
                    ))
                })
                .map(|y| y.0 as u16)
                .unwrap_or(state.observed.cursor_pos.line as u16)
                .saturating_sub(display_scroll_offset)
                + buffer_y_offset;
            (cx, cy)
        }
        CursorMode::Prompt => {
            let prompt_width = line_display_width(&state.observed.status_prompt) as u16;
            let base_cx = status_content_x_offset
                + prompt_width
                + (state.observed.status_content_cursor_pos.max(0) as u16);
            match focused_pane_rect {
                Some(r) => {
                    let cy = if state.config.status_at_top {
                        r.y
                    } else {
                        r.y + r.h - 1
                    };
                    (base_cx + r.x, cy)
                }
                None => {
                    let cy = if state.config.status_at_top {
                        0
                    } else {
                        state.runtime.rows.saturating_sub(1)
                    };
                    (base_cx, cy)
                }
            }
        }
    };
    RenderResult {
        cursor_x: cx,
        cursor_y: cy,
        cursor_style: hint.shape,
        cursor_color: extract_cursor_color(state),
        cursor_blink: hint.blink,
        cursor_movement: hint.movement,
        display_scroll_offset: display_scroll_offset as usize,
        visual_hints: super::VisualHints::default(),
    }
}

/// Extract the cursor visual color from the Kakoune face at the cursor position.
///
/// Walks the atoms in the cursor line to find the face at the cursor column.
/// `cursor_pos.column` is a display column, so atom widths must be measured
/// in display columns (not character count).
/// Under REVERSE (typical Kakoune cursor), the visual cursor block color is `face.fg`.
/// Without REVERSE, it is `face.bg`.
fn extract_cursor_color(state: &AppState) -> crate::protocol::Color {
    use crate::protocol::{Attributes, Color, CursorMode};

    if state.inference.cursor_mode != CursorMode::Buffer {
        return Color::Default;
    }
    let line_idx = state.observed.cursor_pos.line as usize;
    let col = state.observed.cursor_pos.column as usize;
    let Some(atoms) = state.observed.lines.get(line_idx) else {
        return Color::Default;
    };
    let mut pos = 0;
    for atom in atoms {
        let atom_width = crate::layout::line_display_width(std::slice::from_ref(atom));
        if col < pos + atom_width {
            return if atom
                .unresolved_style()
                .to_face()
                .attributes
                .contains(Attributes::REVERSE)
            {
                atom.unresolved_style().to_face().fg
            } else {
                atom.unresolved_style().to_face().bg
            };
        }
        pos += atom_width;
    }
    Color::Default
}

/// Extract a `display_map` Option reference from a `DisplayMapRef`,
/// returning `None` when the map is identity (optimization).
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
/// Captures the common orchestration: `source.prepare` → `view_sections` →
/// `display_map` extraction → `backfill_surface_report_areas` → buffer offset
/// computation.
struct PreparedFrame {
    sections: view::ViewSections,
    base_layout: flex::LayoutResult,
    display_map: DisplayMapRef,
    root_area: Rect,
    buffer_x_offset: u16,
    buffer_y_offset: u16,
    display_scroll_offset: usize,
    segment_map: Option<std::sync::Arc<crate::display::segment_map::SegmentMap>>,
    focused_pane_rect: Option<Rect>,
    focused_pane_state: Option<Box<AppState>>,
    /// Width of the `kasane.status.left` slot, used to offset prompt cursor.
    status_content_x_offset: u16,
}

/// Run the shared pipeline orchestration, returning a `PreparedFrame`.
///
/// Both `render_cached_core` (TUI) and `scene_render_core` (GPU) call this
/// to avoid duplicating ~70% of their setup code.
fn prepare_frame(
    source: &mut SalsaViewSource<'_>,
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
        w: state.runtime.cols,
        h: state.runtime.rows,
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

    let segment_map = sections.segment_map.clone();

    // Compute status-left slot width for prompt cursor offset.
    let status_content_x_offset = find_status_left_slot_width(&sections.base, &base_layout);

    PreparedFrame {
        sections,
        base_layout,
        display_map,
        root_area,
        buffer_x_offset,
        buffer_y_offset,
        display_scroll_offset,
        segment_map,
        focused_pane_rect,
        focused_pane_state,
        status_content_x_offset,
    }
}

// ---------------------------------------------------------------------------
// Core rendering: TUI grid + GPU scene paths share this body
// ---------------------------------------------------------------------------

/// Core cached rendering pipeline (TUI grid).
#[allow(clippy::too_many_arguments)]
fn render_cached_core(
    source: &mut SalsaViewSource<'_>,
    state: &AppState,
    registry: &PluginView<'_>,
    grid: &mut CellGrid,
    dirty: DirtyFlags,
    halfblock_cache: Option<&mut super::halfblock::HalfblockCache>,
    image_protocol: super::ImageProtocol,
    image_requests: Option<&mut Vec<super::ImageRequest>>,
) -> (RenderResult, DisplayMapRef) {
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
    let theme = &state.config.theme;
    walk::walk_paint_grid(
        &element,
        &layout_result,
        grid,
        state,
        theme,
        halfblock_cache,
        image_protocol,
        image_requests,
    );

    // Collect all render ornaments in a single pass and decompose.
    let ornament_ctx = crate::plugin::RenderOrnamentContext::from_screen(
        state.runtime.cols,
        state.runtime.rows,
        frame.display_scroll_offset,
        frame.buffer_x_offset,
        frame.buffer_y_offset,
    );
    let ornaments = registry.collect_ornaments(&AppView::new(state), &ornament_ctx);

    if !ornaments.emphasis.is_empty() {
        cell_decoration::apply_cell_decorations(
            &ornaments.emphasis,
            grid,
            frame.buffer_x_offset,
            dm,
            frame.buffer_y_offset,
            dso,
        );
    }

    let surface_ornaments = resolve_surface_ornaments(
        &ornaments.surfaces,
        source.surface_registry(),
        frame.focused_pane_rect,
        root_area,
    );
    if !surface_ornaments.is_empty() {
        apply_surface_ornaments_tui(grid, &surface_ornaments);
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
            frame.segment_map.as_deref(),
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
        frame.segment_map.as_deref(),
    );

    // Resolve cursor style: ornament Style > default
    let hint = ornaments
        .cursor_style
        .unwrap_or_else(|| cursor_style_default(cursor_state).into());
    let (cx, cy) = cursor_position(
        cursor_state,
        grid,
        frame.buffer_x_offset,
        dm,
        frame.buffer_y_offset,
        dso,
        frame.focused_pane_rect.as_ref(),
        frame.segment_map.as_deref(),
        frame.status_content_x_offset,
    );
    clear_cursor_face_at(cursor_state, grid, hint.shape, cx, cy);

    let mut result = RenderResult {
        cursor_x: cx,
        cursor_y: cy,
        cursor_style: hint.shape,
        cursor_color: extract_cursor_color(cursor_state),
        cursor_blink: hint.blink,
        cursor_movement: hint.movement,
        display_scroll_offset: frame.display_scroll_offset,
        visual_hints: super::VisualHints::default(),
    };
    if let Some((px, py, style, color)) = ornaments.cursor_position {
        result.cursor_x = px;
        result.cursor_y = py;
        result.cursor_style = style;
        result.cursor_color = color;
    }
    (result, frame.display_map)
}

/// Core scene rendering pipeline (GPU). Returns a slice into the
/// `SceneCache`'s composed buffer and the `RenderResult`. Per-section
/// invalidation: only dirty sections are re-rendered.
fn scene_render_core<'a>(
    source: &mut SalsaViewSource<'_>,
    state: &AppState,
    registry: &PluginView<'_>,
    cell_size: scene::CellSize,
    dirty: DirtyFlags,
    scene_cache: &'a mut SceneCache,
) -> (&'a [DrawCommand], RenderResult, DisplayMapRef) {
    scene_cache.invalidate(dirty, cell_size, state.runtime.cols, state.runtime.rows);

    let frame = prepare_frame(source, state, registry, dirty);
    let display_map_out = std::sync::Arc::clone(&frame.display_map);
    let dm = dm_ref(&frame.display_map);
    let dso = frame.display_scroll_offset as u16;
    let root_area = frame.root_area;

    // Collect all render ornaments in a single pass.
    let ornament_ctx = crate::plugin::RenderOrnamentContext::from_screen(
        state.runtime.cols,
        state.runtime.rows,
        frame.display_scroll_offset,
        frame.buffer_x_offset,
        frame.buffer_y_offset,
    );
    let ornaments = registry.collect_ornaments(&AppView::new(state), &ornament_ctx);

    // Use focused pane state for cursor computation in multi-pane mode
    let cursor_state = frame.focused_pane_state.as_deref().unwrap_or(state);
    let cursor_hint = ornaments
        .cursor_style
        .unwrap_or_else(|| cursor_style_default(cursor_state).into());
    let mut result = compute_render_result(
        cursor_state,
        cursor_hint,
        frame.buffer_x_offset,
        dm,
        frame.buffer_y_offset,
        dso,
        frame.focused_pane_rect.as_ref(),
        frame.status_content_x_offset,
    );
    if let Some((px, py, style, color)) = ornaments.cursor_position {
        result.cursor_x = px;
        result.cursor_y = py;
        result.cursor_style = style;
        result.cursor_color = color;
    }

    // Populate visual hints for GPU backend
    {
        let cursor_y = result.cursor_y as f32 * cell_size.height;
        let viewport_width = state.runtime.cols as f32 * cell_size.width;
        result.visual_hints.cursor_line = Some(super::visual_hints::CursorLineHint {
            y: cursor_y,
            height: cell_size.height,
            width: viewport_width,
        });

        // Focused pane hint for non-focused pane dimming
        if let Some(ref rect) = frame.focused_pane_rect {
            result.visual_hints.focused_pane = Some(super::visual_hints::FocusedPaneHint {
                x: rect.x as f32 * cell_size.width,
                y: rect.y as f32 * cell_size.height,
                w: rect.w as f32 * cell_size.width,
                h: rect.h as f32 * cell_size.height,
            });
        }
    }

    // Fast path: all sections cached
    if scene_cache.is_fully_cached() {
        scene_cache.compose();
        return (scene_cache.composed_ref(), result, display_map_out);
    }

    let theme = &state.config.theme;

    // Base section (buffer + status combined)
    if !scene_cache.has_base_commands() {
        let mut cmds = walk::walk_paint_scene_section(
            &frame.sections.base,
            &frame.base_layout,
            state,
            theme,
            cell_size,
            result.cursor_style,
        );
        // ADR-031 Phase 10 Step 2-renderer (Step A.2b): fill in
        // `BufferParagraph::inline_box_paint_commands` by dispatching
        // `paint_inline_box(box_id)` to each slot's owning plugin and
        // pre-painting the returned Element at origin (0, 0). The GPU
        // renderer translates these commands to the box's final rect at
        // emit time.
        populate_inline_box_paint_commands(
            &mut cmds,
            registry,
            state,
            theme,
            cell_size,
            result.cursor_style,
        );
        let surface_ornaments = resolve_surface_ornaments(
            &ornaments.surfaces,
            source.surface_registry(),
            frame.focused_pane_rect,
            root_area,
        );
        if !surface_ornaments.is_empty() {
            cmds.extend(lower_surface_ornaments_gui(&surface_ornaments, cell_size));
        }
        scene_cache.set_base_commands(cmds);
    }

    // Overlay section (menu + info + plugin, unified)
    if scene_cache.overlay_commands.is_none() {
        let mut cmds = Vec::new();
        for (idx, overlay) in frame.sections.overlays.iter().enumerate() {
            cmds.push(DrawCommand::BeginOverlay);
            let overlay_layout = crate::layout::layout_single_overlay(overlay, root_area, state);

            // Collect overlay region hint
            let r = &overlay_layout.area;
            result
                .visual_hints
                .overlay_regions
                .push(super::visual_hints::OverlayRegionHint {
                    rect: scene::PixelRect {
                        x: r.x as f32 * cell_size.width,
                        y: r.y as f32 * cell_size.height,
                        w: r.w as f32 * cell_size.width,
                        h: r.h as f32 * cell_size.height,
                    },
                    id: idx as u32,
                });

            let mut overlay_cmds = walk::walk_paint_scene_section(
                &overlay.element,
                &overlay_layout,
                state,
                theme,
                cell_size,
                result.cursor_style,
            );
            populate_inline_box_paint_commands(
                &mut overlay_cmds,
                registry,
                state,
                theme,
                cell_size,
                result.cursor_style,
            );
            cmds.extend(overlay_cmds);
        }
        scene_cache.overlay_commands = Some(cmds);
    }

    scene_cache.compose();
    (scene_cache.composed_ref(), result, display_map_out)
}

/// Populate `BufferParagraph::inline_box_paint_commands` for each
/// `RenderParagraph` in `cmds` by dispatching `paint_inline_box(box_id)`
/// to the slot's owning plugin and pre-painting the returned `Element` at
/// origin (0, 0).
///
/// ADR-031 Phase 10 Step 2-renderer (Step A.2b). The GPU renderer
/// translates each command's position by the Parley-reported box rect at
/// emit time. Slots whose plugin returns `None` keep an empty inner
/// `Vec`; the GPU renderer falls back to a placeholder fill.
fn populate_inline_box_paint_commands(
    cmds: &mut [DrawCommand],
    registry: &PluginView<'_>,
    state: &AppState,
    theme: &super::theme::Theme,
    cell_size: scene::CellSize,
    cursor_style: super::CursorStyle,
) {
    let app_view = AppView::new(state);
    for cmd in cmds.iter_mut() {
        let DrawCommand::RenderParagraph { paragraph, .. } = cmd else {
            continue;
        };
        if paragraph.inline_box_slots.is_empty() {
            continue;
        }
        for (idx, slot) in paragraph.inline_box_slots.iter().enumerate() {
            let Some(element) = registry.paint_inline_box(&slot.owner, slot.box_id, &app_view)
            else {
                continue;
            };
            // Layout the sub-element at the slot's declared cell
            // geometry. Cell-grid Rect is u16; fractional cells round up
            // so the sub-element gets at least the declared space.
            let area = Rect {
                x: 0,
                y: 0,
                w: slot.width_cells.max(0.0).ceil() as u16,
                h: slot.height_lines.max(0.0).ceil() as u16,
            };
            if area.w == 0 || area.h == 0 {
                continue;
            }
            let layout = flex::place(&element, area, state);
            let sub_cmds = walk::walk_paint_scene_section(
                &element,
                &layout,
                state,
                theme,
                cell_size,
                cursor_style,
            );
            paragraph.inline_box_paint_commands[idx] = sub_cmds;
        }
    }
}

/// Compose buffer + status elements into the base Element tree, reading
/// plugin contributions (slot fills, annotations) from Salsa inputs and
/// applying transforms imperatively via the registry.
fn compose_base_from_salsa(
    buffer_el: Element,
    status_el: Element,
    state: &AppState,
    registry: &PluginView<'_>,
    display_map: &crate::display::DisplayMapRef,
    db: &KasaneDatabase,
    handles: &SalsaInputHandles,
) -> (Element, usize) {
    use std::sync::Arc;

    let buffer_rows = state.available_height() as usize;
    let dm_for_element = if display_map.is_identity() {
        None
    } else {
        Some(Arc::clone(display_map))
    };

    // Collect annotations inline (θ.5: no Salsa intermediate). The
    // `collect_annotations` early-return on `!has_capability(ANNOTATOR)`
    // keeps the cost bounded for plugin-less configurations; per-line
    // result vecs get Arc-wrapped here so the downstream
    // `Element::BufferRef` still gets O(1)-cloneable slices.
    let display_map_ref = Arc::clone(display_map);
    let annotate_ctx = crate::plugin::AnnotateContext {
        line_width: state.runtime.cols,
        gutter_width: 0,
        display_map: Some(display_map_ref),
        pane_surface_id: None,
        pane_focused: true,
    };
    let ann = registry.collect_annotations(&crate::plugin::AppView::new(state), &annotate_ctx);
    let line_backgrounds = ann.line_backgrounds.map(Arc::new);
    let left_gutter = ann.left_gutter;
    let right_gutter = ann.right_gutter;
    let inline_decorations = ann.inline_decorations.map(Arc::new);
    let virtual_text = ann.virtual_text.map(Arc::new);

    // When a non-identity DisplayMap is active, compute scroll offset so
    // the cursor stays visible, then use offset-based line_range.
    let (effective_start, effective_end, display_scroll_offset) = if !display_map.is_identity() {
        let visible_height = display_map.display_line_count().min(buffer_rows);
        let offset = crate::display::compute_display_scroll_offset(
            display_map,
            crate::display::BufferLine(state.observed.cursor_pos.line as usize),
            visible_height,
        );
        let end = (offset.0 + visible_height).min(display_map.display_line_count());
        (offset.0, end, offset.0)
    } else {
        (0, buffer_rows, 0)
    };

    // Incorporate line backgrounds, display_map, inline decorations, and virtual text into buffer element
    let buffer_with_bg = if line_backgrounds.is_some()
        || dm_for_element.is_some()
        || inline_decorations.is_some()
        || virtual_text.is_some()
    {
        Element::BufferRef {
            line_range: effective_start..effective_end,
            line_backgrounds,
            display_map: dm_for_element,
            state: None,
            inline_decorations,
            virtual_text,
        }
    } else {
        buffer_el
    };

    // Segment buffer with content annotations — collected inline (θ.3:
    // no Salsa intermediate). The previous `ContentAnnotationsInput` was
    // a Salsa input with no `#[salsa::tracked]` consumer; the wrapper
    // added a `db` thread without semantics. Skipped when no
    // CONTENT_ANNOTATOR plugin is registered to preserve the original
    // early-return shape.
    let content_annotations: Vec<crate::display::ContentAnnotation> =
        if registry.has_capability(crate::plugin::PluginCapabilities::CONTENT_ANNOTATOR) {
            let annotate_ctx = crate::plugin::AnnotateContext {
                line_width: state.runtime.cols,
                gutter_width: 0,
                display_map: Some(Arc::clone(display_map)),
                pane_surface_id: None,
                pane_focused: true,
            };
            registry.collect_content_annotations(&crate::plugin::AppView::new(state), &annotate_ctx)
        } else {
            Vec::new()
        };
    let segmented_buffer = view::segment_buffer(
        buffer_with_bg,
        &content_annotations,
        if display_map.is_identity() {
            None
        } else {
            Some(display_map)
        },
    );

    // Apply buffer transform chain: use the pure-patch fast path when
    // every TRANSFORMER plugin returns a pure ElementPatch; otherwise
    // fall back to imperative `apply_transform_chain`. Collected inline
    // (θ.4: no Salsa intermediate; no tracked-query consumer existed).
    let transformed_buffer =
        match registry.collect_transform_patches(TransformTarget::BUFFER, &AppView::new(state)) {
            Some(patch) => patch
                .apply(TransformSubject::Element(segmented_buffer))
                .into_element(),
            None => registry
                .apply_transform_chain(
                    TransformTarget::BUFFER,
                    TransformSubject::Element(segmented_buffer),
                    &AppView::new(state),
                )
                .into_element(),
        };

    // Read buffer slot contributions from Salsa input
    let buffer_left = handles.slot_contributions.buffer_left(db).clone();
    let buffer_right = handles.slot_contributions.buffer_right(db).clone();
    let above_buffer = handles.slot_contributions.above_buffer(db).clone();
    let below_buffer = handles.slot_contributions.below_buffer(db).clone();

    // Build buffer row: [left_gutter] [slot:left] [buffer] [slot:right] [right_gutter]
    let mut row_children = Vec::new();
    if let Some(left_gutter) = left_gutter {
        row_children.push(FlexChild::fixed(left_gutter));
    }
    row_children.extend(buffer_left);
    row_children.push(FlexChild::flexible(transformed_buffer, 1.0));
    row_children.extend(buffer_right);
    if let Some(right_gutter) = right_gutter {
        row_children.push(FlexChild::fixed(right_gutter));
    }
    let buffer_row = Element::row(row_children);

    // Wrap with above/below slot contributions if present
    let buffer_section = if above_buffer.is_empty() && below_buffer.is_empty() {
        buffer_row
    } else {
        let mut children = Vec::new();
        children.extend(above_buffer);
        children.push(FlexChild::flexible(buffer_row, 1.0));
        children.extend(below_buffer);
        Element::column(children)
    };

    // Apply status transform chain: same shape as buffer above (θ.4).
    let transformed_status = match registry
        .collect_transform_patches(TransformTarget::STATUS_BAR, &AppView::new(state))
    {
        Some(patch) => patch
            .apply(TransformSubject::Element(status_el))
            .into_element(),
        None => registry
            .apply_transform_chain(
                TransformTarget::STATUS_BAR,
                TransformSubject::Element(status_el),
                &AppView::new(state),
            )
            .into_element(),
    };

    // Read status slot contributions from Salsa input
    let status_left = handles.slot_contributions.status_left(db).clone();
    let status_right = handles.slot_contributions.status_right(db).clone();
    let above_status = handles.slot_contributions.above_status(db).clone();

    // Build status row: [slot:left] [status_core] [slot:right]
    // Wrap left/right contributions in ResolvedSlot so tree-walking helpers
    // (e.g. `find_status_left_slot_width` used to offset the prompt cursor)
    // see the same shape as the legacy `surface::resolve` substitution path.
    let status_inner = if status_left.is_empty() && status_right.is_empty() {
        transformed_status
    } else {
        let mut children = Vec::new();
        if !status_left.is_empty() {
            children.push(FlexChild::fixed(Element::Flex {
                direction: Direction::Row,
                children: status_left,
                gap: 0,
                align: crate::element::Align::Start,
                cross_align: crate::element::Align::Start,
                slot: Some(crate::element::FlexSlotMetadata {
                    surface_key: "primary".into(),
                    slot_name: "kasane.status.left".into(),
                    instance_id: ResolvedSlotInstanceId(1),
                }),
            }));
        }
        children.push(FlexChild::flexible(transformed_status, 1.0));
        if !status_right.is_empty() {
            children.push(FlexChild::fixed(Element::Flex {
                direction: Direction::Row,
                children: status_right,
                gap: 0,
                align: crate::element::Align::Start,
                cross_align: crate::element::Align::Start,
                slot: Some(crate::element::FlexSlotMetadata {
                    surface_key: "primary".into(),
                    slot_name: "kasane.status.right".into(),
                    instance_id: ResolvedSlotInstanceId(2),
                }),
            }));
        }
        Element::row(children)
    };

    let status_styled = Element::container(
        status_inner,
        ElementStyle::from(state.observed.status_default_style.to_face()),
    );

    // Wrap with above_status if present
    let status_section = if above_status.is_empty() {
        status_styled
    } else {
        let mut children = Vec::new();
        children.extend(above_status);
        children.push(FlexChild::fixed(status_styled));
        Element::column(children)
    };

    // Compose buffer + status based on status_at_top policy
    let element = if state.policy().status_at_top() {
        Element::column(vec![
            FlexChild::fixed(status_section),
            FlexChild::flexible(buffer_section, 1.0),
        ])
    } else {
        Element::column(vec![
            FlexChild::flexible(buffer_section, 1.0),
            FlexChild::fixed(status_section),
        ])
    };
    (element, display_scroll_offset)
}

// ---------------------------------------------------------------------------
// Public API: Salsa-backed pipeline wrappers
// ---------------------------------------------------------------------------

/// Optional parameters for [`render_pipeline_cached`].
///
/// All fields default to empty/None/Off — test and benchmark call sites can
/// use `Default::default()` instead of listing a dozen `None` arguments.
#[derive(Default)]
pub struct RenderPipelineOptions<'a> {
    pub surface_registry: Option<&'a SurfaceRegistry>,
    pub pane_states: Option<&'a PaneStates<'a>>,
    pub halfblock_cache: Option<&'a mut super::halfblock::HalfblockCache>,
    pub image_protocol: super::ImageProtocol,
    pub image_requests: Option<&'a mut Vec<super::ImageRequest>>,
}

/// Salsa-backed cached rendering pipeline (TUI).
pub fn render_pipeline_cached(
    db: &KasaneDatabase,
    handles: &SalsaInputHandles,
    state: &AppState,
    registry: &PluginView<'_>,
    grid: &mut CellGrid,
    dirty: DirtyFlags,
    options: RenderPipelineOptions<'_>,
) -> (RenderResult, crate::display::DisplayMapRef) {
    let mut source =
        SalsaViewSource::new(db, handles, options.surface_registry, options.pane_states);
    render_cached_core(
        &mut source,
        state,
        registry,
        grid,
        dirty,
        options.halfblock_cache,
        options.image_protocol,
        options.image_requests,
    )
}

/// Optional parameters for [`scene_render_pipeline_cached`].
#[derive(Default)]
pub struct SceneRenderOptions<'a> {
    pub surface_registry: Option<&'a SurfaceRegistry>,
    pub pane_states: Option<&'a PaneStates<'a>>,
    /// Sub-pixel vertical scroll offset in pixels (GPU-only).
    /// Applied by the GPU renderer to offset buffer content for smooth scrolling.
    pub pixel_y_offset: f32,
}

/// Salsa-backed scene rendering pipeline (GPU).
#[allow(clippy::too_many_arguments)]
pub fn scene_render_pipeline_cached<'a>(
    db: &KasaneDatabase,
    handles: &SalsaInputHandles,
    state: &AppState,
    registry: &PluginView<'_>,
    cell_size: scene::CellSize,
    dirty: DirtyFlags,
    scene_cache: &'a mut SceneCache,
    options: SceneRenderOptions<'_>,
) -> (
    &'a [DrawCommand],
    RenderResult,
    crate::display::DisplayMapRef,
) {
    let mut source =
        SalsaViewSource::new(db, handles, options.surface_registry, options.pane_states);
    scene_render_core(&mut source, state, registry, cell_size, dirty, scene_cache)
}

/// Shift an overlay anchor's absolute coordinates by (dx, dy).
///
/// Used in multi-pane mode to translate pane-local overlay coordinates
/// (produced by the pane's Kakoune after Resize) into full-screen coordinates.
fn offset_overlay_anchor(anchor: &mut crate::element::OverlayAnchor, dx: u16, dy: u16) {
    match anchor {
        crate::element::OverlayAnchor::Absolute { x, y, .. } => {
            *x += dx;
            *y += dy;
        }
        crate::element::OverlayAnchor::AnchorPoint { coord, .. } => {
            coord.column += dx as i32;
            coord.line += dy as i32;
        }
        crate::element::OverlayAnchor::Fill => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::element::OverlayAnchor;
    use crate::protocol::Coord;

    #[test]
    fn offset_absolute_anchor() {
        let mut anchor = OverlayAnchor::Absolute {
            x: 5,
            y: 3,
            w: 10,
            h: 4,
        };
        offset_overlay_anchor(&mut anchor, 20, 10);
        assert!(matches!(
            anchor,
            OverlayAnchor::Absolute {
                x: 25,
                y: 13,
                w: 10,
                h: 4
            }
        ));
    }

    #[test]
    fn offset_anchor_point() {
        let mut anchor = OverlayAnchor::AnchorPoint {
            coord: Coord { line: 2, column: 8 },
            prefer_above: false,
            avoid: vec![],
        };
        offset_overlay_anchor(&mut anchor, 15, 7);
        match &anchor {
            OverlayAnchor::AnchorPoint { coord, .. } => {
                assert_eq!(coord.column, 23);
                assert_eq!(coord.line, 9);
            }
            _ => panic!("expected AnchorPoint"),
        }
    }

    #[test]
    fn offset_fill_is_noop() {
        let mut anchor = OverlayAnchor::Fill;
        offset_overlay_anchor(&mut anchor, 10, 10);
        assert!(matches!(anchor, OverlayAnchor::Fill));
    }
}

#[cfg(test)]
mod inline_box_dispatch_tests {
    use super::*;
    use crate::display::InlineBoxAlignment;
    use crate::element::Element;

    use crate::plugin::handler_registry::HandlerRegistry;
    use crate::plugin::state::Plugin;
    use crate::plugin::{PluginId, PluginRuntime};
    use crate::protocol::NamedColor;
    use crate::render::CursorStyle;
    use crate::render::inline_decoration::InlineBoxSlotMeta;
    use crate::render::scene::{
        BufferParagraph, CellSize, DrawCommand, ParagraphAnnotation, PixelPos,
    };
    use crate::render::theme::Theme;

    /// Plugin that paints a 2-cell × 1-line FillRect inside its inline box.
    struct InlineBoxPainterPlugin;

    impl Plugin for InlineBoxPainterPlugin {
        type State = ();

        fn id(&self) -> PluginId {
            PluginId::from("test.inline-box-painter")
        }

        fn register(&self, r: &mut HandlerRegistry<()>) {
            r.on_paint_inline_box(|_state, box_id, _app| {
                // Match the box_id our test will pass through.
                if box_id == 0xCAFE {
                    Some(Element::text(
                        "X",
                        crate::protocol::Style {
                            bg: crate::protocol::Brush::Named(NamedColor::Magenta),
                            ..crate::protocol::Style::default()
                        },
                    ))
                } else {
                    None
                }
            });
        }
    }

    #[test]
    fn populate_inline_box_paint_commands_fills_paint_for_dispatched_slot() {
        // ADR-031 Phase 10 Step 2-renderer (Step A.2b): verify that the
        // post-walk dispatch helper populates `inline_box_paint_commands`
        // for slots whose owning plugin returns Some(Element).
        let owner = PluginId::from("test.inline-box-painter");
        let mut runtime = PluginRuntime::new();
        runtime.register(InlineBoxPainterPlugin);

        let mut commands = vec![DrawCommand::RenderParagraph {
            pos: PixelPos { x: 0.0, y: 0.0 },
            max_width: 800.0,
            paragraph: BufferParagraph {
                atoms: Vec::new(),
                base_face: crate::protocol::Style::default(),
                annotations: Vec::<ParagraphAnnotation>::new(),
                inline_box_slots: vec![InlineBoxSlotMeta {
                    byte_offset: 0,
                    width_cells: 2.0,
                    height_lines: 1.0,
                    box_id: 0xCAFE,
                    alignment: InlineBoxAlignment::Center,
                    owner: owner.clone(),
                }],
                inline_box_paint_commands: vec![Vec::new()],
            },
            line_idx: 0,
        }];

        let state = crate::test_support::test_state_80x24();
        let view = runtime.view();
        let theme = Theme::default_theme();
        let cell_size = CellSize {
            width: 10.0,
            height: 20.0,
        };

        populate_inline_box_paint_commands(
            &mut commands,
            &view,
            &state,
            &theme,
            cell_size,
            CursorStyle::Block,
        );

        let DrawCommand::RenderParagraph { paragraph, .. } = &commands[0] else {
            panic!("expected RenderParagraph");
        };
        assert!(
            !paragraph.inline_box_paint_commands[0].is_empty(),
            "expected paint commands populated for matching box_id"
        );
    }

    #[test]
    fn populate_inline_box_paint_commands_leaves_slot_empty_when_plugin_returns_none() {
        // Plugin returns None for a non-matching box_id; the slot's paint
        // commands stay empty so the GPU renderer falls back to the
        // placeholder fill.
        let owner = PluginId::from("test.inline-box-painter");
        let mut runtime = PluginRuntime::new();
        runtime.register(InlineBoxPainterPlugin);

        let mut commands = vec![DrawCommand::RenderParagraph {
            pos: PixelPos { x: 0.0, y: 0.0 },
            max_width: 800.0,
            paragraph: BufferParagraph {
                atoms: Vec::new(),
                base_face: crate::protocol::Style::default(),
                annotations: Vec::new(),
                inline_box_slots: vec![InlineBoxSlotMeta {
                    byte_offset: 0,
                    width_cells: 2.0,
                    height_lines: 1.0,
                    box_id: 0xDEAD,
                    alignment: InlineBoxAlignment::Center,
                    owner: owner.clone(),
                }],
                inline_box_paint_commands: vec![Vec::new()],
            },
            line_idx: 0,
        }];

        let state = crate::test_support::test_state_80x24();
        let view = runtime.view();
        let theme = Theme::default_theme();
        let cell_size = CellSize {
            width: 10.0,
            height: 20.0,
        };

        populate_inline_box_paint_commands(
            &mut commands,
            &view,
            &state,
            &theme,
            cell_size,
            CursorStyle::Block,
        );

        let DrawCommand::RenderParagraph { paragraph, .. } = &commands[0] else {
            panic!("expected RenderParagraph");
        };
        assert!(
            paragraph.inline_box_paint_commands[0].is_empty(),
            "expected paint commands empty when plugin returns None"
        );
    }
}
