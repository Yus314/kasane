// W0-5: Component Model benchmarks
// Compares Component Model (WIT) overhead vs raw module overhead.

use criterion::{Criterion, criterion_group, criterion_main};
use kasane_wasm_bench::load_wasm_fixture;
use wasmtime::component::HasSelf;
use wasmtime::component::{Component, Linker};
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

impl Default for HostState {
    fn default() -> Self {
        Self {
            cursor_line: 10,
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

fn add_host_to_linker(linker: &mut Linker<HostState>) {
    wasmtime_wasi::p2::add_to_linker_sync(linker).unwrap();
    bindings::kasane::bench::host_api::add_to_linker::<HostState, HasSelf<HostState>>(
        linker,
        |state: &mut HostState| state,
    )
    .unwrap();
}

fn setup_component() -> (Store<HostState>, BenchPlugin) {
    let mut config = Config::new();
    config.wasm_component_model(true);
    let engine = Engine::new(&config).unwrap();
    let wasm_bytes = load_wasm_fixture("component-plugin.wasm").unwrap();
    let component = Component::new(&engine, &wasm_bytes).unwrap();

    let mut linker: Linker<HostState> = Linker::new(&engine);
    add_host_to_linker(&mut linker);

    let mut store = Store::new(&engine, HostState::default());
    let instance = BenchPlugin::instantiate(&mut store, &component, &linker).unwrap();
    (store, instance)
}

// ---------------------------------------------------------------------------
// Benchmarks
// ---------------------------------------------------------------------------

fn bench_cm_instantiation(c: &mut Criterion) {
    let mut group = c.benchmark_group("cm_instantiation");

    let mut config = Config::new();
    config.wasm_component_model(true);
    let engine = Engine::new(&config).unwrap();
    let wasm_bytes = load_wasm_fixture("component-plugin.wasm").unwrap();

    // Component compilation
    group.bench_function("component_new", |b| {
        b.iter(|| Component::new(&engine, &wasm_bytes).unwrap());
    });

    // Component instantiation (with compiled component)
    let component = Component::new(&engine, &wasm_bytes).unwrap();
    let mut linker: Linker<HostState> = Linker::new(&engine);
    add_host_to_linker(&mut linker);
    group.bench_function("component_instantiate", |b| {
        b.iter(|| {
            let mut store = Store::new(&engine, HostState::default());
            BenchPlugin::instantiate(&mut store, &component, &linker).unwrap()
        });
    });

    group.finish();
}

fn bench_cm_calls(c: &mut Criterion) {
    let mut group = c.benchmark_group("cm_calls");
    let (mut store, instance) = setup_component();

    let plugin = instance.kasane_bench_plugin_api();

    // C1: noop
    group.bench_function("noop", |b| {
        b.iter(|| plugin.call_noop(&mut store).unwrap());
    });

    // C2: integer add
    group.bench_function("add", |b| {
        b.iter(|| plugin.call_add(&mut store, 1, 2).unwrap());
    });

    // C3: echo string (100 bytes)
    let input_100 = "x".repeat(100);
    group.bench_function("echo_string_100", |b| {
        b.iter(|| plugin.call_echo_string(&mut store, &input_100).unwrap());
    });

    // echo string (10 bytes)
    let input_10 = "x".repeat(10);
    group.bench_function("echo_string_10", |b| {
        b.iter(|| plugin.call_echo_string(&mut store, &input_10).unwrap());
    });

    // C4: build gutter (24 lines)
    group.bench_function("build_gutter_24", |b| {
        b.iter(|| plugin.call_build_gutter(&mut store, 24).unwrap());
    });

    // C5: on_state_changed (3 host calls inside)
    group.bench_function("on_state_changed", |b| {
        b.iter(|| plugin.call_on_state_changed(&mut store, 0x01).unwrap());
    });

    // C6: contribute_lines (24 lines, 1 host call inside)
    group.bench_function("contribute_lines_24", |b| {
        b.iter(|| plugin.call_contribute_lines(&mut store, 0, 24).unwrap());
    });

    // Full cycle: state_changed + contribute_lines
    group.bench_function("full_cycle", |b| {
        b.iter(|| {
            plugin.call_on_state_changed(&mut store, 0x01).unwrap();
            plugin.call_contribute_lines(&mut store, 0, 24).unwrap()
        });
    });

    group.finish();
}

criterion_group!(benches, bench_cm_instantiation, bench_cm_calls,);
criterion_main!(benches);
