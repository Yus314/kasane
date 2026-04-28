//! Latency budget tests — safety net for catastrophic regressions (10x) in CI.
//!
//! These tests use wall-clock time with generous budgets designed for shared CI runners.
//! Run with: `cargo test -p kasane-core --test latency_budget -- --ignored`

use std::time::Instant;

use kasane_core::layout::Rect;
use kasane_core::layout::flex;
use kasane_core::plugin::PluginRuntime;
use kasane_core::protocol::{Atom, Color, Face, KakouneRequest, NamedColor, parse_request};
use kasane_core::render::CellGrid;
use kasane_core::render::paint;
use kasane_core::render::view;
use kasane_core::state::AppState;

const RUNS: usize = 100;

fn typical_state(line_count: usize) -> AppState {
    let mut state = AppState::default();
    state.runtime.cols = 80;
    state.runtime.rows = 24;
    state.observed.default_style = Face {
        fg: Color::Named(NamedColor::White),
        bg: Color::Named(NamedColor::Black),
        ..Face::default()
    }
    .into();
    state.observed.padding_style = state.observed.default_style.clone();
    state.observed.status_default_style = Face {
        fg: Color::Named(NamedColor::Cyan),
        bg: Color::Named(NamedColor::Black),
        ..Face::default()
    }
    .into();
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
    state.observed.lines = (0..line_count)
        .map(|i| {
            vec![
                Atom::from_face(keyword_face, "let"),
                Atom::from_face(Face::default(), " "),
                Atom::from_face(ident_face, format!("var_{i}")),
                Atom::from_face(Face::default(), " = "),
                Atom::from_face(literal_face, format!("\"{i}_value\"")),
                Atom::from_face(Face::default(), ";"),
            ]
        })
        .collect();
    state.inference.status_line = vec![Atom::from_face(Face::default(), " NORMAL ")];
    state.observed.status_mode_line = vec![Atom::from_face(Face::default(), "normal")];
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
    let registry = PluginRuntime::new();
    let area = Rect {
        x: 0,
        y: 0,
        w: state.runtime.cols,
        h: state.runtime.rows,
    };
    let mut grid = CellGrid::new(state.runtime.cols, state.runtime.rows);

    // Warmup
    for _ in 0..20 {
        let element = view::view(&state, &registry.view());
        let layout = flex::place(&element, area, &state);
        grid.clear(&state.observed.default_style.to_face());
        paint::paint(&element, &layout, &mut grid, &state);
        let _ = grid.diff();
        grid.swap();
    }

    let durations: Vec<u128> = (0..RUNS)
        .map(|_| {
            let start = Instant::now();
            let element = view::view(&state, &registry.view());
            let layout = flex::place(&element, area, &state);
            grid.clear(&state.observed.default_style.to_face());
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
    // Build a 100-line draw JSON message. Atom no longer derives Serialize
    // (its style_id is host-side state), so we construct the wire-format
    // JSON directly with Face values.
    let default_face = Face {
        fg: Color::Named(NamedColor::White),
        bg: Color::Named(NamedColor::Black),
        ..Face::default()
    };
    let cursor_pos = kasane_core::protocol::Coord::default();
    let lines_json: Vec<Vec<serde_json::Value>> = (0..100)
        .map(|i| {
            vec![serde_json::json!({
                "face": Face::default(),
                "contents": format!("line {i}"),
            })]
        })
        .collect();
    let json = serde_json::to_vec(&serde_json::json!({
        "jsonrpc": "2.0",
        "method": "draw",
        "params": [lines_json, cursor_pos, default_face, default_face, 0]
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
            .map(|i| vec![Atom::from_face(Face::default(), format!("line {i}"))])
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

#[test]
#[ignore]
fn salsa_full_frame_under_2ms() {
    use kasane_core::render::render_pipeline_cached;
    use kasane_core::salsa_db::KasaneDatabase;
    use kasane_core::salsa_sync::{
        SalsaInputHandles, sync_display_directives, sync_inputs_from_state,
        sync_plugin_contributions,
    };
    use kasane_core::state::DirtyFlags;

    let state = typical_state(23);
    let registry = PluginRuntime::new();
    let mut grid = CellGrid::new(state.runtime.cols, state.runtime.rows);
    let mut db = KasaneDatabase::default();
    let mut handles = SalsaInputHandles::new(&mut db);
    let dirty = DirtyFlags::ALL;

    // Warmup
    for _ in 0..20 {
        sync_inputs_from_state(&mut db, &state, &handles);
        sync_display_directives(&mut db, &state, &registry.view(), &handles);
        sync_plugin_contributions(&mut db, &state, &registry.view(), &mut handles);
        let (_result, _) = render_pipeline_cached(
            &db,
            &handles,
            &state,
            &registry.view(),
            &mut grid,
            dirty,
            Default::default(),
        );
        let _ = grid.diff();
        grid.swap();
    }

    let durations: Vec<u128> = (0..RUNS)
        .map(|_| {
            let start = Instant::now();
            sync_inputs_from_state(&mut db, &state, &handles);
            sync_display_directives(&mut db, &state, &registry.view(), &handles);
            sync_plugin_contributions(&mut db, &state, &registry.view(), &mut handles);
            let (_result, _) = render_pipeline_cached(
                &db,
                &handles,
                &state,
                &registry.view(),
                &mut grid,
                dirty,
                Default::default(),
            );
            let _ = grid.diff();
            grid.swap();
            start.elapsed().as_micros()
        })
        .collect();

    let med = median_us(durations);
    assert!(
        med < 2000,
        "salsa_full_frame median {med}μs exceeds 2ms budget"
    );
}
