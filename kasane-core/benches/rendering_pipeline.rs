mod fixtures;

use criterion::{BatchSize, BenchmarkId, Criterion, criterion_group, criterion_main};
use kasane_core::layout::Rect;
use kasane_core::layout::flex;
use kasane_core::plugin::PluginRuntime;
use kasane_core::protocol::{Color, NamedColor, parse_request};
use kasane_core::render::CellGrid;
use kasane_core::render::paint;
use kasane_core::render::render_pipeline_direct;
use kasane_core::render::scene::CellSize;
use kasane_core::render::scene_render_pipeline;
use kasane_core::render::view;
use kasane_core::state::DirtyFlags;

use fixtures::{
    draw_json, draw_request, draw_status_json, menu_show_json, realistic_state,
    registry_with_plugins, state_with_edit, state_with_menu, typical_state,
};

// ---------------------------------------------------------------------------
// Micro-benchmarks
// ---------------------------------------------------------------------------

/// Bench 1: Element tree construction via view()
fn bench_element_construct(c: &mut Criterion) {
    let mut group = c.benchmark_group("element_construct");

    let state = typical_state(23);

    // No plugins
    let registry_0 = PluginRuntime::new();
    group.bench_function("plugins_0", |b| {
        b.iter(|| view::view(&state, &registry_0.view()));
    });

    // 10 plugins
    let registry_10 = registry_with_plugins(10);
    group.bench_function("plugins_10", |b| {
        b.iter(|| view::view(&state, &registry_10.view()));
    });

    group.finish();
}

/// Bench 2: Flex layout (place) only
fn bench_flex_layout(c: &mut Criterion) {
    let state = typical_state(23);
    let registry = PluginRuntime::new();
    let element = view::view(&state, &registry.view());
    let area = Rect {
        x: 0,
        y: 0,
        w: state.runtime.cols,
        h: state.runtime.rows,
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

        group.bench_function("80x24", |b| {
            b.iter(|| {
                grid.clear(&kasane_core::render::TerminalStyle::from_style(
                    &state.observed.default_style,
                ));
                paint::paint(&element, &layout, &mut grid, &state);
            });
        });
    }

    // 200x60
    {
        let mut state = typical_state(59);
        state.runtime.cols = 200;
        state.runtime.rows = 60;
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

        group.bench_function("200x60", |b| {
            b.iter(|| {
                grid.clear(&kasane_core::render::TerminalStyle::from_style(
                    &state.observed.default_style,
                ));
                paint::paint(&element, &layout, &mut grid, &state);
            });
        });
    }

    // realistic 80x24 (diverse faces, varied line lengths, wide chars)
    {
        let state = realistic_state(23);
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

        group.bench_function("80x24_realistic", |b| {
            b.iter(|| {
                grid.clear(&kasane_core::render::TerminalStyle::from_style(
                    &state.observed.default_style,
                ));
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
    let registry = PluginRuntime::new();
    let element = view::view(&state, &registry.view());
    let area = Rect {
        x: 0,
        y: 0,
        w: state.runtime.cols,
        h: state.runtime.rows,
    };
    let layout = flex::place(&element, area, &state);

    // Full redraw (previous is empty)
    {
        let mut grid = CellGrid::new(state.runtime.cols, state.runtime.rows);
        grid.clear(&kasane_core::render::TerminalStyle::from_style(
            &state.observed.default_style,
        ));
        paint::paint(&element, &layout, &mut grid, &state);

        group.bench_function("full_redraw", |b| {
            b.iter(|| grid.diff());
        });
    }

    // Incremental (previous populated, same content → empty diff)
    {
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

        group.bench_function("incremental", |b| {
            b.iter(|| grid.diff());
        });
    }

    group.finish();
}

/// Bench: grid.diff_into() — zero-allocation alternative to diff()
fn bench_grid_diff_into(c: &mut Criterion) {
    let mut group = c.benchmark_group("grid_diff_into");

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

    // Full redraw (previous is empty)
    {
        let mut grid = CellGrid::new(state.runtime.cols, state.runtime.rows);
        grid.clear(&kasane_core::render::TerminalStyle::from_style(
            &state.observed.default_style,
        ));
        paint::paint(&element, &layout, &mut grid, &state);

        let mut buf = Vec::with_capacity(state.runtime.cols as usize * state.runtime.rows as usize);
        group.bench_function("full_redraw", |b| {
            b.iter(|| grid.diff_into(&mut buf));
        });
    }

    // Incremental (previous populated, same content → empty diff)
    {
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

        let mut buf = Vec::with_capacity(state.runtime.cols as usize * state.runtime.rows as usize);
        group.bench_function("incremental", |b| {
            b.iter(|| grid.diff_into(&mut buf));
        });
    }

    group.finish();
}

/// Bench: line_dirty optimization with BUFFER|STATUS (P3)
fn bench_line_dirty_buffer_status(c: &mut Criterion) {
    let mut group = c.benchmark_group("line_dirty_buffer_status");

    let mut state = typical_state(23);
    state.observed.status_default_style = state.observed.default_style.clone();
    let registry = PluginRuntime::new();

    // Prepare warm grid (2 frames to get past swap fallback)
    let mut grid = CellGrid::new(state.runtime.cols, state.runtime.rows);
    render_pipeline_direct(&state, &registry.view(), &mut grid, DirtyFlags::ALL);
    grid.swap_with_dirty();
    render_pipeline_direct(&state, &registry.view(), &mut grid, DirtyFlags::ALL);
    grid.swap_with_dirty();

    // Now simulate editing 1 line with BUFFER|STATUS dirty
    let mut edited = state.clone();
    edited.observed.lines[10] = vec![kasane_core::protocol::Atom::from_wire(
        kasane_core::protocol::WireFace::default(),
        "EDITED_LINE",
    )];
    edited.inference.lines_dirty = vec![false; 23];
    edited.inference.lines_dirty[10] = true;

    group.bench_function("1_line_changed", |b| {
        b.iter_batched(
            || {
                // Setup: create a warm grid each iteration
                let mut g = CellGrid::new(state.runtime.cols, state.runtime.rows);
                render_pipeline_direct(&state, &registry.view(), &mut g, DirtyFlags::ALL);
                g.swap_with_dirty();
                render_pipeline_direct(&state, &registry.view(), &mut g, DirtyFlags::ALL);
                g.swap_with_dirty();
                g
            },
            |mut g| {
                render_pipeline_direct(
                    &edited,
                    &registry.view(),
                    &mut g,
                    DirtyFlags::BUFFER | DirtyFlags::STATUS,
                );
            },
            BatchSize::SmallInput,
        );
    });

    group.finish();
}

/// Bench: grid.clear() standalone — isolate O(w×h) clear cost from paint
fn bench_grid_clear(c: &mut Criterion) {
    let mut group = c.benchmark_group("grid_clear");
    let face = kasane_core::protocol::WireFace {
        fg: Color::Named(NamedColor::White),
        bg: Color::Named(NamedColor::Black),
        ..kasane_core::protocol::WireFace::default()
    };
    let term_style = kasane_core::render::TerminalStyle::from_face(&face);

    for (cols, rows, label) in [(80, 24, "80x24"), (200, 60, "200x60")] {
        let mut grid = CellGrid::new(cols, rows);
        group.bench_function(label, |b| {
            b.iter(|| grid.clear(&term_style));
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
    let registry = PluginRuntime::new();
    let area = Rect {
        x: 0,
        y: 0,
        w: state.runtime.cols,
        h: state.runtime.rows,
    };
    let mut grid = CellGrid::new(state.runtime.cols, state.runtime.rows);

    c.bench_function("full_frame", |b| {
        b.iter(|| {
            let element = view::view(&state, &registry.view());
            let layout = flex::place(&element, area, &state);
            grid.clear(&kasane_core::render::TerminalStyle::from_style(
                &state.observed.default_style,
            ));
            paint::paint(&element, &layout, &mut grid, &state);
            let diffs = grid.diff();
            grid.swap();
            diffs.len()
        });
    });
}

/// Bench 8: Apply Draw message + full frame
fn bench_draw_message(c: &mut Criterion) {
    let registry = PluginRuntime::new();

    c.bench_function("draw_message", |b| {
        let base_state = typical_state(23);
        let draw = draw_request(23);
        let area = Rect {
            x: 0,
            y: 0,
            w: base_state.runtime.cols,
            h: base_state.runtime.rows,
        };
        let mut grid = CellGrid::new(base_state.runtime.cols, base_state.runtime.rows);

        b.iter_batched(
            || (base_state.clone(), draw.clone()),
            |(mut state, req)| {
                state.apply(req);
                let element = view::view(&state, &registry.view());
                let layout = flex::place(&element, area, &state);
                grid.clear(&kasane_core::render::TerminalStyle::from_style(
                    &state.observed.default_style,
                ));
                paint::paint(&element, &layout, &mut grid, &state);
                let diffs = grid.diff();
                grid.swap();
                diffs.len()
            },
            BatchSize::SmallInput,
        );
    });
}

/// Bench 9: Menu show + full frame
fn bench_menu_show(c: &mut Criterion) {
    let mut group = c.benchmark_group("menu_show");

    let registry = PluginRuntime::new();

    for item_count in [10, 50, 100] {
        group.bench_with_input(
            BenchmarkId::new("items", item_count),
            &item_count,
            |b, &n| {
                let state = state_with_menu(n);
                let area = Rect {
                    x: 0,
                    y: 0,
                    w: state.runtime.cols,
                    h: state.runtime.rows,
                };
                let mut grid = CellGrid::new(state.runtime.cols, state.runtime.rows);

                b.iter(|| {
                    let element = view::view(&state, &registry.view());
                    let layout = flex::place(&element, area, &state);
                    grid.clear(&kasane_core::render::TerminalStyle::from_style(
                        &state.observed.default_style,
                    ));
                    paint::paint(&element, &layout, &mut grid, &state);
                    let diffs = grid.diff();
                    grid.swap();
                    diffs.len()
                });
            },
        );
    }

    group.finish();
}

/// Bench 10: Incremental edit — most frequent operation pattern
fn bench_incremental_edit(c: &mut Criterion) {
    let mut group = c.benchmark_group("incremental_edit");

    let state = typical_state(23);
    let registry = PluginRuntime::new();
    let area = Rect {
        x: 0,
        y: 0,
        w: state.runtime.cols,
        h: state.runtime.rows,
    };

    for edit_lines in [1, 5] {
        // Render "before" frame into previous buffer
        let mut grid = CellGrid::new(state.runtime.cols, state.runtime.rows);
        let element = view::view(&state, &registry.view());
        let layout = flex::place(&element, area, &state);
        grid.clear(&kasane_core::render::TerminalStyle::from_style(
            &state.observed.default_style,
        ));
        paint::paint(&element, &layout, &mut grid, &state);
        grid.swap();

        // "after" state with edited lines
        let edited_state = state_with_edit(&state, 10, edit_lines);

        group.bench_function(BenchmarkId::new("lines", edit_lines), |b| {
            b.iter(|| {
                // Re-render into current buffer and diff (previous stays fixed — no swap)
                let element = view::view(&edited_state, &registry.view());
                let layout = flex::place(&element, area, &edited_state);
                grid.clear(&kasane_core::render::TerminalStyle::from_style(
                    &edited_state.observed.default_style,
                ));
                paint::paint(&element, &layout, &mut grid, &edited_state);
                grid.diff().len()
            });
        });
    }

    group.finish();
}

/// Bench 11: Realistic message sequence — draw_status + draw → full render
fn bench_message_sequence(c: &mut Criterion) {
    let registry = PluginRuntime::new();
    let draw_status = kasane_core::protocol::KakouneRequest::DrawStatus {
        prompt: vec![kasane_core::protocol::Atom::from_wire(
            kasane_core::protocol::WireFace::default(),
            " INSERT ",
        )],
        content: Vec::new(),
        content_cursor_pos: -1,
        mode_line: vec![kasane_core::protocol::Atom::from_wire(
            kasane_core::protocol::WireFace::default(),
            "insert",
        )],
        default_style: kasane_core::protocol::default_unresolved_style(),
        style: kasane_core::protocol::StatusStyle::Status,
    };
    let draw = draw_request(23);
    let base_state = typical_state(23);
    let area = Rect {
        x: 0,
        y: 0,
        w: base_state.runtime.cols,
        h: base_state.runtime.rows,
    };
    let mut grid = CellGrid::new(base_state.runtime.cols, base_state.runtime.rows);

    c.bench_function("message_sequence", |b| {
        b.iter_batched(
            || (base_state.clone(), draw_status.clone(), draw.clone()),
            |(mut state, msg1, msg2)| {
                state.apply(msg1);
                state.apply(msg2);
                let element = view::view(&state, &registry.view());
                let layout = flex::place(&element, area, &state);
                grid.clear(&kasane_core::render::TerminalStyle::from_style(
                    &state.observed.default_style,
                ));
                paint::paint(&element, &layout, &mut grid, &state);
                let diffs = grid.diff();
                grid.swap();
                diffs.len()
            },
            BatchSize::SmallInput,
        );
    });
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
                b.iter_batched(
                    || json.clone(),
                    |mut buf| parse_request(&mut buf).unwrap(),
                    BatchSize::SmallInput,
                )
            },
        );
    }

    // draw_status (small, high-frequency message)
    let json = draw_status_json();
    group.bench_function("draw_status", |b| {
        b.iter_batched(
            || json.clone(),
            |mut buf| parse_request(&mut buf).unwrap(),
            BatchSize::SmallInput,
        )
    });

    // menu_show with 50 items
    let json = menu_show_json(50);
    group.bench_function("menu_show_50", |b| {
        b.iter_batched(
            || json.clone(),
            |mut buf| parse_request(&mut buf).unwrap(),
            BatchSize::SmallInput,
        )
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
                b.iter_batched(
                    || (base_state.clone(), draw.clone()),
                    |(mut state, req)| state.apply(req),
                    BatchSize::SmallInput,
                )
            },
        );
    }

    // DrawStatus
    {
        let request = kasane_core::protocol::KakouneRequest::DrawStatus {
            prompt: vec![kasane_core::protocol::Atom::from_wire(
                kasane_core::protocol::WireFace::default(),
                " NORMAL ",
            )],
            content: Vec::new(),
            content_cursor_pos: -1,
            mode_line: vec![kasane_core::protocol::Atom::from_wire(
                kasane_core::protocol::WireFace::default(),
                "normal",
            )],
            default_style: kasane_core::protocol::default_unresolved_style(),
            style: kasane_core::protocol::StatusStyle::Status,
        };
        let base_state = typical_state(23);
        group.bench_function("draw_status", |b| {
            b.iter_batched(
                || (base_state.clone(), request.clone()),
                |(mut state, req)| state.apply(req),
                BatchSize::SmallInput,
            )
        });
    }

    // MenuShow 50 items
    {
        let items: Vec<kasane_core::protocol::Line> = (0..50)
            .map(|i| {
                vec![kasane_core::protocol::Atom::from_wire(
                    kasane_core::protocol::WireFace::default(),
                    format!("completion_{i}"),
                )]
            })
            .collect();
        let request = kasane_core::protocol::KakouneRequest::MenuShow {
            items,
            anchor: kasane_core::protocol::Coord {
                line: 5,
                column: 10,
            },
            selected_item_style: kasane_core::protocol::default_unresolved_style(),
            menu_style: kasane_core::protocol::default_unresolved_style(),
            style: kasane_core::protocol::MenuStyle::Inline,
        };
        let base_state = typical_state(23);
        group.bench_function("menu_show_50", |b| {
            b.iter_batched(
                || (base_state.clone(), request.clone()),
                |(mut state, req)| state.apply(req),
                BatchSize::SmallInput,
            )
        });
    }

    group.finish();
}

/// Bench: Scaling characteristics for large terminals and buffers
fn bench_scaling(c: &mut Criterion) {
    let mut group = c.benchmark_group("scaling");
    group.sample_size(50);

    let registry = PluginRuntime::new();

    // Full frame at various terminal sizes
    for (cols, rows, lines, label) in [
        (80, 24, 23, "80x24"),
        (200, 60, 59, "200x60"),
        (300, 80, 79, "300x80"),
    ] {
        let mut state = typical_state(lines);
        state.runtime.cols = cols;
        state.runtime.rows = rows;
        let area = Rect {
            x: 0,
            y: 0,
            w: cols,
            h: rows,
        };
        let mut grid = CellGrid::new(cols, rows);

        group.bench_function(BenchmarkId::new("full_frame", label), |b| {
            b.iter(|| {
                let element = view::view(&state, &registry.view());
                let layout = flex::place(&element, area, &state);
                grid.clear(&kasane_core::render::TerminalStyle::from_style(
                    &state.observed.default_style,
                ));
                paint::paint(&element, &layout, &mut grid, &state);
                let diffs = grid.diff();
                grid.swap();
                diffs.len()
            });
        });
    }

    // Parse + apply for large Draw messages
    for line_count in [500, 1000] {
        let json = draw_json(line_count);
        let base_state = typical_state(23);
        group.bench_function(BenchmarkId::new("parse_apply_draw", line_count), |b| {
            b.iter_batched(
                || (json.clone(), base_state.clone()),
                |(mut buf, mut state)| {
                    let request = parse_request(&mut buf).unwrap();
                    state.apply(request)
                },
                BatchSize::SmallInput,
            )
        });
    }

    // diff() at large sizes (incremental — same content, empty diff)
    for (cols, rows, lines, label) in [
        (80, 24, 23, "80x24"),
        (200, 60, 59, "200x60"),
        (300, 80, 79, "300x80"),
    ] {
        let mut state = typical_state(lines);
        state.runtime.cols = cols;
        state.runtime.rows = rows;
        let area = Rect {
            x: 0,
            y: 0,
            w: cols,
            h: rows,
        };
        let element = view::view(&state, &registry.view());
        let layout = flex::place(&element, area, &state);
        let mut grid = CellGrid::new(cols, rows);
        // Populate both buffers with the same content
        grid.clear(&kasane_core::render::TerminalStyle::from_style(
            &state.observed.default_style,
        ));
        paint::paint(&element, &layout, &mut grid, &state);
        grid.swap();
        grid.clear(&kasane_core::render::TerminalStyle::from_style(
            &state.observed.default_style,
        ));
        paint::paint(&element, &layout, &mut grid, &state);

        group.bench_function(BenchmarkId::new("diff_incremental", label), |b| {
            b.iter(|| grid.diff());
        });
    }

    group.finish();
}

// ---------------------------------------------------------------------------
// View cache benchmarks
// ---------------------------------------------------------------------------

/// Bench: Scene pipeline cold (full pipeline)
fn bench_scene_cache_cold(c: &mut Criterion) {
    let state = typical_state(23);
    let registry = PluginRuntime::new();
    let cs = CellSize {
        width: 10.0,
        height: 20.0,
    };

    c.bench_function("scene_cache_cold", |b| {
        b.iter(|| {
            let (cmds, result, _) = scene_render_pipeline(&state, &registry.view(), cs);
            std::hint::black_box((cmds.len(), result));
        });
    });
}

/// Bench: Scene pipeline (same state, measures steady-state cost)
fn bench_scene_cache_warm(c: &mut Criterion) {
    let state = typical_state(23);
    let registry = PluginRuntime::new();
    let cs = CellSize {
        width: 10.0,
        height: 20.0,
    };

    c.bench_function("scene_cache_warm", |b| {
        b.iter(|| {
            let (cmds, result, _) = scene_render_pipeline(&state, &registry.view(), cs);
            std::hint::black_box((cmds.len(), result));
        });
    });
}

/// Bench: Scene pipeline with menu (measures full pipeline cost with menu state)
fn bench_scene_cache_menu_select(c: &mut Criterion) {
    let state = state_with_menu(50);
    let registry = PluginRuntime::new();
    let cs = CellSize {
        width: 10.0,
        height: 20.0,
    };

    c.bench_function("scene_cache_menu_select", |b| {
        b.iter(|| {
            let (cmds, result, _) = scene_render_pipeline(&state, &registry.view(), cs);
            std::hint::black_box((cmds.len(), result));
        });
    });
}

/// Bench: render_pipeline_direct with ALL vs specific dirty flags
fn bench_cached_pipeline_dirty_flags(c: &mut Criterion) {
    let mut group = c.benchmark_group("cached_pipeline_dirty_flags");

    let state = state_with_menu(50);
    let registry = PluginRuntime::new();

    // ALL dirty (full pipeline)
    group.bench_function("all_dirty", |b| {
        let mut grid = CellGrid::new(state.runtime.cols, state.runtime.rows);
        b.iter(|| {
            render_pipeline_direct(&state, &registry.view(), &mut grid, DirtyFlags::ALL);
        });
    });

    // MENU_SELECTION only
    group.bench_function("menu_select_only", |b| {
        let mut grid = CellGrid::new(state.runtime.cols, state.runtime.rows);
        render_pipeline_direct(&state, &registry.view(), &mut grid, DirtyFlags::ALL);
        grid.swap_with_dirty();
        b.iter(|| {
            render_pipeline_direct(
                &state,
                &registry.view(),
                &mut grid,
                DirtyFlags::MENU_SELECTION,
            );
        });
    });

    group.finish();
}

// ---------------------------------------------------------------------------
// Section-level paint benchmarks (S1)
// ---------------------------------------------------------------------------

/// Bench: Cached pipeline — STATUS only dirty
fn bench_section_paint_status_only(c: &mut Criterion) {
    let state = typical_state(23);
    let registry = PluginRuntime::new();
    let mut grid = CellGrid::new(state.runtime.cols, state.runtime.rows);

    // Initial full render
    render_pipeline_direct(&state, &registry.view(), &mut grid, DirtyFlags::ALL);
    grid.swap();

    c.bench_function("section_paint_status_only", |b| {
        b.iter(|| render_pipeline_direct(&state, &registry.view(), &mut grid, DirtyFlags::STATUS));
    });
}

/// Bench: Cached pipeline — MENU_SELECTION only dirty
fn bench_section_paint_menu_select(c: &mut Criterion) {
    let state = state_with_menu(50);
    let registry = PluginRuntime::new();
    let mut grid = CellGrid::new(state.runtime.cols, state.runtime.rows);

    // Initial full render
    render_pipeline_direct(&state, &registry.view(), &mut grid, DirtyFlags::ALL);
    grid.swap();

    c.bench_function("section_paint_menu_select", |b| {
        b.iter(|| {
            render_pipeline_direct(
                &state,
                &registry.view(),
                &mut grid,
                DirtyFlags::MENU_SELECTION,
            )
        });
    });
}

// ---------------------------------------------------------------------------
// Line-level dirty tracking benchmarks
// ---------------------------------------------------------------------------

/// Bench: Line-dirty single edit — render after 1-line change with swap_with_dirty
fn bench_line_dirty_single_edit(c: &mut Criterion) {
    let state = typical_state(23);
    let registry = PluginRuntime::new();
    let mut grid = CellGrid::new(state.runtime.cols, state.runtime.rows);

    // Initial full render
    render_pipeline_direct(&state, &registry.view(), &mut grid, DirtyFlags::ALL);
    grid.swap();

    // "After" state: edit line 10
    let edited_state = state_with_edit(&state, 10, 1);
    // Simulate apply(Draw) to get lines_dirty
    let mut state_after = state.clone();
    let edited_lines = edited_state.observed.lines.clone();
    state_after.apply(kasane_core::protocol::KakouneRequest::Draw {
        lines: edited_lines,
        cursor_pos: kasane_core::protocol::Coord::default(),
        default_style: std::sync::Arc::new(kasane_core::protocol::UnresolvedStyle {
            style: state.observed.default_style.clone(),
            final_fg: false,
            final_bg: false,
            final_style: false,
        }),
        padding_style: std::sync::Arc::new(kasane_core::protocol::UnresolvedStyle {
            style: state.observed.padding_style.clone(),
            final_fg: false,
            final_bg: false,
            final_style: false,
        }),
        widget_columns: 0,
    });

    c.bench_function("line_dirty_single_edit", |b| {
        b.iter(|| {
            render_pipeline_direct(
                &state_after,
                &registry.view(),
                &mut grid,
                DirtyFlags::BUFFER,
            );
            let diffs = grid.diff();
            grid.swap_with_dirty();
            diffs.len()
        });
    });
}

/// Bench: Line-dirty all changed — no regression vs baseline when all lines differ
fn bench_line_dirty_all_changed(c: &mut Criterion) {
    let state = typical_state(23);
    let registry = PluginRuntime::new();
    let mut grid = CellGrid::new(state.runtime.cols, state.runtime.rows);

    // Initial full render
    render_pipeline_direct(&state, &registry.view(), &mut grid, DirtyFlags::ALL);
    grid.swap();

    // All lines changed
    let mut state_after = state.clone();
    let draw = draw_request(23);
    state_after.apply(draw);

    c.bench_function("line_dirty_all_changed", |b| {
        b.iter(|| {
            render_pipeline_direct(
                &state_after,
                &registry.view(),
                &mut grid,
                DirtyFlags::BUFFER,
            );
            let diffs = grid.diff();
            grid.swap_with_dirty();
            diffs.len()
        });
    });
}

/// Bench: apply(Draw) with line comparison overhead
fn bench_apply_draw_line_comparison(c: &mut Criterion) {
    let base_state = typical_state(23);
    let draw = draw_request(23);

    c.bench_function("apply_draw_line_comparison", |b| {
        b.iter_batched(
            || (base_state.clone(), draw.clone()),
            |(mut state, req)| state.apply(req),
            BatchSize::SmallInput,
        );
    });
}

// ---------------------------------------------------------------------------
// Incremental cursor detection benchmarks
// ---------------------------------------------------------------------------

fn bench_detect_cursors_incremental(c: &mut Criterion) {
    use kasane_core::protocol::{Atom, Attributes, Coord, WireFace};
    use kasane_core::state::derived::{self, CursorCache};

    let mut state = typical_state(23);
    // Add a cursor atom on line 5 (simulates Kakoune's baked cursor face)
    let cursor_face = WireFace {
        attributes: Attributes::FINAL_FG | Attributes::REVERSE,
        ..WireFace::default()
    };
    state.observed.lines[5] = vec![
        Atom {
            style: kasane_core::protocol::default_unresolved_style(),
            contents: "hel".into(),
        },
        Atom::from_wire(cursor_face, "l"),
        Atom {
            style: kasane_core::protocol::default_unresolved_style(),
            contents: "o world".into(),
        },
    ];
    let primary = Coord { line: 5, column: 3 };

    let mut group = c.benchmark_group("detect_cursors");

    // Full scan (baseline)
    group.bench_function("full_scan_23_lines", |b| {
        b.iter(|| derived::detect_cursors(&state.observed.lines, primary));
    });

    // Incremental with warm cache + 2 dirty lines
    group.bench_function("incremental_2_dirty", |b| {
        b.iter_batched(
            || {
                let mut cache = CursorCache::default();
                let all_dirty = vec![true; state.observed.lines.len()];
                derived::detect_cursors_incremental(
                    &state.observed.lines,
                    primary,
                    &all_dirty,
                    &mut cache,
                );
                let mut lines_dirty = vec![false; state.observed.lines.len()];
                lines_dirty[5] = true;
                lines_dirty[6] = true;
                (cache, lines_dirty)
            },
            |(mut cache, lines_dirty)| {
                derived::detect_cursors_incremental(
                    &state.observed.lines,
                    primary,
                    &lines_dirty,
                    &mut cache,
                )
            },
            BatchSize::SmallInput,
        );
    });

    // Incremental with warm cache + all dirty (worst case)
    group.bench_function("incremental_all_dirty", |b| {
        b.iter_batched(
            || {
                let mut cache = CursorCache::default();
                let all_dirty = vec![true; state.observed.lines.len()];
                derived::detect_cursors_incremental(
                    &state.observed.lines,
                    primary,
                    &all_dirty,
                    &mut cache,
                );
                (cache, all_dirty)
            },
            |(mut cache, all_dirty)| {
                derived::detect_cursors_incremental(
                    &state.observed.lines,
                    primary,
                    &all_dirty,
                    &mut cache,
                )
            },
            BatchSize::SmallInput,
        );
    });

    group.finish();
}

// ---------------------------------------------------------------------------
// Salsa pipeline benchmarks
// ---------------------------------------------------------------------------

mod salsa_benches {
    use criterion::{BatchSize, BenchmarkId, Criterion};
    use kasane_core::plugin::PluginRuntime;
    use kasane_core::render::CellGrid;
    use kasane_core::render::SceneCache;
    use kasane_core::render::render_pipeline_cached;
    use kasane_core::render::scene::CellSize;
    use kasane_core::render::scene_render_pipeline_cached;
    use kasane_core::salsa_db::KasaneDatabase;
    use kasane_core::salsa_sync::{SalsaInputHandles, sync_inputs_from_state};
    use kasane_core::state::DirtyFlags;

    use super::fixtures::{realistic_state, state_with_edit, state_with_menu, typical_state};

    /// Helper: create a Salsa DB fully synced with the given state.
    fn init_salsa(state: &kasane_core::state::AppState) -> (KasaneDatabase, SalsaInputHandles) {
        let mut db = KasaneDatabase::default();
        let handles = SalsaInputHandles::new(&mut db);
        sync_inputs_from_state(&mut db, state, &handles);
        (db, handles)
    }

    /// Bench: sync_inputs_from_state for various DirtyFlags patterns and sizes.
    pub fn bench_sync_inputs(c: &mut Criterion) {
        let mut group = c.benchmark_group("salsa_sync_inputs");

        // BUFFER_CONTENT (lines.clone) at various sizes
        for (lines, label) in [(23, "23_lines"), (59, "59_lines"), (79, "79_lines")] {
            let state = typical_state(lines);
            let (mut db, handles) = init_salsa(&state);

            group.bench_function(BenchmarkId::new("buffer_content", label), |b| {
                b.iter(|| {
                    sync_inputs_from_state(&mut db, &state, &handles);
                });
            });
        }

        // BUFFER_CONTENT with realistic (CJK) content
        {
            let state = realistic_state(23);
            let (mut db, handles) = init_salsa(&state);

            group.bench_function("buffer_content/realistic_23", |b| {
                b.iter(|| {
                    sync_inputs_from_state(&mut db, &state, &handles);
                });
            });
        }

        // BUFFER (cursor only — no lines.clone)
        {
            let state = typical_state(23);
            let (mut db, handles) = init_salsa(&state);

            group.bench_function("buffer_cursor_only", |b| {
                b.iter(|| {
                    sync_inputs_from_state(&mut db, &state, &handles);
                });
            });
        }

        // STATUS
        {
            let state = typical_state(23);
            let (mut db, handles) = init_salsa(&state);

            group.bench_function("status", |b| {
                b.iter(|| {
                    sync_inputs_from_state(&mut db, &state, &handles);
                });
            });
        }

        // MENU (with 100-item menu)
        {
            let state = state_with_menu(100);
            let (mut db, handles) = init_salsa(&state);

            group.bench_function("menu/100_items", |b| {
                b.iter(|| {
                    sync_inputs_from_state(&mut db, &state, &handles);
                });
            });
        }

        // ALL flags (worst case)
        {
            let state = typical_state(23);
            let (mut db, handles) = init_salsa(&state);

            group.bench_function("all_flags/80x24", |b| {
                b.iter(|| {
                    sync_inputs_from_state(&mut db, &state, &handles);
                });
            });
        }

        // ALL flags at 300x80
        {
            let mut state = typical_state(79);
            state.runtime.cols = 300;
            state.runtime.rows = 80;
            let (mut db, handles) = init_salsa(&state);

            group.bench_function("all_flags/300x80", |b| {
                b.iter(|| {
                    sync_inputs_from_state(&mut db, &state, &handles);
                });
            });
        }

        group.finish();
    }

    /// Bench: Salsa full pipeline vs legacy pipeline (direct comparison).
    pub fn bench_salsa_vs_legacy(c: &mut Criterion) {
        let mut group = c.benchmark_group("salsa_vs_legacy");

        // Full frame cold (ALL dirty)
        {
            let state = typical_state(23);
            let registry = PluginRuntime::new();

            group.bench_function("full_cold/salsa", |b| {
                b.iter_batched(
                    || {
                        let (db, handles) = init_salsa(&state);
                        let grid = CellGrid::new(state.runtime.cols, state.runtime.rows);
                        (db, handles, grid)
                    },
                    |(db, handles, mut grid)| {
                        render_pipeline_cached(
                            &db,
                            &handles,
                            &state,
                            &registry.view(),
                            &mut grid,
                            DirtyFlags::ALL,
                            Default::default(),
                        );
                    },
                    BatchSize::SmallInput,
                );
            });

            group.bench_function("full_cold/legacy", |b| {
                b.iter_batched(
                    || CellGrid::new(state.runtime.cols, state.runtime.rows),
                    |mut grid| {
                        kasane_core::render::render_pipeline_direct(
                            &state,
                            &registry.view(),
                            &mut grid,
                            DirtyFlags::ALL,
                        );
                    },
                    BatchSize::SmallInput,
                );
            });
        }

        // Warm cache hit (MENU_SELECTION only)
        {
            let state = state_with_menu(50);
            let registry = PluginRuntime::new();

            group.bench_function("menu_select_warm/salsa", |b| {
                let (db, handles) = init_salsa(&state);
                let mut grid = CellGrid::new(state.runtime.cols, state.runtime.rows);
                render_pipeline_cached(
                    &db,
                    &handles,
                    &state,
                    &registry.view(),
                    &mut grid,
                    DirtyFlags::ALL,
                    Default::default(),
                );
                grid.swap_with_dirty();

                b.iter(|| {
                    render_pipeline_cached(
                        &db,
                        &handles,
                        &state,
                        &registry.view(),
                        &mut grid,
                        DirtyFlags::MENU_SELECTION,
                        Default::default(),
                    );
                });
            });

            group.bench_function("menu_select_warm/legacy", |b| {
                let mut grid = CellGrid::new(state.runtime.cols, state.runtime.rows);
                kasane_core::render::render_pipeline_direct(
                    &state,
                    &registry.view(),
                    &mut grid,
                    DirtyFlags::ALL,
                );
                grid.swap_with_dirty();

                b.iter(|| {
                    kasane_core::render::render_pipeline_direct(
                        &state,
                        &registry.view(),
                        &mut grid,
                        DirtyFlags::MENU_SELECTION,
                    );
                });
            });
        }

        // Incremental edit (BUFFER dirty, warm cache)
        {
            let state = typical_state(23);
            let edited = state_with_edit(&state, 10, 1);
            let registry = PluginRuntime::new();

            group.bench_function("incremental_edit/salsa", |b| {
                b.iter_batched(
                    || {
                        let (mut db, handles) = init_salsa(&state);
                        let mut grid = CellGrid::new(state.runtime.cols, state.runtime.rows);
                        render_pipeline_cached(
                            &db,
                            &handles,
                            &state,
                            &registry.view(),
                            &mut grid,
                            DirtyFlags::ALL,
                            Default::default(),
                        );
                        grid.swap_with_dirty();
                        sync_inputs_from_state(&mut db, &edited, &handles);
                        (db, handles, grid)
                    },
                    |(db, handles, mut grid)| {
                        render_pipeline_cached(
                            &db,
                            &handles,
                            &edited,
                            &registry.view(),
                            &mut grid,
                            DirtyFlags::BUFFER,
                            Default::default(),
                        );
                    },
                    BatchSize::SmallInput,
                );
            });

            group.bench_function("incremental_edit/legacy", |b| {
                b.iter_batched(
                    || {
                        let mut grid = CellGrid::new(state.runtime.cols, state.runtime.rows);
                        kasane_core::render::render_pipeline_direct(
                            &state,
                            &registry.view(),
                            &mut grid,
                            DirtyFlags::ALL,
                        );
                        grid.swap_with_dirty();
                        grid
                    },
                    |mut grid| {
                        kasane_core::render::render_pipeline_direct(
                            &edited,
                            &registry.view(),
                            &mut grid,
                            DirtyFlags::BUFFER,
                        );
                    },
                    BatchSize::SmallInput,
                );
            });
        }

        group.finish();
    }

    /// Bench: Salsa scene pipeline (GPU path).
    pub fn bench_salsa_scene(c: &mut Criterion) {
        let mut group = c.benchmark_group("salsa_scene");

        let state = typical_state(23);
        let registry = PluginRuntime::new();
        let cell_size = CellSize {
            width: 8.0,
            height: 16.0,
        };

        // Cold
        group.bench_function("cold", |b| {
            b.iter_batched(
                || {
                    let (db, handles) = init_salsa(&state);
                    let scene_cache = SceneCache::new();
                    (db, handles, scene_cache)
                },
                |(db, handles, mut scene_cache)| {
                    scene_render_pipeline_cached(
                        &db,
                        &handles,
                        &state,
                        &registry.view(),
                        cell_size,
                        DirtyFlags::ALL,
                        &mut scene_cache,
                        Default::default(),
                    );
                },
                BatchSize::SmallInput,
            );
        });

        // Warm
        {
            let (db, handles) = init_salsa(&state);
            let mut scene_cache = SceneCache::new();
            scene_render_pipeline_cached(
                &db,
                &handles,
                &state,
                &registry.view(),
                cell_size,
                DirtyFlags::ALL,
                &mut scene_cache,
                Default::default(),
            );

            group.bench_function("warm", |b| {
                b.iter(|| {
                    scene_render_pipeline_cached(
                        &db,
                        &handles,
                        &state,
                        &registry.view(),
                        cell_size,
                        DirtyFlags::MENU_SELECTION,
                        &mut scene_cache,
                        Default::default(),
                    );
                });
            });
        }

        group.finish();
    }

    /// Bench: Scaling — Salsa full_frame at different terminal sizes.
    pub fn bench_salsa_scaling(c: &mut Criterion) {
        let mut group = c.benchmark_group("salsa_scaling");
        group.sample_size(50);

        for (cols, rows, label) in [(80, 24, "80x24"), (200, 60, "200x60"), (300, 80, "300x80")] {
            let mut state = typical_state(rows as usize - 1);
            state.runtime.cols = cols;
            state.runtime.rows = rows;
            let registry = PluginRuntime::new();

            group.bench_function(BenchmarkId::new("full_frame", label), |b| {
                b.iter_batched(
                    || {
                        let (db, handles) = init_salsa(&state);
                        let grid = CellGrid::new(cols, rows);
                        (db, handles, grid)
                    },
                    |(db, handles, mut grid)| {
                        render_pipeline_cached(
                            &db,
                            &handles,
                            &state,
                            &registry.view(),
                            &mut grid,
                            DirtyFlags::ALL,
                            Default::default(),
                        );
                    },
                    BatchSize::SmallInput,
                );
            });
        }

        group.finish();
    }
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
        let registry = PluginRuntime::new();
        let area = Rect {
            x: 0,
            y: 0,
            w: state.runtime.cols,
            h: state.runtime.rows,
        };
        let mut grid = CellGrid::new(state.runtime.cols, state.runtime.rows);

        group.bench_function("full_frame", |b| {
            b.iter(|| {
                alloc_counter::reset();
                let element = view::view(&state, &registry.view());
                let layout = flex::place(&element, area, &state);
                grid.clear(&kasane_core::render::TerminalStyle::from_style(
                    &state.observed.default_style,
                ));
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

    // Report allocation counts from a single iteration
    {
        let state = typical_state(23);
        let registry = PluginRuntime::new();
        let area = Rect {
            x: 0,
            y: 0,
            w: state.runtime.cols,
            h: state.runtime.rows,
        };
        let mut grid = CellGrid::new(state.runtime.cols, state.runtime.rows);

        alloc_counter::reset();
        let element = view::view(&state, &registry.view());
        let layout = flex::place(&element, area, &state);
        grid.clear(&kasane_core::render::TerminalStyle::from_style(
            &state.observed.default_style,
        ));
        paint::paint(&element, &layout, &mut grid, &state);
        let _diffs = grid.diff();
        grid.swap();
        let (count, bytes) = alloc_counter::snapshot();
        eprintln!(
            "\n[alloc] full_frame: {} allocations, {} bytes ({:.1} KB)",
            count,
            bytes,
            bytes as f64 / 1024.0
        );

        let json = draw_json(100);
        alloc_counter::reset();
        let mut buf = json.clone();
        let _ = parse_request(&mut buf).unwrap();
        let (count, bytes) = alloc_counter::snapshot();
        eprintln!(
            "[alloc] parse_request (100 lines): {} allocations, {} bytes ({:.1} KB)",
            count,
            bytes,
            bytes as f64 / 1024.0
        );
    }

    // Per-phase allocation breakdown
    {
        let state = typical_state(23);
        let registry = PluginRuntime::new();
        let area = Rect {
            x: 0,
            y: 0,
            w: state.runtime.cols,
            h: state.runtime.rows,
        };
        let mut grid = CellGrid::new(state.runtime.cols, state.runtime.rows);

        // view
        alloc_counter::reset();
        let element = view::view(&state, &registry.view());
        let (c1, b1) = alloc_counter::snapshot();

        // place
        alloc_counter::reset();
        let layout = flex::place(&element, area, &state);
        let (c2, b2) = alloc_counter::snapshot();

        // clear + paint
        alloc_counter::reset();
        grid.clear(&kasane_core::render::TerminalStyle::from_style(
            &state.observed.default_style,
        ));
        paint::paint(&element, &layout, &mut grid, &state);
        let (c3, b3) = alloc_counter::snapshot();

        // diff
        alloc_counter::reset();
        let _diffs = grid.diff();
        let (c4, b4) = alloc_counter::snapshot();

        // swap
        alloc_counter::reset();
        grid.swap();
        let (c5, b5) = alloc_counter::snapshot();

        eprintln!("\n[alloc] Per-phase breakdown (80x24, 0 plugins):");
        eprintln!(
            "  view:        {:>4} allocs, {:>8} bytes ({:.1} KB)",
            c1,
            b1,
            b1 as f64 / 1024.0
        );
        eprintln!(
            "  place:       {:>4} allocs, {:>8} bytes ({:.1} KB)",
            c2,
            b2,
            b2 as f64 / 1024.0
        );
        eprintln!(
            "  clear+paint: {:>4} allocs, {:>8} bytes ({:.1} KB)",
            c3,
            b3,
            b3 as f64 / 1024.0
        );
        eprintln!(
            "  diff:        {:>4} allocs, {:>8} bytes ({:.1} KB)",
            c4,
            b4,
            b4 as f64 / 1024.0
        );
        eprintln!(
            "  swap:        {:>4} allocs, {:>8} bytes ({:.1} KB)",
            c5,
            b5,
            b5 as f64 / 1024.0
        );
        let total_c = c1 + c2 + c3 + c4 + c5;
        let total_b = b1 + b2 + b3 + b4 + b5;
        eprintln!(
            "  total:       {:>4} allocs, {:>8} bytes ({:.1} KB)",
            total_c,
            total_b,
            total_b as f64 / 1024.0
        );
    }
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
    bench_grid_diff_into,
    bench_grid_clear,
);

criterion_group!(
    integration,
    bench_full_frame,
    bench_draw_message,
    bench_menu_show,
    bench_incremental_edit,
    bench_message_sequence,
);

criterion_group!(
    name = extended;
    config = Criterion::default().sample_size(50);
    targets =
        bench_parse_request,
        bench_state_apply,
        bench_scaling,
);

criterion_group!(
    cache,
    bench_cached_pipeline_dirty_flags,
    bench_scene_cache_cold,
    bench_scene_cache_warm,
    bench_scene_cache_menu_select,
);

criterion_group!(
    sectioned,
    bench_section_paint_status_only,
    bench_section_paint_menu_select,
);

criterion_group!(
    line_dirty,
    bench_line_dirty_single_edit,
    bench_line_dirty_all_changed,
    bench_apply_draw_line_comparison,
    bench_line_dirty_buffer_status,
    bench_detect_cursors_incremental,
);

criterion_group!(salsa_sync, salsa_benches::bench_sync_inputs,);

criterion_group!(
    salsa_pipeline,
    salsa_benches::bench_salsa_vs_legacy,
    salsa_benches::bench_salsa_scene,
    salsa_benches::bench_salsa_scaling,
);

#[cfg(not(feature = "bench-alloc"))]
criterion_main!(
    micro,
    integration,
    extended,
    cache,
    sectioned,
    line_dirty,
    salsa_sync,
    salsa_pipeline,
);

#[cfg(feature = "bench-alloc")]
criterion_group!(alloc_benches, bench_allocations);

#[cfg(feature = "bench-alloc")]
criterion_main!(
    micro,
    integration,
    extended,
    cache,
    sectioned,
    line_dirty,
    alloc_benches,
    salsa_sync,
    salsa_pipeline,
);
