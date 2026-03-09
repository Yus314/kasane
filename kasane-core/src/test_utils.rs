use crate::layout::Rect;
use crate::protocol::{Atom, Face, Line};
use crate::state::AppState;

pub fn default_state() -> AppState {
    AppState::default()
}

pub fn root_area(w: u16, h: u16) -> Rect {
    Rect { x: 0, y: 0, w, h }
}

pub fn make_line(s: &str) -> Line {
    vec![Atom {
        face: Face::default(),
        contents: s.into(),
    }]
}
