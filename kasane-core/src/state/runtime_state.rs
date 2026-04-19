//! Runtime/ephemeral state sub-struct.
//!
//! Contains transient state that is neither part of the protocol, config,
//! nor session metadata. These fields are not serialized or preserved
//! across session switches (except cols/rows/focused which are preserved).

use std::sync::Arc;

use crate::display::segment_map::SegmentMap;
use crate::display::{DisplayMapRef, DisplayUnitMap, ProjectionDescriptor};
use crate::layout::HitMap;
use crate::syntax::SyntaxProvider;

use super::DragState;

/// Ephemeral runtime state.
///
/// Every field here carries `#[epistemic(runtime)]` semantics.
#[derive(Clone)]
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
    /// Segment map from the last rendered frame (present when content annotations exist).
    pub segment_map: Option<Arc<SegmentMap>>,
    /// Available projection descriptors from all registered plugins.
    pub available_projections: Vec<ProjectionDescriptor>,
    /// Syntax analysis provider for the current buffer (e.g., tree-sitter).
    pub syntax_provider: Option<Arc<dyn SyntaxProvider>>,
}

impl std::fmt::Debug for RuntimeState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("RuntimeState")
            .field("focused", &self.focused)
            .field("drag", &self.drag)
            .field("cols", &self.cols)
            .field("rows", &self.rows)
            .field("display_scroll_offset", &self.display_scroll_offset)
            .field(
                "syntax_provider",
                &self
                    .syntax_provider
                    .as_ref()
                    .map(|sp| format!("SyntaxProvider(gen={})", sp.generation())),
            )
            .finish_non_exhaustive()
    }
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
            segment_map: None,
            available_projections: Vec::new(),
            syntax_provider: None,
        }
    }
}
