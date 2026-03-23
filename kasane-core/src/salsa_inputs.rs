//! Salsa input structs for Kasane's incremental computation layer.
//!
//! These structs are set once per frame from `AppState` via `sync_inputs_from_state()`.
//! They follow the protocol message boundary grouping from `apply.rs`.

use crate::config::MenuPosition;
use crate::protocol::{Coord, CursorMode, Face, Line, StatusStyle};
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
    pub status_style: StatusStyle,
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

/// Plugin slot contributions snapshot.
///
/// Stores the FlexChild vectors for each slot, collected from plugins
/// during the sync phase.
#[salsa::input]
pub struct SlotContributionsInput {
    #[returns(ref)]
    pub buffer_left: Vec<crate::element::FlexChild>,
    #[returns(ref)]
    pub buffer_right: Vec<crate::element::FlexChild>,
    #[returns(ref)]
    pub above_buffer: Vec<crate::element::FlexChild>,
    #[returns(ref)]
    pub below_buffer: Vec<crate::element::FlexChild>,
    #[returns(ref)]
    pub status_left: Vec<crate::element::FlexChild>,
    #[returns(ref)]
    pub status_right: Vec<crate::element::FlexChild>,
    #[returns(ref)]
    pub above_status: Vec<crate::element::FlexChild>,
}

/// Plugin line annotation results.
///
/// Stores gutter elements and line backgrounds from plugin annotations,
/// collected during the sync phase.
#[salsa::input]
pub struct AnnotationResultInput {
    #[returns(ref)]
    pub line_backgrounds: Option<Vec<Option<crate::protocol::Face>>>,
    #[returns(ref)]
    pub left_gutter: Option<crate::element::Element>,
    #[returns(ref)]
    pub right_gutter: Option<crate::element::Element>,
    #[returns(ref)]
    pub inline_decorations: Option<Vec<Option<crate::render::InlineDecoration>>>,
    #[returns(ref)]
    pub virtual_text: Option<Vec<Option<Vec<crate::protocol::Atom>>>>,
}

/// Plugin overlay contributions.
///
/// Stores overlay elements collected from plugins during the sync phase.
#[salsa::input]
pub struct PluginOverlaysInput {
    #[returns(ref)]
    pub overlays: Vec<crate::element::Overlay>,
}

/// Display transformation directives from plugins.
///
/// Contains the raw directives and buffer line count needed to build a `DisplayMap`.
/// Set by `sync_display_directives()` when `BUFFER_CONTENT` changes.
#[salsa::input]
pub struct DisplayDirectivesInput {
    #[returns(ref)]
    pub directives: Vec<crate::display::DisplayDirective>,
    pub buffer_line_count: usize,
}
