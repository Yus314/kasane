use super::CursorStyleHint;
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
use super::walk;
use super::{RenderResult, view};
use crate::display::{DisplayMap, DisplayMapRef};
use crate::layout::Rect;
use crate::layout::flex;
use crate::layout::line_display_width;
use crate::plugin::AppView;
use crate::plugin::PluginView;
use crate::protocol::CursorMode;
use crate::state::{AppState, DirtyFlags};
use crate::surface::SurfaceRegistry;

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

    /// Expose the surface registry when the source is backed by multi-pane composition.
    fn surface_registry(&self) -> Option<&SurfaceRegistry> {
        None
    }
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
    pub segment_map: Option<std::sync::Arc<crate::display::segment_map::SegmentMap>>,
    pub focused_pane_rect: Option<Rect>,
    pub focused_pane_state: Option<Box<AppState>>,
    /// Width of the `kasane.status.left` slot, used to offset prompt cursor.
    pub status_content_x_offset: u16,
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
// Generic core pipeline functions
// ---------------------------------------------------------------------------

/// Core cached rendering pipeline, generic over the view section source.
#[allow(clippy::too_many_arguments)]
pub(crate) fn render_cached_core(
    source: &mut impl ViewSource,
    state: &AppState,
    registry: &PluginView<'_>,
    grid: &mut CellGrid,
    dirty: DirtyFlags,
    halfblock_cache: Option<&mut super::halfblock::HalfblockCache>,
    image_protocol: super::ImageProtocol,
    image_requests: Option<&mut Vec<super::ImageRequest>>,
) -> (RenderResult, DisplayMapRef) {
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
) -> (&'a [DrawCommand], RenderResult, DisplayMapRef) {
    crate::perf::perf_span!("scene_render_pipeline");

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

// ---------------------------------------------------------------------------
// Public API
// ---------------------------------------------------------------------------

/// GUI scene rendering pipeline.
pub fn scene_render_pipeline(
    state: &AppState,
    registry: &PluginView<'_>,
    cell_size: scene::CellSize,
) -> (Vec<DrawCommand>, RenderResult, DisplayMapRef) {
    let mut scene_cache = SceneCache::new();
    let mut source = DirectViewSource;
    let (commands, result, display_map) = scene_render_core(
        &mut source,
        state,
        registry,
        cell_size,
        DirtyFlags::ALL,
        &mut scene_cache,
    );
    (commands.to_vec(), result, display_map)
}

/// Declarative rendering pipeline.
pub fn render_pipeline(
    state: &AppState,
    registry: &PluginView<'_>,
    grid: &mut CellGrid,
) -> (RenderResult, DisplayMapRef) {
    let mut source = DirectViewSource;
    render_cached_core(
        &mut source,
        state,
        registry,
        grid,
        DirtyFlags::ALL,
        None,
        super::ImageProtocol::Off,
        None,
    )
}

/// Declarative rendering pipeline with explicit dirty flags for incremental rendering.
/// Uses `DirectViewSource` (no Salsa memoization).
pub fn render_pipeline_direct(
    state: &AppState,
    registry: &PluginView<'_>,
    grid: &mut CellGrid,
    dirty: DirtyFlags,
) -> (RenderResult, DisplayMapRef) {
    let mut source = DirectViewSource;
    render_cached_core(
        &mut source,
        state,
        registry,
        grid,
        dirty,
        None,
        super::ImageProtocol::Off,
        None,
    )
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

#[cfg(test)]
mod inline_box_dispatch_tests {
    use super::*;
    use crate::display::InlineBoxAlignment;
    use crate::element::Element;

    use crate::plugin::handler_registry::HandlerRegistry;
    use crate::plugin::state::Plugin;
    use crate::plugin::{PluginId, PluginRuntime};
    use crate::protocol::{Color, NamedColor, WireFace};
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
            PluginId("test.inline-box-painter".into())
        }

        fn register(&self, r: &mut HandlerRegistry<()>) {
            r.on_paint_inline_box(|_state, box_id, _app| {
                // Match the box_id our test will pass through.
                if box_id == 0xCAFE {
                    Some(Element::text(
                        "X",
                        WireFace {
                            bg: Color::Named(NamedColor::Magenta),
                            ..WireFace::default()
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
        let owner = PluginId("test.inline-box-painter".into());
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
        let owner = PluginId("test.inline-box-painter".into());
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
