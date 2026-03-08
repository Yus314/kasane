mod fixtures;

use criterion::{BenchmarkId, Criterion, criterion_group, criterion_main};
use kasane_core::layout::Rect;
use kasane_core::layout::flex;
use kasane_core::plugin::{DecorateTarget, PluginRegistry, Slot};
use kasane_core::protocol::parse_request;
use kasane_core::render::CellGrid;
use kasane_core::render::paint;
use kasane_core::render::view;

use fixtures::{
    draw_json, draw_request, draw_status_json, menu_show_json, registry_with_plugins,
    set_cursor_json, state_with_menu, typical_state,
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
            b.iter(|| registry.apply_decorator(DecorateTarget::Buffer, element.clone(), &state));
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
// Extended benchmarks
// ---------------------------------------------------------------------------

/// Bench: JSON-RPC parse_request at various message sizes
fn bench_parse_request(c: &mut Criterion) {
    let mut group = c.benchmark_group("parse_request");

    // Draw messages: 10, 100, 500 lines
    for line_count in [10, 100, 500] {
        let json = draw_json(line_count);
        group.bench_with_input(
            BenchmarkId::new("draw_lines", line_count),
            &json,
            |b, json| {
                b.iter(|| {
                    let mut buf = json.clone();
                    parse_request(&mut buf).unwrap()
                })
            },
        );
    }

    // draw_status (small, high-frequency message)
    let json = draw_status_json();
    group.bench_function("draw_status", |b| {
        b.iter(|| {
            let mut buf = json.clone();
            parse_request(&mut buf).unwrap()
        })
    });

    // set_cursor (minimal message)
    let json = set_cursor_json();
    group.bench_function("set_cursor", |b| {
        b.iter(|| {
            let mut buf = json.clone();
            parse_request(&mut buf).unwrap()
        })
    });

    // menu_show with 50 items
    let json = menu_show_json(50);
    group.bench_function("menu_show_50", |b| {
        b.iter(|| {
            let mut buf = json.clone();
            parse_request(&mut buf).unwrap()
        })
    });

    group.finish();
}

/// Bench: state.apply() isolated from rendering
fn bench_state_apply(c: &mut Criterion) {
    let mut group = c.benchmark_group("state_apply");

    // Draw at various sizes
    for line_count in [23, 100, 500] {
        let draw = draw_request(line_count);
        let base_state = typical_state(23);
        group.bench_with_input(
            BenchmarkId::new("draw_lines", line_count),
            &line_count,
            |b, _| {
                b.iter(|| {
                    let mut state = base_state.clone();
                    state.apply(draw.clone())
                })
            },
        );
    }

    // DrawStatus
    {
        let request = kasane_core::protocol::KakouneRequest::DrawStatus {
            status_line: vec![kasane_core::protocol::Atom {
                face: kasane_core::protocol::Face::default(),
                contents: " NORMAL ".to_string(),
            }],
            mode_line: vec![kasane_core::protocol::Atom {
                face: kasane_core::protocol::Face::default(),
                contents: "normal".to_string(),
            }],
            default_face: kasane_core::protocol::Face::default(),
        };
        let base_state = typical_state(23);
        group.bench_function("draw_status", |b| {
            b.iter(|| {
                let mut state = base_state.clone();
                state.apply(request.clone())
            })
        });
    }

    // SetCursor
    {
        let request = kasane_core::protocol::KakouneRequest::SetCursor {
            mode: kasane_core::protocol::CursorMode::Buffer,
            coord: kasane_core::protocol::Coord {
                line: 5,
                column: 10,
            },
        };
        let base_state = typical_state(23);
        group.bench_function("set_cursor", |b| {
            b.iter(|| {
                let mut state = base_state.clone();
                state.apply(request.clone())
            })
        });
    }

    // MenuShow 50 items
    {
        let items: Vec<kasane_core::protocol::Line> = (0..50)
            .map(|i| {
                vec![kasane_core::protocol::Atom {
                    face: kasane_core::protocol::Face::default(),
                    contents: format!("completion_{i}"),
                }]
            })
            .collect();
        let request = kasane_core::protocol::KakouneRequest::MenuShow {
            items,
            anchor: kasane_core::protocol::Coord {
                line: 5,
                column: 10,
            },
            selected_item_face: kasane_core::protocol::Face::default(),
            menu_face: kasane_core::protocol::Face::default(),
            style: kasane_core::protocol::MenuStyle::Inline,
        };
        let base_state = typical_state(23);
        group.bench_function("menu_show_50", |b| {
            b.iter(|| {
                let mut state = base_state.clone();
                state.apply(request.clone())
            })
        });
    }

    group.finish();
}

/// Bench: Scaling characteristics for large terminals and buffers
fn bench_scaling(c: &mut Criterion) {
    let mut group = c.benchmark_group("scaling");
    group.sample_size(50);

    let registry = PluginRegistry::new();

    // Full frame at various terminal sizes
    for (cols, rows, lines, label) in [
        (80, 24, 23, "80x24"),
        (200, 60, 59, "200x60"),
        (300, 80, 79, "300x80"),
    ] {
        let mut state = typical_state(lines);
        state.cols = cols;
        state.rows = rows;
        let area = Rect {
            x: 0,
            y: 0,
            w: cols,
            h: rows,
        };
        let mut grid = CellGrid::new(cols, rows);

        group.bench_function(BenchmarkId::new("full_frame", label), |b| {
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

    // Parse + apply for large Draw messages
    for line_count in [500, 1000] {
        let json = draw_json(line_count);
        let base_state = typical_state(23);
        group.bench_function(BenchmarkId::new("parse_apply_draw", line_count), |b| {
            b.iter(|| {
                let mut buf = json.clone();
                let request = parse_request(&mut buf).unwrap();
                let mut state = base_state.clone();
                state.apply(request)
            })
        });
    }

    // diff() at large sizes (incremental — same content, empty diff)
    for (cols, rows, lines, label) in [
        (80, 24, 23, "80x24"),
        (200, 60, 59, "200x60"),
        (300, 80, 79, "300x80"),
    ] {
        let mut state = typical_state(lines);
        state.cols = cols;
        state.rows = rows;
        let area = Rect {
            x: 0,
            y: 0,
            w: cols,
            h: rows,
        };
        let element = view::view(&state, &registry);
        let layout = flex::place(&element, area, &state);
        let mut grid = CellGrid::new(cols, rows);
        // Populate both buffers with the same content
        grid.clear(&state.default_face);
        paint::paint(&element, &layout, &mut grid, &state);
        grid.swap();
        grid.clear(&state.default_face);
        paint::paint(&element, &layout, &mut grid, &state);

        group.bench_function(BenchmarkId::new("diff_incremental", label), |b| {
            b.iter(|| grid.diff());
        });
    }

    group.finish();
}

// ---------------------------------------------------------------------------
// Allocation benchmarks (feature-gated)
// ---------------------------------------------------------------------------

#[cfg(feature = "bench-alloc")]
mod alloc_counter {
    use std::alloc::{GlobalAlloc, Layout, System};
    use std::sync::atomic::{AtomicUsize, Ordering};

    static ALLOC_COUNT: AtomicUsize = AtomicUsize::new(0);
    static ALLOC_BYTES: AtomicUsize = AtomicUsize::new(0);

    pub struct CountingAllocator;

    unsafe impl GlobalAlloc for CountingAllocator {
        unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
            ALLOC_COUNT.fetch_add(1, Ordering::Relaxed);
            ALLOC_BYTES.fetch_add(layout.size(), Ordering::Relaxed);
            unsafe { System.alloc(layout) }
        }

        unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
            unsafe { System.dealloc(ptr, layout) }
        }
    }

    pub fn reset() {
        ALLOC_COUNT.store(0, Ordering::Relaxed);
        ALLOC_BYTES.store(0, Ordering::Relaxed);
    }

    pub fn snapshot() -> (usize, usize) {
        (
            ALLOC_COUNT.load(Ordering::Relaxed),
            ALLOC_BYTES.load(Ordering::Relaxed),
        )
    }
}

#[cfg(feature = "bench-alloc")]
#[global_allocator]
static ALLOC: alloc_counter::CountingAllocator = alloc_counter::CountingAllocator;

#[cfg(feature = "bench-alloc")]
fn bench_allocations(c: &mut Criterion) {
    let mut group = c.benchmark_group("allocations");

    // Full frame allocation count
    {
        let state = typical_state(23);
        let registry = PluginRegistry::new();
        let area = Rect {
            x: 0,
            y: 0,
            w: state.cols,
            h: state.rows,
        };
        let mut grid = CellGrid::new(state.cols, state.rows);

        group.bench_function("full_frame", |b| {
            b.iter(|| {
                alloc_counter::reset();
                let element = view::view(&state, &registry);
                let layout = flex::place(&element, area, &state);
                grid.clear(&state.default_face);
                paint::paint(&element, &layout, &mut grid, &state);
                let _diffs = grid.diff();
                grid.swap();
                alloc_counter::snapshot()
            });
        });
    }

    // Parse request allocation count
    {
        let json = draw_json(100);
        group.bench_function("parse_request", |b| {
            b.iter(|| {
                alloc_counter::reset();
                let mut buf = json.clone();
                let _ = parse_request(&mut buf).unwrap();
                alloc_counter::snapshot()
            });
        });
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

criterion_group!(
    name = extended;
    config = Criterion::default().sample_size(50);
    targets =
        bench_parse_request,
        bench_state_apply,
        bench_scaling,
);

#[cfg(not(feature = "bench-alloc"))]
criterion_main!(micro, integration, extended);

#[cfg(feature = "bench-alloc")]
criterion_group!(alloc_benches, bench_allocations);

#[cfg(feature = "bench-alloc")]
criterion_main!(micro, integration, extended, alloc_benches);
