use criterion::{BatchSize, BenchmarkId, Criterion, criterion_group, criterion_main};
use crossterm::{
    cursor, queue,
    style::{
        self, Attribute as CtAttribute, SetAttribute, SetBackgroundColor, SetForegroundColor,
        SetUnderlineColor,
    },
    terminal::{BeginSynchronizedUpdate, EndSynchronizedUpdate},
};
use kasane_core::layout::Rect;
use kasane_core::layout::flex;
use kasane_core::plugin::PluginRuntime;
use kasane_core::protocol::{
    Atom, Attributes, Color, Coord, Face, Line, NamedColor, parse_request,
};
use kasane_core::render::paint;
use kasane_core::render::view;
use kasane_core::render::{CellDiff, CellGrid, render_pipeline};
use kasane_core::state::AppState;
use kasane_tui::sgr::{convert_attribute, convert_color, emit_sgr_diff};
use serde::Serialize;

// ---------------------------------------------------------------------------
// MockBackend — same escape-sequence logic as TuiBackend, but writes to Vec<u8>
// ---------------------------------------------------------------------------

struct MockBackend {
    buf: Vec<u8>,
}

impl MockBackend {
    fn new() -> Self {
        Self {
            buf: Vec::with_capacity(1 << 16),
        }
    }

    fn begin_frame(&mut self) -> anyhow::Result<()> {
        queue!(self.buf, BeginSynchronizedUpdate, cursor::Hide)?;
        Ok(())
    }

    fn end_frame(&mut self) -> anyhow::Result<()> {
        queue!(self.buf, EndSynchronizedUpdate)?;
        Ok(())
    }

    fn draw(&mut self, diffs: &[CellDiff]) -> anyhow::Result<()> {
        let mut last_face: Option<Face> = None;

        for diff in diffs {
            queue!(self.buf, cursor::MoveTo(diff.x, diff.y))?;

            let face = &diff.cell.face;
            let need_style_update = last_face.as_ref() != Some(face);

            if need_style_update {
                queue!(self.buf, SetAttribute(CtAttribute::Reset))?;
                queue!(
                    self.buf,
                    SetForegroundColor(convert_color(face.fg)),
                    SetBackgroundColor(convert_color(face.bg))
                )?;

                if face.underline != Color::Default {
                    queue!(self.buf, SetUnderlineColor(convert_color(face.underline)))?;
                }

                for attr in face.attributes.iter() {
                    if let Some(ct_attr) = convert_attribute(attr) {
                        queue!(self.buf, SetAttribute(ct_attr))?;
                    }
                }

                last_face = Some(*face);
            }

            let s = if diff.cell.grapheme.is_empty() {
                " "
            } else {
                &diff.cell.grapheme
            };
            queue!(self.buf, style::Print(s))?;
        }

        queue!(self.buf, SetAttribute(CtAttribute::Reset))?;
        Ok(())
    }

    fn draw_grid(&mut self, grid: &CellGrid) -> anyhow::Result<()> {
        let mut last_face: Option<Face> = None;
        let mut last_x: u16 = u16::MAX;
        let mut last_y: u16 = u16::MAX;

        for (x, y, cell) in grid.iter_diffs() {
            let expected_x = if last_y == y { last_x } else { u16::MAX };
            if x != expected_x {
                queue!(self.buf, cursor::MoveTo(x, y))?;
            }

            let face = &cell.face;
            if last_face.as_ref() != Some(face) {
                emit_sgr_diff(&mut self.buf, last_face.as_ref(), face)?;
                last_face = Some(*face);
            }

            let s = if cell.grapheme.is_empty() {
                " "
            } else {
                &cell.grapheme
            };
            queue!(self.buf, style::Print(s))?;

            last_x = x + cell.width.max(1) as u16;
            last_y = y;
        }

        queue!(self.buf, SetAttribute(CtAttribute::Reset))?;
        Ok(())
    }

    fn flush(&mut self) {
        self.buf.clear();
    }

    fn bytes_generated(&self) -> usize {
        self.buf.len()
    }
}

// ---------------------------------------------------------------------------
// Fixture helpers
// ---------------------------------------------------------------------------

fn make_colored_line(i: usize) -> Vec<Atom> {
    let keyword_face = Face {
        fg: Color::Rgb {
            r: 255,
            g: 100,
            b: 0,
        },
        bg: Color::Default,
        ..Face::default()
    };
    let ident_face = Face {
        fg: Color::Rgb {
            r: 0,
            g: 200,
            b: 100,
        },
        bg: Color::Default,
        ..Face::default()
    };
    let literal_face = Face {
        fg: Color::Rgb {
            r: 100,
            g: 100,
            b: 255,
        },
        bg: Color::Default,
        ..Face::default()
    };
    let plain_face = Face::default();

    vec![
        Atom {
            face: keyword_face,
            contents: "let".into(),
        },
        Atom {
            face: plain_face,
            contents: " ".into(),
        },
        Atom {
            face: ident_face,
            contents: format!("var_{i}").into(),
        },
        Atom {
            face: plain_face,
            contents: " = ".into(),
        },
        Atom {
            face: literal_face,
            contents: format!("\"{i}_value\"").into(),
        },
        Atom {
            face: plain_face,
            contents: ";".into(),
        },
    ]
}

fn typical_state(line_count: usize) -> AppState {
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
    state.lines = (0..line_count).map(make_colored_line).collect();
    state.status_line = vec![Atom {
        face: Face::default(),
        contents: " NORMAL ".into(),
    }];
    state.status_mode_line = vec![Atom {
        face: Face::default(),
        contents: "normal".into(),
    }];
    state
}

/// Render a full frame and return the grid (previous buffer empty = full redraw).
fn generate_grid(cols: u16, rows: u16, line_count: usize) -> CellGrid {
    let mut state = typical_state(line_count);
    state.cols = cols;
    state.rows = rows;
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
    grid.clear(&state.default_face);
    paint::paint(&element, &layout, &mut grid, &state);
    grid
}

/// Generate grid for an incremental 1-line edit.
fn generate_incremental_grid() -> CellGrid {
    let state = typical_state(23);
    let registry = PluginRuntime::new();
    let area = Rect {
        x: 0,
        y: 0,
        w: state.cols,
        h: state.rows,
    };

    let mut grid = CellGrid::new(state.cols, state.rows);

    // "before" frame
    let element = view::view(&state, &registry.view());
    let layout = flex::place(&element, area, &state);
    grid.clear(&state.default_face);
    paint::paint(&element, &layout, &mut grid, &state);
    grid.swap();

    // "after": modify 1 line
    let mut edited = state.clone();
    edited.lines[10] = vec![
        Atom {
            face: Face {
                fg: Color::Rgb { r: 255, g: 0, b: 0 },
                bg: Color::Default,
                ..Face::default()
            },
            contents: "edited_line_10".into(),
        },
        Atom {
            face: Face::default(),
            contents: " // modified".into(),
        },
    ];

    let element = view::view(&edited, &registry.view());
    let layout = flex::place(&element, area, &edited);
    grid.clear(&edited.default_face);
    paint::paint(&element, &layout, &mut grid, &edited);
    grid
}

/// Generate grid with realistic data.
fn generate_realistic_grid(cols: u16, rows: u16, line_count: usize) -> CellGrid {
    let mut state = realistic_state(line_count);
    state.cols = cols;
    state.rows = rows;
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
    grid.clear(&state.default_face);
    paint::paint(&element, &layout, &mut grid, &state);
    grid
}

/// Render a full frame and return the diffs (full redraw — previous buffer is empty).
fn generate_diffs(cols: u16, rows: u16, line_count: usize) -> Vec<CellDiff> {
    let mut state = typical_state(line_count);
    state.cols = cols;
    state.rows = rows;
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
    grid.clear(&state.default_face);
    paint::paint(&element, &layout, &mut grid, &state);
    grid.diff()
}

/// Generate diffs for an incremental 1-line edit (previous buffer populated, 1 line changed).
fn generate_incremental_diffs() -> Vec<CellDiff> {
    let state = typical_state(23);
    let registry = PluginRuntime::new();
    let area = Rect {
        x: 0,
        y: 0,
        w: state.cols,
        h: state.rows,
    };

    let mut grid = CellGrid::new(state.cols, state.rows);

    // "before" frame
    let element = view::view(&state, &registry.view());
    let layout = flex::place(&element, area, &state);
    grid.clear(&state.default_face);
    paint::paint(&element, &layout, &mut grid, &state);
    grid.swap();

    // "after" state: modify 1 line
    let mut edited_state = state.clone();
    edited_state.lines[10] = vec![
        Atom {
            face: Face {
                fg: Color::Rgb { r: 255, g: 0, b: 0 },
                bg: Color::Default,
                ..Face::default()
            },
            contents: "edited_line_10".into(),
        },
        Atom {
            face: Face::default(),
            contents: " // modified".into(),
        },
    ];

    let element = view::view(&edited_state, &registry.view());
    let layout = flex::place(&element, area, &edited_state);
    grid.clear(&edited_state.default_face);
    paint::paint(&element, &layout, &mut grid, &edited_state);
    grid.diff()
}

// ---------------------------------------------------------------------------
// Realistic fixture builders (diverse faces, varied line lengths, wide chars)
// ---------------------------------------------------------------------------

fn keyword_face() -> Face {
    Face {
        fg: Color::Rgb {
            r: 255,
            g: 100,
            b: 0,
        },
        bg: Color::Default,
        ..Face::default()
    }
}

fn ident_face() -> Face {
    Face {
        fg: Color::Rgb {
            r: 0,
            g: 200,
            b: 100,
        },
        bg: Color::Default,
        ..Face::default()
    }
}

fn literal_face() -> Face {
    Face {
        fg: Color::Rgb {
            r: 100,
            g: 100,
            b: 255,
        },
        bg: Color::Default,
        ..Face::default()
    }
}

fn comment_face() -> Face {
    Face {
        fg: Color::Rgb {
            r: 128,
            g: 128,
            b: 128,
        },
        bg: Color::Default,
        attributes: Attributes::ITALIC,
        ..Face::default()
    }
}

fn type_face() -> Face {
    Face {
        fg: Color::Named(NamedColor::Cyan),
        bg: Color::Default,
        ..Face::default()
    }
}

fn operator_face() -> Face {
    Face {
        fg: Color::Named(NamedColor::White),
        bg: Color::Default,
        ..Face::default()
    }
}

fn string_face() -> Face {
    Face {
        fg: Color::Named(NamedColor::Yellow),
        bg: Color::Default,
        ..Face::default()
    }
}

fn error_face() -> Face {
    Face {
        fg: Color::Named(NamedColor::BrightRed),
        bg: Color::Default,
        attributes: Attributes::BOLD | Attributes::UNDERLINE,
        ..Face::default()
    }
}

fn namespace_face() -> Face {
    Face {
        fg: Color::Named(NamedColor::Magenta),
        bg: Color::Default,
        ..Face::default()
    }
}

fn constant_face() -> Face {
    Face {
        fg: Color::Named(NamedColor::BrightBlue),
        bg: Color::Default,
        ..Face::default()
    }
}

fn make_realistic_line(i: usize) -> Vec<Atom> {
    match i % 8 {
        0 => vec![], // empty line
        1 => vec![Atom {
            face: comment_face(),
            contents: format!("// comment line {i}").into(),
        }],
        2 => vec![
            Atom {
                face: keyword_face(),
                contents: "fn ".into(),
            },
            Atom {
                face: ident_face(),
                contents: format!("process_{i}").into(),
            },
            Atom {
                face: operator_face(),
                contents: "(".into(),
            },
            Atom {
                face: type_face(),
                contents: "u32".into(),
            },
            Atom {
                face: operator_face(),
                contents: ") {".into(),
            },
        ],
        3 => vec![
            Atom {
                face: keyword_face(),
                contents: "    let ".into(),
            },
            Atom {
                face: ident_face(),
                contents: format!("result_{i}").into(),
            },
            Atom {
                face: operator_face(),
                contents: " = ".into(),
            },
            Atom {
                face: namespace_face(),
                contents: "self".into(),
            },
            Atom {
                face: operator_face(),
                contents: ".".into(),
            },
            Atom {
                face: ident_face(),
                contents: format!("compute_{i}").into(),
            },
            Atom {
                face: operator_face(),
                contents: "(".into(),
            },
            Atom {
                face: literal_face(),
                contents: format!("{}", i * 42).into(),
            },
            Atom {
                face: operator_face(),
                contents: ", ".into(),
            },
            Atom {
                face: string_face(),
                contents: format!("\"value_{i}\"").into(),
            },
            Atom {
                face: operator_face(),
                contents: ");".into(),
            },
        ],
        4 => vec![
            Atom {
                face: keyword_face(),
                contents: "    const ".into(),
            },
            Atom {
                face: constant_face(),
                contents: format!("MSG_{i}").into(),
            },
            Atom {
                face: operator_face(),
                contents: ": &str = ".into(),
            },
            Atom {
                face: string_face(),
                contents: format!("\"Hello from module {i}, processing data\"").into(),
            },
            Atom {
                face: operator_face(),
                contents: ";".into(),
            },
        ],
        5 => vec![
            Atom {
                face: Face::default(),
                contents: "    ".into(),
            },
            Atom {
                face: keyword_face(),
                contents: "if ".into(),
            },
            Atom {
                face: ident_face(),
                contents: format!("count_{i}").into(),
            },
            Atom {
                face: operator_face(),
                contents: " > ".into(),
            },
            Atom {
                face: literal_face(),
                contents: format!("{}", i * 10).into(),
            },
            Atom {
                face: operator_face(),
                contents: " {".into(),
            },
        ],
        6 => vec![Atom {
            face: comment_face(),
            contents: format!("// 処理{i}: データ変換と検証").into(),
        }],
        7 => vec![
            Atom {
                face: Face {
                    attributes: Attributes::BOLD,
                    ..error_face()
                },
                contents: "ERROR".into(),
            },
            Atom {
                face: operator_face(),
                contents: ": ".into(),
            },
            Atom {
                face: Face {
                    attributes: Attributes::ITALIC | Attributes::UNDERLINE,
                    ..string_face()
                },
                contents: format!("\"unexpected token at line {i}\"").into(),
            },
        ],
        _ => unreachable!(),
    }
}

fn realistic_state(line_count: usize) -> AppState {
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
    state.lines = (0..line_count).map(make_realistic_line).collect();
    state.status_line = vec![Atom {
        face: Face::default(),
        contents: " NORMAL ".into(),
    }];
    state.status_mode_line = vec![Atom {
        face: Face::default(),
        contents: "normal".into(),
    }];
    state
}

/// Render a full frame with realistic data and return the diffs.
fn generate_realistic_diffs(cols: u16, rows: u16, line_count: usize) -> Vec<CellDiff> {
    let mut state = realistic_state(line_count);
    state.cols = cols;
    state.rows = rows;
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
    grid.clear(&state.default_face);
    paint::paint(&element, &layout, &mut grid, &state);
    grid.diff()
}

// ---------------------------------------------------------------------------
// JSON fixture builders (for E2E benchmarks)
// ---------------------------------------------------------------------------

#[derive(Serialize)]
struct JsonRpcMsg<P: Serialize> {
    jsonrpc: &'static str,
    method: &'static str,
    params: P,
}

fn to_json_bytes<P: Serialize>(method: &'static str, params: P) -> Vec<u8> {
    serde_json::to_vec(&JsonRpcMsg {
        jsonrpc: "2.0",
        method,
        params,
    })
    .expect("fixture serialization should not fail")
}

fn draw_json(line_count: usize) -> Vec<u8> {
    let lines: Vec<Line> = (0..line_count).map(make_colored_line).collect();
    let default_face = Face {
        fg: Color::Named(NamedColor::White),
        bg: Color::Named(NamedColor::Black),
        ..Face::default()
    };
    let padding_face = default_face;
    to_json_bytes(
        "draw",
        (
            &lines,
            &Coord::default(),
            &default_face,
            &padding_face,
            0u16,
        ),
    )
}

fn draw_realistic_json(line_count: usize) -> Vec<u8> {
    let lines: Vec<Line> = (0..line_count).map(make_realistic_line).collect();
    let default_face = Face {
        fg: Color::Named(NamedColor::White),
        bg: Color::Named(NamedColor::Black),
        ..Face::default()
    };
    let padding_face = default_face;
    to_json_bytes(
        "draw",
        (
            &lines,
            &Coord::default(),
            &default_face,
            &padding_face,
            0u16,
        ),
    )
}

// ---------------------------------------------------------------------------
// Benchmarks
// ---------------------------------------------------------------------------

fn bench_backend_draw(c: &mut Criterion) {
    let mut group = c.benchmark_group("backend_draw");

    // Full redraw at various sizes
    for (cols, rows, lines, label) in [(80, 24, 23, "80x24"), (200, 60, 59, "200x60")] {
        let diffs = generate_diffs(cols, rows, lines);
        group.bench_function(BenchmarkId::new("full_redraw", label), |b| {
            let mut backend = MockBackend::new();
            b.iter(|| {
                backend.begin_frame().unwrap();
                backend.draw(&diffs).unwrap();
                backend.end_frame().unwrap();
                let bytes = backend.bytes_generated();
                backend.flush();
                criterion::black_box(bytes)
            });
        });
    }

    // Incremental: 1 line changed (~80 diffs)
    {
        let diffs = generate_incremental_diffs();
        group.bench_function("incremental_1line", |b| {
            let mut backend = MockBackend::new();
            b.iter(|| {
                backend.begin_frame().unwrap();
                backend.draw(&diffs).unwrap();
                backend.end_frame().unwrap();
                let bytes = backend.bytes_generated();
                backend.flush();
                criterion::black_box(bytes)
            });
        });
    }

    // Full redraw with realistic data (diverse faces → more style changes)
    {
        let diffs = generate_realistic_diffs(80, 24, 23);
        group.bench_function(BenchmarkId::new("full_redraw_realistic", "80x24"), |b| {
            let mut backend = MockBackend::new();
            b.iter(|| {
                backend.begin_frame().unwrap();
                backend.draw(&diffs).unwrap();
                backend.end_frame().unwrap();
                let bytes = backend.bytes_generated();
                backend.flush();
                criterion::black_box(bytes)
            });
        });
    }

    group.finish();
}

/// Benchmark draw_grid (optimized path with cursor auto-advance + incremental SGR)
fn bench_draw_grid(c: &mut Criterion) {
    let mut group = c.benchmark_group("draw_grid");

    // Full redraw
    for (cols, rows, lines, label) in [(80, 24, 23, "80x24"), (200, 60, 59, "200x60")] {
        let grid = generate_grid(cols, rows, lines);
        group.bench_function(BenchmarkId::new("full_redraw", label), |b| {
            let mut backend = MockBackend::new();
            b.iter(|| {
                backend.begin_frame().unwrap();
                backend.draw_grid(&grid).unwrap();
                backend.end_frame().unwrap();
                let bytes = backend.bytes_generated();
                backend.flush();
                criterion::black_box(bytes)
            });
        });
    }

    // Incremental: 1 line changed
    {
        let grid = generate_incremental_grid();
        group.bench_function("incremental_1line", |b| {
            let mut backend = MockBackend::new();
            b.iter(|| {
                backend.begin_frame().unwrap();
                backend.draw_grid(&grid).unwrap();
                backend.end_frame().unwrap();
                let bytes = backend.bytes_generated();
                backend.flush();
                criterion::black_box(bytes)
            });
        });
    }

    // Realistic data
    {
        let grid = generate_realistic_grid(80, 24, 23);
        group.bench_function(BenchmarkId::new("full_redraw_realistic", "80x24"), |b| {
            let mut backend = MockBackend::new();
            b.iter(|| {
                backend.begin_frame().unwrap();
                backend.draw_grid(&grid).unwrap();
                backend.end_frame().unwrap();
                let bytes = backend.bytes_generated();
                backend.flush();
                criterion::black_box(bytes)
            });
        });
    }

    group.finish();
}

/// Benchmark: compare escape byte output between draw() and draw_grid()
fn bench_sgr_bytes(c: &mut Criterion) {
    let mut group = c.benchmark_group("sgr_bytes");

    let diffs = generate_realistic_diffs(80, 24, 23);
    let grid = generate_realistic_grid(80, 24, 23);

    group.bench_function("draw_old", |b| {
        let mut backend = MockBackend::new();
        b.iter(|| {
            backend.draw(&diffs).unwrap();
            let bytes = backend.bytes_generated();
            backend.flush();
            criterion::black_box(bytes)
        });
    });

    group.bench_function("draw_grid_new", |b| {
        let mut backend = MockBackend::new();
        b.iter(|| {
            backend.draw_grid(&grid).unwrap();
            let bytes = backend.bytes_generated();
            backend.flush();
            criterion::black_box(bytes)
        });
    });

    group.finish();
}

/// E2E pipeline: JSON bytes → parse → apply → render → diff → backend.draw → escape bytes
fn bench_e2e_pipeline(c: &mut Criterion) {
    let mut group = c.benchmark_group("e2e_pipeline");

    let registry = PluginRuntime::new();

    // E2E: parse JSON → apply → render → diff → backend.draw (uniform data)
    {
        let json = draw_json(23);
        let base_state = typical_state(23);
        let mut grid = CellGrid::new(base_state.cols, base_state.rows);
        let mut backend = MockBackend::new();

        group.bench_function("json_to_escape_80x24", |b| {
            b.iter_batched(
                || (json.clone(), base_state.clone()),
                |(mut buf, mut state)| {
                    let request = parse_request(&mut buf).unwrap();
                    state.apply(request);
                    let _result = render_pipeline(&state, &registry.view(), &mut grid);
                    let diffs = grid.diff();
                    backend.begin_frame().unwrap();
                    backend.draw(&diffs).unwrap();
                    backend.end_frame().unwrap();
                    let bytes = backend.bytes_generated();
                    backend.flush();
                    grid.swap();
                    bytes
                },
                BatchSize::SmallInput,
            );
        });
    }

    // E2E with realistic data
    {
        let json = draw_realistic_json(23);
        let base_state = realistic_state(23);
        let mut grid = CellGrid::new(base_state.cols, base_state.rows);
        let mut backend = MockBackend::new();

        group.bench_function("json_to_escape_realistic", |b| {
            b.iter_batched(
                || (json.clone(), base_state.clone()),
                |(mut buf, mut state)| {
                    let request = parse_request(&mut buf).unwrap();
                    state.apply(request);
                    let _result = render_pipeline(&state, &registry.view(), &mut grid);
                    let diffs = grid.diff();
                    backend.begin_frame().unwrap();
                    backend.draw(&diffs).unwrap();
                    backend.end_frame().unwrap();
                    let bytes = backend.bytes_generated();
                    backend.flush();
                    grid.swap();
                    bytes
                },
                BatchSize::SmallInput,
            );
        });
    }

    group.finish();
}

criterion_group!(
    benches,
    bench_backend_draw,
    bench_draw_grid,
    bench_sgr_bytes,
    bench_e2e_pipeline
);
criterion_main!(benches);
