//! Latency budget tests — safety net for catastrophic regressions (10x) in CI.
//!
//! These tests use wall-clock time with generous budgets designed for shared CI runners.
//! Run with: `cargo test -p kasane-core --test latency_budget -- --ignored`

use std::time::Instant;

use kasane_core::layout::Rect;
use kasane_core::layout::flex;
use kasane_core::plugin::PluginRegistry;
use kasane_core::protocol::{Atom, Color, Face, KakouneRequest, NamedColor, parse_request};
use kasane_core::render::CellGrid;
use kasane_core::render::paint;
use kasane_core::render::view;
use kasane_core::state::AppState;

const RUNS: usize = 100;

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
    state.lines = (0..line_count)
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
    state
}

/// Measure median of RUNS iterations (in microseconds).
fn median_us(mut durations: Vec<u128>) -> u128 {
    durations.sort();
    durations[durations.len() / 2]
}

#[test]
#[ignore]
fn full_frame_under_2ms() {
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
    for _ in 0..20 {
        let element = view::view(&state, &registry);
        let layout = flex::place(&element, area, &state);
        grid.clear(&state.default_face);
        paint::paint(&element, &layout, &mut grid, &state);
        let _ = grid.diff();
        grid.swap();
    }

    let durations: Vec<u128> = (0..RUNS)
        .map(|_| {
            let start = Instant::now();
            let element = view::view(&state, &registry);
            let layout = flex::place(&element, area, &state);
            grid.clear(&state.default_face);
            paint::paint(&element, &layout, &mut grid, &state);
            let _ = grid.diff();
            grid.swap();
            start.elapsed().as_micros()
        })
        .collect();

    let med = median_us(durations);
    assert!(med < 2000, "full_frame median {med}μs exceeds 2ms budget");
}

#[test]
#[ignore]
fn parse_request_under_500us() {
    // Build a 100-line draw JSON message
    let lines: Vec<kasane_core::protocol::Line> = (0..100)
        .map(|i| {
            vec![Atom {
                face: Face::default(),
                contents: format!("line {i}").into(),
            }]
        })
        .collect();
    let default_face = Face {
        fg: Color::Named(NamedColor::White),
        bg: Color::Named(NamedColor::Black),
        ..Face::default()
    };
    let cursor_pos = kasane_core::protocol::Coord::default();
    let json = serde_json::to_vec(&serde_json::json!({
        "jsonrpc": "2.0",
        "method": "draw",
        "params": [lines, cursor_pos, default_face, default_face, 0]
    }))
    .unwrap();

    // Warmup
    for _ in 0..20 {
        let mut buf = json.clone();
        let _ = parse_request(&mut buf).unwrap();
    }

    let durations: Vec<u128> = (0..RUNS)
        .map(|_| {
            let mut buf = json.clone();
            let start = Instant::now();
            let _ = parse_request(&mut buf).unwrap();
            start.elapsed().as_micros()
        })
        .collect();

    let med = median_us(durations);
    assert!(
        med < 500,
        "parse_request (100 lines) median {med}μs exceeds 500μs budget"
    );
}

#[test]
#[ignore]
fn state_apply_under_200us() {
    let draw = KakouneRequest::Draw {
        lines: (0..23)
            .map(|i| {
                vec![Atom {
                    face: Face::default(),
                    contents: format!("line {i}").into(),
                }]
            })
            .collect(),
        cursor_pos: kasane_core::protocol::Coord::default(),
        default_face: Face {
            fg: Color::Named(NamedColor::White),
            bg: Color::Named(NamedColor::Black),
            ..Face::default()
        },
        padding_face: Face {
            fg: Color::Named(NamedColor::White),
            bg: Color::Named(NamedColor::Black),
            ..Face::default()
        },
        widget_columns: 0,
    };
    let base = typical_state(23);

    // Warmup
    for _ in 0..20 {
        let mut state = base.clone();
        state.apply(draw.clone());
    }

    let durations: Vec<u128> = (0..RUNS)
        .map(|_| {
            let mut state = base.clone();
            let start = Instant::now();
            state.apply(draw.clone());
            start.elapsed().as_micros()
        })
        .collect();

    let med = median_us(durations);
    assert!(
        med < 200,
        "state_apply (draw 23 lines) median {med}μs exceeds 200μs budget"
    );
}
