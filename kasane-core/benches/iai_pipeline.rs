mod fixtures;

use iai_callgrind::{
    Callgrind, EventKind, LibraryBenchmarkConfig, library_benchmark, library_benchmark_group, main,
};
use kasane_core::layout::Rect;
use kasane_core::layout::flex;
use kasane_core::plugin::PluginRuntime;
use kasane_core::protocol::parse_request;
use kasane_core::render::CellGrid;
use kasane_core::render::paint;
use kasane_core::render::view;

use fixtures::{draw_json, typical_state};

// ---------------------------------------------------------------------------
// Setup helpers (called outside measurement)
// ---------------------------------------------------------------------------

fn setup_full_frame() -> (kasane_core::state::AppState, PluginRuntime, CellGrid) {
    let state = typical_state(23);
    let registry = PluginRuntime::new();
    let grid = CellGrid::new(state.runtime.cols, state.runtime.rows);
    (state, registry, grid)
}

fn setup_parse_draw_100() -> Vec<u8> {
    draw_json(100)
}

fn setup_state_apply_draw() -> (kasane_core::state::AppState, Vec<u8>) {
    let state = typical_state(23);
    let json = draw_json(23);
    (state, json)
}

fn setup_paint() -> (
    kasane_core::state::AppState,
    kasane_core::element::Element,
    kasane_core::layout::flex::LayoutResult,
    CellGrid,
) {
    let state = typical_state(23);
    let registry = PluginRuntime::new();
    let element = view::view(&state, &registry.view());
    let area = Rect {
        x: 0,
        y: 0,
        w: state.runtime.cols,
        h: state.runtime.rows,
    };
    let layout = flex::place(&element, area, &state);
    let grid = CellGrid::new(state.runtime.cols, state.runtime.rows);
    (state, element, layout, grid)
}

fn setup_grid_diff_full() -> CellGrid {
    let state = typical_state(23);
    let registry = PluginRuntime::new();
    let element = view::view(&state, &registry.view());
    let area = Rect {
        x: 0,
        y: 0,
        w: state.runtime.cols,
        h: state.runtime.rows,
    };
    let layout = flex::place(&element, area, &state);
    let mut grid = CellGrid::new(state.runtime.cols, state.runtime.rows);
    grid.clear(&kasane_core::render::TerminalStyle::from_style(
        &state.observed.default_style,
    ));
    paint::paint(&element, &layout, &mut grid, &state);
    grid
}

fn setup_grid_diff_incremental() -> CellGrid {
    let state = typical_state(23);
    let registry = PluginRuntime::new();
    let element = view::view(&state, &registry.view());
    let area = Rect {
        x: 0,
        y: 0,
        w: state.runtime.cols,
        h: state.runtime.rows,
    };
    let layout = flex::place(&element, area, &state);
    let mut grid = CellGrid::new(state.runtime.cols, state.runtime.rows);
    grid.clear(&kasane_core::render::TerminalStyle::from_style(
        &state.observed.default_style,
    ));
    paint::paint(&element, &layout, &mut grid, &state);
    grid.swap();
    grid.clear(&kasane_core::render::TerminalStyle::from_style(
        &state.observed.default_style,
    ));
    paint::paint(&element, &layout, &mut grid, &state);
    grid
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

// Full pipeline: view -> place -> paint -> diff -> swap (80x24)
#[library_benchmark(config = regression_config())]
#[bench::default(setup_full_frame())]
fn iai_full_frame(
    (state, registry, mut grid): (kasane_core::state::AppState, PluginRuntime, CellGrid),
) {
    let element = view::view(&state, &registry.view());
    let area = Rect {
        x: 0,
        y: 0,
        w: state.runtime.cols,
        h: state.runtime.rows,
    };
    let layout = flex::place(&element, area, &state);
    grid.clear(&kasane_core::render::TerminalStyle::from_style(
        &state.observed.default_style,
    ));
    paint::paint(&element, &layout, &mut grid, &state);
    let _diffs = grid.diff();
    grid.swap();
}

// Parse 100-line draw JSON-RPC message
#[library_benchmark(config = regression_config())]
#[bench::default(setup_parse_draw_100())]
fn iai_parse_draw_100(mut json: Vec<u8>) {
    let _ = parse_request(&mut json).unwrap();
}

// state.apply() for a 23-line draw message
#[library_benchmark(config = regression_config())]
#[bench::default(setup_state_apply_draw())]
fn iai_state_apply_draw((mut state, mut json): (kasane_core::state::AppState, Vec<u8>)) {
    let request = parse_request(&mut json).unwrap();
    state.apply(request);
}

// Paint only (80x24)
#[library_benchmark(config = regression_config())]
#[bench::default(setup_paint())]
fn iai_paint_80x24(
    (state, element, layout, mut grid): (
        kasane_core::state::AppState,
        kasane_core::element::Element,
        kasane_core::layout::flex::LayoutResult,
        CellGrid,
    ),
) {
    grid.clear(&kasane_core::render::TerminalStyle::from_style(
        &state.observed.default_style,
    ));
    paint::paint(&element, &layout, &mut grid, &state);
}

// Grid diff: full redraw (previous buffer empty)
#[library_benchmark(config = regression_config())]
#[bench::default(setup_grid_diff_full())]
fn iai_grid_diff_full(grid: CellGrid) {
    let _ = grid.diff();
}

// Grid diff: incremental (identical content — empty diff)
#[library_benchmark(config = regression_config())]
#[bench::default(setup_grid_diff_incremental())]
fn iai_grid_diff_incremental(grid: CellGrid) {
    let _ = grid.diff();
}

// ---------------------------------------------------------------------------
// Harness
// ---------------------------------------------------------------------------

library_benchmark_group!(
    name = iai_pipeline;
    benchmarks =
        iai_full_frame,
        iai_parse_draw_100,
        iai_state_apply_draw,
        iai_paint_80x24,
        iai_grid_diff_full,
        iai_grid_diff_incremental
);

main!(library_benchmark_groups = iai_pipeline);
