pub(crate) mod info;
pub(crate) mod menu;
#[cfg(test)]
#[allow(clippy::field_reassign_with_default)]
mod tests;

use crate::element::{Element, FlexChild, Overlay, OverlayAnchor, Style};
use crate::layout::line_display_width;
use crate::plugin::{
    AnnotateContext, ContribSizeHint, ContributeContext, Contribution, PluginRegistry, SlotId,
    TransformTarget,
};
use crate::protocol::{Atom, Face, InfoStyle, Line, MenuStyle};
use crate::render::cache::{ViewCache, cache_dirty_snapshot};
use crate::state::AppState;

use crate::state::DirtyFlags;

// DirtyFlags dependency masks for each component function.
// These match the deps() annotations on the #[kasane_component] attributes.
pub(crate) const BUILD_BASE_DEPS: DirtyFlags = DirtyFlags::from_bits_truncate(
    DirtyFlags::BUFFER_CONTENT.bits() | DirtyFlags::STATUS.bits() | DirtyFlags::OPTIONS.bits(),
);
pub(crate) const BUILD_MENU_SECTION_DEPS: DirtyFlags = DirtyFlags::from_bits_truncate(
    DirtyFlags::MENU_STRUCTURE.bits()
        | DirtyFlags::MENU_SELECTION.bits()
        | DirtyFlags::OPTIONS.bits(),
);
pub(crate) const BUILD_INFO_SECTION_DEPS: DirtyFlags =
    DirtyFlags::from_bits_truncate(DirtyFlags::INFO.bits() | DirtyFlags::OPTIONS.bits());

/// Build the full Element tree from application state (backward-compatible).
pub fn view(state: &AppState, registry: &PluginRegistry) -> Element {
    view_cached(state, registry, &mut ViewCache::new())
}

/// Decomposed view sections for per-section caching.
pub struct ViewSections {
    pub base: Element,
    pub menu_overlay: Option<Overlay>,
    pub info_overlays: Vec<Overlay>,
    pub plugin_overlays: Vec<Overlay>,
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

/// Build the view sections with subtree memoization via ViewCache.
///
/// Uses `ComponentCache::get_or_insert` with the DEPS constants generated
/// by `#[kasane_component]` to automatically skip recomputation when the
/// relevant DirtyFlags are not set.
pub(crate) fn view_sections_cached(
    state: &AppState,
    registry: &PluginRegistry,
    cache: &mut ViewCache,
) -> ViewSections {
    crate::perf::perf_span!("view_sections");

    let base = cache.base.get_or_insert(
        cache_dirty_snapshot(&cache.base, BUILD_BASE_DEPS),
        BUILD_BASE_DEPS,
        || build_base(state, registry),
    );

    build_sections_with_base(base, state, registry, cache)
}

/// Build the view sections using SurfaceRegistry as the element source.
///
/// Uses the same ViewCache infrastructure as `view_sections_cached`, but the
/// base element comes from `SurfaceRegistry::compose_view_sections()` instead
/// of `build_base()`. This enables workspace-aware layouts while preserving
/// all caching optimizations.
pub fn surface_view_sections_cached(
    state: &AppState,
    registry: &PluginRegistry,
    surface_registry: &crate::surface::SurfaceRegistry,
    cache: &mut ViewCache,
) -> ViewSections {
    crate::perf::perf_span!("surface_view_sections");

    let root_area = crate::layout::Rect {
        x: 0,
        y: 0,
        w: state.cols,
        h: state.rows,
    };

    let base = cache.base.get_or_insert(
        cache_dirty_snapshot(&cache.base, BUILD_BASE_DEPS),
        BUILD_BASE_DEPS,
        || {
            let sections = surface_registry.compose_view_sections(state, registry, root_area);
            sections.base
        },
    );

    build_sections_with_base(base, state, registry, cache)
}

/// Shared helper: build menu, info, and plugin overlay sections around a given base.
fn build_sections_with_base(
    base: Element,
    state: &AppState,
    registry: &PluginRegistry,
    cache: &mut ViewCache,
) -> ViewSections {
    let menu_overlay = cache.menu_overlay.get_or_insert(
        cache_dirty_snapshot(&cache.menu_overlay, BUILD_MENU_SECTION_DEPS),
        BUILD_MENU_SECTION_DEPS,
        || build_menu_section(state, registry),
    );

    let info_overlays = cache.info_overlays.get_or_insert(
        cache_dirty_snapshot(&cache.info_overlays, BUILD_INFO_SECTION_DEPS),
        BUILD_INFO_SECTION_DEPS,
        || build_info_section(state, registry),
    );

    let overlay_ctx = crate::plugin::OverlayContext {
        screen_cols: state.cols,
        screen_rows: state.rows,
        menu_rect: None,
        existing_overlays: vec![],
    };
    let plugin_overlays: Vec<Overlay> = registry
        .collect_overlays_with_ctx(state, &overlay_ctx)
        .into_iter()
        .map(|oc| Overlay {
            element: oc.element,
            anchor: oc.anchor,
        })
        .collect();

    ViewSections {
        base,
        menu_overlay,
        info_overlays,
        plugin_overlays,
    }
}

/// Build the full Element tree with subtree memoization via ViewCache.
pub fn view_cached(state: &AppState, registry: &PluginRegistry, cache: &mut ViewCache) -> Element {
    crate::perf::perf_span!("view");
    view_sections_cached(state, registry, cache).into_element()
}

/// Build the base layout: buffer + status bar + plugin slots.
#[crate::kasane_component(deps(BUFFER_CONTENT, STATUS, OPTIONS))]
fn build_base(state: &AppState, registry: &PluginRegistry) -> Element {
    let buffer_rows = state.available_height() as usize;
    let ctx = ContributeContext::new(state, None);

    // Collect plugin contributions via new API
    let above_buffer = registry.collect_contributions(&SlotId::ABOVE_BUFFER, state, &ctx);
    let below_buffer = registry.collect_contributions(&SlotId::BELOW_BUFFER, state, &ctx);
    let buffer_left = registry.collect_contributions(&SlotId::BUFFER_LEFT, state, &ctx);
    let buffer_right = registry.collect_contributions(&SlotId::BUFFER_RIGHT, state, &ctx);
    let above_status = registry.collect_contributions(&SlotId::ABOVE_STATUS, state, &ctx);
    let status_left = registry.collect_contributions(&SlotId::STATUS_LEFT, state, &ctx);
    let status_right = registry.collect_contributions(&SlotId::STATUS_RIGHT, state, &ctx);

    // Build buffer row (center area + optional sidebars)
    // Add plugin line-annotation gutters
    let annotate_ctx = AnnotateContext {
        line_width: state.cols,
        gutter_width: 0,
    };
    let annotations = registry.collect_annotations(state, &annotate_ctx);
    let mut left_elements: Vec<Element> = Vec::new();
    let mut right_elements: Vec<Element> = Vec::new();
    if let Some(gutter) = annotations.left_gutter {
        left_elements.push(gutter);
    }
    for c in buffer_left {
        left_elements.push(c.element);
    }
    for c in buffer_right {
        right_elements.push(c.element);
    }
    if let Some(gutter) = annotations.right_gutter {
        right_elements.push(gutter);
    }

    // Collect line backgrounds and build BufferRef
    let line_backgrounds = annotations.line_backgrounds;
    let buffer_element = if line_backgrounds.is_some() {
        Element::BufferRef {
            line_range: 0..buffer_rows,
            line_backgrounds,
        }
    } else {
        Element::buffer_ref(0..buffer_rows)
    };
    let buffer_row = if left_elements.is_empty() && right_elements.is_empty() {
        let transformed =
            registry.apply_transform_chain(TransformTarget::Buffer, || buffer_element, state);
        FlexChild::flexible(transformed, 1.0)
    } else {
        let transformed =
            registry.apply_transform_chain(TransformTarget::Buffer, || buffer_element, state);
        let mut row_children = Vec::new();
        for el in left_elements {
            row_children.push(FlexChild::fixed(el));
        }
        row_children.push(FlexChild::flexible(transformed, 1.0));
        for el in right_elements {
            row_children.push(FlexChild::fixed(el));
        }
        FlexChild::flexible(Element::row(row_children), 1.0)
    };

    // Build status bar (with transform chain — replaces get_replacement + apply_decorator)
    let status_left_elements: Vec<Element> = status_left.into_iter().map(|c| c.element).collect();
    let status_right_elements: Vec<Element> = status_right.into_iter().map(|c| c.element).collect();
    let status_bar = registry.apply_transform_chain(
        TransformTarget::StatusBar,
        || build_status_bar(state, status_left_elements, status_right_elements),
        state,
    );

    // Build main column (status bar position: top or bottom)
    let mut column_children = Vec::new();
    if state.status_at_top {
        column_children.push(FlexChild::fixed(status_bar));
        for c in above_status {
            column_children.push(contribution_to_flex_child(c));
        }
        for c in above_buffer {
            column_children.push(contribution_to_flex_child(c));
        }
        column_children.push(buffer_row);
        for c in below_buffer {
            column_children.push(contribution_to_flex_child(c));
        }
    } else {
        for c in above_buffer {
            column_children.push(contribution_to_flex_child(c));
        }
        column_children.push(buffer_row);
        for c in below_buffer {
            column_children.push(contribution_to_flex_child(c));
        }
        for c in above_status {
            column_children.push(contribution_to_flex_child(c));
        }
        column_children.push(FlexChild::fixed(status_bar));
    }

    Element::column(column_children)
}

/// Convert a Contribution to a FlexChild using its size hint.
fn contribution_to_flex_child(c: Contribution) -> FlexChild {
    match c.size_hint {
        ContribSizeHint::Auto => FlexChild::fixed(c.element),
        ContribSizeHint::Fixed(n) => FlexChild {
            element: c.element,
            flex: 0.0,
            min_size: Some(n),
            max_size: Some(n),
        },
        ContribSizeHint::Flex(f) => FlexChild::flexible(c.element, f),
    }
}

/// Build the menu overlay section.
#[crate::kasane_component(deps(MENU_STRUCTURE, MENU_SELECTION))]
fn build_menu_section(state: &AppState, registry: &PluginRegistry) -> Option<Overlay> {
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
#[crate::kasane_component(deps(INFO), stable(cursor_pos))]
fn build_info_section(state: &AppState, registry: &PluginRegistry) -> Vec<Overlay> {
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

#[crate::kasane_component(deps(STATUS))]
fn build_status_bar(
    state: &AppState,
    status_left: Vec<Element>,
    status_right: Vec<Element>,
) -> Element {
    let status_line =
        build_styled_line_with_base(&state.status_line, &state.status_default_face, 0);
    let mode_line =
        build_styled_line_with_base(&state.status_mode_line, &state.status_default_face, 0);
    let mode_width = line_display_width(&state.status_mode_line) as u16;

    // Status bar: [...status_left, status_line(flex:1.0), ...status_right, mode_line(fixed)]
    let mut children = Vec::new();
    for el in status_left {
        children.push(FlexChild::fixed(el));
    }
    children.push(FlexChild::flexible(status_line, 1.0));
    for el in status_right {
        children.push(FlexChild::fixed(el));
    }
    if mode_width > 0 {
        children.push(FlexChild::fixed(mode_line));
    }

    Element::container(
        Element::row(children),
        Style::from(state.status_default_face),
    )
}

/// Build a complete status bar element, collecting slots and applying replacement/decorator.
///
/// Used by StatusBarSurface to produce the status bar element tree.
pub(crate) fn build_status_bar_surface(state: &AppState, registry: &PluginRegistry) -> Element {
    let ctx = ContributeContext::new(state, None);
    let above_status = registry.collect_contributions(&SlotId::ABOVE_STATUS, state, &ctx);
    let status_left: Vec<Element> = registry
        .collect_contributions(&SlotId::STATUS_LEFT, state, &ctx)
        .into_iter()
        .map(|c| c.element)
        .collect();
    let status_right: Vec<Element> = registry
        .collect_contributions(&SlotId::STATUS_RIGHT, state, &ctx)
        .into_iter()
        .map(|c| c.element)
        .collect();

    let status_bar = registry.apply_transform_chain(
        TransformTarget::StatusBar,
        || build_status_bar(state, status_left, status_right),
        state,
    );

    if above_status.is_empty() {
        status_bar
    } else {
        let mut children = Vec::new();
        for c in above_status {
            children.push(contribution_to_flex_child(c));
        }
        children.push(FlexChild::fixed(status_bar));
        Element::column(children)
    }
}

/// Build the buffer content element (without status bar or overlays).
///
/// Used by KakouneBufferSurface to produce the buffer element tree.
pub(crate) fn build_buffer_content(state: &AppState, registry: &PluginRegistry) -> Element {
    let buffer_rows = state.available_height() as usize;
    let ctx = ContributeContext::new(state, None);

    let above_buffer = registry.collect_contributions(&SlotId::ABOVE_BUFFER, state, &ctx);
    let below_buffer = registry.collect_contributions(&SlotId::BELOW_BUFFER, state, &ctx);
    let buffer_left = registry.collect_contributions(&SlotId::BUFFER_LEFT, state, &ctx);
    let buffer_right = registry.collect_contributions(&SlotId::BUFFER_RIGHT, state, &ctx);

    // Collect line annotations (gutter + backgrounds)
    let annotate_ctx = AnnotateContext {
        line_width: state.cols,
        gutter_width: 0,
    };
    let annotations = registry.collect_annotations(state, &annotate_ctx);
    let mut left_elements: Vec<Element> = Vec::new();
    let mut right_elements: Vec<Element> = Vec::new();
    if let Some(gutter) = annotations.left_gutter {
        left_elements.push(gutter);
    }
    for c in buffer_left {
        left_elements.push(c.element);
    }
    for c in buffer_right {
        right_elements.push(c.element);
    }
    if let Some(gutter) = annotations.right_gutter {
        right_elements.push(gutter);
    }

    // Collect line backgrounds and build BufferRef
    let line_backgrounds = annotations.line_backgrounds;
    let buffer_element = if line_backgrounds.is_some() {
        Element::BufferRef {
            line_range: 0..buffer_rows,
            line_backgrounds,
        }
    } else {
        Element::buffer_ref(0..buffer_rows)
    };
    let buffer_row = if left_elements.is_empty() && right_elements.is_empty() {
        let transformed =
            registry.apply_transform_chain(TransformTarget::Buffer, || buffer_element, state);
        FlexChild::flexible(transformed, 1.0)
    } else {
        let transformed =
            registry.apply_transform_chain(TransformTarget::Buffer, || buffer_element, state);
        let mut row_children = Vec::new();
        for el in left_elements {
            row_children.push(FlexChild::fixed(el));
        }
        row_children.push(FlexChild::flexible(transformed, 1.0));
        for el in right_elements {
            row_children.push(FlexChild::fixed(el));
        }
        FlexChild::flexible(Element::row(row_children), 1.0)
    };

    // Build column (above + buffer + below)
    let mut column_children = Vec::new();
    for c in above_buffer {
        column_children.push(contribution_to_flex_child(c));
    }
    column_children.push(buffer_row);
    for c in below_buffer {
        column_children.push(contribution_to_flex_child(c));
    }

    Element::column(column_children)
}

/// Build the menu overlay section (non-cached, for Surface pipeline).
pub(crate) fn build_menu_section_standalone(
    state: &AppState,
    registry: &PluginRegistry,
) -> Option<Overlay> {
    build_menu_section(state, registry)
}

/// Build the info overlay section (non-cached, for Surface pipeline).
pub(crate) fn build_info_section_standalone(
    state: &AppState,
    registry: &PluginRegistry,
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
