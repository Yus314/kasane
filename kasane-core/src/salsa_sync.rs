//! Projection from `AppState` to Salsa inputs (Layer 1 → Layer 2 boundary).
//!
//! `sync_inputs_from_state()` is called once per frame, after all events
//! in the batch have been processed. It unconditionally sets all Salsa inputs
//! from the current AppState. Salsa's `set_*().to()` uses PartialEq to
//! detect unchanged values and skip downstream revalidation automatically.

use salsa::{Durability, Setter};

use crate::plugin::{AppView, ContributionCache, PluginView};
use crate::salsa_db::KasaneDatabase;
use crate::salsa_inputs::*;
use crate::state::AppState;
use crate::state::snapshot::{InfoSnapshot, MenuSnapshot};

/// Handles to Salsa input instances, created once at startup and reused across frames.
pub struct SalsaInputHandles {
    pub buffer: BufferInput,
    pub cursor: CursorInput,
    pub status: StatusInput,
    pub menu: MenuInput,
    pub info: InfoInput,
    pub config: ConfigInput,
    pub display_directives: DisplayDirectivesInput,
    pub slot_contributions: SlotContributionsInput,
    pub annotations: AnnotationResultInput,
    pub plugin_overlays: PluginOverlaysInput,
}

impl SalsaInputHandles {
    /// Create all Salsa input instances with default values.
    pub fn new(db: &mut KasaneDatabase) -> Self {
        Self {
            buffer: BufferInput::new(
                db,
                vec![],
                crate::protocol::Face::default(),
                crate::protocol::Face::default(),
                crate::protocol::Coord::default(),
                0,
            ),
            cursor: CursorInput::new(db, crate::protocol::CursorMode::Buffer, 0, vec![]),
            status: StatusInput::new(
                db,
                vec![],
                vec![],
                crate::protocol::Face::default(),
                crate::protocol::StatusStyle::default(),
            ),
            menu: MenuInput::new(db, None),
            info: InfoInput::new(db, vec![]),
            config: ConfigInput::new(
                db,
                80,
                24,
                true,
                true,
                false,
                0.4,
                crate::config::MenuPosition::Auto,
                false,
                "█".to_string(),
                "░".to_string(),
                None,
            ),
            display_directives: DisplayDirectivesInput::new(db, vec![], 0),
            slot_contributions: SlotContributionsInput::new(
                db,
                vec![],
                vec![],
                vec![],
                vec![],
                vec![],
                vec![],
                vec![],
            ),
            annotations: AnnotationResultInput::new(db, None, None, None, None, None),
            plugin_overlays: PluginOverlaysInput::new(db, vec![]),
        }
    }
}

/// Project all `AppState` fields onto Salsa inputs.
///
/// Salsa's `set_*().to()` compares via PartialEq and only bumps the
/// revision when the value actually changed, so unconditional sync
/// is safe and avoids manual DirtyFlags tracking.
pub fn sync_inputs_from_state(
    db: &mut KasaneDatabase,
    state: &AppState,
    inputs: &SalsaInputHandles,
) {
    // Buffer content
    inputs.buffer.set_lines(db).to(state.lines.clone());
    inputs.buffer.set_default_face(db).to(state.default_face);
    inputs.buffer.set_padding_face(db).to(state.padding_face);
    inputs
        .buffer
        .set_widget_columns(db)
        .to(state.widget_columns);

    // Cursor
    inputs.buffer.set_cursor_pos(db).to(state.cursor_pos);
    inputs.cursor.set_cursor_mode(db).to(state.cursor_mode);
    inputs.cursor.set_cursor_count(db).to(state.cursor_count);
    inputs
        .cursor
        .set_secondary_cursors(db)
        .to(state.secondary_cursors.clone());

    // Status
    inputs
        .status
        .set_status_line(db)
        .to(state.status_line.clone());
    inputs
        .status
        .set_status_mode_line(db)
        .to(state.status_mode_line.clone());
    inputs
        .status
        .set_status_default_face(db)
        .to(state.status_default_face);
    inputs.status.set_status_style(db).to(state.status_style);

    // Menu
    let snapshot = state.menu.as_ref().map(MenuSnapshot::from_menu_state);
    inputs.menu.set_menu(db).to(snapshot);

    // Info
    let snapshots: Vec<_> = state
        .infos
        .iter()
        .map(InfoSnapshot::from_info_state)
        .collect();
    inputs.info.set_infos(db).to(snapshots);

    // Config / options
    inputs
        .config
        .set_cols(db)
        .with_durability(Durability::HIGH)
        .to(state.cols);
    inputs
        .config
        .set_rows(db)
        .with_durability(Durability::HIGH)
        .to(state.rows);
    inputs.config.set_focused(db).to(state.focused);
    inputs
        .config
        .set_shadow_enabled(db)
        .with_durability(Durability::HIGH)
        .to(state.shadow_enabled);
    inputs
        .config
        .set_status_at_top(db)
        .with_durability(Durability::HIGH)
        .to(state.status_at_top);
    inputs
        .config
        .set_secondary_blend_ratio(db)
        .with_durability(Durability::HIGH)
        .to(state.secondary_blend_ratio);
    inputs
        .config
        .set_menu_position(db)
        .with_durability(Durability::HIGH)
        .to(state.menu_position);
    inputs
        .config
        .set_search_dropdown(db)
        .with_durability(Durability::HIGH)
        .to(state.search_dropdown);
    inputs
        .config
        .set_scrollbar_thumb(db)
        .with_durability(Durability::HIGH)
        .to(state.scrollbar_thumb.clone());
    inputs
        .config
        .set_scrollbar_track(db)
        .with_durability(Durability::HIGH)
        .to(state.scrollbar_track.clone());
    inputs
        .config
        .set_assistant_art(db)
        .with_durability(Durability::HIGH)
        .to(state.assistant_art.clone());
}

/// Synchronize plugin contributions (slots, annotations, overlays) into Salsa inputs.
///
/// Call this after `prepare_plugin_cache()` and `sync_display_directives()`.
/// Uses per-extension-point granularity: only re-collects extension points
/// where at least one participating plugin needs recollection. Uses `cache`
/// for per-plugin contribution caching across frames.
pub fn sync_plugin_contributions(
    db: &mut KasaneDatabase,
    state: &AppState,
    registry: &PluginView<'_>,
    inputs: &SalsaInputHandles,
    cache: &mut ContributionCache,
) {
    use crate::display::DisplayMapRef;
    use crate::element::Overlay;
    use crate::plugin::{
        AnnotateContext, ContribSizeHint, ContributeContext, Contribution, OverlayContext, SlotId,
    };
    use std::sync::Arc;

    fn contribution_to_flex_child(c: Contribution) -> crate::element::FlexChild {
        match c.size_hint {
            ContribSizeHint::Auto => crate::element::FlexChild::fixed(c.element),
            ContribSizeHint::Fixed(n) => crate::element::FlexChild {
                element: c.element,
                flex: 0.0,
                min_size: Some(n),
                max_size: Some(n),
            },
            ContribSizeHint::Flex(flex) => crate::element::FlexChild::flexible(c.element, flex),
        }
    }

    fn collect_slot_cached(
        slot: &SlotId,
        state: &AppState,
        registry: &PluginView<'_>,
        ctx: &ContributeContext,
        cache: &mut ContributionCache,
    ) -> Vec<crate::element::FlexChild> {
        registry
            .collect_contributions_cached(slot, &AppView::new(state), ctx, cache)
            .into_iter()
            .map(contribution_to_flex_child)
            .collect()
    }

    // Slot contributions: only re-collect if any contributor is stale
    if registry.any_contributor_needs_recollect() {
        let view = AppView::new(state);
        let ctx = ContributeContext::new(&view, None);
        inputs
            .slot_contributions
            .set_buffer_left(db)
            .to(collect_slot_cached(
                &SlotId::BUFFER_LEFT,
                state,
                registry,
                &ctx,
                cache,
            ));
        inputs
            .slot_contributions
            .set_buffer_right(db)
            .to(collect_slot_cached(
                &SlotId::BUFFER_RIGHT,
                state,
                registry,
                &ctx,
                cache,
            ));
        inputs
            .slot_contributions
            .set_above_buffer(db)
            .to(collect_slot_cached(
                &SlotId::ABOVE_BUFFER,
                state,
                registry,
                &ctx,
                cache,
            ));
        inputs
            .slot_contributions
            .set_below_buffer(db)
            .to(collect_slot_cached(
                &SlotId::BELOW_BUFFER,
                state,
                registry,
                &ctx,
                cache,
            ));
        inputs
            .slot_contributions
            .set_status_left(db)
            .to(collect_slot_cached(
                &SlotId::STATUS_LEFT,
                state,
                registry,
                &ctx,
                cache,
            ));
        inputs
            .slot_contributions
            .set_status_right(db)
            .to(collect_slot_cached(
                &SlotId::STATUS_RIGHT,
                state,
                registry,
                &ctx,
                cache,
            ));
        inputs
            .slot_contributions
            .set_above_status(db)
            .to(collect_slot_cached(
                &SlotId::ABOVE_STATUS,
                state,
                registry,
                &ctx,
                cache,
            ));
    }

    // Annotations: only re-collect if any annotator is stale
    if registry.any_annotator_needs_recollect() {
        let display_map: DisplayMapRef =
            crate::salsa_views::display_map_query(db, inputs.display_directives);
        let annotate_ctx = AnnotateContext {
            line_width: state.cols,
            gutter_width: 0,
            display_map: Some(Arc::clone(&display_map)),
            pane_surface_id: None,
            pane_focused: true,
        };
        let result = registry.collect_annotations(&AppView::new(state), &annotate_ctx);
        inputs
            .annotations
            .set_line_backgrounds(db)
            .to(result.line_backgrounds);
        inputs
            .annotations
            .set_left_gutter(db)
            .to(result.left_gutter);
        inputs
            .annotations
            .set_right_gutter(db)
            .to(result.right_gutter);
        inputs
            .annotations
            .set_inline_decorations(db)
            .to(result.inline_decorations);
        inputs
            .annotations
            .set_virtual_text(db)
            .to(result.virtual_text);
    }

    // Plugin overlays: only re-collect if any overlay provider is stale
    if registry.any_overlay_needs_recollect() {
        let overlay_ctx = OverlayContext {
            screen_cols: state.cols,
            screen_rows: state.rows,
            menu_rect: None,
            existing_overlays: vec![],
            focused_surface_id: None,
        };
        let overlays: Vec<Overlay> = registry
            .collect_overlays_with_ctx(&AppView::new(state), &overlay_ctx)
            .into_iter()
            .map(|oc| Overlay {
                element: oc.element,
                anchor: oc.anchor,
            })
            .collect();
        inputs.plugin_overlays.set_overlays(db).to(overlays);
    }
}

/// Synchronize display directives from plugins into Salsa.
///
/// Call this after `prepare_plugin_cache()`.
/// Only re-collects if any DISPLAY_TRANSFORM plugin needs recollection.
pub fn sync_display_directives(
    db: &mut KasaneDatabase,
    state: &AppState,
    registry: &PluginView<'_>,
    inputs: &SalsaInputHandles,
) {
    if !registry.any_display_transform_needs_recollect() {
        return;
    }

    let directives = registry.collect_display_directives(&AppView::new(state));
    let line_count = state.visible_line_range().len();

    inputs.display_directives.set_directives(db).to(directives);
    inputs
        .display_directives
        .set_buffer_line_count(db)
        .to(line_count);
}
