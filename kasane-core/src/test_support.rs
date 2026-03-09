//! Shared test utilities available to both unit tests and integration tests.
//!
//! This module is `#[doc(hidden)]` but `pub` so that integration tests
//! (which are separate crates) can access these helpers.

use crate::layout::Rect;
use crate::protocol::{Atom, Color, Face, Line, NamedColor};
use crate::state::AppState;

pub fn make_line(s: &str) -> Line {
    vec![Atom {
        face: Face::default(),
        contents: s.into(),
    }]
}

pub fn default_state() -> AppState {
    AppState::default()
}

pub fn root_area(w: u16, h: u16) -> Rect {
    Rect { x: 0, y: 0, w, h }
}

/// Standard 80×24 AppState with reasonable default faces.
/// Tests can customize individual fields after the call.
pub fn test_state_80x24() -> AppState {
    let mut state = AppState::default();
    state.cols = 80;
    state.rows = 24;
    state.default_face = Face {
        fg: Color::Named(NamedColor::White),
        bg: Color::Named(NamedColor::Black),
        ..Face::default()
    };
    state.padding_face = state.default_face;
    state.status_default_face = Face {
        fg: Color::Named(NamedColor::Cyan),
        bg: Color::Named(NamedColor::Black),
        ..Face::default()
    };
    state
}
