//! Salsa input structs for Kasane's incremental computation layer.
//!
//! These structs are set once per frame from `AppState` via `sync_inputs_from_state()`.
//! They follow the protocol message boundary grouping from `apply.rs`.

use crate::config::MenuPosition;
use crate::protocol::{Coord, CursorMode, Face, Line};
use crate::state::snapshot::{InfoSnapshot, MenuSnapshot};

/// Buffer content from `KakouneRequest::Draw`.
#[salsa::input]
pub struct BufferInput {
    #[returns(ref)]
    pub lines: Vec<Line>,
    pub default_face: Face,
    pub padding_face: Face,
    pub cursor_pos: Coord,
    pub widget_columns: u16,
}

/// Cursor state — derived from heuristic detection in Layer 1.
#[salsa::input]
pub struct CursorInput {
    pub cursor_mode: CursorMode,
    pub cursor_count: usize,
    #[returns(ref)]
    pub secondary_cursors: Vec<Coord>,
}

/// Status bar from `KakouneRequest::DrawStatus`.
#[salsa::input]
pub struct StatusInput {
    #[returns(ref)]
    pub status_line: Line,
    #[returns(ref)]
    pub status_mode_line: Line,
    pub status_default_face: Face,
}

/// Menu from `KakouneRequest::MenuShow/Select/Hide`.
#[salsa::input]
pub struct MenuInput {
    #[returns(ref)]
    pub menu: Option<MenuSnapshot>,
}

/// Info popups from `KakouneRequest::InfoShow/Hide`.
#[salsa::input]
pub struct InfoInput {
    #[returns(ref)]
    pub infos: Vec<InfoSnapshot>,
}

/// Configuration and runtime dimensions.
///
/// These fields change infrequently and are set with `Durability::HIGH`.
#[salsa::input]
pub struct ConfigInput {
    pub cols: u16,
    pub rows: u16,
    pub focused: bool,
    pub shadow_enabled: bool,
    pub status_at_top: bool,
    pub secondary_blend_ratio: f32,
    pub menu_position: MenuPosition,
    pub search_dropdown: bool,
    #[returns(ref)]
    pub scrollbar_thumb: String,
    #[returns(ref)]
    pub scrollbar_track: String,
    #[returns(ref)]
    pub assistant_art: Option<Vec<String>>,
}

/// Plugin contribution epoch — increments when plugin outputs change.
///
/// Tracked functions that depend on plugin contributions read this input
/// to detect when re-evaluation is needed, without storing the actual
/// contribution data in Salsa.
#[salsa::input]
pub struct PluginEpochInput {
    pub epoch: u64,
}
