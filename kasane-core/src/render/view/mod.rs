pub(crate) mod info;
pub(crate) mod menu;
#[cfg(test)]
#[allow(clippy::field_reassign_with_default)]
mod tests;

use crate::display::DisplayMapRef;
use crate::element::{Direction, Element, FlexChild, Overlay, OverlayAnchor, Style};
use crate::layout::line_display_width;
use crate::plugin::{AnnotateContext, PluginView, TransformTarget};
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
    let display_map = registry.collect_display_map(state);
    let menu_overlay = build_menu_section(state, registry);
    let info_overlays = build_info_section(state, registry);
    let overlay_ctx = crate::plugin::OverlayContext {
        screen_cols: state.cols,
        screen_rows: state.rows,
        menu_rect: None,
        existing_overlays: vec![],
    };
    let plugin_overlays: Vec<crate::element::Overlay> = registry
        .collect_overlays_with_ctx(state, &overlay_ctx)
        .into_iter()
        .map(|oc| crate::element::Overlay {
            element: oc.element,
            anchor: oc.anchor,
        })
        .collect();

    ViewSections {
        base: base.base.unwrap_or(Element::Empty),
        menu_overlay,
        info_overlays,
        plugin_overlays,
        surface_reports: base.surface_reports,
        display_map,
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
    menu_overlay.map(|mut overlay| {
        // Apply transform chain (Menu generic + style-specific)
        overlay.element = registry.apply_transform_chain(
            TransformTarget::Menu,
            || overlay.element.clone(),
            state,
        );
        overlay.element =
            registry.apply_transform_chain(transform_target, || overlay.element.clone(), state);
        overlay
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
        if let Some(mut overlay) = info_overlay {
            // Track this overlay's rect for subsequent infos to avoid
            if let OverlayAnchor::Absolute { x, y, w, h } = &overlay.anchor {
                avoid_rects.push(crate::layout::Rect {
                    x: *x,
                    y: *y,
                    w: *w,
                    h: *h,
                });
            }
            // Apply transform chain (Info generic + style-specific)
            overlay.element = registry.apply_transform_chain(
                TransformTarget::Info,
                || overlay.element.clone(),
                state,
            );
            if let Some(transform_target) = match info_state.style {
                InfoStyle::Prompt => Some(TransformTarget::InfoPrompt),
                InfoStyle::Modal => Some(TransformTarget::InfoModal),
                _ => None,
            } {
                overlay.element = registry.apply_transform_chain(
                    transform_target,
                    || overlay.element.clone(),
                    state,
                );
            }
            overlays.push(overlay);
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
    let transformed_core = registry.apply_transform_chain(
        TransformTarget::StatusBar,
        || build_status_core(state),
        state,
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
    let display_map = registry.collect_display_map(state);
    let dm_for_element = if display_map.is_identity() {
        None
    } else {
        Some(Arc::clone(&display_map))
    };
    let annotate_ctx = AnnotateContext {
        line_width: state.cols,
        gutter_width: 0,
        display_map: Some(Arc::clone(&display_map)),
    };
    let annotations = registry.collect_annotations(state, &annotate_ctx);
    let line_backgrounds = annotations.line_backgrounds;
    // When a non-identity DisplayMap is active, line_range must reflect
    // the display line count (which is fewer than buffer lines after fold).
    let effective_rows = if !display_map.is_identity() {
        display_map.display_line_count().min(buffer_rows)
    } else {
        buffer_rows
    };
    let buffer_element = if line_backgrounds.is_some() || dm_for_element.is_some() {
        Element::BufferRef {
            line_range: 0..effective_rows,
            line_backgrounds,
            display_map: dm_for_element,
            state: None,
        }
    } else {
        Element::buffer_ref(0..buffer_rows)
    };
    let transformed_buffer =
        registry.apply_transform_chain(TransformTarget::Buffer, || buffer_element, state);
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
