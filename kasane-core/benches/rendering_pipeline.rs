mod fixtures;

use criterion::{BenchmarkId, Criterion, criterion_group, criterion_main};
use kasane_core::layout::Rect;
use kasane_core::layout::flex;
use kasane_core::plugin::{DecorateTarget, PluginRegistry, Slot};
use kasane_core::render::CellGrid;
use kasane_core::render::paint;
use kasane_core::render::view;

use fixtures::{
    draw_request, registry_with_plugins, state_with_menu, typical_state,
};

// ---------------------------------------------------------------------------
// Micro-benchmarks
// ---------------------------------------------------------------------------

/// Bench 1: Element tree construction via view()
fn bench_element_construct(c: &mut Criterion) {
    let mut group = c.benchmark_group("element_construct");

    let state = typical_state(23);

    // No plugins
    let registry_0 = PluginRegistry::new();
    group.bench_function("plugins_0", |b| {
        b.iter(|| view::view(&state, &registry_0));
    });

    // 10 plugins
    let registry_10 = registry_with_plugins(10);
    group.bench_function("plugins_10", |b| {
        b.iter(|| view::view(&state, &registry_10));
    });

    group.finish();
}

/// Bench 2: Flex layout (place) only
fn bench_flex_layout(c: &mut Criterion) {
    let state = typical_state(23);
    let registry = PluginRegistry::new();
    let element = view::view(&state, &registry);
    let area = Rect {
        x: 0,
        y: 0,
        w: state.cols,
        h: state.rows,
    };

    c.bench_function("flex_layout", |b| {
        b.iter(|| flex::place(&element, area, &state));
    });
}

/// Bench 3: Paint into grid
fn bench_paint(c: &mut Criterion) {
    let mut group = c.benchmark_group("paint");

    // 80x24
    {
        let state = typical_state(23);
        let registry = PluginRegistry::new();
        let element = view::view(&state, &registry);
        let area = Rect {
            x: 0,
            y: 0,
            w: state.cols,
            h: state.rows,
        };
        let layout = flex::place(&element, area, &state);
        let mut grid = CellGrid::new(state.cols, state.rows);

        group.bench_function("80x24", |b| {
            b.iter(|| {
                grid.clear(&state.default_face);
                paint::paint(&element, &layout, &mut grid, &state);
            });
        });
    }

    // 200x60
    {
        let mut state = typical_state(59);
        state.cols = 200;
        state.rows = 60;
        let registry = PluginRegistry::new();
        let element = view::view(&state, &registry);
        let area = Rect {
            x: 0,
            y: 0,
            w: state.cols,
            h: state.rows,
        };
        let layout = flex::place(&element, area, &state);
        let mut grid = CellGrid::new(state.cols, state.rows);

        group.bench_function("200x60", |b| {
            b.iter(|| {
                grid.clear(&state.default_face);
                paint::paint(&element, &layout, &mut grid, &state);
            });
        });
    }

    group.finish();
}

/// Bench 4: Grid diff
fn bench_grid_diff(c: &mut Criterion) {
    let mut group = c.benchmark_group("grid_diff");

    let state = typical_state(23);
    let registry = PluginRegistry::new();
    let element = view::view(&state, &registry);
    let area = Rect {
        x: 0,
        y: 0,
        w: state.cols,
        h: state.rows,
    };
    let layout = flex::place(&element, area, &state);

    // Full redraw (previous is empty)
    {
        let mut grid = CellGrid::new(state.cols, state.rows);
        grid.clear(&state.default_face);
        paint::paint(&element, &layout, &mut grid, &state);

        group.bench_function("full_redraw", |b| {
            b.iter(|| grid.diff());
        });
    }

    // Incremental (previous populated, same content → empty diff)
    {
        let mut grid = CellGrid::new(state.cols, state.rows);
        grid.clear(&state.default_face);
        paint::paint(&element, &layout, &mut grid, &state);
        grid.swap();
        grid.clear(&state.default_face);
        paint::paint(&element, &layout, &mut grid, &state);

        group.bench_function("incremental", |b| {
            b.iter(|| grid.diff());
        });
    }

    group.finish();
}

/// Bench 5: Decorator chain
fn bench_decorator_chain(c: &mut Criterion) {
    let mut group = c.benchmark_group("decorator_chain");

    let state = typical_state(23);
    let element = kasane_core::element::Element::buffer_ref(0..23);

    for n in [1, 5, 10] {
        let registry = registry_with_plugins(n);
        group.bench_with_input(BenchmarkId::new("plugins", n), &n, |b, _| {
            b.iter(|| {
                registry.apply_decorator(DecorateTarget::Buffer, element.clone(), &state)
            });
        });
    }

    group.finish();
}

/// Bench 6: Plugin dispatch (all 8 slots collect + decorator apply)
fn bench_plugin_dispatch(c: &mut Criterion) {
    let mut group = c.benchmark_group("plugin_dispatch");

    let state = typical_state(23);
    let element = kasane_core::element::Element::buffer_ref(0..23);

    let all_slots = [
        Slot::BufferLeft,
        Slot::BufferRight,
        Slot::AboveBuffer,
        Slot::BelowBuffer,
        Slot::AboveStatus,
        Slot::StatusLeft,
        Slot::StatusRight,
        Slot::Overlay,
    ];

    for n in [1, 5, 10] {
        let registry = registry_with_plugins(n);
        group.bench_with_input(BenchmarkId::new("plugins", n), &n, |b, _| {
            b.iter(|| {
                for &slot in &all_slots {
                    let _ = registry.collect_slot(slot, &state);
                }
                registry.apply_decorator(DecorateTarget::Buffer, element.clone(), &state);
            });
        });
    }

    group.finish();
}

// ---------------------------------------------------------------------------
// Integration benchmarks
// ---------------------------------------------------------------------------

/// Bench 7: Full frame pipeline (view → layout → paint → diff → swap), excluding backend I/O
fn bench_full_frame(c: &mut Criterion) {
    let state = typical_state(23);
    let registry = PluginRegistry::new();
    let area = Rect {
        x: 0,
        y: 0,
        w: state.cols,
        h: state.rows,
    };
    let mut grid = CellGrid::new(state.cols, state.rows);

    c.bench_function("full_frame", |b| {
        b.iter(|| {
            let element = view::view(&state, &registry);
            let layout = flex::place(&element, area, &state);
            grid.clear(&state.default_face);
            paint::paint(&element, &layout, &mut grid, &state);
            let _diffs = grid.diff();
            grid.swap();
        });
    });
}

/// Bench 8: Apply Draw message + full frame
fn bench_draw_message(c: &mut Criterion) {
    let registry = PluginRegistry::new();

    c.bench_function("draw_message", |b| {
        let base_state = typical_state(23);
        let draw = draw_request(23);
        let area = Rect {
            x: 0,
            y: 0,
            w: base_state.cols,
            h: base_state.rows,
        };
        let mut grid = CellGrid::new(base_state.cols, base_state.rows);

        b.iter(|| {
            let mut state = base_state.clone();
            state.apply(draw.clone());

            let element = view::view(&state, &registry);
            let layout = flex::place(&element, area, &state);
            grid.clear(&state.default_face);
            paint::paint(&element, &layout, &mut grid, &state);
            let _diffs = grid.diff();
            grid.swap();
        });
    });
}

/// Bench 9: Menu show + full frame
fn bench_menu_show(c: &mut Criterion) {
    let mut group = c.benchmark_group("menu_show");

    let registry = PluginRegistry::new();

    for item_count in [10, 50, 100] {
        group.bench_with_input(
            BenchmarkId::new("items", item_count),
            &item_count,
            |b, &n| {
                let state = state_with_menu(n);
                let area = Rect {
                    x: 0,
                    y: 0,
                    w: state.cols,
                    h: state.rows,
                };
                let mut grid = CellGrid::new(state.cols, state.rows);

                b.iter(|| {
                    let element = view::view(&state, &registry);
                    let layout = flex::place(&element, area, &state);
                    grid.clear(&state.default_face);
                    paint::paint(&element, &layout, &mut grid, &state);
                    let _diffs = grid.diff();
                    grid.swap();
                });
            },
        );
    }

    group.finish();
}

// ---------------------------------------------------------------------------
// Criterion harness
// ---------------------------------------------------------------------------

criterion_group!(
    micro,
    bench_element_construct,
    bench_flex_layout,
    bench_paint,
    bench_grid_diff,
    bench_decorator_chain,
    bench_plugin_dispatch,
);

criterion_group!(
    integration,
    bench_full_frame,
    bench_draw_message,
    bench_menu_show,
);

criterion_main!(micro, integration);
