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
    Atom, Attributes, Color, Coord, Line, NamedColor, WireFace, parse_request,
};
use kasane_core::render::paint;
use kasane_core::render::view;
use kasane_core::render::{
    Cell, CellGrid, CursorStyle, RenderResult, TerminalStyle, render_pipeline,
};
use kasane_core::state::AppState;
use kasane_tui::sgr::emit_sgr_diff_style;
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

        let mut last_style: Option<TerminalStyle> = None;
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

                let style = &cell.style;
                if last_style.as_ref() != Some(style) {
                    emit_sgr_diff_style(&mut self.buf, last_style.as_ref(), style).unwrap();
                    last_style = Some(*style);
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
        visual_hints: Default::default(),
    }
}

// ---------------------------------------------------------------------------
// Fixture helpers
// ---------------------------------------------------------------------------

fn make_colored_line(i: usize) -> Vec<Atom> {
    let keyword_face = WireFace {
        fg: Color::Rgb {
            r: 255,
            g: 100,
            b: 0,
        },
        bg: Color::Default,
        ..WireFace::default()
    };
    let ident_face = WireFace {
        fg: Color::Rgb {
            r: 0,
            g: 200,
            b: 100,
        },
        bg: Color::Default,
        ..WireFace::default()
    };
    let literal_face = WireFace {
        fg: Color::Rgb {
            r: 100,
            g: 100,
            b: 255,
        },
        bg: Color::Default,
        ..WireFace::default()
    };
    let plain_face = WireFace::default();

    vec![
        Atom::from_wire(keyword_face, "let"),
        Atom::from_wire(plain_face, " "),
        Atom::from_wire(ident_face, format!("var_{i}")),
        Atom::from_wire(plain_face, " = "),
        Atom::from_wire(literal_face, format!("\"{i}_value\"")),
        Atom::from_wire(plain_face, ";"),
    ]
}

fn typical_state(line_count: usize) -> AppState {
    let mut state = AppState::default();
    state.runtime.cols = 80;
    state.runtime.rows = 24;
    state.observed.default_style = WireFace {
        fg: Color::Named(NamedColor::White),
        bg: Color::Named(NamedColor::Black),
        ..WireFace::default()
    }
    .into();
    state.observed.padding_style = state.observed.default_style.clone();
    state.observed.status_default_style = WireFace {
        fg: Color::Named(NamedColor::Cyan),
        bg: Color::Named(NamedColor::Black),
        ..WireFace::default()
    }
    .into();
    state.observed.lines = std::sync::Arc::new((0..line_count).map(make_colored_line).collect());
    state.inference.status_line = vec![Atom::from_wire(WireFace::default(), " NORMAL ")];
    state.observed.status_mode_line = vec![Atom::from_wire(WireFace::default(), "normal")];
    state
}

/// Render a full frame and return the grid (previous buffer empty = full redraw).
fn generate_grid(cols: u16, rows: u16, line_count: usize) -> CellGrid {
    let mut state = typical_state(line_count);
    state.runtime.cols = cols;
    state.runtime.rows = rows;
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
    grid.clear(&kasane_core::render::TerminalStyle::from_style(
        &state.observed.default_style,
    ));
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
        w: state.runtime.cols,
        h: state.runtime.rows,
    };

    let mut grid = CellGrid::new(state.runtime.cols, state.runtime.rows);

    // "before" frame
    let element = view::view(&state, &registry.view());
    let layout = flex::place(&element, area, &state);
    grid.clear(&kasane_core::render::TerminalStyle::from_style(
        &state.observed.default_style,
    ));
    paint::paint(&element, &layout, &mut grid, &state);
    grid.swap();

    // "after": modify 1 line
    let mut edited = state.clone();
    std::sync::Arc::make_mut(&mut edited.observed.lines)[10] = vec![
        Atom::from_wire(
            WireFace {
                fg: Color::Rgb { r: 255, g: 0, b: 0 },
                bg: Color::Default,
                ..WireFace::default()
            },
            "edited_line_10",
        ),
        Atom::from_wire(WireFace::default(), " // modified"),
    ];

    let element = view::view(&edited, &registry.view());
    let layout = flex::place(&element, area, &edited);
    grid.clear(&kasane_core::render::TerminalStyle::from_style(
        &edited.observed.default_style,
    ));
    paint::paint(&element, &layout, &mut grid, &edited);
    grid
}

/// Generate grid with realistic data.
fn generate_realistic_grid(cols: u16, rows: u16, line_count: usize) -> CellGrid {
    let mut state = realistic_state(line_count);
    state.runtime.cols = cols;
    state.runtime.rows = rows;
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
    grid.clear(&kasane_core::render::TerminalStyle::from_style(
        &state.observed.default_style,
    ));
    paint::paint(&element, &layout, &mut grid, &state);
    grid
}

// ---------------------------------------------------------------------------
// Realistic fixture builders (diverse faces, varied line lengths, wide chars)
// ---------------------------------------------------------------------------

fn keyword_face() -> WireFace {
    WireFace {
        fg: Color::Rgb {
            r: 255,
            g: 100,
            b: 0,
        },
        bg: Color::Default,
        ..WireFace::default()
    }
}

fn ident_face() -> WireFace {
    WireFace {
        fg: Color::Rgb {
            r: 0,
            g: 200,
            b: 100,
        },
        bg: Color::Default,
        ..WireFace::default()
    }
}

fn literal_face() -> WireFace {
    WireFace {
        fg: Color::Rgb {
            r: 100,
            g: 100,
            b: 255,
        },
        bg: Color::Default,
        ..WireFace::default()
    }
}

fn comment_face() -> WireFace {
    WireFace {
        fg: Color::Rgb {
            r: 128,
            g: 128,
            b: 128,
        },
        bg: Color::Default,
        attributes: Attributes::ITALIC,
        ..WireFace::default()
    }
}

fn type_face() -> WireFace {
    WireFace {
        fg: Color::Named(NamedColor::Cyan),
        bg: Color::Default,
        ..WireFace::default()
    }
}

fn operator_face() -> WireFace {
    WireFace {
        fg: Color::Named(NamedColor::White),
        bg: Color::Default,
        ..WireFace::default()
    }
}

fn string_face() -> WireFace {
    WireFace {
        fg: Color::Named(NamedColor::Yellow),
        bg: Color::Default,
        ..WireFace::default()
    }
}

fn error_face() -> WireFace {
    WireFace {
        fg: Color::Named(NamedColor::BrightRed),
        bg: Color::Default,
        attributes: Attributes::BOLD | Attributes::UNDERLINE,
        ..WireFace::default()
    }
}

fn namespace_face() -> WireFace {
    WireFace {
        fg: Color::Named(NamedColor::Magenta),
        bg: Color::Default,
        ..WireFace::default()
    }
}

fn constant_face() -> WireFace {
    WireFace {
        fg: Color::Named(NamedColor::BrightBlue),
        bg: Color::Default,
        ..WireFace::default()
    }
}

fn make_realistic_line(i: usize) -> Vec<Atom> {
    match i % 8 {
        0 => vec![], // empty line
        1 => vec![Atom::from_wire(
            comment_face(),
            format!("// comment line {i}"),
        )],
        2 => vec![
            Atom::from_wire(keyword_face(), "fn "),
            Atom::from_wire(ident_face(), format!("process_{i}")),
            Atom::from_wire(operator_face(), "("),
            Atom::from_wire(type_face(), "u32"),
            Atom::from_wire(operator_face(), ") {"),
        ],
        3 => vec![
            Atom::from_wire(keyword_face(), "    let "),
            Atom::from_wire(ident_face(), format!("result_{i}")),
            Atom::from_wire(operator_face(), " = "),
            Atom::from_wire(namespace_face(), "self"),
            Atom::from_wire(operator_face(), "."),
            Atom::from_wire(ident_face(), format!("compute_{i}")),
            Atom::from_wire(operator_face(), "("),
            Atom::from_wire(literal_face(), format!("{}", i * 42)),
            Atom::from_wire(operator_face(), ", "),
            Atom::from_wire(string_face(), format!("\"value_{i}\"")),
            Atom::from_wire(operator_face(), ");"),
        ],
        4 => vec![
            Atom::from_wire(keyword_face(), "    const "),
            Atom::from_wire(constant_face(), format!("MSG_{i}")),
            Atom::from_wire(operator_face(), ": &str = "),
            Atom::from_wire(
                string_face(),
                format!("\"Hello from module {i}, processing data\""),
            ),
            Atom::from_wire(operator_face(), ";"),
        ],
        5 => vec![
            Atom::from_wire(WireFace::default(), "    "),
            Atom::from_wire(keyword_face(), "if "),
            Atom::from_wire(ident_face(), format!("count_{i}")),
            Atom::from_wire(operator_face(), " > "),
            Atom::from_wire(literal_face(), format!("{}", i * 10)),
            Atom::from_wire(operator_face(), " {"),
        ],
        6 => vec![Atom::from_wire(
            comment_face(),
            format!("// 処理{i}: データ変換と検証"),
        )],
        7 => vec![
            Atom::from_wire(
                WireFace {
                    attributes: Attributes::BOLD,
                    ..error_face()
                },
                "ERROR",
            ),
            Atom::from_wire(operator_face(), ": "),
            Atom::from_wire(
                WireFace {
                    attributes: Attributes::ITALIC | Attributes::UNDERLINE,
                    ..string_face()
                },
                format!("\"unexpected token at line {i}\""),
            ),
        ],
        _ => unreachable!(),
    }
}

fn realistic_state(line_count: usize) -> AppState {
    let mut state = AppState::default();
    state.runtime.cols = 80;
    state.runtime.rows = 24;
    state.observed.default_style = WireFace {
        fg: Color::Named(NamedColor::White),
        bg: Color::Named(NamedColor::Black),
        ..WireFace::default()
    }
    .into();
    state.observed.padding_style = state.observed.default_style.clone();
    state.observed.status_default_style = WireFace {
        fg: Color::Named(NamedColor::Cyan),
        bg: Color::Named(NamedColor::Black),
        ..WireFace::default()
    }
    .into();
    state.observed.lines = std::sync::Arc::new((0..line_count).map(make_realistic_line).collect());
    state.inference.status_line = vec![Atom::from_wire(WireFace::default(), " NORMAL ")];
    state.observed.status_mode_line = vec![Atom::from_wire(WireFace::default(), "normal")];
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

/// Wire-shaped atom for benchmark JSON fixtures. Mirrors `WireAtom` in
/// `kasane_core::protocol::parse` (which is private). `Atom` itself
/// holds an `Arc<UnresolvedStyle>` and is opaque to the wire format,
/// so we project to the legacy `{ face, contents }` shape here.
#[derive(Serialize)]
struct WireAtomBench<'a> {
    face: WireFace,
    contents: &'a str,
}

fn atoms_to_wire(line: &[Atom]) -> Vec<WireAtomBench<'_>> {
    line.iter()
        .map(|a| WireAtomBench {
            face: a.unresolved_style().to_face(),
            contents: a.contents.as_str(),
        })
        .collect()
}

fn lines_to_wire(lines: &[Vec<Atom>]) -> Vec<Vec<WireAtomBench<'_>>> {
    lines.iter().map(|l| atoms_to_wire(l)).collect()
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
    let wire_lines = lines_to_wire(&lines);
    let default_face = WireFace {
        fg: Color::Named(NamedColor::White),
        bg: Color::Named(NamedColor::Black),
        ..WireFace::default()
    };
    let padding_face = default_face;
    to_json_bytes(
        "draw",
        (
            &wire_lines,
            &Coord::default(),
            &default_face,
            &padding_face,
            0u16,
        ),
    )
}

fn draw_realistic_json(line_count: usize) -> Vec<u8> {
    let lines: Vec<Line> = (0..line_count).map(make_realistic_line).collect();
    let wire_lines = lines_to_wire(&lines);
    let default_face = WireFace {
        fg: Color::Named(NamedColor::White),
        bg: Color::Named(NamedColor::Black),
        ..WireFace::default()
    };
    let padding_face = default_face;
    to_json_bytes(
        "draw",
        (
            &wire_lines,
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
                std::hint::black_box(bytes)
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
                    std::hint::black_box(bytes)
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
                std::hint::black_box(bytes)
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
        let mut grid = CellGrid::new(base_state.runtime.cols, base_state.runtime.rows);
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
        let mut grid = CellGrid::new(base_state.runtime.cols, base_state.runtime.rows);
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
