mod fixtures;

use std::time::Instant;

use kasane_core::layout::Rect;
use kasane_core::layout::flex;
use kasane_core::plugin::PluginRuntime;
use kasane_core::render::CellGrid;
use kasane_core::render::paint;
use kasane_core::render::view;

use fixtures::typical_state;

const ITERATIONS: u64 = 10_000;

fn percentile(sorted: &[u64], p: f64) -> u64 {
    let idx = ((sorted.len() as f64 * p).ceil() as usize).min(sorted.len() - 1);
    sorted[idx]
}

fn print_percentiles(name: &str, data: &mut Vec<u64>) {
    data.sort_unstable();
    println!("{name}:");
    println!(
        "  p50:   {:>8.1} us",
        percentile(data, 0.50) as f64 / 1000.0
    );
    println!(
        "  p90:   {:>8.1} us",
        percentile(data, 0.90) as f64 / 1000.0
    );
    println!(
        "  p99:   {:>8.1} us",
        percentile(data, 0.99) as f64 / 1000.0
    );
    println!(
        "  p99.9: {:>8.1} us",
        percentile(data, 0.999) as f64 / 1000.0
    );
    println!("  max:   {:>8.1} us", *data.last().unwrap() as f64 / 1000.0);
    println!();
}

fn main() {
    let state = typical_state(23);
    let registry = PluginRuntime::new();
    let area = Rect {
        x: 0,
        y: 0,
        w: state.runtime.cols,
        h: state.runtime.rows,
    };
    let mut grid = CellGrid::new(state.runtime.cols, state.runtime.rows);

    // Warmup
    for _ in 0..100 {
        let element = view::view(&state, &registry.view());
        let layout = flex::place(&element, area, &state);
        grid.clear(&kasane_core::render::TerminalStyle::from_style(
            &state.observed.default_style,
        ));
        paint::paint(&element, &layout, &mut grid, &state);
        let _ = grid.diff();
        grid.swap();
    }

    // --- Full frame latency ---
    let mut data_full = Vec::<u64>::with_capacity(ITERATIONS as usize);
    for _ in 0..ITERATIONS {
        let start = Instant::now();
        let element = view::view(&state, &registry.view());
        let layout = flex::place(&element, area, &state);
        grid.clear(&kasane_core::render::TerminalStyle::from_style(
            &state.observed.default_style,
        ));
        paint::paint(&element, &layout, &mut grid, &state);
        let _ = grid.diff();
        grid.swap();
        let elapsed = start.elapsed().as_nanos() as u64;
        data_full.push(elapsed);
    }
    print_percentiles(
        &format!("Full frame latency distribution ({ITERATIONS} iterations)"),
        &mut data_full,
    );

    // --- Per-phase latency ---

    // view
    let mut data_view = Vec::<u64>::with_capacity(ITERATIONS as usize);
    for _ in 0..ITERATIONS {
        let start = Instant::now();
        let _ = view::view(&state, &registry.view());
        let elapsed = start.elapsed().as_nanos() as u64;
        data_view.push(elapsed);
    }
    print_percentiles("view() latency", &mut data_view);

    // place
    let element = view::view(&state, &registry.view());
    let mut data_place = Vec::<u64>::with_capacity(ITERATIONS as usize);
    for _ in 0..ITERATIONS {
        let start = Instant::now();
        let _ = flex::place(&element, area, &state);
        let elapsed = start.elapsed().as_nanos() as u64;
        data_place.push(elapsed);
    }
    print_percentiles("place() latency", &mut data_place);

    // paint
    let layout = flex::place(&element, area, &state);
    let mut data_paint = Vec::<u64>::with_capacity(ITERATIONS as usize);
    for _ in 0..ITERATIONS {
        let start = Instant::now();
        grid.clear(&kasane_core::render::TerminalStyle::from_style(
            &state.observed.default_style,
        ));
        paint::paint(&element, &layout, &mut grid, &state);
        let elapsed = start.elapsed().as_nanos() as u64;
        data_paint.push(elapsed);
    }
    print_percentiles("paint() latency", &mut data_paint);

    // diff
    // Re-render to get a fresh grid for diffing
    grid.clear(&kasane_core::render::TerminalStyle::from_style(
        &state.observed.default_style,
    ));
    paint::paint(&element, &layout, &mut grid, &state);
    let mut data_diff = Vec::<u64>::with_capacity(ITERATIONS as usize);
    for _ in 0..ITERATIONS {
        let start = Instant::now();
        let _ = grid.diff();
        let elapsed = start.elapsed().as_nanos() as u64;
        data_diff.push(elapsed);
    }
    print_percentiles("diff() latency", &mut data_diff);
}
