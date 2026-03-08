mod grid;
mod info;
pub mod markup;
pub(crate) mod menu;
pub mod paint;
pub mod scene;
#[cfg(test)]
pub(crate) mod test_helpers;
pub(crate) mod theme;
pub mod view;

#[cfg(test)]
#[allow(clippy::field_reassign_with_default)]
mod tests;

pub use grid::{Cell, CellDiff, CellGrid};
pub use scene::{CellSize, DrawCommand, PixelPos, PixelRect, ResolvedAtom};
pub use theme::Theme;

use crate::layout::Rect;
use crate::layout::flex;
use crate::plugin::PluginRegistry;
use crate::protocol::CursorMode;
use crate::state::AppState;

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

    /// Read text from the system clipboard. Returns None if unavailable.
    fn clipboard_get(&mut self) -> Option<String> {
        None
    }
    /// Write text to the system clipboard. Returns true on success.
    fn clipboard_set(&mut self, _text: &str) -> bool {
        false
    }
}

// ---------------------------------------------------------------------------
// Cursor utilities
// ---------------------------------------------------------------------------

/// Compute the terminal cursor position from the application state.
/// Returns (x, y) coordinates for the terminal cursor.
pub fn cursor_position(state: &AppState, grid: &CellGrid) -> (u16, u16) {
    let cx = state.cursor_pos.column as u16;
    let cy = match state.cursor_mode {
        CursorMode::Prompt => grid.height.saturating_sub(1),
        CursorMode::Buffer => state.cursor_pos.line as u16,
    };
    (cx, cy)
}

/// Determine the cursor style from the application state.
///
/// Priority: ui_option `kasane_cursor_style` > prompt mode > mode_line heuristic > Block.
pub fn cursor_style(state: &AppState) -> CursorStyle {
    if let Some(style) = state.ui_options.get("kasane_cursor_style") {
        return match style.as_str() {
            "bar" => CursorStyle::Bar,
            "underline" => CursorStyle::Underline,
            _ => CursorStyle::Block,
        };
    }
    if !state.focused {
        return CursorStyle::Outline;
    }
    if state.cursor_mode == CursorMode::Prompt {
        return CursorStyle::Bar;
    }
    let mode = state
        .status_mode_line
        .iter()
        .find_map(|atom| match atom.contents.as_str() {
            "insert" => Some(CursorStyle::Bar),
            "replace" => Some(CursorStyle::Underline),
            _ => None,
        });
    mode.unwrap_or(CursorStyle::Block)
}

/// In non-block cursor modes (insert/replace), clear the PrimaryCursor face
/// highlight from the cursor cell so the terminal cursor shape is visible.
pub fn clear_block_cursor_face(state: &AppState, grid: &mut CellGrid, style: CursorStyle) {
    if style == CursorStyle::Block || style == CursorStyle::Outline {
        return;
    }
    let cx = state.cursor_pos.column as u16;
    let cy = match state.cursor_mode {
        CursorMode::Prompt => grid.height.saturating_sub(1),
        CursorMode::Buffer => state.cursor_pos.line as u16,
    };
    let base_face = match state.cursor_mode {
        CursorMode::Buffer => &state.default_face,
        CursorMode::Prompt => &state.status_default_face,
    };
    if let Some(cell) = grid.get_mut(cx, cy) {
        cell.face = *base_face;
    }
}

// ---------------------------------------------------------------------------
// Declarative render pipeline
// ---------------------------------------------------------------------------

/// レンダリングパイプラインの結果。バックエンド固有の描画に必要な情報を返す。
pub struct RenderResult {
    pub cursor_x: u16,
    pub cursor_y: u16,
    pub cursor_style: CursorStyle,
}

/// GUI 用シーンレンダリングパイプライン。
/// view → layout → scene_paint → cursor を実行し、DrawCommand リストを返す。
pub fn scene_render_pipeline(
    state: &AppState,
    registry: &PluginRegistry,
    cell_size: scene::CellSize,
) -> (Vec<DrawCommand>, RenderResult) {
    crate::perf::perf_span!("scene_render_pipeline");

    let element = view::view(state, registry);
    let root_area = Rect {
        x: 0,
        y: 0,
        w: state.cols,
        h: state.rows,
    };
    let layout_result = flex::place(&element, root_area, state);
    let theme = Theme::default_theme();
    let style = cursor_style(state);
    let commands = scene::scene_paint(&element, &layout_result, state, &theme, cell_size, style);
    let cx = state.cursor_pos.column as u16;
    let cy = match state.cursor_mode {
        CursorMode::Prompt => state.rows.saturating_sub(1),
        CursorMode::Buffer => state.cursor_pos.line as u16,
    };

    (
        commands,
        RenderResult {
            cursor_x: cx,
            cursor_y: cy,
            cursor_style: style,
        },
    )
}

/// 宣言的レンダリングパイプラインを実行する。
/// view → layout → paint → cursor 処理を行い、grid を更新する。
pub fn render_pipeline(
    state: &AppState,
    registry: &PluginRegistry,
    grid: &mut CellGrid,
) -> RenderResult {
    crate::perf::perf_span!("render_pipeline");

    let element = view::view(state, registry);
    let root_area = Rect {
        x: 0,
        y: 0,
        w: state.cols,
        h: state.rows,
    };
    let layout_result = flex::place(&element, root_area, state);
    grid.clear(&state.default_face);
    paint::paint(&element, &layout_result, grid, state);

    let style = cursor_style(state);
    clear_block_cursor_face(state, grid, style);
    let (cx, cy) = cursor_position(state, grid);

    RenderResult {
        cursor_x: cx,
        cursor_y: cy,
        cursor_style: style,
    }
}
