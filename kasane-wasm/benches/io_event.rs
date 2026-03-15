// Phase P-3: I/O event delivery benchmarks
// Measures the cost of delivering IoEvent to WASM plugins via the Plugin trait.

use criterion::{BenchmarkId, Criterion, criterion_group, criterion_main};
use kasane_core::input::{Key, KeyEvent, Modifiers};
use kasane_core::plugin::{IoEvent, Plugin, ProcessEvent};
use kasane_core::state::{AppState, DirtyFlags};

fn load_fuzzy_finder() -> kasane_wasm::WasmPlugin {
    let loader = kasane_wasm::WasmPluginLoader::new().expect("failed to create loader");
    let bytes =
        kasane_wasm::load_wasm_fixture("fuzzy-finder.wasm").expect("failed to load fixture");
    let config = kasane_wasm::WasiCapabilityConfig {
        data_base_dir: std::env::temp_dir().join("kasane_bench_fuzzy_finder"),
        ..Default::default()
    };
    loader.load(&bytes, &config).expect("failed to load plugin")
}

fn load_cursor_line() -> kasane_wasm::WasmPlugin {
    let loader = kasane_wasm::WasmPluginLoader::new().expect("failed to create loader");
    let bytes = kasane_wasm::load_wasm_fixture("cursor-line.wasm").expect("failed to load fixture");
    loader
        .load(&bytes, &kasane_wasm::WasiCapabilityConfig::default())
        .expect("failed to load plugin")
}

// ---------------------------------------------------------------------------
// B1: on_io_event call cost (stdout event delivery)
// ---------------------------------------------------------------------------

fn bench_io_event_stdout(c: &mut Criterion) {
    let mut group = c.benchmark_group("io_event");
    let mut plugin = load_fuzzy_finder();
    let state = AppState::default();

    // Activate the plugin first (Ctrl+P)
    let ctrl_p = KeyEvent {
        key: Key::Char('p'),
        modifiers: Modifiers::CTRL,
    };
    plugin.on_init(&state);
    let _ = plugin.handle_key(&ctrl_p, &state);

    // Small stdout event (single line)
    let small_event = IoEvent::Process(ProcessEvent::Stdout {
        job_id: 1,
        data: b"src/main.rs\n".to_vec(),
    });

    group.bench_function("stdout_small", |b| {
        b.iter(|| plugin.on_io_event(&small_event, &state));
    });

    // Medium stdout event (10 file paths)
    let medium_data: Vec<u8> = (0..10)
        .map(|i| format!("src/module_{i}/lib.rs\n"))
        .collect::<String>()
        .into_bytes();
    let medium_event = IoEvent::Process(ProcessEvent::Stdout {
        job_id: 1,
        data: medium_data,
    });

    group.bench_function("stdout_medium_10", |b| {
        b.iter(|| plugin.on_io_event(&medium_event, &state));
    });

    // Large stdout event (100 file paths)
    let large_data: Vec<u8> = (0..100)
        .map(|i| format!("src/deeply/nested/module_{i}/component.rs\n"))
        .collect::<String>()
        .into_bytes();
    let large_event = IoEvent::Process(ProcessEvent::Stdout {
        job_id: 1,
        data: large_data,
    });

    group.bench_function("stdout_large_100", |b| {
        b.iter(|| plugin.on_io_event(&large_event, &state));
    });

    group.finish();
}

// ---------------------------------------------------------------------------
// B2: on_io_event exit event (triggers line splitting + state transition)
// ---------------------------------------------------------------------------

fn bench_io_event_exit(c: &mut Criterion) {
    let mut group = c.benchmark_group("io_event_exit");

    // Measure exit event processing with varying accumulated data
    for file_count in [10, 50, 100] {
        group.bench_with_input(
            BenchmarkId::new("process_exit", file_count),
            &file_count,
            |b, &count| {
                b.iter_batched(
                    || {
                        // Setup: create fresh plugin, activate, feed stdout data
                        let mut plugin = load_fuzzy_finder();
                        let state = AppState::default();
                        plugin.on_init(&state);
                        let ctrl_p = KeyEvent {
                            key: Key::Char('p'),
                            modifiers: Modifiers::CTRL,
                        };
                        let _ = plugin.handle_key(&ctrl_p, &state);

                        let data: Vec<u8> = (0..count)
                            .map(|i| format!("src/file_{i}.rs\n"))
                            .collect::<String>()
                            .into_bytes();
                        let stdout_event =
                            IoEvent::Process(ProcessEvent::Stdout { job_id: 1, data });
                        plugin.on_io_event(&stdout_event, &state);
                        (plugin, state)
                    },
                    |(mut plugin, state)| {
                        let exit_event = IoEvent::Process(ProcessEvent::Exited {
                            job_id: 1,
                            exit_code: 0,
                        });
                        plugin.on_io_event(&exit_event, &state)
                    },
                    criterion::BatchSize::SmallInput,
                );
            },
        );
    }

    group.finish();
}

// ---------------------------------------------------------------------------
// B3: handle_key cost (when plugin is active vs inactive)
// ---------------------------------------------------------------------------

fn bench_handle_key(c: &mut Criterion) {
    let mut group = c.benchmark_group("handle_key");

    // Inactive plugin: should pass through immediately
    {
        let mut plugin = load_fuzzy_finder();
        let state = AppState::default();
        plugin.on_init(&state);
        let key = KeyEvent {
            key: Key::Char('a'),
            modifiers: Modifiers::empty(),
        };

        group.bench_function("inactive_passthrough", |b| {
            b.iter(|| plugin.handle_key(&key, &state));
        });
    }

    // Active plugin: consumes key input
    {
        let mut plugin = load_fuzzy_finder();
        let state = AppState::default();
        plugin.on_init(&state);
        let ctrl_p = KeyEvent {
            key: Key::Char('p'),
            modifiers: Modifiers::CTRL,
        };
        let _ = plugin.handle_key(&ctrl_p, &state);

        let key = KeyEvent {
            key: Key::Char('a'),
            modifiers: Modifiers::empty(),
        };

        group.bench_function("active_char_input", |b| {
            b.iter(|| plugin.handle_key(&key, &state));
        });
    }

    group.finish();
}

// ---------------------------------------------------------------------------
// B4: Comparison with cursor-line (no I/O) for baseline
// ---------------------------------------------------------------------------

fn bench_cursor_line_baseline(c: &mut Criterion) {
    let mut group = c.benchmark_group("baseline");
    let mut plugin = load_cursor_line();
    let mut state = AppState::default();
    state.cursor_pos.line = 10;

    group.bench_function("cursor_line_on_state_changed", |b| {
        b.iter(|| {
            state.cursor_pos.line += 1;
            plugin.on_state_changed(&state, DirtyFlags::BUFFER)
        });
    });

    group.finish();
}

criterion_group!(
    benches,
    bench_io_event_stdout,
    bench_io_event_exit,
    bench_handle_key,
    bench_cursor_line_baseline,
);
criterion_main!(benches);
