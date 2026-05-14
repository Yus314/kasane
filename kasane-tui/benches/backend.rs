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
    Atom, Brush, Color, Coord, DecorationStyle, FontSlant, FontWeight, Line, NamedColor, Style,
    TextDecoration, WireFace, parse_request,
};
use kasane_core::render::paint;
use kasane_core::render::view;
use kasane_core::render::{
    Cell, CellGrid, CursorStyle, RenderPipelineOptions, RenderResult, TerminalStyle,
    render_pipeline_cached,
};
use kasane_core::state::{AppState, DirtyFlags};
use kasane_internal::salsa_db::KasaneDatabase;
use kasane_internal::salsa_sync::{
    SalsaInputHandles, sync_display_directives, sync_inputs_from_state, sync_plugin_contributions,
};
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
    let keyword_style = Style {
        fg: Brush::rgb(255, 100, 0),
        ..Style::default()
    };
    let ident_style = Style {
        fg: Brush::rgb(0, 200, 100),
        ..Style::default()
    };
    let literal_style = Style {
        fg: Brush::rgb(100, 100, 255),
        ..Style::default()
    };
    let plain_style = Style::default();

    vec![
        Atom::with_style("let", keyword_style),
        Atom::with_style(" ", plain_style.clone()),
        Atom::with_style(format!("var_{i}"), ident_style),
        Atom::with_style(" = ", plain_style.clone()),
        Atom::with_style(format!("\"{i}_value\""), literal_style),
        Atom::with_style(";", plain_style),
    ]
}

fn typical_state(line_count: usize) -> AppState {
    let mut state = AppState::default();
    state.runtime.cols = 80;
    state.runtime.rows = 24;
    state.observed.default_style = Style {
        fg: Brush::Named(NamedColor::White),
        bg: Brush::Named(NamedColor::Black),
        ..Style::default()
    };
    state.observed.padding_style = state.observed.default_style.clone();
    state.observed.status_default_style = Style {
        fg: Brush::Named(NamedColor::Cyan),
        bg: Brush::Named(NamedColor::Black),
        ..Style::default()
    };
    state.observed.lines = std::sync::Arc::new((0..line_count).map(make_colored_line).collect());
    state.inference.status_line = vec![Atom::with_style(" NORMAL ", Style::default())];
    state.observed.status_mode_line = vec![Atom::with_style("normal", Style::default())];
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
        Atom::with_style(
            "edited_line_10",
            Style {
                fg: Brush::rgb(255, 0, 0),
                ..Style::default()
            },
        ),
        Atom::with_style(" // modified", Style::default()),
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
// Realistic fixture builders (diverse styles, varied line lengths, wide chars)
// ---------------------------------------------------------------------------

fn keyword_style() -> Style {
    Style {
        fg: Brush::rgb(255, 100, 0),
        ..Style::default()
    }
}

fn ident_style() -> Style {
    Style {
        fg: Brush::rgb(0, 200, 100),
        ..Style::default()
    }
}

fn literal_style() -> Style {
    Style {
        fg: Brush::rgb(100, 100, 255),
        ..Style::default()
    }
}

fn comment_style() -> Style {
    Style {
        fg: Brush::rgb(128, 128, 128),
        font_slant: FontSlant::Italic,
        ..Style::default()
    }
}

fn type_style() -> Style {
    Style {
        fg: Brush::Named(NamedColor::Cyan),
        ..Style::default()
    }
}

fn operator_style() -> Style {
    Style {
        fg: Brush::Named(NamedColor::White),
        ..Style::default()
    }
}

fn string_style() -> Style {
    Style {
        fg: Brush::Named(NamedColor::Yellow),
        ..Style::default()
    }
}

fn error_style() -> Style {
    Style {
        fg: Brush::Named(NamedColor::BrightRed),
        font_weight: FontWeight::BOLD,
        underline: Some(TextDecoration {
            style: DecorationStyle::Solid,
            color: Brush::Default,
            thickness: None,
        }),
        ..Style::default()
    }
}

fn namespace_style() -> Style {
    Style {
        fg: Brush::Named(NamedColor::Magenta),
        ..Style::default()
    }
}

fn constant_style() -> Style {
    Style {
        fg: Brush::Named(NamedColor::BrightBlue),
        ..Style::default()
    }
}

fn make_realistic_line(i: usize) -> Vec<Atom> {
    match i % 8 {
        0 => vec![], // empty line
        1 => vec![Atom::with_style(
            format!("// comment line {i}"),
            comment_style(),
        )],
        2 => vec![
            Atom::with_style("fn ", keyword_style()),
            Atom::with_style(format!("process_{i}"), ident_style()),
            Atom::with_style("(", operator_style()),
            Atom::with_style("u32", type_style()),
            Atom::with_style(") {", operator_style()),
        ],
        3 => vec![
            Atom::with_style("    let ", keyword_style()),
            Atom::with_style(format!("result_{i}"), ident_style()),
            Atom::with_style(" = ", operator_style()),
            Atom::with_style("self", namespace_style()),
            Atom::with_style(".", operator_style()),
            Atom::with_style(format!("compute_{i}"), ident_style()),
            Atom::with_style("(", operator_style()),
            Atom::with_style(format!("{}", i * 42), literal_style()),
            Atom::with_style(", ", operator_style()),
            Atom::with_style(format!("\"value_{i}\""), string_style()),
            Atom::with_style(");", operator_style()),
        ],
        4 => vec![
            Atom::with_style("    const ", keyword_style()),
            Atom::with_style(format!("MSG_{i}"), constant_style()),
            Atom::with_style(": &str = ", operator_style()),
            Atom::with_style(
                format!("\"Hello from module {i}, processing data\""),
                string_style(),
            ),
            Atom::with_style(";", operator_style()),
        ],
        5 => vec![
            Atom::with_style("    ", Style::default()),
            Atom::with_style("if ", keyword_style()),
            Atom::with_style(format!("count_{i}"), ident_style()),
            Atom::with_style(" > ", operator_style()),
            Atom::with_style(format!("{}", i * 10), literal_style()),
            Atom::with_style(" {", operator_style()),
        ],
        6 => vec![Atom::with_style(
            format!("// 処理{i}: データ変換と検証"),
            comment_style(),
        )],
        7 => vec![
            Atom::with_style(
                "ERROR",
                Style {
                    font_weight: FontWeight::BOLD,
                    ..error_style()
                },
            ),
            Atom::with_style(": ", operator_style()),
            Atom::with_style(
                format!("\"unexpected token at line {i}\""),
                Style {
                    font_slant: FontSlant::Italic,
                    underline: Some(TextDecoration {
                        style: DecorationStyle::Solid,
                        color: Brush::Default,
                        thickness: None,
                    }),
                    ..string_style()
                },
            ),
        ],
        _ => unreachable!(),
    }
}

fn realistic_state(line_count: usize) -> AppState {
    let mut state = AppState::default();
    state.runtime.cols = 80;
    state.runtime.rows = 24;
    state.observed.default_style = Style {
        fg: Brush::Named(NamedColor::White),
        bg: Brush::Named(NamedColor::Black),
        ..Style::default()
    };
    state.observed.padding_style = state.observed.default_style.clone();
    state.observed.status_default_style = Style {
        fg: Brush::Named(NamedColor::Cyan),
        bg: Brush::Named(NamedColor::Black),
        ..Style::default()
    };
    state.observed.lines = std::sync::Arc::new((0..line_count).map(make_realistic_line).collect());
    state.inference.status_line = vec![Atom::with_style(" NORMAL ", Style::default())];
    state.observed.status_mode_line = vec![Atom::with_style("normal", Style::default())];
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

fn default_face_wire() -> WireFace {
    Style {
        fg: Brush::Named(NamedColor::White),
        bg: Brush::Named(NamedColor::Black),
        ..Style::default()
    }
    .to_face()
}

fn draw_json(line_count: usize) -> Vec<u8> {
    let lines: Vec<Line> = (0..line_count).map(make_colored_line).collect();
    let wire_lines = lines_to_wire(&lines);
    let default_face = default_face_wire();
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
    let default_face = default_face_wire();
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
            grid.mark_all_dirty();
            backend.present(&mut grid, default_result());
            backend.flush();
            b.iter_batched(
                || generate_incremental_grid(),
                |mut g| {
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
                    let mut db = KasaneDatabase::default();
                    let mut handles = SalsaInputHandles::new(&mut db);
                    sync_inputs_from_state(&mut db, &state, &handles);
                    sync_display_directives(&mut db, &state, &registry.view(), &handles);
                    sync_plugin_contributions(
                        &mut db,
                        &state,
                        &registry.view(),
                        &mut handles,
                        DirtyFlags::ALL,
                    );
                    let (result, _) = render_pipeline_cached(
                        &db,
                        &handles,
                        &state,
                        &registry.view(),
                        &mut grid,
                        DirtyFlags::ALL,
                        RenderPipelineOptions::default(),
                    );
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
                    let mut db = KasaneDatabase::default();
                    let mut handles = SalsaInputHandles::new(&mut db);
                    sync_inputs_from_state(&mut db, &state, &handles);
                    sync_display_directives(&mut db, &state, &registry.view(), &handles);
                    sync_plugin_contributions(
                        &mut db,
                        &state,
                        &registry.view(),
                        &mut handles,
                        DirtyFlags::ALL,
                    );
                    let (result, _) = render_pipeline_cached(
                        &db,
                        &handles,
                        &state,
                        &registry.view(),
                        &mut grid,
                        DirtyFlags::ALL,
                        RenderPipelineOptions::default(),
                    );
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
