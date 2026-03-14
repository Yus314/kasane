use crate::protocol::{Coord, Face, InfoStyle, Line};

/// Heuristic identity key for deduplicating simultaneous info popups.
///
/// Uses `style` + `anchor_line` as an approximation of identity -- two infos with the same
/// style and anchor line are treated as the same popup being updated. This is not guaranteed
/// by the protocol; Kakoune may in theory send distinct infos with identical style and anchor.
///
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
