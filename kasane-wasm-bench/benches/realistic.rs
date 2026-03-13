// W0-6: Realistic simulation benchmarks
// Simulates actual plugin usage patterns with Component Model.

use criterion::{BenchmarkId, Criterion, criterion_group, criterion_main};
use kasane_wasm_bench::load_wasm_fixture;
use wasmtime::component::{Component, HasSelf, Linker};
use wasmtime::{Config, Engine, Store};
use wasmtime_wasi::WasiCtxBuilder;

mod bindings {
    wasmtime::component::bindgen!({
        world: "bench-plugin",
        path: "wit",
    });
}

use bindings::BenchPlugin;

struct HostState {
    cursor_line: i32,
    cursor_col: i32,
    line_count: u32,
    cols: u16,
    rows: u16,
    focused: bool,
    wasi: wasmtime_wasi::WasiCtx,
    table: wasmtime::component::ResourceTable,
}

impl HostState {
    fn new(cursor_line: i32) -> Self {
        Self {
            cursor_line,
            cursor_col: 5,
            line_count: 100,
            cols: 80,
            rows: 24,
            focused: true,
            wasi: WasiCtxBuilder::new().build(),
            table: wasmtime::component::ResourceTable::new(),
        }
    }
}

impl wasmtime_wasi::WasiView for HostState {
    fn ctx(&mut self) -> wasmtime_wasi::WasiCtxView<'_> {
        wasmtime_wasi::WasiCtxView {
            ctx: &mut self.wasi,
            table: &mut self.table,
        }
    }
}

impl bindings::kasane::bench::host_api::Host for HostState {
    fn get_cursor_line(&mut self) -> i32 {
        self.cursor_line
    }
    fn get_cursor_col(&mut self) -> i32 {
        self.cursor_col
    }
    fn get_line_count(&mut self) -> u32 {
        self.line_count
    }
    fn get_cols(&mut self) -> u16 {
        self.cols
    }
    fn get_rows(&mut self) -> u16 {
        self.rows
    }
    fn is_focused(&mut self) -> bool {
        self.focused
    }
}

struct PluginSetup {
    engine: Engine,
    component: Component,
    linker: Linker<HostState>,
}

fn create_setup() -> PluginSetup {
    let mut config = Config::new();
    config.wasm_component_model(true);
    let engine = Engine::new(&config).unwrap();
    let wasm_bytes = load_wasm_fixture("component-plugin.wasm").unwrap();
    let component = Component::new(&engine, &wasm_bytes).unwrap();

    let mut linker: Linker<HostState> = Linker::new(&engine);
    wasmtime_wasi::p2::add_to_linker_sync(&mut linker).unwrap();
    bindings::kasane::bench::host_api::add_to_linker::<HostState, HasSelf<HostState>>(
        &mut linker,
        |state| state,
    )
    .unwrap();

    PluginSetup {
        engine,
        component,
        linker,
    }
}

// ---------------------------------------------------------------------------
// S1: CursorLinePlugin equivalent (single plugin)
// ---------------------------------------------------------------------------

fn bench_cursor_line_plugin(c: &mut Criterion) {
    let mut group = c.benchmark_group("cursor_line_plugin");
    let setup = create_setup();

    let mut store = Store::new(&setup.engine, HostState::new(10));
    let instance = BenchPlugin::instantiate(&mut store, &setup.component, &setup.linker).unwrap();
    let plugin = instance.kasane_bench_plugin_api();

    // S1: Full CursorLinePlugin equivalent cycle
    group.bench_function("full_cycle", |b| {
        b.iter(|| {
            store.data_mut().cursor_line += 1;
            plugin.call_on_state_changed(&mut store, 0x01).unwrap();
            plugin.call_contribute_lines(&mut store, 0, 24).unwrap()
        });
    });

    // Cache hit: no state change, just check
    group.bench_function("cache_hit_no_call", |b| {
        b.iter(|| {
            // In a real cache-hit scenario, we wouldn't call WASM at all.
            // This measures the host-side overhead of checking the cache.
            std::hint::black_box(42u64); // simulate hash check
        });
    });

    group.finish();
}

// ---------------------------------------------------------------------------
// S2: LineNumbers plugin equivalent (single plugin)
// ---------------------------------------------------------------------------

fn bench_line_numbers_plugin(c: &mut Criterion) {
    let setup = create_setup();
    let mut store = Store::new(&setup.engine, HostState::new(10));
    let instance = BenchPlugin::instantiate(&mut store, &setup.component, &setup.linker).unwrap();
    let plugin = instance.kasane_bench_plugin_api();

    c.bench_function("line_numbers_gutter_24", |b| {
        b.iter(|| plugin.call_build_gutter(&mut store, 24).unwrap());
    });
}

// ---------------------------------------------------------------------------
// S3-S5: Multi-plugin scaling
// ---------------------------------------------------------------------------

fn bench_multi_plugin(c: &mut Criterion) {
    let mut group = c.benchmark_group("multi_plugin");

    for count in [1, 3, 5, 10] {
        let setup = create_setup();
        let mut store = Store::new(&setup.engine, HostState::new(10));

        // Instantiate N copies of the same plugin
        let mut instances = Vec::new();
        for _ in 0..count {
            instances.push(
                BenchPlugin::instantiate(&mut store, &setup.component, &setup.linker).unwrap(),
            );
        }

        // Full frame: state_changed + contribute_lines for each plugin
        group.bench_with_input(
            BenchmarkId::new("full_frame", count),
            &count,
            |b, &_count| {
                b.iter(|| {
                    store.data_mut().cursor_line += 1;
                    for inst in &instances {
                        let plugin = inst.kasane_bench_plugin_api();
                        plugin.call_on_state_changed(&mut store, 0x01).unwrap();
                        plugin.call_contribute_lines(&mut store, 0, 24).unwrap();
                    }
                });
            },
        );
    }

    group.finish();
}

// ---------------------------------------------------------------------------
// S7: Memory overhead
// ---------------------------------------------------------------------------

fn bench_memory_overhead(c: &mut Criterion) {
    // Not a criterion benchmark — just measure and print
    let setup = create_setup();

    // Measure instantiation time for N instances
    let mut group = c.benchmark_group("instantiation_scaling");
    for count in [1, 5, 10] {
        group.bench_with_input(
            BenchmarkId::new("instantiate_n", count),
            &count,
            |b, &count| {
                b.iter(|| {
                    let mut store = Store::new(&setup.engine, HostState::new(10));
                    for _ in 0..count {
                        BenchPlugin::instantiate(&mut store, &setup.component, &setup.linker)
                            .unwrap();
                    }
                });
            },
        );
    }
    group.finish();
}

// ---------------------------------------------------------------------------
// S8: Native comparison baseline
// ---------------------------------------------------------------------------

fn bench_native_baseline(c: &mut Criterion) {
    let mut group = c.benchmark_group("native_baseline");

    // Simulate what CursorLinePlugin does natively
    let mut active_line: i32 = 0;
    let cursor_line: i32 = 10;

    // Native state_changed + contribute_lines equivalent
    group.bench_function("native_cursor_line_full", |b| {
        b.iter(|| {
            // state_changed
            active_line = cursor_line;

            // contribute_lines (24 lines)
            let decorations: Vec<Option<(u8, u8, u8)>> = (0..24)
                .map(|line| {
                    if line == active_line {
                        Some((40, 40, 50))
                    } else {
                        None
                    }
                })
                .collect();
            std::hint::black_box(&decorations);
        });
    });

    // Native gutter build (24 lines)
    group.bench_function("native_gutter_24", |b| {
        b.iter(|| {
            let lines: Vec<String> = (1..=24).map(|i| format!("{:>3} ", i)).collect();
            std::hint::black_box(&lines);
        });
    });

    group.finish();
}

criterion_group!(
    benches,
    bench_cursor_line_plugin,
    bench_line_numbers_plugin,
    bench_multi_plugin,
    bench_memory_overhead,
    bench_native_baseline,
);
criterion_main!(benches);
