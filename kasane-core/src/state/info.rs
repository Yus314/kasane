use crate::protocol::{Coord, Face, InfoStyle, Line};

/// Identity key for deduplicating simultaneous info popups.
/// Infos with the same identity replace each other; different identities coexist.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InfoIdentity {
    pub style: InfoStyle,
    pub anchor_line: u32,
}

#[derive(Debug, Clone)]
pub struct InfoState {
    pub title: Line,
    pub content: Vec<Line>,
    pub anchor: Coord,
    pub face: Face,
    pub style: InfoStyle,
    pub identity: InfoIdentity,
    pub scroll_offset: u16,
}
