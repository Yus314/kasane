//! Rendering pipeline: view construction, paint, cache, pipeline orchestration, scene.

pub(crate) mod builders;
pub mod color_context;
pub mod cursor;
mod grid;
pub mod inline_decoration;
pub mod markup;
pub mod paint;
pub mod pipeline;
mod pipeline_salsa;
pub mod scene;
#[cfg(test)]
pub(crate) mod test_helpers;
pub mod theme;
pub mod view;
pub(crate) mod walk;

#[cfg(test)]
#[allow(clippy::field_reassign_with_default)]
mod tests;

pub use cursor::*;
#[doc(hidden)]
pub use grid::CellDiff;
pub use grid::{Cell, CellGrid};
pub use inline_decoration::{InlineDecoration, InlineOp};
pub use pipeline::{render_pipeline, render_pipeline_direct, scene_render_pipeline};
pub use pipeline_salsa::{render_pipeline_cached, scene_render_pipeline_cached};
pub use scene::{CellSize, DrawCommand, PixelPos, PixelRect, ResolvedAtom, SceneCache};

// ---------------------------------------------------------------------------
// CursorStyle + RenderResult
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CursorStyle {
    Block,
    Bar,
    Underline,
    Outline,
}

/// Rendering pipeline result. Contains cursor position/style for the backend.
#[derive(Debug, Clone, Copy)]
pub struct RenderResult {
    pub cursor_x: u16,
    pub cursor_y: u16,
    pub cursor_style: CursorStyle,
    /// Display scroll offset applied this frame.
    /// Used to update `AppState::display_scroll_offset` for mouse coordinate translation.
    pub display_scroll_offset: usize,
}
