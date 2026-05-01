//! Allocation budget tracker.
//!
//! Runs the rendering pipeline once and reports per-phase allocation counts as JSON.
//! Used in CI to detect allocation regressions deterministically.
//!
//! ```sh
//! cargo run -p kasane-core --bin alloc_budget --features bench-alloc
//! ```

#[cfg(not(feature = "bench-alloc"))]
compile_error!(
    "alloc_budget requires the `bench-alloc` feature: cargo run -p kasane-core --bin alloc_budget --features bench-alloc"
);

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
fn main() {
    use kasane_core::layout::Rect;
    use kasane_core::layout::flex;
    use kasane_core::plugin::PluginRuntime;
    use kasane_core::protocol::{Atom, Color, Coord, NamedColor, WireFace, parse_request};
    use kasane_core::render::CellGrid;
    use kasane_core::render::paint;
    use kasane_core::render::view;
    use kasane_core::state::AppState;

    // Build test state (outside measurement)
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
    state.observed.lines = std::sync::Arc::new(
        (0..23)
            .map(|i| {
                vec![
                    Atom::from_wire(
                        WireFace {
                            fg: Color::Rgb {
                                r: 255,
                                g: 100,
                                b: 0,
                            },
                            bg: Color::Default,
                            ..WireFace::default()
                        },
                        "let",
                    ),
                    Atom::plain(" "),
                    Atom::from_wire(
                        WireFace {
                            fg: Color::Rgb {
                                r: 0,
                                g: 200,
                                b: 100,
                            },
                            bg: Color::Default,
                            ..WireFace::default()
                        },
                        format!("var_{i}"),
                    ),
                    Atom::plain(" = "),
                    Atom::from_wire(
                        WireFace {
                            fg: Color::Rgb {
                                r: 100,
                                g: 100,
                                b: 255,
                            },
                            bg: Color::Default,
                            ..WireFace::default()
                        },
                        format!("\"{i}_value\""),
                    ),
                    Atom::plain(";"),
                ]
            })
            .collect(),
    );
    state.inference.status_line = vec![Atom::plain(" NORMAL ")];
    state.observed.status_mode_line = vec![Atom::plain("normal")];

    let registry = PluginRuntime::new();
    let area = Rect {
        x: 0,
        y: 0,
        w: state.runtime.cols,
        h: state.runtime.rows,
    };
    let mut grid = CellGrid::new(state.runtime.cols, state.runtime.rows);

    // --- Measure per-phase allocations ---

    // view
    alloc_counter::reset();
    let element = view::view(&state, &registry.view());
    let (view_count, view_bytes) = alloc_counter::snapshot();

    // place
    alloc_counter::reset();
    let layout = flex::place(&element, area, &state);
    let (place_count, place_bytes) = alloc_counter::snapshot();

    // clear + paint
    alloc_counter::reset();
    grid.clear(&kasane_core::render::TerminalStyle::from_style(
        &state.observed.default_style,
    ));
    paint::paint(&element, &layout, &mut grid, &state);
    let (paint_count, paint_bytes) = alloc_counter::snapshot();

    // diff
    alloc_counter::reset();
    let _diffs = grid.diff();
    let (diff_count, diff_bytes) = alloc_counter::snapshot();

    // swap
    alloc_counter::reset();
    grid.swap();
    let (swap_count, swap_bytes) = alloc_counter::snapshot();

    // parse_request (100-line draw)
    // The wire format expects `Atom { face, contents }`-shaped JSON; the
    // post-closure `Atom` is opaque to the wire format (carries
    // `Arc<UnresolvedStyle>`), so build a local wire shape for the JSON.
    #[derive(serde::Serialize)]
    struct WireAtomBudget<'a> {
        face: WireFace,
        contents: &'a str,
    }
    let plain_face = WireFace::default();
    let line_strings: Vec<String> = (0..100).map(|i| format!("line {i}")).collect();
    let draw_lines: Vec<Vec<WireAtomBudget<'_>>> = line_strings
        .iter()
        .map(|s| {
            vec![WireAtomBudget {
                face: plain_face,
                contents: s.as_str(),
            }]
        })
        .collect();
    let default_face = WireFace {
        fg: Color::Named(NamedColor::White),
        bg: Color::Named(NamedColor::Black),
        ..WireFace::default()
    };
    let cursor_pos = Coord::default();
    let json_msg = serde_json::to_vec(&serde_json::json!({
        "jsonrpc": "2.0",
        "method": "draw",
        "params": [draw_lines, cursor_pos, default_face, default_face, 0]
    }))
    .unwrap();

    alloc_counter::reset();
    let mut buf = json_msg;
    let _ = parse_request(&mut buf).unwrap();
    let (parse_count, parse_bytes) = alloc_counter::snapshot();

    // Total
    let total_count = view_count + place_count + paint_count + diff_count + swap_count;
    let total_bytes = view_bytes + place_bytes + paint_bytes + diff_bytes + swap_bytes;

    // Output as JSON
    println!(
        r#"{{"full_frame":{{"count":{},"bytes":{}}},"view":{{"count":{},"bytes":{}}},"place":{{"count":{},"bytes":{}}},"paint":{{"count":{},"bytes":{}}},"diff":{{"count":{},"bytes":{}}},"swap":{{"count":{},"bytes":{}}},"parse_request_100":{{"count":{},"bytes":{}}}}}"#,
        total_count,
        total_bytes,
        view_count,
        view_bytes,
        place_count,
        place_bytes,
        paint_count,
        paint_bytes,
        diff_count,
        diff_bytes,
        swap_count,
        swap_bytes,
        parse_count,
        parse_bytes,
    );
}
