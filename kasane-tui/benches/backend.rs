use criterion::{BatchSize, BenchmarkId, Criterion, criterion_group, criterion_main};
use crossterm::{
    cursor, queue,
    style::{self, Attribute as CtAttribute, SetAttribute},
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
use kasane_core::render::{Cell, CellGrid, CursorStyle, RenderResult, render_pipeline};
use kasane_core::state::AppState;
use kasane_tui::sgr::emit_sgr_diff;
use serde::Serialize;

// ---------------------------------------------------------------------------
// MockBackend — same diff+escape logic as TuiBackend, writes to Vec<u8>
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

        let mut last_face: Option<Face> = None;
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

                let face = &cell.face;
                if last_face.as_ref() != Some(face) {
                    emit_sgr_diff(&mut self.buf, last_face.as_ref(), face).unwrap();
                    last_face = Some(*face);
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

    fn invalidate(&mut self) {
        self.previous.clear();
    }

    fn bytes_generated(&self) -> usize {
        self.buf.len()
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

/// Benchmark present() — integrated diff + escape sequence generation
fn bench_present(c: &mut Criterion) {
    let mut group = c.benchmark_group("present");

    // Full redraw at various sizes
    for (cols, rows, lines, label) in [(80, 24, 23, "80x24"), (200, 60, 59, "200x60")] {
        group.bench_function(BenchmarkId::new("full_redraw", label), |b| {
            let mut grid = generate_grid(cols, rows, lines);
            let mut backend = MockBackend::new();
            b.iter(|| {
                backend.invalidate();
                grid.mark_all_dirty();
                backend.present(&mut grid, default_result());
                let bytes = backend.bytes_generated();
                backend.flush();
                criterion::black_box(bytes)
            });
        });
    }

    // Incremental: 1 line changed
    {
        group.bench_function("incremental_1line", |b| {
            let mut grid = generate_incremental_grid();
            let mut backend = MockBackend::new();
            // Populate previous from the grid's built-in previous (set by swap)
            // The incremental grid was produced by: paint frame 1 → swap → paint frame 2.
            // iter_diffs works because the grid carries its own previous buffer.
            // For MockBackend, we need to seed `previous` from the grid's swap state.
            // Do one full present to seed, then on each iteration re-mark and re-present.
            grid.mark_all_dirty();
            backend.present(&mut grid, default_result());
            backend.flush();
            // Now the grid's dirty is cleared but content is the "edited" state.
            // For incremental bench we want to diff the "before→after" each iteration.
            // Re-generate each time since present mutates dirty state:
            b.iter_batched(
                || generate_incremental_grid(),
                |mut g| {
                    // MockBackend already has "before" as previous from the first present.
                    backend.present(&mut g, default_result());
                    let bytes = backend.bytes_generated();
                    backend.flush();
                    criterion::black_box(bytes)
                },
                BatchSize::SmallInput,
            );
        });
    }

    // Realistic data
    {
        group.bench_function(BenchmarkId::new("full_redraw_realistic", "80x24"), |b| {
            let mut grid = generate_realistic_grid(80, 24, 23);
            let mut backend = MockBackend::new();
            b.iter(|| {
                backend.invalidate();
                grid.mark_all_dirty();
                backend.present(&mut grid, default_result());
                let bytes = backend.bytes_generated();
                backend.flush();
                criterion::black_box(bytes)
            });
        });
    }

    group.finish();
}

/// E2E pipeline: JSON bytes → parse → apply → render → present → escape bytes
fn bench_e2e_pipeline(c: &mut Criterion) {
    let mut group = c.benchmark_group("e2e_pipeline");

    let registry = PluginRuntime::new();

    // E2E: parse JSON → apply → render → present (uniform data)
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
                    let (result, _) = render_pipeline(&state, &registry.view(), &mut grid);
                    backend.present(&mut grid, result);
                    let bytes = backend.bytes_generated();
                    backend.flush();
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
                    let (result, _) = render_pipeline(&state, &registry.view(), &mut grid);
                    backend.present(&mut grid, result);
                    let bytes = backend.bytes_generated();
                    backend.flush();
                    bytes
                },
                BatchSize::SmallInput,
            );
        });
    }

    group.finish();
}

criterion_group!(benches, bench_present, bench_e2e_pipeline);
criterion_main!(benches);
