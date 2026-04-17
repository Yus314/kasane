//! Observed protocol state sub-struct.
//!
//! Contains fields in 1:1 correspondence with Kakoune JSON-RPC messages.
//! No transformation is applied; values are stored exactly as received.

use std::collections::HashMap;

use crate::protocol::{Coord, Face, Line, StatusStyle};

use super::{InfoState, MenuState};

/// Protocol-observed state from Kakoune JSON-RPC messages.
///
/// Every field here carries `#[epistemic(observed)]` semantics: it is a
/// direct 1:1 mapping from a Kakoune protocol message with no transformation.
#[derive(Debug, Clone)]
pub struct ObservedState {
    /// Buffer lines from `draw`.
    pub lines: Vec<Line>,
    /// Default face from `draw`.
    pub default_face: Face,
    /// Padding face from `draw`.
    pub padding_face: Face,
    /// Cursor position from `draw` (`cursor_pos` field).
    pub cursor_pos: Coord,
    /// Status prompt atoms from `draw_status`.
    pub status_prompt: Line,
    /// Status content atoms from `draw_status`.
    pub status_content: Line,
    /// Cursor position within status content from `draw_status`.
    pub status_content_cursor_pos: i32,
    /// Mode line atoms from `draw_status`.
    pub status_mode_line: Line,
    /// Default face for the status bar from `draw_status`.
    pub status_default_face: Face,
    /// Status bar context from `draw_status` (PR #5458).
    pub status_style: StatusStyle,
    /// Number of widget columns from `draw`.
    pub widget_columns: u16,
    /// Completion menu state from `menu_show` / `menu_select` / `menu_hide`.
    pub menu: Option<MenuState>,
    /// Info popup state from `info_show` / `info_hide`.
    pub infos: Vec<InfoState>,
    /// UI options from `set_ui_options`.
    pub ui_options: HashMap<String, String>,
}

impl Default for ObservedState {
    fn default() -> Self {
        Self {
            lines: Vec::new(),
            default_face: Face::default(),
            padding_face: Face::default(),
            cursor_pos: Coord::default(),
            status_prompt: Vec::new(),
            status_content: Vec::new(),
            status_content_cursor_pos: -1,
            status_mode_line: Vec::new(),
            status_default_face: Face::default(),
            status_style: StatusStyle::default(),
            widget_columns: 0,
            menu: None,
            infos: Vec::new(),
            ui_options: HashMap::new(),
        }
    }
}
