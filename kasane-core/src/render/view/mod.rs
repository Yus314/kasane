pub(crate) mod info;
pub(crate) mod menu;
#[cfg(test)]
#[allow(clippy::field_reassign_with_default)]
mod tests;

use crate::display::DisplayMapRef;
use crate::element::{Direction, Element, FlexChild, Overlay, OverlayAnchor, Style};
use crate::layout::line_display_width;
use crate::plugin::{AnnotateContext, AppView, PluginView, TransformSubject, TransformTarget};
use crate::protocol::{Atom, Face, InfoStyle, Line, MenuStyle};
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
    let menu_overlay = build_menu_section(state, registry);
    let info_overlays = build_info_section(state, registry);
    let overlay_ctx = crate::plugin::OverlayContext {
        screen_cols: state.cols,
        screen_rows: state.rows,
        menu_rect: None,
        existing_overlays: vec![],
        focused_surface_id: None,
    };
    let plugin_overlays: Vec<crate::element::Overlay> = registry
        .collect_overlays_with_ctx(&app_view, &overlay_ctx)
        .into_iter()
        .map(|oc| crate::element::Overlay {
            element: oc.element,
            anchor: oc.anchor,
        })
        .collect();

    let buffer_rows = state.available_height() as usize;
    let display_scroll_offset = crate::display::compute_display_scroll_offset(
        &display_map,
        state.cursor_pos.line as usize,
        buffer_rows,
    );

    ViewSections {
        base: base.base.unwrap_or(Element::Empty),
        menu_overlay,
        info_overlays,
        plugin_overlays,
        surface_reports: base.surface_reports,
        display_map,
        display_scroll_offset,
        focused_pane_rect: None,
        focused_pane_state: None,
    }
}

/// Decomposed view sections for per-section caching.
pub struct ViewSections {
    pub base: Element,
    pub menu_overlay: Option<Overlay>,
    pub info_overlays: Vec<Overlay>,
    pub plugin_overlays: Vec<Overlay>,
    pub surface_reports: Vec<SurfaceRenderReport>,
    /// The active DisplayMap for the current frame (identity if no transforms).
    pub display_map: DisplayMapRef,
    /// Display scroll offset: first display line to render.
    /// Non-zero when virtual lines push the cursor below the viewport.
    pub display_scroll_offset: usize,
    /// Multi-pane: focused pane rectangle. None = single pane.
    pub focused_pane_rect: Option<crate::layout::Rect>,
    /// Multi-pane: focused pane's AppState. When Some, cursor functions use this
    /// instead of the primary state.
    pub focused_pane_state: Option<Box<AppState>>,
}

impl ViewSections {
    /// Assemble sections into the final Element tree.
    pub fn into_element(self) -> Element {
        let mut overlays = Vec::new();
        if let Some(overlay) = self.menu_overlay {
            overlays.push(overlay);
        }
        overlays.extend(self.info_overlays);
        overlays.extend(self.plugin_overlays);

        if overlays.is_empty() {
            self.base
        } else {
            Element::stack(self.base, overlays)
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
        let mut overlays = Vec::new();
        if let Some(overlay) = self.menu_overlay {
            overlays.push(overlay);
        }
        overlays.extend(self.info_overlays);
        overlays.extend(self.plugin_overlays);

        if overlays.is_empty() {
            (self.base, base_layout)
        } else {
            let mut layout_children = vec![base_layout];
            for overlay in &overlays {
                layout_children.push(crate::layout::layout_single_overlay(
                    overlay, root_area, state,
                ));
            }
            let layout = crate::layout::flex::LayoutResult {
                area: root_area,
                children: layout_children,
            };
            (Element::stack(self.base, overlays), layout)
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
            w: state.cols,
            h: state.rows,
        },
    )
}

/// Build the menu overlay section.
#[crate::kasane_component]
fn build_menu_section(state: &AppState, registry: &PluginView<'_>) -> Option<Overlay> {
    let menu_state = state.menu.as_ref()?;
    let transform_target = match menu_state.style {
        MenuStyle::Prompt => TransformTarget::MenuPrompt,
        MenuStyle::Inline => TransformTarget::MenuInline,
        MenuStyle::Search => TransformTarget::MenuSearch,
    };

    // Build the default menu overlay; apply_transform_chain handles
    // replacement internally (Phase 1) so no explicit get_replacement() needed.
    let menu_overlay = menu::build_menu_overlay(menu_state, state, registry);
    menu_overlay.map(|overlay| {
        // Apply hierarchical transform chain (Menu generic → style-specific)
        let app_view = AppView::new(state);
        let result = registry.apply_transform_chain_hierarchical(
            transform_target,
            TransformSubject::Overlay(overlay),
            &app_view,
        );
        result
            .into_overlay()
            .expect("overlay transform preserves variant")
    })
}

/// Build info overlay section with collision avoidance.
#[crate::kasane_component]
fn build_info_section(state: &AppState, registry: &PluginView<'_>) -> Vec<Overlay> {
    let menu_rect = crate::layout::get_menu_rect(state);
    let mut avoid_rects: Vec<crate::layout::Rect> = Vec::new();
    if let Some(mr) = menu_rect {
        avoid_rects.push(mr);
    }
    // Add cursor position as a 1×1 avoid rect (collision avoidance)
    avoid_rects.push(crate::layout::Rect {
        x: state.cursor_pos.column as u16,
        y: state.cursor_pos.line as u16,
        w: 1,
        h: 1,
    });

    let mut overlays = Vec::new();
    for (info_idx, info_state) in state.infos.iter().enumerate() {
        // Build the default info overlay; apply_transform_chain handles
        // replacement internally (Phase 1) so no explicit get_replacement() needed.
        let info_overlay =
            info::build_info_overlay_indexed(info_state, state, &avoid_rects, info_idx);
        if let Some(overlay) = info_overlay {
            // Apply hierarchical transform chain (Info generic → style-specific)
            let app_view = AppView::new(state);
            let info_target = match info_state.style {
                InfoStyle::Prompt => TransformTarget::InfoPrompt,
                InfoStyle::Modal => TransformTarget::InfoModal,
                _ => TransformTarget::Info,
            };
            let result = registry.apply_transform_chain_hierarchical(
                info_target,
                TransformSubject::Overlay(overlay),
                &app_view,
            );
            let transformed = result
                .into_overlay()
                .expect("overlay transform preserves variant");
            // Track this overlay's rect for subsequent infos to avoid
            // (using post-transform anchor, since transform may modify it)
            if let OverlayAnchor::Absolute { x, y, w, h } = &transformed.anchor {
                avoid_rects.push(crate::layout::Rect {
                    x: *x,
                    y: *y,
                    w: *w,
                    h: *h,
                });
            }
            overlays.push(transformed);
        }
    }
    overlays
}

fn build_status_core(state: &AppState) -> Element {
    let status_line =
        build_styled_line_with_base(&state.status_line, &state.status_default_face, 0);
    let mode_line =
        build_styled_line_with_base(&state.status_mode_line, &state.status_default_face, 0);
    let mode_width = line_display_width(&state.status_mode_line) as u16;

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
            TransformTarget::StatusBar,
            TransformSubject::Element(build_status_core(state)),
            &AppView::new(state),
        )
        .into_element();

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
        Style::from(state.status_default_face),
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
        line_width: state.cols,
        gutter_width: 0,
        display_map: Some(Arc::clone(&display_map)),
        pane_surface_id: None,
        pane_focused: true,
    };
    let annotations = registry.collect_annotations(&app_view, &annotate_ctx);
    let line_backgrounds = annotations.line_backgrounds;
    let inline_decorations = annotations.inline_decorations;
    // When a non-identity DisplayMap is active, compute scroll offset so
    // the cursor stays visible, then use offset-based line_range.
    let (effective_start, effective_end, _display_scroll_offset) = if !display_map.is_identity() {
        let visible_height = display_map.display_line_count().min(buffer_rows);
        let offset = crate::display::compute_display_scroll_offset(
            &display_map,
            state.cursor_pos.line as usize,
            visible_height,
        );
        let end = (offset + visible_height).min(display_map.display_line_count());
        (offset, end, offset)
    } else {
        (0, buffer_rows, 0)
    };
    let buffer_element =
        if line_backgrounds.is_some() || dm_for_element.is_some() || inline_decorations.is_some() {
            Element::BufferRef {
                line_range: effective_start..effective_end,
                line_backgrounds,
                display_map: dm_for_element,
                state: None,
                inline_decorations,
            }
        } else {
            Element::buffer_ref(0..buffer_rows)
        };
    let transformed_buffer = registry
        .apply_transform_chain(
            TransformTarget::Buffer,
            TransformSubject::Element(buffer_element),
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
        .map(|atom| Atom {
            face: crate::protocol::resolve_face(&atom.face, base_face),
            contents: atom.contents.clone(),
        })
        .collect();
    Element::StyledLine(resolved)
}
