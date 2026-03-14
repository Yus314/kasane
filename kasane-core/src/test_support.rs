//! Shared test utilities available to both unit tests and integration tests.
//!
//! This module is `#[doc(hidden)]` but `pub` so that integration tests
//! (which are separate crates) can access these helpers.

use crate::layout::Rect;
use crate::layout::flex::place;
use crate::plugin::PluginRegistry;
use crate::protocol::{Atom, Color, Face, Line, NamedColor};
use crate::render::pipeline::render_pipeline_cached;
use crate::render::view;
use crate::render::{CellGrid, ViewCache, paint};
use crate::state::{AppState, DirtyFlags};

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

/// Extract a row from the grid as a string (trimming trailing spaces).
pub fn row_text(grid: &CellGrid, y: u16) -> String {
    let mut s = String::new();
    for x in 0..grid.width() {
        if let Some(cell) = grid.get(x, y)
            && cell.width > 0
        {
            s.push_str(&cell.grapheme);
        }
    }
    s.trim_end().to_string()
}

/// Run the full pipeline (view → place → paint) with a given registry.
pub fn render_with_registry(state: &AppState, registry: &PluginRegistry) -> CellGrid {
    let element = view::view(state, registry);
    let root = Rect {
        x: 0,
        y: 0,
        w: state.cols,
        h: state.rows,
    };
    let layout = place(&element, root, state);
    let mut grid = CellGrid::new(state.cols, state.rows);
    grid.clear(&state.default_face);
    paint::paint(&element, &layout, &mut grid, state);
    grid
}

/// Render to a fresh CellGrid using the cached pipeline with given dirty flags.
pub fn render_to_grid(
    state: &AppState,
    registry: &PluginRegistry,
    dirty: DirtyFlags,
    cache: &mut ViewCache,
) -> CellGrid {
    let mut grid = CellGrid::new(state.cols, state.rows);
    grid.clear(&state.default_face);
    render_pipeline_cached(state, registry, &mut grid, dirty, cache);
    grid
}

/// Compare two grids cell-by-cell, panicking with a descriptive message on mismatch.
pub fn assert_grids_equal(actual: &CellGrid, expected: &CellGrid, context: &str) {
    assert_eq!(
        actual.width(),
        expected.width(),
        "{context}: width mismatch"
    );
    assert_eq!(
        actual.height(),
        expected.height(),
        "{context}: height mismatch"
    );
    for y in 0..actual.height() {
        for x in 0..actual.width() {
            let a = actual.get(x, y).unwrap();
            let e = expected.get(x, y).unwrap();
            assert_eq!(
                a, e,
                "{context}: cell mismatch at ({x}, {y})\n  actual:   {a:?}\n  expected: {e:?}",
            );
        }
    }
}
