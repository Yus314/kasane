use crossterm::{
    cursor, queue,
    style::{self, Attribute as CtAttribute, SetAttribute},
    terminal::{BeginSynchronizedUpdate, EndSynchronizedUpdate},
};
use iai_callgrind::{
    Callgrind, EventKind, LibraryBenchmarkConfig, library_benchmark, library_benchmark_group, main,
};
use kasane_core::layout::Rect;
use kasane_core::layout::flex;
use kasane_core::plugin::PluginRuntime;
use kasane_core::protocol::{Atom, Color, NamedColor, WireFace};
use kasane_core::render::paint;
use kasane_core::render::view;
use kasane_core::render::{Cell, CellGrid, CursorStyle, RenderResult, TerminalStyle};
use kasane_tui::sgr::emit_sgr_diff_style;

// ---------------------------------------------------------------------------
// MockBackend — mirrors TuiBackend's diff+escape logic, writes to Vec<u8>
// ---------------------------------------------------------------------------

struct MockBackend {
    buf: Vec<u8>,
    previous: Vec<Cell>,
}

impl MockBackend {
    fn new() -> Self {
        Self {
            buf: Vec::with_capacity(1 << 16),
            previous: Vec::new(),
        }
    }

    fn present(&mut self, grid: &mut CellGrid, result: RenderResult) {
        queue!(self.buf, BeginSynchronizedUpdate, cursor::Hide).unwrap();

        let cells = grid.cells();
        let dirty_rows = grid.dirty_rows();
        let w = grid.width() as usize;
        let full_redraw = self.previous.is_empty();

        let mut last_style: Option<TerminalStyle> = None;
        let mut last_x: u16 = u16::MAX;
        let mut last_y: u16 = u16::MAX;

        for row in 0..grid.height() as usize {
            if !full_redraw && !dirty_rows[row] {
                continue;
            }
            let row_start = row * w;
            let row_end = row_start + w;
            for i in row_start..row_end {
                let cell = &cells[i];
                if cell.width == 0 {
                    continue;
                }
                if !full_redraw && *cell == self.previous[i] {
                    continue;
                }

                let x = (i % w) as u16;
                let y = row as u16;

                let expected_x = if last_y == y { last_x } else { u16::MAX };
                if x != expected_x {
                    queue!(self.buf, cursor::MoveTo(x, y)).unwrap();
                }

                let style = &cell.style;
                if last_style.as_ref() != Some(style) {
                    emit_sgr_diff_style(&mut self.buf, last_style.as_ref(), style).unwrap();
                    last_style = Some(*style);
                }

                let s = if cell.grapheme.is_empty() {
                    " "
                } else {
                    &cell.grapheme
                };
                queue!(self.buf, style::Print(s)).unwrap();

                last_x = x + cell.width.max(1) as u16;
                last_y = y;
            }
        }

        queue!(self.buf, SetAttribute(CtAttribute::Reset)).unwrap();

        let ct_style = match result.cursor_style {
            CursorStyle::Block => cursor::SetCursorStyle::SteadyBlock,
            CursorStyle::Bar => cursor::SetCursorStyle::SteadyBar,
            CursorStyle::Underline => cursor::SetCursorStyle::SteadyUnderScore,
            CursorStyle::Outline => cursor::SetCursorStyle::DefaultUserShape,
        };
        queue!(
            self.buf,
            cursor::MoveTo(result.cursor_x, result.cursor_y),
            ct_style,
            cursor::Show
        )
        .unwrap();

        queue!(self.buf, EndSynchronizedUpdate).unwrap();

        // Update previous
        let size = w * grid.height() as usize;
        if self.previous.len() != size {
            self.previous = cells.to_vec();
        } else {
            for y in 0..grid.height() as usize {
                if dirty_rows[y] {
                    let start = y * w;
                    let end = start + w;
                    self.previous[start..end].clone_from_slice(&cells[start..end]);
                }
            }
        }
        grid.clear_dirty();
    }

    fn flush(&mut self) {
        self.buf.clear();
    }
}

fn default_result() -> RenderResult {
    RenderResult {
        cursor_x: 0,
        cursor_y: 0,
        cursor_style: CursorStyle::Block,
        cursor_color: Color::Default,
        cursor_blink: None,
        cursor_movement: None,
        display_scroll_offset: 0,
        visual_hints: Default::default(),
    }
}

// ---------------------------------------------------------------------------
// Fixture helpers
// ---------------------------------------------------------------------------

fn make_colored_line(i: usize) -> Vec<Atom> {
    let keyword_face = WireFace {
        fg: Color::Rgb {
            r: 255,
            g: 100,
            b: 0,
        },
        bg: Color::Default,
        ..WireFace::default()
    };
    let ident_face = WireFace {
        fg: Color::Rgb {
            r: 0,
            g: 200,
            b: 100,
        },
        bg: Color::Default,
        ..WireFace::default()
    };
    let literal_face = WireFace {
        fg: Color::Rgb {
            r: 100,
            g: 100,
            b: 255,
        },
        bg: Color::Default,
        ..WireFace::default()
    };
    let plain_face = WireFace::default();

    vec![
        Atom::from_wire(keyword_face, "let"),
        Atom::from_wire(plain_face, " "),
        Atom::from_wire(ident_face, format!("var_{i}")),
        Atom::from_wire(plain_face, " = "),
        Atom::from_wire(literal_face, format!("\"{i}_value\"")),
        Atom::from_wire(plain_face, ";"),
    ]
}

fn typical_state(line_count: usize) -> kasane_core::state::AppState {
    let mut state = kasane_core::state::AppState::default();
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
    state.observed.lines = (0..line_count).map(make_colored_line).collect();
    state.inference.status_line = vec![Atom::from_wire(WireFace::default(), " NORMAL ")];
    state.observed.status_mode_line = vec![Atom::from_wire(WireFace::default(), "normal")];
    state
}

fn generate_grid(cols: u16, rows: u16, line_count: usize) -> CellGrid {
    let mut state = typical_state(line_count);
    state.runtime.cols = cols;
    state.runtime.rows = rows;
    let registry = PluginRuntime::new();
    let area = Rect {
        x: 0,
        y: 0,
        w: cols,
        h: rows,
    };
    let element = view::view(&state, &registry.view());
    let layout = flex::place(&element, area, &state);
    let mut grid = CellGrid::new(cols, rows);
    grid.clear(&kasane_core::render::TerminalStyle::from_style(
        &state.observed.default_style,
    ));
    paint::paint(&element, &layout, &mut grid, &state);
    grid
}

fn generate_incremental_grid() -> CellGrid {
    let state = typical_state(23);
    let registry = PluginRuntime::new();
    let area = Rect {
        x: 0,
        y: 0,
        w: state.runtime.cols,
        h: state.runtime.rows,
    };

    let mut grid = CellGrid::new(state.runtime.cols, state.runtime.rows);

    // "before" frame
    let element = view::view(&state, &registry.view());
    let layout = flex::place(&element, area, &state);
    grid.clear(&kasane_core::render::TerminalStyle::from_style(
        &state.observed.default_style,
    ));
    paint::paint(&element, &layout, &mut grid, &state);
    grid.swap();

    // "after": modify 1 line
    let mut edited = state.clone();
    edited.observed.lines[10] = vec![
        Atom::from_wire(
            WireFace {
                fg: Color::Rgb { r: 255, g: 0, b: 0 },
                bg: Color::Default,
                ..WireFace::default()
            },
            "edited_line_10",
        ),
        Atom::from_wire(WireFace::default(), " // modified"),
    ];

    let element = view::view(&edited, &registry.view());
    let layout = flex::place(&element, area, &edited);
    grid.clear(&kasane_core::render::TerminalStyle::from_style(
        &edited.observed.default_style,
    ));
    paint::paint(&element, &layout, &mut grid, &edited);
    grid
}

// ---------------------------------------------------------------------------
// Setup helpers (called outside measurement)
// ---------------------------------------------------------------------------

fn setup_present_full_redraw() -> (CellGrid, MockBackend) {
    let mut grid = generate_grid(80, 24, 23);
    grid.mark_all_dirty();
    let backend = MockBackend::new();
    (grid, backend)
}

fn setup_present_incremental() -> (CellGrid, MockBackend) {
    let grid = generate_incremental_grid();
    // Seed `previous` in the backend from a full redraw
    let mut seed_grid = generate_grid(80, 24, 23);
    seed_grid.mark_all_dirty();
    let mut backend = MockBackend::new();
    backend.present(&mut seed_grid, default_result());
    backend.flush();
    (grid, backend)
}

// ---------------------------------------------------------------------------
// Regression config
// ---------------------------------------------------------------------------

fn regression_config() -> LibraryBenchmarkConfig {
    let mut config = LibraryBenchmarkConfig::default();
    config.tool(Callgrind::default().soft_limits([(EventKind::Ir, 5.0)]));
    config
}

// ---------------------------------------------------------------------------
// Benchmarks
// ---------------------------------------------------------------------------

// present() full redraw: diff + SGR escape generation + update_previous (80x24)
#[library_benchmark(config = regression_config())]
#[bench::default(setup_present_full_redraw())]
fn iai_present_full_redraw((mut grid, mut backend): (CellGrid, MockBackend)) {
    backend.present(&mut grid, default_result());
}

// present() incremental: 1-line change diff + SGR + update_previous (80x24)
#[library_benchmark(config = regression_config())]
#[bench::default(setup_present_incremental())]
fn iai_present_incremental((mut grid, mut backend): (CellGrid, MockBackend)) {
    backend.present(&mut grid, default_result());
}

// ---------------------------------------------------------------------------
// Harness
// ---------------------------------------------------------------------------

library_benchmark_group!(
    name = iai_backend;
    benchmarks =
        iai_present_full_redraw,
        iai_present_incremental
);

main!(library_benchmark_groups = iai_backend);
