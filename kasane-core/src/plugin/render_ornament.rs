use crate::plugin::{DecorationTarget, FaceMerge, PluginId};
use crate::protocol::Face;
use crate::render::CursorStyleHint;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum OrnamentModality {
    Must,
    May,
    #[default]
    Approximate,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct RenderOrnamentContext {
    pub screen_cols: u16,
    pub screen_rows: u16,
    pub visible_line_start: u32,
    pub visible_line_end: u32,
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct OrnamentBatch {
    pub emphasis: Vec<EmphasisOrn>,
    pub cursor: Option<CursorOrn>,
    pub surfaces: Vec<SurfaceOrn>,
}

impl OrnamentBatch {
    pub fn is_empty(&self) -> bool {
        self.emphasis.is_empty() && self.cursor.is_none() && self.surfaces.is_empty()
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct SourcedOrnamentBatch {
    pub plugin_id: PluginId,
    pub batch: OrnamentBatch,
}

#[derive(Debug, Clone, PartialEq)]
pub struct EmphasisOrn {
    pub target: DecorationTarget,
    pub face: Face,
    pub merge: FaceMerge,
    pub priority: i16,
}

#[derive(Debug, Clone, PartialEq)]
pub enum CursorOrnKind {
    Halo,
    Ring,
    Emphasis,
    Style(CursorStyleHint),
}

#[derive(Debug, Clone, PartialEq)]
pub struct CursorOrn {
    pub kind: CursorOrnKind,
    pub face: Face,
    pub priority: i16,
    pub modality: OrnamentModality,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SurfaceOrnAnchor {
    FocusedSurface,
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
