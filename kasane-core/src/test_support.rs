//! Shared test utilities available to both unit tests and integration tests.
//!
//! This module is `#[doc(hidden)]` but `pub` so that integration tests
//! (which are separate crates) can access these helpers.
//!
//! ## Wire-format-aware helpers
//!
//! The [`wire`] submodule provides ergonomic constructors for tests
//! that exercise the Kakoune wire-format path — specifically the
//! `Attributes::FINAL_FG` / `FINAL_BG` / `FINAL_ATTR` resolution
//! flags that distinguish "this colour is final, do not inherit" from
//! "this colour will be resolved later" semantics. Production code
//! uses [`Style`](crate::protocol::Style) and never constructs `WireFace`
//! directly; tests that simulate wire input (cursor detection
//! fixtures, protocol-parser fixtures, etc.) belong in `wire`.

use compact_str::CompactString;

use crate::element::Element;
use crate::layout::Rect;
use crate::layout::flex::place;
use crate::plugin::{Command, PluginRuntime};
use crate::protocol::{Atom, Color, Line, NamedColor, WireFace};
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
    state.observed.default_style = WireFace {
        fg: Color::Named(NamedColor::White),
        bg: Color::Named(NamedColor::Black),
        ..WireFace::default()
    }
    .into();
    state.observed.padding_style = state.observed.default_style.clone();
    state.observed.status_default_style = WireFace {
        fg: Color::Named(NamedColor::Cyan),
        bg: Color::Named(NamedColor::Black),
        ..WireFace::default()
    }
    .into();
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
    grid.clear(&crate::render::TerminalStyle::from_style(
        &state.observed.default_style,
    ));
    paint::paint(&element, &layout, &mut grid, state);
    grid
}

/// Render to a fresh CellGrid using the non-cached pipeline.
pub fn render_to_grid(state: &AppState, registry: &PluginRuntime) -> CellGrid {
    let mut grid = CellGrid::new(state.runtime.cols, state.runtime.rows);
    grid.clear(&crate::render::TerminalStyle::from_style(
        &state.observed.default_style,
    ));
    render_pipeline(state, &registry.view(), &mut grid);
    grid
}

/// Render to a fresh CellGrid and return the RenderResult (cursor position, style, etc.).
pub fn render_to_grid_with_result(
    state: &AppState,
    registry: &PluginRuntime,
) -> (CellGrid, crate::render::RenderResult) {
    let mut grid = CellGrid::new(state.runtime.cols, state.runtime.rows);
    grid.clear(&crate::render::TerminalStyle::from_style(
        &state.observed.default_style,
    ));
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

// ---------------------------------------------------------------------------
// Wire-format-aware WireFace constructors
// ---------------------------------------------------------------------------

/// Wire-format-aware [`WireFace`](crate::protocol::WireFace) constructors for tests.
///
/// Production code uses [`Style`](crate::protocol::Style); the only
/// remaining legitimate `WireFace` callers are the Kakoune wire-format
/// parser (which receives `WireFace` over JSON-RPC) and tests that
/// simulate that parser's input.
///
/// The helpers here name the wire-format patterns that would
/// otherwise require multi-line `WireFace { fg, attributes:
/// Attributes::FINAL_FG | ..., ..WireFace::default() }` literal struct
/// updates at every call site.
///
/// `cursor_atom`, `cursor_text`, and the `_with_final_*` variants
/// preserve the resolution flags that
/// [`detect_cursors`](crate::state::derived::cursor::detect_cursors)
/// reads to identify cursor atoms — see
/// `kasane-core/src/state/derived/cursor.rs` and the closure rationale
/// in `project_adr_031_phase_b3_semantic_split.md` (memory).
pub mod wire {
    use crate::protocol::{Atom, Attributes, Color, NamedColor, WireFace};

    /// Plain default face. Equivalent to `WireFace::default()` but named
    /// for symmetry with the other constructors below.
    #[inline]
    pub fn default_face() -> WireFace {
        WireFace::default()
    }

    /// `WireFace { fg, ..default }` — the simplest wire-format face.
    pub fn face_with_fg(fg: Color) -> WireFace {
        WireFace {
            fg,
            ..WireFace::default()
        }
    }

    /// `WireFace { bg, ..default }` — background-only face.
    pub fn face_with_bg(bg: Color) -> WireFace {
        WireFace {
            bg,
            ..WireFace::default()
        }
    }

    /// `WireFace { fg, bg, ..default }`.
    pub fn face_with_fg_bg(fg: Color, bg: Color) -> WireFace {
        WireFace {
            fg,
            bg,
            ..WireFace::default()
        }
    }

    /// WireFace carrying the `FINAL_FG` resolution flag — instructs the
    /// inheritance resolver to treat `fg` as final and skip parent
    /// lookup. Used by Kakoune for theme-resolved foreground that
    /// should not be re-inherited downstream, and by
    /// [`detect_cursors`](crate::state::derived::cursor::detect_cursors)
    /// fixtures to mark a cell as a cursor.
    pub fn face_with_final_fg(fg: Color) -> WireFace {
        WireFace {
            fg,
            attributes: Attributes::FINAL_FG,
            ..WireFace::default()
        }
    }

    /// WireFace with the `FINAL_BG` resolution flag set.
    pub fn face_with_final_bg(bg: Color) -> WireFace {
        WireFace {
            bg,
            attributes: Attributes::FINAL_BG,
            ..WireFace::default()
        }
    }

    /// WireFace with both `FINAL_FG` and `FINAL_BG` set — typical for a
    /// theme-resolved `default_face` arriving over the wire.
    pub fn face_with_final_fg_bg(fg: Color, bg: Color) -> WireFace {
        WireFace {
            fg,
            bg,
            attributes: Attributes::FINAL_FG | Attributes::FINAL_BG,
            ..WireFace::default()
        }
    }

    /// WireFace with arbitrary attribute bits set on top of fg/bg. Use
    /// when a fixture needs e.g. `REVERSE | FINAL_FG`.
    pub fn face_with_attrs(fg: Color, bg: Color, attributes: Attributes) -> WireFace {
        WireFace {
            fg,
            bg,
            attributes,
            ..WireFace::default()
        }
    }

    /// Atom carrying a cursor-marker face (`FINAL_FG` set, white-on-
    /// black). Mirrors the wire-format shape that
    /// `detect_cursors` recognises.
    pub fn cursor_atom(text: impl Into<String>) -> Atom {
        Atom::from_wire(
            face_with_final_fg(Color::Named(NamedColor::White)),
            text.into(),
        )
    }

    /// Atom with the supplied wire-format face.
    pub fn atom_with_face(face: WireFace, text: impl Into<String>) -> Atom {
        Atom::from_wire(face, text.into())
    }

    #[cfg(test)]
    mod tests {
        use super::*;

        #[test]
        fn default_face_equals_face_default() {
            assert_eq!(default_face(), WireFace::default());
        }

        #[test]
        fn face_with_fg_sets_only_fg() {
            let f = face_with_fg(Color::Named(NamedColor::Red));
            assert_eq!(f.fg, Color::Named(NamedColor::Red));
            assert_eq!(f.bg, Color::Default);
            assert!(f.attributes.is_empty());
        }

        #[test]
        fn face_with_final_fg_sets_resolution_flag() {
            let f = face_with_final_fg(Color::Named(NamedColor::Cyan));
            assert!(f.attributes.contains(Attributes::FINAL_FG));
            assert!(!f.attributes.contains(Attributes::FINAL_BG));
        }

        #[test]
        fn face_with_final_fg_bg_sets_both_flags() {
            let f = face_with_final_fg_bg(
                Color::Named(NamedColor::White),
                Color::Named(NamedColor::Black),
            );
            assert!(f.attributes.contains(Attributes::FINAL_FG));
            assert!(f.attributes.contains(Attributes::FINAL_BG));
            assert_eq!(f.fg, Color::Named(NamedColor::White));
            assert_eq!(f.bg, Color::Named(NamedColor::Black));
        }

        #[test]
        fn cursor_atom_is_detected_by_detect_cursors_logic() {
            // The contract: `detect_cursors` keys on FINAL_FG. The
            // helper produces an atom whose face has FINAL_FG set, so
            // any test fixture using `cursor_atom` will be picked up
            // by the cursor-detection pipeline.
            let atom = cursor_atom("X");
            assert!(
                atom.unresolved_style()
                    .to_face()
                    .attributes
                    .contains(Attributes::FINAL_FG)
            );
        }

        #[test]
        fn face_with_attrs_combines_inputs() {
            let f = face_with_attrs(
                Color::Named(NamedColor::Red),
                Color::Default,
                Attributes::REVERSE | Attributes::FINAL_FG,
            );
            assert_eq!(f.fg, Color::Named(NamedColor::Red));
            assert!(f.attributes.contains(Attributes::REVERSE));
            assert!(f.attributes.contains(Attributes::FINAL_FG));
        }
    }
}
