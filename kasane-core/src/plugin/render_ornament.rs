use crate::plugin::CellDecoration;
use crate::protocol::{Color, Face};
use crate::render::{CursorStyle, CursorStyleHint};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum OrnamentModality {
    Must,
    May,
    #[default]
    Approximate,
}

impl OrnamentModality {
    /// Numeric rank for modality-based tie-breaking: Must(2) > Approximate(1) > May(0).
    pub fn rank(self) -> i8 {
        match self {
            Self::Must => 2,
            Self::Approximate => 1,
            Self::May => 0,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct RenderOrnamentContext {
    pub screen_cols: u16,
    pub screen_rows: u16,
    pub visible_line_start: u32,
    pub visible_line_end: u32,
    /// X offset of the buffer region within the screen (accounts for gutters).
    pub buffer_x_offset: u16,
    /// Y offset of the buffer region within the screen (accounts for status line).
    pub buffer_y_offset: u16,
}

impl RenderOrnamentContext {
    /// Build from screen dimensions and the pipeline-computed display scroll offset.
    pub fn from_screen(
        cols: u16,
        rows: u16,
        display_scroll_offset: usize,
        buffer_x_offset: u16,
        buffer_y_offset: u16,
    ) -> Self {
        Self {
            screen_cols: cols,
            screen_rows: rows,
            visible_line_start: display_scroll_offset as u32,
            visible_line_end: display_scroll_offset as u32 + rows as u32,
            buffer_x_offset,
            buffer_y_offset,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct OrnamentBatch {
    pub emphasis: Vec<CellDecoration>,
    pub cursor_style: Option<CursorStyleOrn>,
    pub cursor_position: Option<CursorPositionOrn>,
    pub cursor_effects: Vec<CursorEffectOrn>,
    pub surfaces: Vec<SurfaceOrn>,
}

impl OrnamentBatch {
    pub fn is_empty(&self) -> bool {
        self.emphasis.is_empty()
            && self.cursor_style.is_none()
            && self.cursor_position.is_none()
            && self.cursor_effects.is_empty()
            && self.surfaces.is_empty()
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct CursorStyleOrn {
    pub hint: CursorStyleHint,
    pub priority: i16,
    pub modality: OrnamentModality,
}

/// Cursor position override ornament.
///
/// When present, the rendering pipeline uses this position instead of the
/// normal cursor position derived from AppState.
#[derive(Debug, Clone, PartialEq)]
pub struct CursorPositionOrn {
    pub x: u16,
    pub y: u16,
    pub style: CursorStyle,
    pub color: Color,
    pub priority: i16,
    pub modality: OrnamentModality,
}

/// Reserved for future use. Currently collected but not rendered by the host.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CursorEffect {
    Halo,
    Ring,
    Emphasis,
}

/// Reserved for future use. Currently collected but not rendered by the host.
#[derive(Debug, Clone, PartialEq)]
pub struct CursorEffectOrn {
    pub kind: CursorEffect,
    pub face: Face,
    pub priority: i16,
    pub modality: OrnamentModality,
}

/// Target surface for a surface ornament.
///
/// Valid (anchor, kind) combinations:
/// - `FocusedSurface` + `FocusFrame` — draws a frame around the focused surface.
/// - `SurfaceKey` + `FocusFrame` — allowed only when the named surface **is** focused.
/// - `SurfaceKey` + `InactiveTint` — allowed only when the named surface is **not** focused.
///
/// Other combinations are silently dropped during resolution.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SurfaceOrnAnchor {
    /// The currently focused surface. Only valid with `SurfaceOrnKind::FocusFrame`.
    FocusedSurface,
    /// A surface identified by its registration key (e.g. `"kasane.buffer"`).
    SurfaceKey(String),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SurfaceOrnKind {
    FocusFrame,
    InactiveTint,
}

#[derive(Debug, Clone, PartialEq)]
pub struct SurfaceOrn {
    pub anchor: SurfaceOrnAnchor,
    pub kind: SurfaceOrnKind,
    pub face: Face,
    pub priority: i16,
    pub modality: OrnamentModality,
}
