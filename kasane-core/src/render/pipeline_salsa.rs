//! SalsaViewSource: ViewSource implementation backed by Salsa tracked functions.
//!
//! Uses Salsa-memoized pure element generation (Stage 1) combined with
//! imperative plugin contribution/transform application (Stage 2).
//!
//! The flow per section:
//! 1. Salsa tracked function produces the core element (auto-memoized)
//! 2. Plugin transforms are applied on top (using PluginRegistry)
//! 3. ViewCache caches the combined result
//!
//! This is a parallel path to PluginViewSource/SurfaceViewSource, enabled
//! via the `salsa-view` feature flag.

use super::cache::{LayoutCache, ViewCache, cache_dirty_snapshot};
use super::grid::CellGrid;
use super::pipeline::{
    ViewSource, render_cached_core, render_patched_core, render_sectioned_core, scene_render_core,
};
use super::scene::{self, DrawCommand, SceneCache};
use super::view::{self, BUILD_INFO_SECTION_DEPS, BUILD_MENU_SECTION_DEPS};
use super::{RenderResult, patch};
use crate::element::{Element, FlexChild, Overlay, Style};
use crate::plugin::{
    AnnotateContext, ContribSizeHint, ContributeContext, Contribution, PaintHook, PluginRegistry,
    SlotId, TransformTarget,
};
use crate::protocol::MenuStyle;
use crate::salsa_db::KasaneDatabase;
use crate::salsa_sync::SalsaInputHandles;
use crate::salsa_views;
use crate::state::{AppState, DirtyFlags};
use crate::surface::SurfaceComposeResult;

/// ViewSource that uses Salsa tracked functions for core element generation.
///
/// Stage 1 (pure, Salsa-memoized): `pure_status_element`, `pure_buffer_element`,
/// `pure_menu_overlay`, `pure_info_overlays`.
///
/// Stage 2 (imperative): plugin slot fills, annotations, transforms, overlays.
pub(crate) struct SalsaViewSource<'a> {
    db: &'a KasaneDatabase,
    handles: &'a SalsaInputHandles,
}

impl<'a> SalsaViewSource<'a> {
    pub(crate) fn new(db: &'a KasaneDatabase, handles: &'a SalsaInputHandles) -> Self {
        Self { db, handles }
    }
}

impl ViewSource for SalsaViewSource<'_> {
    fn invalidate_view_cache(
        &self,
        dirty: DirtyFlags,
        registry: &PluginRegistry,
        cache: &mut ViewCache,
    ) {
        // ViewCache still caches the combined (pure + plugins) result.
        // Invalidation follows existing DirtyFlags + plugin deps logic.
        cache.invalidate_with_deps(dirty, registry.section_deps());
    }

    fn view_sections(
        &self,
        state: &AppState,
        registry: &PluginRegistry,
        cache: &mut ViewCache,
    ) -> view::ViewSections {
        crate::perf::perf_span!("salsa_view_sections");

        let db = self.db;
        let h = self.handles;

        // --- Base section (buffer + status) ---
        // DirtyFlags for base: BUFFER_CONTENT | STATUS | OPTIONS
        let base_deps = view::BUILD_BASE_DEPS;
        let base = cache.base.get_or_insert(
            cache_dirty_snapshot(&cache.base, base_deps),
            base_deps,
            || {
                // Stage 1: get pure elements from Salsa
                let status_el = salsa_views::pure_status_element(db, h.status);
                let buffer_el = salsa_views::pure_buffer_element(db, h.config);

                // Stage 2: apply plugin contributions
                let base_el = compose_base_with_plugins(buffer_el, status_el, state, registry);

                SurfaceComposeResult {
                    base: Some(base_el),
                    surface_reports: vec![],
                }
            },
        );

        // --- Menu overlay ---
        let menu_overlay = cache.menu_overlay.get_or_insert(
            cache_dirty_snapshot(&cache.menu_overlay, BUILD_MENU_SECTION_DEPS),
            BUILD_MENU_SECTION_DEPS,
            || {
                // Stage 1: get pure menu from Salsa
                let pure = salsa_views::pure_menu_overlay(db, h.menu, h.config);

                // Stage 2: apply plugin transforms
                pure.map(|mut overlay| {
                    let menu_state = state.menu.as_ref();
                    let transform_target = menu_state.map(|m| match m.style {
                        MenuStyle::Prompt => TransformTarget::MenuPrompt,
                        MenuStyle::Inline => TransformTarget::MenuInline,
                        MenuStyle::Search => TransformTarget::MenuSearch,
                    });

                    overlay.element = registry.apply_transform_chain(
                        TransformTarget::Menu,
                        || overlay.element.clone(),
                        state,
                    );
                    if let Some(target) = transform_target {
                        overlay.element = registry.apply_transform_chain(
                            target,
                            || overlay.element.clone(),
                            state,
                        );
                    }
                    overlay
                })
            },
        );

        // --- Info overlays ---
        let info_overlays = cache.info_overlays.get_or_insert(
            cache_dirty_snapshot(&cache.info_overlays, BUILD_INFO_SECTION_DEPS),
            BUILD_INFO_SECTION_DEPS,
            || {
                // Stage 1: get pure info overlays from Salsa
                let pure = salsa_views::pure_info_overlays(db, h.info, h.buffer, h.config);

                // Stage 2: apply plugin transforms to each overlay
                pure.into_iter()
                    .map(|mut overlay| {
                        // Unwrap Interactive to get the inner element for transforms
                        let (inner, interactive_id) = match overlay.element {
                            Element::Interactive { child, id } => (*child, Some(id)),
                            other => (other, None),
                        };

                        let mut el = registry.apply_transform_chain(
                            TransformTarget::Info,
                            || inner.clone(),
                            state,
                        );

                        // Re-wrap with Interactive if it was present
                        if let Some(id) = interactive_id {
                            el = Element::Interactive {
                                child: Box::new(el),
                                id,
                            };
                        }

                        overlay.element = el;
                        overlay
                    })
                    .collect()
            },
        );

        // --- Plugin overlays (fully imperative, no Salsa involvement) ---
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

        view::ViewSections {
            base: base.base.unwrap_or(Element::Empty),
            menu_overlay,
            info_overlays,
            plugin_overlays,
            surface_reports: base.surface_reports,
        }
    }
}

/// Convert a plugin `Contribution` to a `FlexChild` based on its size hint.
fn contribution_to_flex_child(c: Contribution) -> FlexChild {
    match c.size_hint {
        ContribSizeHint::Auto => FlexChild::fixed(c.element),
        ContribSizeHint::Fixed(n) => FlexChild {
            element: c.element,
            flex: 0.0,
            min_size: Some(n),
            max_size: Some(n),
        },
        ContribSizeHint::Flex(flex) => FlexChild::flexible(c.element, flex),
    }
}

/// Collect slot contributions and convert to FlexChildren.
fn collect_slot_children(
    slot: &SlotId,
    state: &AppState,
    registry: &PluginRegistry,
    ctx: &ContributeContext,
) -> Vec<FlexChild> {
    registry
        .collect_contributions(slot, state, ctx)
        .into_iter()
        .map(contribution_to_flex_child)
        .collect()
}

/// Compose buffer + status elements into the base Element tree, applying
/// plugin contributions (annotations, transforms, slot fills).
fn compose_base_with_plugins(
    buffer_el: Element,
    status_el: Element,
    state: &AppState,
    registry: &PluginRegistry,
) -> Element {
    let ctx = ContributeContext::new(state, None);

    // Apply plugin annotations (line backgrounds, gutters)
    let buffer_rows = state.available_height() as usize;
    let annotate_ctx = AnnotateContext {
        line_width: state.cols,
        gutter_width: 0,
    };
    let annotations = registry.collect_annotations(state, &annotate_ctx);

    // Incorporate line backgrounds into buffer element
    let buffer_with_bg = if annotations.line_backgrounds.is_some() {
        Element::BufferRef {
            line_range: 0..buffer_rows,
            line_backgrounds: annotations.line_backgrounds,
        }
    } else {
        buffer_el
    };

    // Apply buffer transform chain
    let transformed_buffer =
        registry.apply_transform_chain(TransformTarget::Buffer, || buffer_with_bg, state);

    // Collect buffer slot contributions
    let buffer_left = collect_slot_children(&SlotId::BUFFER_LEFT, state, registry, &ctx);
    let buffer_right = collect_slot_children(&SlotId::BUFFER_RIGHT, state, registry, &ctx);
    let above_buffer = collect_slot_children(&SlotId::ABOVE_BUFFER, state, registry, &ctx);
    let below_buffer = collect_slot_children(&SlotId::BELOW_BUFFER, state, registry, &ctx);

    // Build buffer row: [left_gutter] [slot:left] [buffer] [slot:right] [right_gutter]
    let mut row_children = Vec::new();
    if let Some(left_gutter) = annotations.left_gutter {
        row_children.push(FlexChild::fixed(left_gutter));
    }
    row_children.extend(buffer_left);
    row_children.push(FlexChild::flexible(transformed_buffer, 1.0));
    row_children.extend(buffer_right);
    if let Some(right_gutter) = annotations.right_gutter {
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

    // Apply status transform chain
    let transformed_status =
        registry.apply_transform_chain(TransformTarget::StatusBar, || status_el, state);

    // Collect status slot contributions
    let status_left = collect_slot_children(&SlotId::STATUS_LEFT, state, registry, &ctx);
    let status_right = collect_slot_children(&SlotId::STATUS_RIGHT, state, registry, &ctx);
    let above_status = collect_slot_children(&SlotId::ABOVE_STATUS, state, registry, &ctx);

    // Build status row: [slot:left] [status_core] [slot:right]
    let status_inner = if status_left.is_empty() && status_right.is_empty() {
        transformed_status
    } else {
        let mut children = Vec::new();
        children.extend(status_left);
        children.push(FlexChild::flexible(transformed_status, 1.0));
        children.extend(status_right);
        Element::row(children)
    };

    let status_styled = Element::container(status_inner, Style::from(state.status_default_face));

    // Wrap with above_status if present
    let status_section = if above_status.is_empty() {
        status_styled
    } else {
        let mut children = Vec::new();
        children.extend(above_status);
        children.push(FlexChild::fixed(status_styled));
        Element::column(children)
    };

    // Compose buffer + status based on status_at_top config
    if state.status_at_top {
        Element::column(vec![
            FlexChild::fixed(status_section),
            FlexChild::flexible(buffer_section, 1.0),
        ])
    } else {
        Element::column(vec![
            FlexChild::flexible(buffer_section, 1.0),
            FlexChild::fixed(status_section),
        ])
    }
}

// ---------------------------------------------------------------------------
// Public API: Salsa-backed pipeline wrappers
// ---------------------------------------------------------------------------

/// Salsa-backed cached rendering pipeline (TUI).
#[allow(clippy::too_many_arguments)]
pub fn render_pipeline_salsa_cached(
    db: &KasaneDatabase,
    handles: &SalsaInputHandles,
    state: &AppState,
    registry: &PluginRegistry,
    grid: &mut CellGrid,
    dirty: DirtyFlags,
    cache: &mut ViewCache,
    paint_hooks: &[Box<dyn PaintHook>],
) -> RenderResult {
    let source = SalsaViewSource::new(db, handles);
    render_cached_core(&source, state, registry, grid, dirty, cache, paint_hooks)
}

/// Salsa-backed section-aware rendering pipeline (TUI).
#[allow(clippy::too_many_arguments)]
pub fn render_pipeline_salsa_sectioned(
    db: &KasaneDatabase,
    handles: &SalsaInputHandles,
    state: &AppState,
    registry: &PluginRegistry,
    grid: &mut CellGrid,
    dirty: DirtyFlags,
    view_cache: &mut ViewCache,
    layout_cache: &mut LayoutCache,
    paint_hooks: &[Box<dyn PaintHook>],
) -> RenderResult {
    let source = SalsaViewSource::new(db, handles);
    render_sectioned_core(
        &source,
        state,
        registry,
        grid,
        dirty,
        view_cache,
        layout_cache,
        paint_hooks,
    )
}

/// Salsa-backed patched rendering pipeline (TUI).
#[allow(clippy::too_many_arguments)]
pub fn render_pipeline_salsa_patched(
    db: &KasaneDatabase,
    handles: &SalsaInputHandles,
    state: &AppState,
    registry: &PluginRegistry,
    grid: &mut CellGrid,
    dirty: DirtyFlags,
    view_cache: &mut ViewCache,
    layout_cache: &mut LayoutCache,
    patches: &[&dyn patch::PaintPatch],
    paint_hooks: &[Box<dyn PaintHook>],
) -> RenderResult {
    let source = SalsaViewSource::new(db, handles);
    render_patched_core(
        &source,
        state,
        registry,
        grid,
        dirty,
        view_cache,
        layout_cache,
        patches,
        paint_hooks,
    )
}

/// Salsa-backed scene rendering pipeline (GPU).
#[allow(clippy::too_many_arguments)]
pub fn scene_render_pipeline_salsa_cached<'a>(
    db: &KasaneDatabase,
    handles: &SalsaInputHandles,
    state: &AppState,
    registry: &PluginRegistry,
    cell_size: scene::CellSize,
    dirty: DirtyFlags,
    view_cache: &mut ViewCache,
    scene_cache: &'a mut SceneCache,
) -> (&'a [DrawCommand], RenderResult) {
    let source = SalsaViewSource::new(db, handles);
    scene_render_core(
        &source,
        state,
        registry,
        cell_size,
        dirty,
        view_cache,
        scene_cache,
    )
}
