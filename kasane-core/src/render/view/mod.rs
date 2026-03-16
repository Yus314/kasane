pub(crate) mod info;
pub(crate) mod menu;
#[cfg(test)]
#[allow(clippy::field_reassign_with_default)]
mod tests;

use crate::element::{Direction, Element, FlexChild, Overlay, OverlayAnchor, Style};
use crate::layout::line_display_width;
use crate::plugin::{AnnotateContext, PluginRegistry, TransformTarget};
#[cfg(test)]
use crate::plugin::{EffectiveSectionDeps, SlotId};
use crate::protocol::{Atom, Face, InfoStyle, Line, MenuStyle};
use crate::render::cache::{ViewCache, cache_dirty_snapshot};
use crate::state::AppState;
use crate::surface::{SurfaceComposeResult, SurfaceRenderReport};

use crate::state::DirtyFlags;

// DirtyFlags dependency masks for each component function.
// These match the deps() annotations on the #[kasane_component] attributes.
pub(crate) const BUILD_BASE_DEPS: DirtyFlags = DirtyFlags::from_bits_truncate(
    DirtyFlags::BUFFER_CONTENT.bits()
        | DirtyFlags::STATUS.bits()
        | DirtyFlags::OPTIONS.bits()
        | DirtyFlags::PLUGIN_STATE.bits(),
);
pub(crate) const BUILD_MENU_SECTION_DEPS: DirtyFlags = DirtyFlags::from_bits_truncate(
    DirtyFlags::MENU_STRUCTURE.bits()
        | DirtyFlags::MENU_SELECTION.bits()
        | DirtyFlags::OPTIONS.bits(),
);
pub(crate) const BUILD_INFO_SECTION_DEPS: DirtyFlags =
    DirtyFlags::from_bits_truncate(DirtyFlags::INFO.bits() | DirtyFlags::OPTIONS.bits());

#[cfg(test)]
pub(crate) fn effective_surface_section_deps(
    cached_base: Option<&SurfaceComposeResult>,
    registry: &PluginRegistry,
    surface_registry: &crate::surface::SurfaceRegistry,
) -> EffectiveSectionDeps {
    let mut deps = *registry.section_deps();
    deps.base = surface_base_deps(cached_base, registry, surface_registry);
    deps
}

#[cfg(test)]
fn surface_base_deps(
    cached_base: Option<&SurfaceComposeResult>,
    registry: &PluginRegistry,
    surface_registry: &crate::surface::SurfaceRegistry,
) -> DirtyFlags {
    let mut base = BUILD_BASE_DEPS;
    let Some(cached_base) = cached_base else {
        return base;
    };

    let mut buffer_active = false;
    let mut status_active = false;

    for report in &cached_base.surface_reports {
        match report.surface_key.as_str() {
            "kasane.buffer" => buffer_active = true,
            "kasane.status" => status_active = true,
            _ => {}
        }

        if report.owner_errors.is_empty() {
            for record in &report.slot_records {
                base |= registry.contribute_deps(&SlotId::new(record.slot_name.clone()));
            }
            continue;
        }

        if let Some(surface_id) = surface_registry.surface_id_by_key(report.surface_key.as_str())
            && let Some(descriptor) = surface_registry.descriptor(surface_id)
        {
            for slot in &descriptor.declared_slots {
                base |= registry.contribute_deps(&SlotId::new(slot.name.clone()));
            }
        } else {
            for record in &report.slot_records {
                base |= registry.contribute_deps(&SlotId::new(record.slot_name.clone()));
            }
        }
    }

    if buffer_active {
        base |= registry.annotate_deps();
        base |= registry.transform_deps(&TransformTarget::Buffer);
    }
    if status_active {
        base |= registry.transform_deps(&TransformTarget::StatusBar);
    }

    base
}

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
    pub surface_reports: Vec<SurfaceRenderReport>,
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
        || legacy_surface_compose_result(state, registry),
    );

    build_sections_with_base(base, state, registry, cache)
}

/// Build the view sections using SurfaceRegistry as the element source.
///
/// Uses the same ViewCache infrastructure as `view_sections_cached`, but the
/// base element comes from `SurfaceRegistry::compose_base_result()` instead of
/// the legacy inline base builder. This enables workspace-aware layouts while
/// preserving all caching optimizations.
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
        || surface_registry.compose_base_result(state, registry, root_area),
    );

    build_sections_with_base(base, state, registry, cache)
}

/// Shared helper: build menu, info, and plugin overlay sections around a given base.
fn build_sections_with_base(
    base_result: SurfaceComposeResult,
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
        base: base_result.base.unwrap_or(Element::Empty),
        menu_overlay,
        info_overlays,
        plugin_overlays,
        surface_reports: base_result.surface_reports,
    }
}

/// Build the full Element tree with subtree memoization via ViewCache.
pub fn view_cached(state: &AppState, registry: &PluginRegistry, cache: &mut ViewCache) -> Element {
    crate::perf::perf_span!("view");
    view_sections_cached(state, registry, cache).into_element()
}

fn legacy_surface_compose_result(
    state: &AppState,
    registry: &PluginRegistry,
) -> SurfaceComposeResult {
    let mut surface_registry = crate::surface::SurfaceRegistry::new();
    surface_registry.register(Box::new(crate::surface::buffer::KakouneBufferSurface::new()));
    surface_registry.register(Box::new(crate::surface::status::StatusBarSurface::new()));
    surface_registry.compose_base_result(
        state,
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
    registry: &PluginRegistry,
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

struct BufferCoreParts {
    left_gutter: Option<Element>,
    buffer: Element,
    right_gutter: Option<Element>,
}

fn build_buffer_core_parts(state: &AppState, registry: &PluginRegistry) -> BufferCoreParts {
    let buffer_rows = state.available_height() as usize;
    let annotate_ctx = AnnotateContext {
        line_width: state.cols,
        gutter_width: 0,
    };
    let annotations = registry.collect_annotations(state, &annotate_ctx);
    let line_backgrounds = annotations.line_backgrounds;
    let buffer_element = if line_backgrounds.is_some() {
        Element::BufferRef {
            line_range: 0..buffer_rows,
            line_backgrounds,
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
    registry: &PluginRegistry,
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
