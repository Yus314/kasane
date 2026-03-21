//! Rendering pipeline: view construction, paint, cache, pipeline orchestration, scene.

pub(crate) mod builders;
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
pub(crate) mod theme;
pub mod view;
pub(crate) mod walk;

#[cfg(test)]
#[allow(clippy::field_reassign_with_default)]
mod tests;

pub use cursor::*;
pub use grid::{Cell, CellDiff, CellGrid};
pub use inline_decoration::{InlineDecoration, InlineOp};
pub use pipeline::{render_pipeline, render_pipeline_direct, scene_render_pipeline};
pub use pipeline_salsa::{render_pipeline_cached, scene_render_pipeline_cached};
pub use scene::{CellSize, DrawCommand, PixelPos, PixelRect, ResolvedAtom, SceneCache};

// ---------------------------------------------------------------------------
// RenderBackend trait
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CursorStyle {
    Block,
    Bar,
    Underline,
    Outline,
}

pub trait RenderBackend {
    fn size(&self) -> (u16, u16);
    fn begin_frame(&mut self) -> anyhow::Result<()> {
        Ok(())
    }
    fn end_frame(&mut self) -> anyhow::Result<()> {
        Ok(())
    }
    fn draw(&mut self, diffs: &[CellDiff]) -> anyhow::Result<()>;
    fn flush(&mut self) -> anyhow::Result<()>;
    fn show_cursor(&mut self, x: u16, y: u16, style: CursorStyle) -> anyhow::Result<()>;
    fn hide_cursor(&mut self) -> anyhow::Result<()>;

    /// Draw directly from the CellGrid, avoiding the CellDiff intermediate.
    /// Backends can override this for optimized rendering (e.g. cursor
    /// auto-advance, incremental SGR). Default falls back to `draw()`.
    fn draw_grid(&mut self, grid: &CellGrid) -> anyhow::Result<()> {
        self.draw(&grid.diff())
    }

    /// Read text from the system clipboard. Returns None if unavailable.
    fn clipboard_get(&mut self) -> Option<String> {
        None
    }
    /// Write text to the system clipboard. Returns true on success.
    fn clipboard_set(&mut self, _text: &str) -> bool {
        false
    }
}

/// レンダリングパイプラインの結果。バックエンド固有の描画に必要な情報を返す。
#[derive(Debug, Clone, Copy)]
pub struct RenderResult {
    pub cursor_x: u16,
    pub cursor_y: u16,
    pub cursor_style: CursorStyle,
}
