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
    use kasane_core::plugin::PluginRegistry;
    use kasane_core::protocol::{Atom, Color, Face, NamedColor, parse_request};
    use kasane_core::render::CellGrid;
    use kasane_core::render::paint;
    use kasane_core::render::view;
    use kasane_core::state::AppState;

    // Build test state (outside measurement)
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
    let area = Rect {
        x: 0,
        y: 0,
        w: state.cols,
        h: state.rows,
    };
    let mut grid = CellGrid::new(state.cols, state.rows);

    // --- Measure per-phase allocations ---

    // view
    alloc_counter::reset();
    let element = view::view(&state, &registry);
    let (view_count, view_bytes) = alloc_counter::snapshot();

    // place
    alloc_counter::reset();
    let layout = flex::place(&element, area, &state);
    let (place_count, place_bytes) = alloc_counter::snapshot();

    // clear + paint
    alloc_counter::reset();
    grid.clear(&state.default_face);
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
    let draw_lines: Vec<kasane_core::protocol::Line> = (0..100)
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
    let json_msg = serde_json::to_vec(&serde_json::json!({
        "jsonrpc": "2.0",
        "method": "draw",
        "params": [draw_lines, default_face, default_face]
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
