//! Canvas drawing operations for WASM plugins.
//!
//! Plugins submit `CanvasDrawOp`s to render GPU primitives within their
//! allocated element area. The ops map to `GpuPrimitive`s in the GPU
//! backend; the TUI backend ignores them (canvas areas appear empty).

use crate::protocol::Color;

/// A drawing operation submitted by a WASM plugin.
///
/// Coordinates are relative to the element's top-left corner, in cells.
/// The GPU renderer converts cell coordinates to pixels.
#[derive(Debug, Clone, PartialEq)]
pub enum CanvasDrawOp {
    /// Fill a rectangle with a solid color.
    FillRect {
        x: f32,
        y: f32,
        w: f32,
        h: f32,
        color: Color,
    },

    /// Draw a rounded rectangle border.
    RoundedRect {
        x: f32,
        y: f32,
        w: f32,
        h: f32,
        corner_radius: f32,
        border_width: f32,
        fill_color: Color,
        border_color: Color,
    },

    /// Draw a line between two points.
    Line {
        x1: f32,
        y1: f32,
        x2: f32,
        y2: f32,
        color: Color,
        width: f32,
    },

    /// Draw text at a position.
    Text {
        x: f32,
        y: f32,
        text: String,
        color: Color,
    },

    /// Draw a circle (filled or stroked).
    Circle {
        cx: f32,
        cy: f32,
        radius: f32,
        fill_color: Option<Color>,
        stroke_color: Option<Color>,
        stroke_width: f32,
    },
}

/// A canvas element's rendering state: a list of draw operations
/// submitted by a plugin for a specific element area.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct CanvasContent {
    pub ops: Vec<CanvasDrawOp>,
}

impl CanvasContent {
    pub fn new() -> Self {
        Self { ops: Vec::new() }
    }

    pub fn push(&mut self, op: CanvasDrawOp) {
        self.ops.push(op);
    }

    pub fn is_empty(&self) -> bool {
        self.ops.is_empty()
    }
}
