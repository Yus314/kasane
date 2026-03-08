mod fixtures;

use criterion::{Criterion, criterion_group, criterion_main};
use kasane_core::plugin::PluginRegistry;
use kasane_core::protocol::parse_request;
use kasane_core::render::{CellGrid, render_pipeline};

use fixtures::{draw_json, draw_status_json, menu_show_json, set_cursor_json, typical_state};

// ---------------------------------------------------------------------------
// Trace generators — build message sequences programmatically
// ---------------------------------------------------------------------------

/// Normal editing: draw_status + set_cursor + draw repeated (typing simulation)
fn generate_normal_editing() -> Vec<Vec<u8>> {
    let mut msgs = Vec::with_capacity(50);
    for _ in 0..16 {
        msgs.push(draw_status_json());
        msgs.push(set_cursor_json());
        msgs.push(draw_json(23));
    }
    // Pad to ~50
    msgs.push(draw_status_json());
    msgs.push(set_cursor_json());
    msgs.truncate(50);
    msgs
}

/// Fast scroll: every message is a full-screen draw (page scroll simulation)
fn generate_fast_scroll() -> Vec<Vec<u8>> {
    (0..100).map(|_| draw_json(23)).collect()
}

/// Menu completion: menu_show → several draws → menu_show (different sizes) → draw
fn generate_menu_completion() -> Vec<Vec<u8>> {
    let mut msgs = Vec::with_capacity(20);
    // Show menu
    msgs.push(menu_show_json(10));
    msgs.push(draw_json(23));
    msgs.push(set_cursor_json());
    // Browse completions (status updates)
    for _ in 0..5 {
        msgs.push(draw_status_json());
        msgs.push(draw_json(23));
    }
    // Larger menu
    msgs.push(menu_show_json(30));
    msgs.push(draw_json(23));
    msgs.push(draw_status_json());
    msgs.push(set_cursor_json());
    msgs.truncate(20);
    msgs
}

/// Mixed session: combination of all the above
fn generate_mixed_session() -> Vec<Vec<u8>> {
    let mut msgs = Vec::with_capacity(200);
    // Typing phase
    for _ in 0..20 {
        msgs.push(draw_status_json());
        msgs.push(set_cursor_json());
        msgs.push(draw_json(23));
    }
    // Scroll phase
    for _ in 0..30 {
        msgs.push(draw_json(23));
    }
    // Menu phase
    msgs.push(menu_show_json(15));
    for _ in 0..10 {
        msgs.push(draw_status_json());
        msgs.push(draw_json(23));
    }
    // More typing
    for _ in 0..20 {
        msgs.push(draw_status_json());
        msgs.push(set_cursor_json());
        msgs.push(draw_json(23));
    }
    msgs.truncate(200);
    msgs
}

// ---------------------------------------------------------------------------
// Replay runner
// ---------------------------------------------------------------------------

fn replay_session(msgs: &[Vec<u8>]) {
    let mut state = typical_state(23);
    let registry = PluginRegistry::new();
    let mut grid = CellGrid::new(state.cols, state.rows);

    for msg in msgs {
        let mut buf = msg.clone();
        let request = parse_request(&mut buf).unwrap();
        state.apply(request);
        let _ = render_pipeline(&state, &registry, &mut grid);
        let _ = grid.diff();
        grid.swap();
    }
}

// ---------------------------------------------------------------------------
// Benchmarks
// ---------------------------------------------------------------------------

fn bench_replay(c: &mut Criterion) {
    let mut group = c.benchmark_group("replay");
    group.sample_size(30);

    let normal = generate_normal_editing();
    group.bench_function("normal_editing_50msg", |b| {
        b.iter(|| replay_session(&normal));
    });

    let scroll = generate_fast_scroll();
    group.bench_function("fast_scroll_100msg", |b| {
        b.iter(|| replay_session(&scroll));
    });

    let menu = generate_menu_completion();
    group.bench_function("menu_completion_20msg", |b| {
        b.iter(|| replay_session(&menu));
    });

    let mixed = generate_mixed_session();
    group.bench_function("mixed_session_200msg", |b| {
        b.iter(|| replay_session(&mixed));
    });

    group.finish();
}

criterion_group!(replay, bench_replay);
criterion_main!(replay);
