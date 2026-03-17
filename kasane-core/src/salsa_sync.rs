//! Projection from `AppState` to Salsa inputs (Layer 1 → Layer 2 boundary).
//!
//! `sync_inputs_from_state()` is called once per frame, after all events
//! in the batch have been processed. It selectively sets only the Salsa inputs
//! whose corresponding `DirtyFlags` are set, so Salsa can skip revalidation
//! of unchanged inputs.

use salsa::{Durability, Setter};

use crate::plugin::PluginRegistry;
use crate::salsa_db::KasaneDatabase;
use crate::salsa_inputs::*;
use crate::state::snapshot::{InfoSnapshot, MenuSnapshot};
use crate::state::{AppState, DirtyFlags};

/// Handles to Salsa input instances, created once at startup and reused across frames.
pub struct SalsaInputHandles {
    pub buffer: BufferInput,
    pub cursor: CursorInput,
    pub status: StatusInput,
    pub menu: MenuInput,
    pub info: InfoInput,
    pub config: ConfigInput,
    pub plugin_epoch: PluginEpochInput,
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
            status: StatusInput::new(db, vec![], vec![], crate::protocol::Face::default()),
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
            plugin_epoch: PluginEpochInput::new(db, 0),
            display_directives: DisplayDirectivesInput::new(db, vec![], 0),
            slot_contributions: SlotContributionsInput::new(
                db,
                0,
                vec![],
                vec![],
                vec![],
                vec![],
                vec![],
                vec![],
                vec![],
            ),
            annotations: AnnotationResultInput::new(db, 0, None, None, None),
            plugin_overlays: PluginOverlaysInput::new(db, 0, vec![]),
        }
    }
}

/// Project `AppState` onto Salsa inputs based on which flags are dirty.
///
/// Only sets the inputs whose corresponding DirtyFlags are active,
/// so Salsa's revision tracking can detect unchanged inputs and skip
/// downstream revalidation.
pub fn sync_inputs_from_state(
    db: &mut KasaneDatabase,
    state: &AppState,
    dirty: DirtyFlags,
    inputs: &SalsaInputHandles,
) {
    if dirty.intersects(DirtyFlags::BUFFER_CONTENT) {
        inputs.buffer.set_lines(db).to(state.lines.clone());
        inputs.buffer.set_default_face(db).to(state.default_face);
        inputs.buffer.set_padding_face(db).to(state.padding_face);
        inputs
            .buffer
            .set_widget_columns(db)
            .to(state.widget_columns);
    }

    if dirty.intersects(DirtyFlags::BUFFER) {
        inputs.buffer.set_cursor_pos(db).to(state.cursor_pos);
        inputs.cursor.set_cursor_mode(db).to(state.cursor_mode);
        inputs.cursor.set_cursor_count(db).to(state.cursor_count);
        inputs
            .cursor
            .set_secondary_cursors(db)
            .to(state.secondary_cursors.clone());
    }

    if dirty.intersects(DirtyFlags::STATUS) {
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
    }

    if dirty.intersects(DirtyFlags::MENU) {
        let snapshot = state.menu.as_ref().map(MenuSnapshot::from_menu_state);
        inputs.menu.set_menu(db).to(snapshot);
    }

    if dirty.intersects(DirtyFlags::INFO) {
        let snapshots: Vec<_> = state
            .infos
            .iter()
            .map(InfoSnapshot::from_info_state)
            .collect();
        inputs.info.set_infos(db).to(snapshots);
    }

    if dirty.intersects(DirtyFlags::OPTIONS) {
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
}

/// Synchronize plugin contributions (slots, annotations, overlays) into Salsa inputs.
///
/// Call this after `prepare_plugin_cache()`, `sync_plugin_epoch()`, and
/// `sync_display_directives()`. Collects plugin contributions and stores
/// them as Salsa inputs for memoization.
pub fn sync_plugin_contributions(
    db: &mut KasaneDatabase,
    state: &AppState,
    registry: &PluginRegistry,
    inputs: &SalsaInputHandles,
    dirty: DirtyFlags,
    plugin_epoch_changed: bool,
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

    fn collect_slot(
        slot: &SlotId,
        state: &AppState,
        registry: &PluginRegistry,
        ctx: &ContributeContext,
    ) -> Vec<crate::element::FlexChild> {
        registry
            .collect_contributions(slot, state, ctx)
            .into_iter()
            .map(contribution_to_flex_child)
            .collect()
    }

    // Determine if slot contributions need updating
    let slot_deps = registry.contribute_deps_union();
    let needs_slot_update =
        plugin_epoch_changed || dirty.intersects(slot_deps | DirtyFlags::BUFFER_CONTENT);
    if needs_slot_update {
        let ctx = ContributeContext::new(state, None);
        let next_gen = inputs.slot_contributions.generation(db) + 1;
        inputs.slot_contributions.set_generation(db).to(next_gen);
        inputs
            .slot_contributions
            .set_buffer_left(db)
            .to(collect_slot(&SlotId::BUFFER_LEFT, state, registry, &ctx));
        inputs
            .slot_contributions
            .set_buffer_right(db)
            .to(collect_slot(&SlotId::BUFFER_RIGHT, state, registry, &ctx));
        inputs
            .slot_contributions
            .set_above_buffer(db)
            .to(collect_slot(&SlotId::ABOVE_BUFFER, state, registry, &ctx));
        inputs
            .slot_contributions
            .set_below_buffer(db)
            .to(collect_slot(&SlotId::BELOW_BUFFER, state, registry, &ctx));
        inputs
            .slot_contributions
            .set_status_left(db)
            .to(collect_slot(&SlotId::STATUS_LEFT, state, registry, &ctx));
        inputs
            .slot_contributions
            .set_status_right(db)
            .to(collect_slot(&SlotId::STATUS_RIGHT, state, registry, &ctx));
        inputs
            .slot_contributions
            .set_above_status(db)
            .to(collect_slot(&SlotId::ABOVE_STATUS, state, registry, &ctx));
    }

    // Determine if annotations need updating
    let annotate_deps = registry.annotate_deps();
    let needs_annotation_update =
        plugin_epoch_changed || dirty.intersects(annotate_deps | DirtyFlags::BUFFER_CONTENT);
    if needs_annotation_update {
        let display_map: DisplayMapRef =
            crate::salsa_views::display_map_query(db, inputs.display_directives);
        let annotate_ctx = AnnotateContext {
            line_width: state.cols,
            gutter_width: 0,
            display_map: Some(Arc::clone(&display_map)),
        };
        let result = registry.collect_annotations(state, &annotate_ctx);
        let next_gen = inputs.annotations.generation(db) + 1;
        inputs.annotations.set_generation(db).to(next_gen);
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
    }

    // Determine if overlays need updating
    let needs_overlay_update = plugin_epoch_changed || !dirty.is_empty();
    if needs_overlay_update {
        let overlay_ctx = OverlayContext {
            screen_cols: state.cols,
            screen_rows: state.rows,
            menu_rect: None,
            existing_overlays: vec![],
        };
        let overlays: Vec<Overlay> = registry
            .collect_overlays_with_ctx(state, &overlay_ctx)
            .into_iter()
            .map(|oc| Overlay {
                element: oc.element,
                anchor: oc.anchor,
            })
            .collect();
        let next_gen = inputs.plugin_overlays.generation(db) + 1;
        inputs.plugin_overlays.set_generation(db).to(next_gen);
        inputs.plugin_overlays.set_overlays(db).to(overlays);
    }
}

/// Synchronize display directives from plugins into Salsa.
///
/// Call this after `prepare_plugin_cache()` and `sync_plugin_epoch()`.
/// Only collects directives when `BUFFER_CONTENT` is dirty or the plugin
/// epoch changed, since directives depend on buffer content and plugin state.
pub fn sync_display_directives(
    db: &mut KasaneDatabase,
    state: &AppState,
    registry: &PluginRegistry,
    inputs: &SalsaInputHandles,
    dirty: DirtyFlags,
    plugin_epoch_changed: bool,
) {
    let needs_update = plugin_epoch_changed || dirty.intersects(DirtyFlags::BUFFER_CONTENT);
    if !needs_update {
        return;
    }

    let directives = registry.collect_display_directives(state);
    let line_count = state.visible_line_range().len();

    inputs.display_directives.set_directives(db).to(directives);
    inputs
        .display_directives
        .set_buffer_line_count(db)
        .to(line_count);
}

/// Synchronize plugin epoch into Salsa.
///
/// Call this after `PluginRegistry::prepare_plugin_cache()` each frame.
/// If any plugin's state hash changed, increments the epoch counter so
/// Salsa tracked functions that depend on `PluginEpochInput` will re-evaluate.
///
/// Returns `true` if the epoch was bumped (i.e., plugin outputs may have changed).
pub fn sync_plugin_epoch(
    db: &mut KasaneDatabase,
    registry: &PluginRegistry,
    inputs: &SalsaInputHandles,
) -> bool {
    if registry.any_plugin_state_changed() {
        let next = inputs.plugin_epoch.epoch(db) + 1;
        inputs.plugin_epoch.set_epoch(db).to(next);
        true
    } else {
        false
    }
}
