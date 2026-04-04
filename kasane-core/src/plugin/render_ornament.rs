use crate::plugin::CellDecoration;
use crate::protocol::Face;
use crate::render::CursorStyleHint;

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
}

impl RenderOrnamentContext {
    /// Build from screen dimensions and the pipeline-computed display scroll offset.
    pub fn from_screen(cols: u16, rows: u16, display_scroll_offset: usize) -> Self {
        Self {
            screen_cols: cols,
            screen_rows: rows,
            visible_line_start: display_scroll_offset as u32,
            visible_line_end: display_scroll_offset as u32 + rows as u32,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct OrnamentBatch {
    pub emphasis: Vec<CellDecoration>,
    pub cursor_style: Option<CursorStyleOrn>,
    pub cursor_effects: Vec<CursorEffectOrn>,
    pub surfaces: Vec<SurfaceOrn>,
}

impl OrnamentBatch {
    pub fn is_empty(&self) -> bool {
        self.emphasis.is_empty()
            && self.cursor_style.is_none()
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
