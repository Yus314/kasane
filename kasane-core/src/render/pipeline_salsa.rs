//! SalsaViewSource: ViewSource implementation backed by Salsa tracked functions.
//!
//! Uses Salsa-memoized pure element generation (Stage 1) combined with
//! plugin contributions read from Salsa inputs (Stage 2) and imperative
//! transform application (Stage 3).
//!
//! The flow per section:
//! 1. Salsa tracked function produces the core element (auto-memoized)
//! 2. Plugin contributions (slots, annotations, overlays) read from Salsa inputs
//! 3. Plugin transforms are applied on top (using PluginRegistry)
//!
//! Salsa handles memoization of pure elements,
//! and `sync_plugin_contributions()` pre-computes plugin contributions
//! into Salsa inputs each frame.

use super::RenderResult;
use super::grid::CellGrid;
use super::pipeline::{ViewSource, render_cached_core, scene_render_core};
use super::scene::{self, DrawCommand, SceneCache};
use super::view;
use crate::element::{Element, FlexChild, Style};
use crate::plugin::{PaintHook, PluginRegistry, TransformTarget};
use crate::protocol::MenuStyle;
use crate::salsa_db::KasaneDatabase;
use crate::salsa_sync::SalsaInputHandles;
use crate::salsa_views;
use crate::state::{AppState, DirtyFlags};

/// ViewSource that uses Salsa tracked functions for core element generation
/// and reads plugin contributions from Salsa inputs.
///
/// Stage 1 (pure, Salsa-memoized): `pure_status_element`, `pure_buffer_element`,
/// `pure_menu_overlay`, `pure_info_overlays`.
///
/// Stage 2 (Salsa inputs): slot contributions, annotations, overlays
/// (set by `sync_plugin_contributions()` each frame).
///
/// Stage 3 (imperative): plugin transforms applied via `PluginRegistry`.
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
    fn prepare(&mut self, _dirty: DirtyFlags, _registry: &PluginRegistry) {
        // No-op: Salsa handles invalidation automatically.
        // Plugin contributions are synced by sync_plugin_contributions() before rendering.
    }

    fn view_sections(&mut self, state: &AppState, registry: &PluginRegistry) -> view::ViewSections {
        crate::perf::perf_span!("salsa_view_sections");

        let db = self.db;
        let h = self.handles;

        // --- Base section (buffer + status + slots + annotations) ---
        let status_el = salsa_views::pure_status_element(db, h.status);
        let buffer_el = salsa_views::pure_buffer_element(db, h.config);
        let display_map_ref = salsa_views::display_map_query(db, h.display_directives);
        let base_el = compose_base_from_salsa(
            buffer_el,
            status_el,
            state,
            registry,
            &display_map_ref,
            db,
            h,
        );

        // --- Menu overlay ---
        let menu_overlay = {
            let pure = salsa_views::pure_menu_overlay(db, h.menu, h.config);
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
                    overlay.element =
                        registry.apply_transform_chain(target, || overlay.element.clone(), state);
                }
                overlay
            })
        };

        // --- Info overlays ---
        let info_overlays = {
            let pure = salsa_views::pure_info_overlays(db, h.info, h.menu, h.buffer, h.config);
            pure.into_iter()
                .map(|mut overlay| {
                    let (inner, interactive_id) = match overlay.element {
                        Element::Interactive { child, id } => (*child, Some(id)),
                        other => (other, None),
                    };

                    let mut el = registry.apply_transform_chain(
                        TransformTarget::Info,
                        || inner.clone(),
                        state,
                    );

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
        };

        // --- Plugin overlays from Salsa input ---
        let plugin_overlays = h.plugin_overlays.overlays(db).clone();

        let display_map = salsa_views::display_map_query(db, h.display_directives);
        view::ViewSections {
            base: base_el,
            menu_overlay,
            info_overlays,
            plugin_overlays,
            surface_reports: vec![],
            display_map,
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
    registry: &PluginRegistry,
    display_map: &crate::display::DisplayMapRef,
    db: &KasaneDatabase,
    handles: &SalsaInputHandles,
) -> Element {
    use std::sync::Arc;

    let buffer_rows = state.available_height() as usize;
    let dm_for_element = if display_map.is_identity() {
        None
    } else {
        Some(Arc::clone(display_map))
    };

    // Read annotations from Salsa input (set by sync_plugin_contributions)
    let line_backgrounds = handles.annotations.line_backgrounds(db).clone();
    let left_gutter = handles.annotations.left_gutter(db).clone();
    let right_gutter = handles.annotations.right_gutter(db).clone();

    // When a non-identity DisplayMap is active, line_range must reflect
    // the display line count (which is fewer than buffer lines after fold).
    let effective_rows = if !display_map.is_identity() {
        display_map.display_line_count().min(buffer_rows)
    } else {
        buffer_rows
    };

    // Incorporate line backgrounds and display_map into buffer element
    let buffer_with_bg = if line_backgrounds.is_some() || dm_for_element.is_some() {
        Element::BufferRef {
            line_range: 0..effective_rows,
            line_backgrounds,
            display_map: dm_for_element,
        }
    } else {
        buffer_el
    };

    // Apply buffer transform chain (imperative)
    let transformed_buffer =
        registry.apply_transform_chain(TransformTarget::Buffer, || buffer_with_bg, state);

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

    // Apply status transform chain (imperative)
    let transformed_status =
        registry.apply_transform_chain(TransformTarget::StatusBar, || status_el, state);

    // Read status slot contributions from Salsa input
    let status_left = handles.slot_contributions.status_left(db).clone();
    let status_right = handles.slot_contributions.status_right(db).clone();
    let above_status = handles.slot_contributions.above_status(db).clone();

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
pub fn render_pipeline_cached(
    db: &KasaneDatabase,
    handles: &SalsaInputHandles,
    state: &AppState,
    registry: &PluginRegistry,
    grid: &mut CellGrid,
    dirty: DirtyFlags,
    paint_hooks: &[Box<dyn PaintHook>],
) -> RenderResult {
    let mut source = SalsaViewSource::new(db, handles);
    render_cached_core(&mut source, state, registry, grid, dirty, paint_hooks)
}

/// Salsa-backed scene rendering pipeline (GPU).
#[allow(clippy::too_many_arguments)]
pub fn scene_render_pipeline_cached<'a>(
    db: &KasaneDatabase,
    handles: &SalsaInputHandles,
    state: &AppState,
    registry: &PluginRegistry,
    cell_size: scene::CellSize,
    dirty: DirtyFlags,
    scene_cache: &'a mut SceneCache,
) -> (&'a [DrawCommand], RenderResult) {
    let mut source = SalsaViewSource::new(db, handles);
    scene_render_core(&mut source, state, registry, cell_size, dirty, scene_cache)
}
