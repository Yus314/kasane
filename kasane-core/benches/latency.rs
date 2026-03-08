mod fixtures;

use std::time::Instant;

use hdrhistogram::Histogram;
use kasane_core::layout::Rect;
use kasane_core::layout::flex;
use kasane_core::plugin::PluginRegistry;
use kasane_core::render::CellGrid;
use kasane_core::render::paint;
use kasane_core::render::view;

use fixtures::typical_state;

const ITERATIONS: u64 = 10_000;

fn print_histogram(name: &str, hist: &Histogram<u64>) {
    println!("{name}:");
    println!(
        "  p50:   {:>8.1} us",
        hist.value_at_quantile(0.50) as f64 / 1000.0
    );
    println!(
        "  p90:   {:>8.1} us",
        hist.value_at_quantile(0.90) as f64 / 1000.0
    );
    println!(
        "  p99:   {:>8.1} us",
        hist.value_at_quantile(0.99) as f64 / 1000.0
    );
    println!(
        "  p99.9: {:>8.1} us",
        hist.value_at_quantile(0.999) as f64 / 1000.0
    );
    println!("  max:   {:>8.1} us", hist.max() as f64 / 1000.0);
    println!();
}

fn main() {
    let state = typical_state(23);
    let registry = PluginRegistry::new();
    let area = Rect {
        x: 0,
        y: 0,
        w: state.cols,
        h: state.rows,
    };
    let mut grid = CellGrid::new(state.cols, state.rows);

    // Warmup
    for _ in 0..100 {
        let element = view::view(&state, &registry);
        let layout = flex::place(&element, area, &state);
        grid.clear(&state.default_face);
        paint::paint(&element, &layout, &mut grid, &state);
        let _ = grid.diff();
        grid.swap();
    }

    // --- Full frame latency ---
    let mut hist_full = Histogram::<u64>::new(3).unwrap();
    for _ in 0..ITERATIONS {
        let start = Instant::now();
        let element = view::view(&state, &registry);
        let layout = flex::place(&element, area, &state);
        grid.clear(&state.default_face);
        paint::paint(&element, &layout, &mut grid, &state);
        let _ = grid.diff();
        grid.swap();
        let elapsed = start.elapsed().as_nanos() as u64;
        let _ = hist_full.record(elapsed);
    }
    print_histogram(
        &format!("Full frame latency distribution ({ITERATIONS} iterations)"),
        &hist_full,
    );

    // --- Per-phase latency ---

    // view
    let mut hist_view = Histogram::<u64>::new(3).unwrap();
    for _ in 0..ITERATIONS {
        let start = Instant::now();
        let _ = view::view(&state, &registry);
        let elapsed = start.elapsed().as_nanos() as u64;
        let _ = hist_view.record(elapsed);
    }
    print_histogram("view() latency", &hist_view);

    // place
    let element = view::view(&state, &registry);
    let mut hist_place = Histogram::<u64>::new(3).unwrap();
    for _ in 0..ITERATIONS {
        let start = Instant::now();
        let _ = flex::place(&element, area, &state);
        let elapsed = start.elapsed().as_nanos() as u64;
        let _ = hist_place.record(elapsed);
    }
    print_histogram("place() latency", &hist_place);

    // paint
    let layout = flex::place(&element, area, &state);
    let mut hist_paint = Histogram::<u64>::new(3).unwrap();
    for _ in 0..ITERATIONS {
        let start = Instant::now();
        grid.clear(&state.default_face);
        paint::paint(&element, &layout, &mut grid, &state);
        let elapsed = start.elapsed().as_nanos() as u64;
        let _ = hist_paint.record(elapsed);
    }
    print_histogram("paint() latency", &hist_paint);

    // diff
    // Re-render to get a fresh grid for diffing
    grid.clear(&state.default_face);
    paint::paint(&element, &layout, &mut grid, &state);
    let mut hist_diff = Histogram::<u64>::new(3).unwrap();
    for _ in 0..ITERATIONS {
        let start = Instant::now();
        let _ = grid.diff();
        let elapsed = start.elapsed().as_nanos() as u64;
        let _ = hist_diff.record(elapsed);
    }
    print_histogram("diff() latency", &hist_diff);
}
