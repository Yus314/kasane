//! GPU CPU-side benchmarks — no GPU/display server required.
//!
//! Benchmarks the CPU work done in the GUI rendering pipeline:
//! background instance data construction, row hashing, text span building,
//! and color resolution.

use criterion::{Criterion, criterion_group, criterion_main};
use kasane_core::config::ColorsConfig;
use kasane_core::plugin::PluginRegistry;
use kasane_core::protocol::{Atom, Color, Face, NamedColor};
use kasane_core::render::{CellGrid, render_pipeline};
use kasane_core::state::AppState;
use kasane_gui::colors::ColorResolver;
use kasane_gui::gpu::cell_renderer::{build_bg_instances, build_row_spans, compute_row_hash};

/// Build a typical state + rendered grid for benchmarking.
fn setup_grid() -> (CellGrid, ColorResolver) {
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
    state.lines = (0..23)
        .map(|i| {
            vec![
                Atom {
                    face: Face {
                        fg: Color::Rgb {
                            r: 255,
                            g: 100,
                            b: 0,
                        },
                        bg: Color::Default,
                        ..Face::default()
                    },
                    contents: "let".into(),
                },
                Atom {
                    face: Face::default(),
                    contents: " ".into(),
                },
                Atom {
                    face: Face {
                        fg: Color::Rgb {
                            r: 0,
                            g: 200,
                            b: 100,
                        },
                        bg: Color::Default,
                        ..Face::default()
                    },
                    contents: format!("var_{i}").into(),
                },
                Atom {
                    face: Face::default(),
                    contents: " = ".into(),
                },
                Atom {
                    face: Face {
                        fg: Color::Rgb {
                            r: 100,
                            g: 100,
                            b: 255,
                        },
                        bg: Color::Default,
                        ..Face::default()
                    },
                    contents: format!("\"{i}_value\"").into(),
                },
                Atom {
                    face: Face::default(),
                    contents: ";".into(),
                },
            ]
        })
        .collect();
    state.status_line = vec![Atom {
        face: Face::default(),
        contents: " NORMAL ".into(),
    }];
    state.status_mode_line = vec![Atom {
        face: Face::default(),
        contents: "normal".into(),
    }];

    let registry = PluginRegistry::new();
    let mut grid = CellGrid::new(state.cols, state.rows);
    let _ = render_pipeline(&state, &registry, &mut grid);

    let resolver = ColorResolver::from_config(&ColorsConfig::default());
    (grid, resolver)
}

fn bench_bg_instances(c: &mut Criterion) {
    let (grid, resolver) = setup_grid();
    let cell_w = 8.0_f32;
    let cell_h = 16.0_f32;
    let mut out = Vec::with_capacity(80 * 24 * 8);

    c.bench_function("gpu/bg_instances_80x24", |b| {
        b.iter(|| {
            out.clear();
            build_bg_instances(&grid, &resolver, cell_w, cell_h, None, &mut out);
            out.len()
        });
    });
}

fn bench_row_hash(c: &mut Criterion) {
    let (grid, resolver) = setup_grid();

    c.bench_function("gpu/row_hash_24rows", |b| {
        b.iter(|| {
            let mut total = 0u64;
            for row in 0..grid.height() {
                total = total.wrapping_add(compute_row_hash(&grid, row, &resolver));
            }
            total
        });
    });
}

fn bench_row_spans(c: &mut Criterion) {
    let (grid, resolver) = setup_grid();
    let mut row_text = String::with_capacity(256);
    let mut span_ranges = Vec::with_capacity(128);

    c.bench_function("gpu/row_spans_80cols", |b| {
        b.iter(|| {
            build_row_spans(&grid, 5, &resolver, &mut row_text, &mut span_ranges);
            span_ranges.len()
        });
    });
}

fn bench_color_resolve(c: &mut Criterion) {
    let (grid, resolver) = setup_grid();

    c.bench_function("gpu/color_resolve_1920cells", |b| {
        b.iter(|| {
            let mut sum = 0.0_f32;
            for row in 0..grid.height() {
                for col in 0..grid.width() {
                    let cell = grid.get(col, row).unwrap();
                    let c = resolver.resolve(cell.face.fg, true);
                    sum += c[0];
                }
            }
            sum
        });
    });
}

criterion_group!(
    gpu_cpu,
    bench_bg_instances,
    bench_row_hash,
    bench_row_spans,
    bench_color_resolve,
);
criterion_main!(gpu_cpu);
