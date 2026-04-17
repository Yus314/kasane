//! Runtime/ephemeral state sub-struct.
//!
//! Contains transient state that is neither part of the protocol, config,
//! nor session metadata. These fields are not serialized or preserved
//! across session switches (except cols/rows/focused which are preserved).

use crate::display::{DisplayMapRef, DisplayUnitMap};
use crate::layout::HitMap;

use super::DragState;

/// Ephemeral runtime state.
///
/// Every field here carries `#[epistemic(runtime)]` semantics.
#[derive(Debug, Clone)]
pub struct RuntimeState {
    pub focused: bool,
    pub drag: DragState,
    pub cols: u16,
    pub rows: u16,
    /// Post-render hit map for interactive element mouse routing.
    pub hit_map: HitMap,
    /// Display scroll offset from the last rendered frame.
    pub display_scroll_offset: usize,
    /// Display map from the last rendered frame.
    pub display_map: Option<DisplayMapRef>,
    /// Display unit map from the last rendered frame.
    pub display_unit_map: Option<DisplayUnitMap>,
}

impl Default for RuntimeState {
    fn default() -> Self {
        Self {
            focused: true,
            drag: DragState::None,
            cols: 80,
            rows: 24,
            hit_map: HitMap::new(),
            display_scroll_offset: 0,
            display_map: None,
            display_unit_map: None,
        }
    }
}
