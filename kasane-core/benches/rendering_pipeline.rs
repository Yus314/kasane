mod fixtures;

use criterion::{BatchSize, BenchmarkId, Criterion, criterion_group, criterion_main};
use kasane_core::layout::Rect;
use kasane_core::layout::flex;
use kasane_core::plugin::PluginRegistry;
use kasane_core::protocol::{Color, NamedColor, parse_request};
use kasane_core::render::CellGrid;
use kasane_core::render::LayoutCache;
use kasane_core::render::SceneCache;
use kasane_core::render::ViewCache;
use kasane_core::render::paint;
use kasane_core::render::render_pipeline_cached;
use kasane_core::render::render_pipeline_sectioned;
use kasane_core::render::scene::CellSize;
use kasane_core::render::scene_render_pipeline_scene_cached;
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

    // realistic 80x24 (diverse faces, varied line lengths, wide chars)
    {
        let state = realistic_state(23);
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

        group.bench_function("80x24_realistic", |b| {
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

/// Bench: grid.diff_into() — zero-allocation alternative to diff()
fn bench_grid_diff_into(c: &mut Criterion) {
    let mut group = c.benchmark_group("grid_diff_into");

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

        let mut buf = Vec::with_capacity(state.cols as usize * state.rows as usize);
        group.bench_function("full_redraw", |b| {
            b.iter(|| grid.diff_into(&mut buf));
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

        let mut buf = Vec::with_capacity(state.cols as usize * state.rows as usize);
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
    state.status_default_face = state.default_face;
    let registry = PluginRegistry::new();

    // Prepare warm grid (2 frames to get past swap fallback)
    let mut grid = CellGrid::new(state.cols, state.rows);
    let mut cache = ViewCache::new();
    render_pipeline_cached(&state, &registry, &mut grid, DirtyFlags::ALL, &mut cache);
    grid.swap_with_dirty();
    render_pipeline_cached(&state, &registry, &mut grid, DirtyFlags::ALL, &mut cache);
    grid.swap_with_dirty();

    // Now simulate editing 1 line with BUFFER|STATUS dirty
    let mut edited = state.clone();
    edited.lines[10] = vec![kasane_core::protocol::Atom {
        face: kasane_core::protocol::Face::default(),
        contents: "EDITED_LINE".into(),
    }];
    edited.lines_dirty = vec![false; 23];
    edited.lines_dirty[10] = true;

    group.bench_function("1_line_changed", |b| {
        b.iter_batched(
            || {
                // Setup: create a warm grid each iteration
                let mut g = CellGrid::new(state.cols, state.rows);
                let mut c = ViewCache::new();
                render_pipeline_cached(&state, &registry, &mut g, DirtyFlags::ALL, &mut c);
                g.swap_with_dirty();
                render_pipeline_cached(&state, &registry, &mut g, DirtyFlags::ALL, &mut c);
                g.swap_with_dirty();
                (g, c)
            },
            |(mut g, mut c)| {
                render_pipeline_cached(
                    &edited,
                    &registry,
                    &mut g,
                    DirtyFlags::BUFFER | DirtyFlags::STATUS,
                    &mut c,
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
    let face = kasane_core::protocol::Face {
        fg: Color::Named(NamedColor::White),
        bg: Color::Named(NamedColor::Black),
        ..kasane_core::protocol::Face::default()
    };

    for (cols, rows, label) in [(80, 24, "80x24"), (200, 60, "200x60")] {
        let mut grid = CellGrid::new(cols, rows);
        group.bench_function(label, |b| {
            b.iter(|| grid.clear(&face));
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
            let diffs = grid.diff();
            grid.swap();
            diffs.len()
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

        b.iter_batched(
            || (base_state.clone(), draw.clone()),
            |(mut state, req)| {
                state.apply(req);
                let element = view::view(&state, &registry);
                let layout = flex::place(&element, area, &state);
                grid.clear(&state.default_face);
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
    let registry = PluginRegistry::new();
    let area = Rect {
        x: 0,
        y: 0,
        w: state.cols,
        h: state.rows,
    };

    for edit_lines in [1, 5] {
        // Render "before" frame into previous buffer
        let mut grid = CellGrid::new(state.cols, state.rows);
        let element = view::view(&state, &registry);
        let layout = flex::place(&element, area, &state);
        grid.clear(&state.default_face);
        paint::paint(&element, &layout, &mut grid, &state);
        grid.swap();

        // "after" state with edited lines
        let edited_state = state_with_edit(&state, 10, edit_lines);

        group.bench_function(BenchmarkId::new("lines", edit_lines), |b| {
            b.iter(|| {
                // Re-render into current buffer and diff (previous stays fixed — no swap)
                let element = view::view(&edited_state, &registry);
                let layout = flex::place(&element, area, &edited_state);
                grid.clear(&edited_state.default_face);
                paint::paint(&element, &layout, &mut grid, &edited_state);
                grid.diff().len()
            });
        });
    }

    group.finish();
}

/// Bench 11: Realistic message sequence — draw_status + draw → full render
fn bench_message_sequence(c: &mut Criterion) {
    let registry = PluginRegistry::new();
    let draw_status = kasane_core::protocol::KakouneRequest::DrawStatus {
        prompt: vec![kasane_core::protocol::Atom {
            face: kasane_core::protocol::Face::default(),
            contents: " INSERT ".into(),
        }],
        content: Vec::new(),
        content_cursor_pos: -1,
        mode_line: vec![kasane_core::protocol::Atom {
            face: kasane_core::protocol::Face::default(),
            contents: "insert".into(),
        }],
        default_face: kasane_core::protocol::Face::default(),
    };
    let draw = draw_request(23);
    let base_state = typical_state(23);
    let area = Rect {
        x: 0,
        y: 0,
        w: base_state.cols,
        h: base_state.rows,
    };
    let mut grid = CellGrid::new(base_state.cols, base_state.rows);

    c.bench_function("message_sequence", |b| {
        b.iter_batched(
            || (base_state.clone(), draw_status.clone(), draw.clone()),
            |(mut state, msg1, msg2)| {
                state.apply(msg1);
                state.apply(msg2);
                let element = view::view(&state, &registry);
                let layout = flex::place(&element, area, &state);
                grid.clear(&state.default_face);
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
            prompt: vec![kasane_core::protocol::Atom {
                face: kasane_core::protocol::Face::default(),
                contents: " NORMAL ".into(),
            }],
            content: Vec::new(),
            content_cursor_pos: -1,
            mode_line: vec![kasane_core::protocol::Atom {
                face: kasane_core::protocol::Face::default(),
                contents: "normal".into(),
            }],
            default_face: kasane_core::protocol::Face::default(),
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
                vec![kasane_core::protocol::Atom {
                    face: kasane_core::protocol::Face::default(),
                    contents: format!("completion_{i}").into(),
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
// View cache benchmarks
// ---------------------------------------------------------------------------

/// Bench: SceneCache cold (full pipeline, DirtyFlags::ALL)
fn bench_scene_cache_cold(c: &mut Criterion) {
    let state = typical_state(23);
    let registry = PluginRegistry::new();
    let cs = CellSize {
        width: 10.0,
        height: 20.0,
    };

    c.bench_function("scene_cache_cold", |b| {
        b.iter(|| {
            let mut view_cache = ViewCache::new();
            let mut scene_cache = SceneCache::new();
            let (cmds, result) = scene_render_pipeline_scene_cached(
                &state,
                &registry,
                cs,
                DirtyFlags::ALL,
                &mut view_cache,
                &mut scene_cache,
            );
            criterion::black_box((cmds.len(), result));
        });
    });
}

/// Bench: SceneCache warm (all cached, near-zero work)
fn bench_scene_cache_warm(c: &mut Criterion) {
    let state = typical_state(23);
    let registry = PluginRegistry::new();
    let cs = CellSize {
        width: 10.0,
        height: 20.0,
    };

    // Pre-populate caches
    let mut view_cache = ViewCache::new();
    let mut scene_cache = SceneCache::new();
    scene_render_pipeline_scene_cached(
        &state,
        &registry,
        cs,
        DirtyFlags::ALL,
        &mut view_cache,
        &mut scene_cache,
    );

    c.bench_function("scene_cache_warm", |b| {
        b.iter(|| {
            let (cmds, result) = scene_render_pipeline_scene_cached(
                &state,
                &registry,
                cs,
                DirtyFlags::empty(),
                &mut view_cache,
                &mut scene_cache,
            );
            criterion::black_box((cmds.len(), result));
        });
    });
}

/// Bench: SceneCache with MENU_SELECTION only (base + info cached)
fn bench_scene_cache_menu_select(c: &mut Criterion) {
    let state = state_with_menu(50);
    let registry = PluginRegistry::new();
    let cs = CellSize {
        width: 10.0,
        height: 20.0,
    };

    // Pre-populate caches
    let mut view_cache = ViewCache::new();
    let mut scene_cache = SceneCache::new();
    scene_render_pipeline_scene_cached(
        &state,
        &registry,
        cs,
        DirtyFlags::ALL,
        &mut view_cache,
        &mut scene_cache,
    );

    c.bench_function("scene_cache_menu_select", |b| {
        b.iter(|| {
            let (cmds, result) = scene_render_pipeline_scene_cached(
                &state,
                &registry,
                cs,
                DirtyFlags::MENU_SELECTION,
                &mut view_cache,
                &mut scene_cache,
            );
            criterion::black_box((cmds.len(), result));
        });
    });
}

/// Bench: ViewCache warm vs cold on menu selection change
fn bench_view_cache(c: &mut Criterion) {
    let mut group = c.benchmark_group("view_cache");

    let state = state_with_menu(50);
    let registry = PluginRegistry::new();

    // Cold: fresh cache (baseline — equivalent to uncached view)
    group.bench_function("menu_select_cold", |b| {
        b.iter(|| {
            let mut cache = ViewCache::new();
            cache.invalidate(DirtyFlags::ALL);
            view::view_cached(&state, &registry, &mut cache)
        });
    });

    // Warm: base is cached, only MENU_SELECTION is dirty
    group.bench_function("menu_select_warm", |b| {
        // Pre-populate cache
        let mut cache = ViewCache::new();
        cache.invalidate(DirtyFlags::ALL);
        let _ = view::view_cached(&state, &registry, &mut cache);

        b.iter(|| {
            cache.invalidate(DirtyFlags::MENU_SELECTION);
            view::view_cached(&state, &registry, &mut cache)
        });
    });

    group.finish();
}

// ---------------------------------------------------------------------------
// Section-level paint benchmarks (S1)
// ---------------------------------------------------------------------------

/// Bench: Sectioned pipeline — STATUS only dirty (should repaint only status row)
fn bench_section_paint_status_only(c: &mut Criterion) {
    let state = typical_state(23);
    let registry = PluginRegistry::new();
    let mut grid = CellGrid::new(state.cols, state.rows);
    let mut view_cache = ViewCache::new();
    let mut layout_cache = LayoutCache::new();

    // Initial full render to populate caches
    render_pipeline_sectioned(
        &state,
        &registry,
        &mut grid,
        DirtyFlags::ALL,
        &mut view_cache,
        &mut layout_cache,
    );
    grid.swap();

    c.bench_function("section_paint_status_only", |b| {
        b.iter(|| {
            render_pipeline_sectioned(
                &state,
                &registry,
                &mut grid,
                DirtyFlags::STATUS,
                &mut view_cache,
                &mut layout_cache,
            )
        });
    });
}

/// Bench: Sectioned pipeline — MENU_SELECTION only dirty
fn bench_section_paint_menu_select(c: &mut Criterion) {
    let state = state_with_menu(50);
    let registry = PluginRegistry::new();
    let mut grid = CellGrid::new(state.cols, state.rows);
    let mut view_cache = ViewCache::new();
    let mut layout_cache = LayoutCache::new();

    // Initial full render
    render_pipeline_sectioned(
        &state,
        &registry,
        &mut grid,
        DirtyFlags::ALL,
        &mut view_cache,
        &mut layout_cache,
    );
    grid.swap();

    c.bench_function("section_paint_menu_select", |b| {
        b.iter(|| {
            render_pipeline_sectioned(
                &state,
                &registry,
                &mut grid,
                DirtyFlags::MENU_SELECTION,
                &mut view_cache,
                &mut layout_cache,
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
    let registry = PluginRegistry::new();
    let mut grid = CellGrid::new(state.cols, state.rows);
    let mut view_cache = ViewCache::new();

    // Initial full render
    render_pipeline_cached(
        &state,
        &registry,
        &mut grid,
        DirtyFlags::ALL,
        &mut view_cache,
    );
    grid.swap();

    // "After" state: edit line 10
    let edited_state = state_with_edit(&state, 10, 1);
    // Simulate apply(Draw) to get lines_dirty
    let mut state_after = state.clone();
    let edited_lines = edited_state.lines.clone();
    state_after.apply(kasane_core::protocol::KakouneRequest::Draw {
        lines: edited_lines,
        cursor_pos: kasane_core::protocol::Coord::default(),
        default_face: state.default_face,
        padding_face: state.padding_face,
        widget_columns: 0,
    });

    c.bench_function("line_dirty_single_edit", |b| {
        b.iter(|| {
            render_pipeline_cached(
                &state_after,
                &registry,
                &mut grid,
                DirtyFlags::BUFFER,
                &mut view_cache,
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
    let registry = PluginRegistry::new();
    let mut grid = CellGrid::new(state.cols, state.rows);
    let mut view_cache = ViewCache::new();

    // Initial full render
    render_pipeline_cached(
        &state,
        &registry,
        &mut grid,
        DirtyFlags::ALL,
        &mut view_cache,
    );
    grid.swap();

    // All lines changed
    let mut state_after = state.clone();
    let draw = draw_request(23);
    state_after.apply(draw);

    c.bench_function("line_dirty_all_changed", |b| {
        b.iter(|| {
            render_pipeline_cached(
                &state_after,
                &registry,
                &mut grid,
                DirtyFlags::BUFFER,
                &mut view_cache,
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
// Salsa pipeline benchmarks
// ---------------------------------------------------------------------------

mod salsa_benches {
    use criterion::{BatchSize, BenchmarkId, Criterion};
    use kasane_core::plugin::PluginRegistry;
    use kasane_core::render::CellGrid;
    use kasane_core::render::SceneCache;
    use kasane_core::render::ViewCache;
    use kasane_core::render::render_pipeline_salsa_cached;
    use kasane_core::render::scene::CellSize;
    use kasane_core::render::scene_render_pipeline_salsa_cached;
    use kasane_core::salsa_db::KasaneDatabase;
    use kasane_core::salsa_sync::{SalsaInputHandles, sync_inputs_from_state};
    use kasane_core::state::DirtyFlags;

    use super::fixtures::{realistic_state, state_with_edit, state_with_menu, typical_state};

    /// Helper: create a Salsa DB fully synced with the given state.
    fn init_salsa(state: &kasane_core::state::AppState) -> (KasaneDatabase, SalsaInputHandles) {
        let mut db = KasaneDatabase::default();
        let handles = SalsaInputHandles::new(&mut db);
        sync_inputs_from_state(&mut db, state, DirtyFlags::ALL, &handles);
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
                    sync_inputs_from_state(&mut db, &state, DirtyFlags::BUFFER_CONTENT, &handles);
                });
            });
        }

        // BUFFER_CONTENT with realistic (CJK) content
        {
            let state = realistic_state(23);
            let (mut db, handles) = init_salsa(&state);

            group.bench_function("buffer_content/realistic_23", |b| {
                b.iter(|| {
                    sync_inputs_from_state(&mut db, &state, DirtyFlags::BUFFER_CONTENT, &handles);
                });
            });
        }

        // BUFFER (cursor only — no lines.clone)
        {
            let state = typical_state(23);
            let (mut db, handles) = init_salsa(&state);

            group.bench_function("buffer_cursor_only", |b| {
                b.iter(|| {
                    sync_inputs_from_state(&mut db, &state, DirtyFlags::BUFFER_CURSOR, &handles);
                });
            });
        }

        // STATUS
        {
            let state = typical_state(23);
            let (mut db, handles) = init_salsa(&state);

            group.bench_function("status", |b| {
                b.iter(|| {
                    sync_inputs_from_state(&mut db, &state, DirtyFlags::STATUS, &handles);
                });
            });
        }

        // MENU (with 100-item menu)
        {
            let state = state_with_menu(100);
            let (mut db, handles) = init_salsa(&state);

            group.bench_function("menu/100_items", |b| {
                b.iter(|| {
                    sync_inputs_from_state(&mut db, &state, DirtyFlags::MENU_STRUCTURE, &handles);
                });
            });
        }

        // ALL flags (worst case)
        {
            let state = typical_state(23);
            let (mut db, handles) = init_salsa(&state);

            group.bench_function("all_flags/80x24", |b| {
                b.iter(|| {
                    sync_inputs_from_state(&mut db, &state, DirtyFlags::ALL, &handles);
                });
            });
        }

        // ALL flags at 300x80
        {
            let mut state = typical_state(79);
            state.cols = 300;
            state.rows = 80;
            let (mut db, handles) = init_salsa(&state);

            group.bench_function("all_flags/300x80", |b| {
                b.iter(|| {
                    sync_inputs_from_state(&mut db, &state, DirtyFlags::ALL, &handles);
                });
            });
        }

        group.finish();
    }

    /// Bench: Salsa full pipeline vs legacy pipeline (direct comparison).
    pub fn bench_salsa_vs_legacy(c: &mut Criterion) {
        let mut group = c.benchmark_group("salsa_vs_legacy");
        let paint_hooks: Vec<Box<dyn kasane_core::plugin::PaintHook>> = vec![];

        // Full frame cold (ALL dirty)
        {
            let state = typical_state(23);
            let registry = PluginRegistry::new();

            group.bench_function("full_cold/salsa", |b| {
                b.iter_batched(
                    || {
                        let (db, handles) = init_salsa(&state);
                        let grid = CellGrid::new(state.cols, state.rows);
                        (db, handles, grid)
                    },
                    |(db, handles, mut grid)| {
                        render_pipeline_salsa_cached(
                            &db,
                            &handles,
                            &state,
                            &registry,
                            &mut grid,
                            DirtyFlags::ALL,
                            &paint_hooks,
                        );
                    },
                    BatchSize::SmallInput,
                );
            });

            group.bench_function("full_cold/legacy", |b| {
                b.iter_batched(
                    || {
                        let grid = CellGrid::new(state.cols, state.rows);
                        let cache = ViewCache::new();
                        (grid, cache)
                    },
                    |(mut grid, mut cache)| {
                        kasane_core::render::render_pipeline_cached(
                            &state,
                            &registry,
                            &mut grid,
                            DirtyFlags::ALL,
                            &mut cache,
                        );
                    },
                    BatchSize::SmallInput,
                );
            });
        }

        // Warm cache hit (MENU_SELECTION only — ViewCache hit, Salsa not called)
        {
            let state = state_with_menu(50);
            let registry = PluginRegistry::new();

            group.bench_function("menu_select_warm/salsa", |b| {
                let (db, handles) = init_salsa(&state);
                let mut grid = CellGrid::new(state.cols, state.rows);
                render_pipeline_salsa_cached(
                    &db,
                    &handles,
                    &state,
                    &registry,
                    &mut grid,
                    DirtyFlags::ALL,
                    &paint_hooks,
                );
                grid.swap_with_dirty();

                b.iter(|| {
                    render_pipeline_salsa_cached(
                        &db,
                        &handles,
                        &state,
                        &registry,
                        &mut grid,
                        DirtyFlags::MENU_SELECTION,
                        &paint_hooks,
                    );
                });
            });

            group.bench_function("menu_select_warm/legacy", |b| {
                let mut grid = CellGrid::new(state.cols, state.rows);
                let mut cache = ViewCache::new();
                kasane_core::render::render_pipeline_cached(
                    &state,
                    &registry,
                    &mut grid,
                    DirtyFlags::ALL,
                    &mut cache,
                );
                grid.swap_with_dirty();

                b.iter(|| {
                    kasane_core::render::render_pipeline_cached(
                        &state,
                        &registry,
                        &mut grid,
                        DirtyFlags::MENU_SELECTION,
                        &mut cache,
                    );
                });
            });
        }

        // Incremental edit (BUFFER dirty, warm cache)
        {
            let state = typical_state(23);
            let edited = state_with_edit(&state, 10, 1);
            let registry = PluginRegistry::new();

            group.bench_function("incremental_edit/salsa", |b| {
                b.iter_batched(
                    || {
                        let (mut db, handles) = init_salsa(&state);
                        let mut grid = CellGrid::new(state.cols, state.rows);
                        render_pipeline_salsa_cached(
                            &db,
                            &handles,
                            &state,
                            &registry,
                            &mut grid,
                            DirtyFlags::ALL,
                            &paint_hooks,
                        );
                        grid.swap_with_dirty();
                        sync_inputs_from_state(&mut db, &edited, DirtyFlags::BUFFER, &handles);
                        (db, handles, grid)
                    },
                    |(db, handles, mut grid)| {
                        render_pipeline_salsa_cached(
                            &db,
                            &handles,
                            &edited,
                            &registry,
                            &mut grid,
                            DirtyFlags::BUFFER,
                            &paint_hooks,
                        );
                    },
                    BatchSize::SmallInput,
                );
            });

            group.bench_function("incremental_edit/legacy", |b| {
                b.iter_batched(
                    || {
                        let mut grid = CellGrid::new(state.cols, state.rows);
                        let mut cache = ViewCache::new();
                        kasane_core::render::render_pipeline_cached(
                            &state,
                            &registry,
                            &mut grid,
                            DirtyFlags::ALL,
                            &mut cache,
                        );
                        grid.swap_with_dirty();
                        (grid, cache)
                    },
                    |(mut grid, mut cache)| {
                        kasane_core::render::render_pipeline_cached(
                            &edited,
                            &registry,
                            &mut grid,
                            DirtyFlags::BUFFER,
                            &mut cache,
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
        let registry = PluginRegistry::new();
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
                    scene_render_pipeline_salsa_cached(
                        &db,
                        &handles,
                        &state,
                        &registry,
                        cell_size,
                        DirtyFlags::ALL,
                        &mut scene_cache,
                    );
                },
                BatchSize::SmallInput,
            );
        });

        // Warm
        {
            let (db, handles) = init_salsa(&state);
            let mut scene_cache = SceneCache::new();
            scene_render_pipeline_salsa_cached(
                &db,
                &handles,
                &state,
                &registry,
                cell_size,
                DirtyFlags::ALL,
                &mut scene_cache,
            );

            group.bench_function("warm", |b| {
                b.iter(|| {
                    scene_render_pipeline_salsa_cached(
                        &db,
                        &handles,
                        &state,
                        &registry,
                        cell_size,
                        DirtyFlags::MENU_SELECTION,
                        &mut scene_cache,
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
        let paint_hooks: Vec<Box<dyn kasane_core::plugin::PaintHook>> = vec![];

        for (cols, rows, label) in [(80, 24, "80x24"), (200, 60, "200x60"), (300, 80, "300x80")] {
            let mut state = typical_state(rows as usize - 1);
            state.cols = cols;
            state.rows = rows;
            let registry = PluginRegistry::new();

            group.bench_function(BenchmarkId::new("full_frame", label), |b| {
                b.iter_batched(
                    || {
                        let (db, handles) = init_salsa(&state);
                        let grid = CellGrid::new(cols, rows);
                        (db, handles, grid)
                    },
                    |(db, handles, mut grid)| {
                        render_pipeline_salsa_cached(
                            &db,
                            &handles,
                            &state,
                            &registry,
                            &mut grid,
                            DirtyFlags::ALL,
                            &paint_hooks,
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

    // Report allocation counts from a single iteration
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

        alloc_counter::reset();
        let element = view::view(&state, &registry);
        let layout = flex::place(&element, area, &state);
        grid.clear(&state.default_face);
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
        let registry = PluginRegistry::new();
        let area = Rect {
            x: 0,
            y: 0,
            w: state.cols,
            h: state.rows,
        };
        let mut grid = CellGrid::new(state.cols, state.rows);

        // view
        alloc_counter::reset();
        let element = view::view(&state, &registry);
        let (c1, b1) = alloc_counter::snapshot();

        // place
        alloc_counter::reset();
        let layout = flex::place(&element, area, &state);
        let (c2, b2) = alloc_counter::snapshot();

        // clear + paint
        alloc_counter::reset();
        grid.clear(&state.default_face);
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
    bench_view_cache,
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
