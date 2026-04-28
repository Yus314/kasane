//! Shared test utilities available to both unit tests and integration tests.
//!
//! This module is `#[doc(hidden)]` but `pub` so that integration tests
//! (which are separate crates) can access these helpers.

use compact_str::CompactString;

use crate::element::Element;
use crate::layout::Rect;
use crate::layout::flex::place;
use crate::plugin::{Command, PluginRuntime};
use crate::protocol::{Atom, Color, Face, Line, NamedColor};
// Phase A.2: tests construct atoms with default style via Atom::plain.
use crate::render::pipeline::render_pipeline;
use crate::render::view;
use crate::render::{CellGrid, paint};
use crate::state::{AppState, DirtyFlags};
use crate::surface::*;

pub fn make_line(s: &str) -> Line {
    vec![Atom::plain(s)]
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
    state.runtime.cols = 80;
    state.runtime.rows = 24;
    state.observed.default_face = Face {
        fg: Color::Named(NamedColor::White),
        bg: Color::Named(NamedColor::Black),
        ..Face::default()
    };
    state.observed.padding_face = state.observed.default_face;
    state.observed.status_default_face = Face {
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
pub fn render_with_registry(state: &AppState, registry: &PluginRuntime) -> CellGrid {
    let element = view::view(state, &registry.view());
    let root = Rect {
        x: 0,
        y: 0,
        w: state.runtime.cols,
        h: state.runtime.rows,
    };
    let layout = place(&element, root, state);
    let mut grid = CellGrid::new(state.runtime.cols, state.runtime.rows);
    grid.clear(&state.observed.default_face);
    paint::paint(&element, &layout, &mut grid, state);
    grid
}

/// Render to a fresh CellGrid using the non-cached pipeline.
pub fn render_to_grid(state: &AppState, registry: &PluginRuntime) -> CellGrid {
    let mut grid = CellGrid::new(state.runtime.cols, state.runtime.rows);
    grid.clear(&state.observed.default_face);
    render_pipeline(state, &registry.view(), &mut grid);
    grid
}

/// Render to a fresh CellGrid and return the RenderResult (cursor position, style, etc.).
pub fn render_to_grid_with_result(
    state: &AppState,
    registry: &PluginRuntime,
) -> (CellGrid, crate::render::RenderResult) {
    let mut grid = CellGrid::new(state.runtime.cols, state.runtime.rows);
    grid.clear(&state.observed.default_face);
    let (result, _) = render_pipeline(state, &registry.view(), &mut grid);
    (grid, result)
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

// ---------------------------------------------------------------------------
// TestSurfaceBuilder — unified mock for Surface in tests
// ---------------------------------------------------------------------------

/// Builder for test Surface implementations.
///
/// Replaces per-module test surface structs with a single configurable mock.
/// Default key: `"test.surface.{id}"`.
pub struct TestSurfaceBuilder {
    id: SurfaceId,
    key: Option<CompactString>,
    slots: Vec<SlotDeclaration>,
    size_hint: SizeHint,
    initial_placement: Option<SurfacePlacementRequest>,
    root: Option<Element>,
    event_flag: Option<DirtyFlags>,
    state_changed_flag: Option<DirtyFlags>,
}

impl TestSurfaceBuilder {
    pub fn new(id: SurfaceId) -> Self {
        Self {
            id,
            key: None,
            slots: vec![],
            size_hint: SizeHint::fill(),
            initial_placement: None,
            root: None,
            event_flag: None,
            state_changed_flag: None,
        }
    }

    pub fn key(mut self, key: impl Into<CompactString>) -> Self {
        self.key = Some(key.into());
        self
    }

    pub fn slots(mut self, slots: Vec<SlotDeclaration>) -> Self {
        self.slots = slots;
        self
    }

    pub fn size_hint(mut self, hint: SizeHint) -> Self {
        self.size_hint = hint;
        self
    }

    pub fn initial_placement(mut self, req: SurfacePlacementRequest) -> Self {
        self.initial_placement = Some(req);
        self
    }

    pub fn root(mut self, element: Element) -> Self {
        self.root = Some(element);
        self
    }

    pub fn on_event(mut self, flag: DirtyFlags) -> Self {
        self.event_flag = Some(flag);
        self
    }

    pub fn on_state_changed(mut self, flag: DirtyFlags) -> Self {
        self.state_changed_flag = Some(flag);
        self
    }

    pub fn build(self) -> Box<dyn Surface> {
        let key = self
            .key
            .unwrap_or_else(|| format!("test.surface.{}", self.id.0).into());
        Box::new(TestSurfaceImpl {
            id: self.id,
            key,
            slots: self.slots,
            size_hint: self.size_hint,
            initial_placement: self.initial_placement,
            root: self.root,
            event_flag: self.event_flag,
            state_changed_flag: self.state_changed_flag,
        })
    }
}

struct TestSurfaceImpl {
    id: SurfaceId,
    key: CompactString,
    slots: Vec<SlotDeclaration>,
    size_hint: SizeHint,
    initial_placement: Option<SurfacePlacementRequest>,
    root: Option<Element>,
    event_flag: Option<DirtyFlags>,
    state_changed_flag: Option<DirtyFlags>,
}

impl Surface for TestSurfaceImpl {
    fn id(&self) -> SurfaceId {
        self.id
    }

    fn surface_key(&self) -> CompactString {
        self.key.clone()
    }

    fn size_hint(&self) -> SizeHint {
        self.size_hint
    }

    fn initial_placement(&self) -> Option<SurfacePlacementRequest> {
        self.initial_placement.clone()
    }

    fn view(&self, _ctx: &ViewContext<'_>) -> Element {
        self.root.clone().unwrap_or(Element::Empty)
    }

    fn handle_event(&mut self, _event: SurfaceEvent, _ctx: &EventContext<'_>) -> Vec<Command> {
        match self.event_flag {
            Some(flag) => vec![Command::RequestRedraw(flag)],
            None => vec![],
        }
    }

    fn on_state_changed(&mut self, _state: &AppState, _dirty: DirtyFlags) -> Vec<Command> {
        match self.state_changed_flag {
            Some(flag) => vec![Command::RequestRedraw(flag)],
            None => vec![],
        }
    }

    fn declared_slots(&self) -> &[SlotDeclaration] {
        &self.slots
    }
}
