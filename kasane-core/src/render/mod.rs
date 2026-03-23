//! Rendering pipeline: view construction, paint, cache, pipeline orchestration, scene.

pub(crate) mod builders;
pub mod cell_decoration;
pub mod color_context;
pub mod cursor;
mod grid;
pub mod halfblock;
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

/// Blink animation hint from plugins.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct BlinkHint {
    pub enabled: bool,
    pub delay_ms: u16,
    pub period_ms: u16,
    pub min_opacity: f32,
}

/// Easing curve for cursor movement animation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EasingCurve {
    Linear,
    EaseOut,
    EaseInOut,
}

/// Movement animation hint from plugins.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct MovementHint {
    pub enabled: bool,
    pub duration_ms: u16,
    pub easing: EasingCurve,
}

/// Extended cursor style with optional blink and movement animation hints.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CursorStyleHint {
    pub shape: CursorStyle,
    pub blink: Option<BlinkHint>,
    pub movement: Option<MovementHint>,
}

impl From<CursorStyle> for CursorStyleHint {
    fn from(shape: CursorStyle) -> Self {
        Self {
            shape,
            blink: None,
            movement: None,
        }
    }
}

/// Rendering pipeline result. Contains cursor position/style for the backend.
#[derive(Debug, Clone, Copy)]
pub struct RenderResult {
    pub cursor_x: u16,
    pub cursor_y: u16,
    pub cursor_style: CursorStyle,
    /// Cursor color extracted from the Kakoune face at the cursor position.
    /// Under REVERSE (typical), this is `face.fg`; otherwise `face.bg`.
    /// Falls back to `Color::Default` when face cannot be determined.
    pub cursor_color: crate::protocol::Color,
    /// Blink animation hint from plugin override.
    pub cursor_blink: Option<BlinkHint>,
    /// Movement animation hint from plugin override.
    pub cursor_movement: Option<MovementHint>,
    /// Display scroll offset applied this frame.
    /// Used to update `AppState::display_scroll_offset` for mouse coordinate translation.
    pub display_scroll_offset: usize,
}
