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

use super::RenderResult;
use super::grid::CellGrid;
use super::pipeline::{ViewSource, render_cached_core, scene_render_core};
use super::scene::{self, DrawCommand, SceneCache};
use super::view;
use crate::element::{Element, FlexChild, Style};
use crate::layout::Rect;
use crate::plugin::{AppView, PaintHook, PluginView, TransformSubject, TransformTarget};
use crate::protocol::MenuStyle;
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

impl ViewSource for SalsaViewSource<'_> {
    fn prepare(&mut self, _dirty: DirtyFlags, _registry: &PluginView<'_>) {
        // No-op: Salsa handles invalidation automatically.
        // Plugin contributions are synced by sync_plugin_contributions() before rendering.
    }

    fn view_sections(&mut self, state: &AppState, registry: &PluginView<'_>) -> view::ViewSections {
        crate::perf::perf_span!("salsa_view_sections");

        let db = self.db;
        let h = self.handles;

        let is_multi_pane = self.surface_registry.is_some_and(|sr| sr.is_multi_pane());

        // --- Base section (buffer + status + slots + annotations) ---
        let (base_el, surface_reports, focused_pane_rect, focused_pane_state) = if is_multi_pane {
            let sr = self.surface_registry.unwrap();
            let total = Rect {
                x: 0,
                y: 0,
                w: state.cols,
                h: state.rows,
            };
            let result = sr.compose_base_result(state, self.pane_states, registry, total);
            let focused = sr.workspace().focused();
            let focused_rect = sr.workspace().compute_rects(total).get(&focused).copied();
            let focused_state = self
                .pane_states
                .and_then(|ps| ps.state_for_surface(focused))
                .map(|s| Box::new(s.clone()));
            (
                result.base.unwrap_or(Element::Empty),
                result.surface_reports,
                focused_rect,
                focused_state,
            )
        } else {
            let status_el = salsa_views::pure_status_element(db, h.status);
            let buffer_el = salsa_views::pure_buffer_element(db, h.config);
            let display_map_ref = salsa_views::display_map_query(db, h.display_directives);
            let base = compose_base_from_salsa(
                buffer_el,
                status_el,
                state,
                registry,
                &display_map_ref,
                db,
                h,
            );
            (base, vec![], None, None)
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
                s.cols = fr.w;
                s.rows = fr.h;
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
        } else {
            let pure = salsa_views::pure_menu_overlay(db, h.menu, h.config);
            pure.map(|overlay| {
                let menu_state = state.menu.as_ref();
                let target = menu_state
                    .map(|m| match m.style {
                        MenuStyle::Prompt => TransformTarget::MenuPrompt,
                        MenuStyle::Inline => TransformTarget::MenuInline,
                        MenuStyle::Search => TransformTarget::MenuSearch,
                    })
                    .unwrap_or(TransformTarget::Menu);

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
            pure.into_iter()
                .map(|overlay| {
                    registry
                        .apply_transform_chain_hierarchical(
                            TransformTarget::Info,
                            TransformSubject::Overlay(overlay),
                            &AppView::new(state),
                        )
                        .into_overlay()
                        .expect("overlay transform preserves variant")
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
            surface_reports,
            display_map,
            focused_pane_rect,
            focused_pane_state,
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
    let inline_decorations = handles.annotations.inline_decorations(db).clone();

    // When a non-identity DisplayMap is active, line_range must reflect
    // the display line count (which is fewer than buffer lines after fold).
    let effective_rows = if !display_map.is_identity() {
        display_map.display_line_count().min(buffer_rows)
    } else {
        buffer_rows
    };

    // Incorporate line backgrounds, display_map, and inline decorations into buffer element
    let buffer_with_bg =
        if line_backgrounds.is_some() || dm_for_element.is_some() || inline_decorations.is_some() {
            Element::BufferRef {
                line_range: 0..effective_rows,
                line_backgrounds,
                display_map: dm_for_element,
                state: None,
                inline_decorations,
            }
        } else {
            buffer_el
        };

    // Apply buffer transform chain (imperative)
    let transformed_buffer = registry
        .apply_transform_chain(
            TransformTarget::Buffer,
            TransformSubject::Element(buffer_with_bg),
            &AppView::new(state),
        )
        .into_element();

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
    let transformed_status = registry
        .apply_transform_chain(
            TransformTarget::StatusBar,
            TransformSubject::Element(status_el),
            &AppView::new(state),
        )
        .into_element();

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
    registry: &PluginView<'_>,
    grid: &mut CellGrid,
    dirty: DirtyFlags,
    paint_hooks: &[Box<dyn PaintHook>],
    surface_registry: Option<&SurfaceRegistry>,
    pane_states: Option<&PaneStates<'_>>,
) -> RenderResult {
    let mut source = SalsaViewSource::new(db, handles, surface_registry, pane_states);
    render_cached_core(&mut source, state, registry, grid, dirty, paint_hooks)
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
    surface_registry: Option<&SurfaceRegistry>,
    pane_states: Option<&PaneStates<'_>>,
) -> (&'a [DrawCommand], RenderResult) {
    let mut source = SalsaViewSource::new(db, handles, surface_registry, pane_states);
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
