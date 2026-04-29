pub mod info;
pub mod menu;
#[cfg(test)]
#[allow(clippy::field_reassign_with_default)]
mod tests;

use std::sync::Arc;

use crate::display::DisplayMapRef;
use crate::display::segment_map::SegmentMap;
use crate::element::{
    Direction, Element, ElementStyle, FlexChild, Overlay, OverlayAnchor, StyleToken,
};
use crate::layout::line_display_width;
use crate::plugin::{AnnotateContext, AppView, PluginView, TransformSubject, TransformTarget};
use crate::protocol::{Atom, Face, Line, MenuStyle};
use crate::state::AppState;
use crate::surface::{SurfaceComposeResult, SurfaceRenderReport};

/// Build the full Element tree from application state.
pub fn view(state: &AppState, registry: &PluginView<'_>) -> Element {
    view_sections(state, registry).into_element()
}

/// Build decomposed view sections without caching.
pub(crate) fn view_sections(state: &AppState, registry: &PluginView<'_>) -> ViewSections {
    crate::perf::perf_span!("view_sections");

    let base = legacy_surface_compose_result(state, registry);
    let app_view = AppView::new(state);
    let display_map = registry.collect_display_map(&app_view);
    let mut overlays = Vec::new();
    if let Some(menu) = build_menu_section(state, registry) {
        overlays.push(menu);
    }
    let menu_rect = crate::layout::get_menu_rect(state);
    // Collect plugin overlays before info so info can avoid them.
    let overlay_ctx = crate::plugin::OverlayContext {
        screen_cols: state.runtime.cols,
        screen_rows: state.runtime.rows,
        menu_rect,
        existing_overlays: vec![],
        focused_surface_id: None,
    };
    let plugin_overlay_contributions = registry.collect_overlays_with_ctx(&app_view, &overlay_ctx);
    let plugin_overlay_rects: Vec<crate::layout::Rect> = plugin_overlay_contributions
        .iter()
        .filter_map(|oc| match &oc.anchor {
            crate::element::OverlayAnchor::Absolute { x, y, w, h } => Some(crate::layout::Rect {
                x: *x,
                y: *y,
                w: *w,
                h: *h,
            }),
            _ => None,
        })
        .collect();
    overlays.extend(
        plugin_overlay_contributions
            .into_iter()
            .map(|oc| crate::element::Overlay {
                element: oc.element,
                anchor: oc.anchor,
            }),
    );
    overlays.extend(build_info_section_with_avoid(
        state,
        registry,
        &plugin_overlay_rects,
    ));

    let buffer_rows = state.available_height() as usize;
    let default_offset = crate::display::compute_display_scroll_offset(
        &display_map,
        crate::display::BufferLine(state.observed.cursor_pos.line as usize),
        buffer_rows,
    );
    let cursor_display_y = display_map
        .buffer_to_display(crate::display::BufferLine(
            state.observed.cursor_pos.line as usize,
        ))
        .map(|dl| dl.0)
        .unwrap_or(state.observed.cursor_pos.line as usize);
    let display_scroll_offset = registry.resolve_display_scroll_offset(
        cursor_display_y,
        buffer_rows,
        default_offset.0,
        &app_view,
    );

    ViewSections {
        base: base.base.unwrap_or(Element::Empty),
        overlays,
        surface_reports: base.surface_reports,
        display_map,
        display_scroll_offset,
        segment_map: None,
        focused_pane_rect: None,
        focused_pane_state: None,
    }
}

/// Decomposed view sections for per-section caching.
pub struct ViewSections {
    pub base: Element,
    /// All overlays (menu, info, plugin) in z_index-sorted order.
    pub overlays: Vec<Overlay>,
    pub surface_reports: Vec<SurfaceRenderReport>,
    /// The active DisplayMap for the current frame (identity if no transforms).
    pub display_map: DisplayMapRef,
    /// Display scroll offset: first display line to render.
    /// Non-zero when folds push the cursor below the viewport.
    pub display_scroll_offset: usize,
    /// Segment map for content annotation layout (None when no annotations).
    pub segment_map: Option<Arc<SegmentMap>>,
    /// Multi-pane: focused pane rectangle. None = single pane.
    pub focused_pane_rect: Option<crate::layout::Rect>,
    /// Multi-pane: focused pane's AppState. When Some, cursor functions use this
    /// instead of the primary state.
    pub focused_pane_state: Option<Box<AppState>>,
}

impl ViewSections {
    /// Assemble sections into the final Element tree.
    pub fn into_element(self) -> Element {
        if self.overlays.is_empty() {
            self.base
        } else {
            Element::stack(self.base, self.overlays)
        }
    }

    /// Assemble sections into an Element tree AND layout, reusing a pre-computed
    /// base layout to avoid a redundant `flex::place()` call.
    ///
    /// The base layout was already computed during `backfill_surface_report_areas`.
    /// Overlays are positioned via `layout_single_overlay` (absolute positioning, cheap).
    pub fn into_element_and_layout(
        self,
        base_layout: crate::layout::flex::LayoutResult,
        root_area: crate::layout::Rect,
        state: &AppState,
    ) -> (Element, crate::layout::flex::LayoutResult) {
        if self.overlays.is_empty() {
            (self.base, base_layout)
        } else {
            let mut layout_children = vec![base_layout];
            for overlay in &self.overlays {
                layout_children.push(crate::layout::layout_single_overlay(
                    overlay, root_area, state,
                ));
            }
            let layout = crate::layout::flex::LayoutResult {
                area: root_area,
                children: layout_children,
            };
            (Element::stack(self.base, self.overlays), layout)
        }
    }
}

fn legacy_surface_compose_result(
    state: &AppState,
    registry: &PluginView<'_>,
) -> SurfaceComposeResult {
    let mut surface_registry = crate::surface::SurfaceRegistry::new();
    surface_registry.register(Box::new(crate::surface::buffer::KakouneBufferSurface::new()));
    surface_registry.register(Box::new(crate::surface::status::StatusBarSurface::new()));
    surface_registry.compose_base_result(
        state,
        None,
        registry,
        crate::layout::Rect {
            x: 0,
            y: 0,
            w: state.runtime.cols,
            h: state.runtime.rows,
        },
    )
}

/// Build the menu overlay section.
///
/// All rendering goes through plugin renderers (the builtin menu plugin is the
/// lowest-priority renderer). The overlay-level transform chain is always applied.
#[crate::kasane_component]
fn build_menu_section(state: &AppState, registry: &PluginView<'_>) -> Option<Overlay> {
    let app_view = AppView::new(state);

    let overlay = registry.resolve_menu_overlay(&app_view)?;

    // Apply overlay-level transform chain if menu state is available
    if let Some(menu_state) = state.observed.menu.as_ref() {
        let transform_target = match menu_state.style {
            MenuStyle::Prompt => TransformTarget::MENU_PROMPT,
            MenuStyle::Inline => TransformTarget::MENU_INLINE,
            MenuStyle::Search => TransformTarget::MENU_SEARCH,
        };
        let result = registry.apply_transform_chain_hierarchical(
            transform_target,
            TransformSubject::Overlay(overlay),
            &app_view,
        );
        Some(
            result
                .into_overlay()
                .expect("overlay transform preserves variant"),
        )
    } else {
        Some(overlay)
    }
}

/// Build info overlay section with collision avoidance.
#[crate::kasane_component]
fn build_info_section(state: &AppState, registry: &PluginView<'_>) -> Vec<Overlay> {
    build_info_section_with_avoid(state, registry, &[])
}

/// Build info overlay section with collision avoidance, including additional
/// avoid rects from plugin overlays.
///
/// Computes initial avoid rects (menu, cursor, plugin overlays), then delegates
/// to plugin renderers (the builtin info plugin is the lowest-priority renderer).
fn build_info_section_with_avoid(
    state: &AppState,
    registry: &PluginView<'_>,
    extra_avoid: &[crate::layout::Rect],
) -> Vec<Overlay> {
    let menu_rect = crate::layout::get_menu_rect(state);
    let mut avoid_rects: Vec<crate::layout::Rect> = Vec::new();
    if let Some(mr) = menu_rect {
        avoid_rects.push(mr);
    }
    // Add cursor position as a 1×1 avoid rect (collision avoidance)
    avoid_rects.push(crate::layout::Rect {
        x: state.observed.cursor_pos.column as u16,
        y: state.observed.cursor_pos.line as u16,
        w: 1,
        h: 1,
    });
    // Include plugin overlay rects for collision avoidance.
    avoid_rects.extend_from_slice(extra_avoid);

    // All rendering goes through plugins (builtin info plugin is lowest priority)
    let app_view = AppView::new(state);
    registry
        .resolve_info_overlays(&app_view, &avoid_rects)
        .unwrap_or_default()
}

fn build_status_core(state: &AppState) -> Element {
    let status_face = state.config.theme.resolve_with_protocol_fallback(
        &StyleToken::STATUS_LINE,
        state.observed.status_default_style.to_face(),
    );
    let status_line = build_styled_line_with_base(&state.inference.status_line, &status_face, 0);
    let mode_line = build_styled_line_with_base(&state.observed.status_mode_line, &status_face, 0);
    let mode_width = line_display_width(&state.observed.status_mode_line) as u16;

    let mut children = Vec::new();
    children.push(FlexChild::flexible(status_line, 1.0));
    if mode_width > 0 {
        children.push(FlexChild::fixed(mode_line));
    }
    Element::row(children)
}

pub(crate) fn build_status_surface_abstract(
    state: &AppState,
    registry: &PluginView<'_>,
) -> Element {
    let transformed_core = registry
        .apply_transform_chain(
            TransformTarget::STATUS_BAR,
            TransformSubject::Element(build_status_core(state)),
            &AppView::new(state),
        )
        .into_element();

    let status_face = state.config.theme.resolve_with_protocol_fallback(
        &StyleToken::STATUS_LINE,
        state.observed.status_default_style.to_face(),
    );
    let row = Element::container(
        Element::row(vec![
            FlexChild::fixed(Element::slot_placeholder(
                "kasane.status.left",
                Direction::Row,
            )),
            FlexChild::flexible(transformed_core, 1.0),
            FlexChild::fixed(Element::slot_placeholder(
                "kasane.status.right",
                Direction::Row,
            )),
        ]),
        ElementStyle::from(status_face),
    );

    Element::column(vec![
        FlexChild::fixed(Element::slot_placeholder(
            "kasane.status.above",
            Direction::Column,
        )),
        FlexChild::fixed(row),
    ])
}

pub(crate) struct BufferCoreParts {
    pub(crate) left_gutter: Option<Element>,
    pub(crate) buffer: Element,
    pub(crate) right_gutter: Option<Element>,
}

/// Segment a BufferRef into sub-BufferRefs interleaved with content annotation
/// Elements in a Flex Column.
///
/// If `annotations` is empty, returns the buffer unchanged (zero overhead).
///
/// For each annotation, the buffer is split at the anchor line and the annotation
/// element is inserted between the resulting sub-BufferRefs.
///
/// **T15 (Annotation Suppression)**: Annotations whose anchor line is invisible
/// in the display map (hidden or folded) are filtered out.
pub(crate) fn segment_buffer(
    buffer_ref: Element,
    annotations: &[crate::display::ContentAnnotation],
    display_map: Option<&crate::display::DisplayMap>,
) -> Element {
    use crate::display::ContentAnchor;

    if annotations.is_empty() {
        return buffer_ref;
    }

    // Extract BufferRef fields
    let (line_range, line_backgrounds, dm_ref, state, inline_decorations, virtual_text) =
        match buffer_ref {
            Element::BufferRef {
                line_range,
                line_backgrounds,
                display_map,
                state,
                inline_decorations,
                virtual_text,
            } => (
                line_range,
                line_backgrounds,
                display_map,
                state,
                inline_decorations,
                virtual_text,
            ),
            other => return other, // Not a BufferRef — return unchanged
        };

    // T15: Filter annotations whose anchor line is invisible in the display map.
    let visible_annotations: Vec<_> = annotations
        .iter()
        .filter(|ann| {
            let anchor_line = ann.anchor.line();
            if let Some(dm) = display_map {
                // Anchor line must be visible (mapped to a display line)
                dm.buffer_to_display(crate::display::BufferLine(anchor_line))
                    .is_some()
            } else {
                // No display map → identity, all lines visible within range
                anchor_line < line_range.end
            }
        })
        .collect();

    if visible_annotations.is_empty() {
        // All annotations filtered — reconstruct unchanged BufferRef
        return Element::BufferRef {
            line_range,
            line_backgrounds,
            display_map: dm_ref,
            state,
            inline_decorations,
            virtual_text,
        };
    }

    // Collect unique split points (display line indices where annotations anchor)
    // sorted by line number. We group InsertAfter at line N and InsertBefore at line N+1
    // into the same split point after display line N.
    let mut split_points: Vec<usize> = Vec::new();
    for ann in &visible_annotations {
        let split_after = match &ann.anchor {
            ContentAnchor::InsertAfter(line) => *line,
            ContentAnchor::InsertBefore(line) => {
                if *line > line_range.start {
                    line - 1
                } else {
                    // InsertBefore line 0 → insert before the very first line
                    // We'll handle this as a special case below
                    usize::MAX
                }
            }
        };
        if split_after != usize::MAX && !split_points.contains(&split_after) {
            split_points.push(split_after);
        }
    }
    split_points.sort();

    // Collect annotations that go before the first line
    let before_first: Vec<_> = visible_annotations
        .iter()
        .filter(
            |ann| matches!(&ann.anchor, ContentAnchor::InsertBefore(l) if *l <= line_range.start),
        )
        .collect();

    // Build Flex Column children
    let mut children: Vec<FlexChild> = Vec::new();

    // Insert any InsertBefore annotations targeting the first line
    for ann in &before_first {
        children.push(FlexChild::fixed(ann.element.clone()));
    }

    let mut current_start = line_range.start;

    for &split_after in &split_points {
        if split_after < current_start || split_after >= line_range.end {
            continue;
        }

        let segment_end = split_after + 1;
        // Emit sub-BufferRef for lines [current_start..segment_end]
        if segment_end > current_start {
            children.push(FlexChild::fixed(Element::BufferRef {
                line_range: current_start..segment_end,
                line_backgrounds: line_backgrounds.clone(),
                display_map: dm_ref.clone(),
                state: None, // state is per-pane, handled separately
                inline_decorations: inline_decorations.clone(),
                virtual_text: virtual_text.clone(),
            }));
        }

        // Emit annotation elements anchored at this split point
        for ann in &visible_annotations {
            let matches = match &ann.anchor {
                ContentAnchor::InsertAfter(line) => *line == split_after,
                ContentAnchor::InsertBefore(line) => {
                    *line > line_range.start && *line - 1 == split_after
                }
                #[allow(unreachable_patterns)]
                _ => false,
            };
            if matches {
                children.push(FlexChild::fixed(ann.element.clone()));
            }
        }

        current_start = segment_end;
    }

    // Emit trailing sub-BufferRef for remaining lines
    if current_start < line_range.end {
        children.push(FlexChild::fixed(Element::BufferRef {
            line_range: current_start..line_range.end,
            line_backgrounds: line_backgrounds.clone(),
            display_map: dm_ref.clone(),
            state: None,
            inline_decorations: inline_decorations.clone(),
            virtual_text: virtual_text.clone(),
        }));
    }

    Element::column(children)
}

pub(crate) fn build_buffer_core_parts(
    state: &AppState,
    registry: &PluginView<'_>,
) -> BufferCoreParts {
    use std::sync::Arc;

    let buffer_rows = state.available_height() as usize;

    // Collect display map before annotations (annotations may use it)
    let app_view = AppView::new(state);
    let display_map = registry.collect_display_map(&app_view);
    let dm_for_element = if display_map.is_identity() {
        None
    } else {
        Some(Arc::clone(&display_map))
    };
    let annotate_ctx = AnnotateContext {
        line_width: state.runtime.cols,
        gutter_width: 0,
        display_map: Some(Arc::clone(&display_map)),
        pane_surface_id: None,
        pane_focused: true,
    };
    let annotations = registry.collect_annotations(&app_view, &annotate_ctx);
    let line_backgrounds = annotations.line_backgrounds;
    let inline_decorations = annotations.inline_decorations;
    let virtual_text = annotations.virtual_text;
    // When a non-identity DisplayMap is active, compute scroll offset so
    // the cursor stays visible, then use offset-based line_range.
    let (effective_start, effective_end, _display_scroll_offset) = if !display_map.is_identity() {
        let visible_height = display_map.display_line_count().min(buffer_rows);
        let default_offset = crate::display::compute_display_scroll_offset(
            &display_map,
            crate::display::BufferLine(state.observed.cursor_pos.line as usize),
            visible_height,
        );
        let cursor_display_y = display_map
            .buffer_to_display(crate::display::BufferLine(
                state.observed.cursor_pos.line as usize,
            ))
            .map(|dl| dl.0)
            .unwrap_or(state.observed.cursor_pos.line as usize);
        let offset = registry.resolve_display_scroll_offset(
            cursor_display_y,
            visible_height,
            default_offset.0,
            &app_view,
        );
        let end = (offset + visible_height).min(display_map.display_line_count());
        (offset, end, offset)
    } else {
        (0, buffer_rows, 0)
    };
    let buffer_element = if line_backgrounds.is_some()
        || dm_for_element.is_some()
        || inline_decorations.is_some()
        || virtual_text.is_some()
    {
        Element::BufferRef {
            line_range: effective_start..effective_end,
            line_backgrounds: line_backgrounds.map(Arc::new),
            display_map: dm_for_element,
            state: None,
            inline_decorations: inline_decorations.map(Arc::new),
            virtual_text: virtual_text.map(Arc::new),
        }
    } else {
        Element::buffer_ref(0..buffer_rows)
    };
    // Segment buffer with content annotations (Phase D)
    let content_annotations = registry.collect_content_annotations(&app_view, &annotate_ctx);
    let segmented_buffer = segment_buffer(
        buffer_element,
        &content_annotations,
        if display_map.is_identity() {
            None
        } else {
            Some(&display_map)
        },
    );
    let transformed_buffer = registry
        .apply_transform_chain(
            TransformTarget::BUFFER,
            TransformSubject::Element(segmented_buffer),
            &app_view,
        )
        .into_element();
    BufferCoreParts {
        left_gutter: annotations.left_gutter,
        buffer: transformed_buffer,
        right_gutter: annotations.right_gutter,
    }
}

pub(crate) fn build_buffer_surface_abstract(
    state: &AppState,
    registry: &PluginView<'_>,
) -> Element {
    let parts = build_buffer_core_parts(state, registry);
    let mut row_children = Vec::new();
    if let Some(left_gutter) = parts.left_gutter {
        row_children.push(FlexChild::fixed(left_gutter));
    }
    row_children.push(FlexChild::fixed(Element::slot_placeholder(
        "kasane.buffer.left",
        Direction::Row,
    )));
    row_children.push(FlexChild::flexible(parts.buffer, 1.0));
    row_children.push(FlexChild::fixed(Element::slot_placeholder(
        "kasane.buffer.right",
        Direction::Row,
    )));
    if let Some(right_gutter) = parts.right_gutter {
        row_children.push(FlexChild::fixed(right_gutter));
    }

    let base = Element::column(vec![
        FlexChild::fixed(Element::slot_placeholder(
            "kasane.buffer.above",
            Direction::Column,
        )),
        FlexChild::flexible(Element::row(row_children), 1.0),
        FlexChild::fixed(Element::slot_placeholder(
            "kasane.buffer.below",
            Direction::Column,
        )),
    ]);

    Element::stack(
        base,
        vec![Overlay {
            element: Element::slot_placeholder("kasane.buffer.overlay", Direction::Column),
            anchor: OverlayAnchor::Fill,
        }],
    )
}

/// Build the menu overlay section (non-cached, for Surface pipeline).
pub(crate) fn build_menu_section_standalone(
    state: &AppState,
    registry: &PluginView<'_>,
) -> Option<Overlay> {
    build_menu_section(state, registry)
}

/// Build the info overlay section (non-cached, for Surface pipeline).
pub(crate) fn build_info_section_standalone(
    state: &AppState,
    registry: &PluginView<'_>,
) -> Vec<Overlay> {
    build_info_section(state, registry)
}

/// Build a StyledLine element from a protocol Line, resolving faces against a base.
pub(crate) fn build_styled_line_with_base(
    line: &Line,
    base_face: &Face,
    _max_width: u16,
) -> Element {
    let resolved: Vec<Atom> = line
        .iter()
        .map(|atom| {
            Atom::from_face(
                crate::protocol::resolve_face(&atom.face(), base_face),
                atom.contents.clone(),
            )
        })
        .collect();
    Element::StyledLine(resolved)
}
