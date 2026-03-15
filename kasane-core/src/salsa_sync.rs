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
