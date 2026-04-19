//! Visual hints for GPU-specific rendering effects.
//!
//! These hints are collected during scene rendering and passed alongside
//! `RenderResult` to the GPU backend. The TUI backend ignores them.

use super::scene::PixelRect;

/// GPU-specific visual hints collected during scene rendering.
#[derive(Debug, Clone, Default)]
pub struct VisualHints {
    /// Cursor line position for highlight effects.
    pub cursor_line: Option<CursorLineHint>,
    /// Overlay regions for transition effects.
    pub overlay_regions: Vec<OverlayRegionHint>,
    /// Focused pane in multi-pane mode. `None` when single-pane.
    pub focused_pane: Option<FocusedPaneHint>,
}

/// Position and dimensions of the cursor line (in pixels).
#[derive(Debug, Clone, Copy)]
pub struct CursorLineHint {
    /// Y position of the cursor line.
    pub y: f32,
    /// Height of the cursor line (one cell row).
    pub height: f32,
    /// Width of the full line (viewport width).
    pub width: f32,
}

/// Focused pane rectangle in pixel coordinates (for non-focused pane dimming).
///
/// When `Some`, the GPU backend can dim everything outside this rectangle
/// to visually distinguish the focused pane in multi-pane mode.
#[derive(Debug, Clone, Copy)]
pub struct FocusedPaneHint {
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
}

/// A region occupied by an overlay (menu, info popup, etc.).
#[derive(Debug, Clone)]
pub struct OverlayRegionHint {
    /// Bounding rectangle in pixel coordinates.
    pub rect: PixelRect,
    /// Identifier for tracking overlay transitions.
    pub id: u32,
}
