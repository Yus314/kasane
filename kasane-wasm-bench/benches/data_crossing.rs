// W0-4: Data crossing benchmarks
// Measures string passing, element construction, and host function density.

use criterion::{BenchmarkId, Criterion, criterion_group, criterion_main};
use kasane_wasm_bench::{decode_element, load_wasm_fixture};
use wasmtime::*;

// ---------------------------------------------------------------------------
// D1/D2/D3: String passing
// ---------------------------------------------------------------------------

fn bench_string_passing(c: &mut Criterion) {
    let mut group = c.benchmark_group("string_passing");

    let engine = Engine::default();
    let wasm_bytes = load_wasm_fixture("string-echo.wasm").unwrap();
    let module = Module::new(&engine, &wasm_bytes).unwrap();
    let mut store = Store::new(&engine, ());
    let instance = Instance::new(&mut store, &module, &[]).unwrap();

    let get_buffer_ptr = instance
        .get_typed_func::<(), i32>(&mut store, "get_buffer_ptr")
        .unwrap();
    let build_string = instance
        .get_typed_func::<i32, i32>(&mut store, "build_string")
        .unwrap();
    let echo = instance
        .get_typed_func::<i32, i32>(&mut store, "echo")
        .unwrap();

    let memory = instance.get_memory(&mut store, "memory").unwrap();
    let buf_ptr = get_buffer_ptr.call(&mut store, ()).unwrap() as usize;

    // D1: Host writes string to guest memory, guest echoes back
    for size in [10, 100, 1000] {
        let input = "x".repeat(size);
        group.bench_with_input(BenchmarkId::new("write_echo", size), &size, |b, &_size| {
            b.iter(|| {
                // Write string to guest memory
                memory.write(&mut store, buf_ptr, input.as_bytes()).unwrap();
                // Guest echoes (returns length)
                let len = echo.call(&mut store, input.len() as i32).unwrap();
                // Read string back from guest memory
                let mut out = vec![0u8; len as usize];
                memory.read(&store, buf_ptr, &mut out).unwrap();
                out
            });
        });
    }

    // D2/D3: Guest builds string of various sizes, host reads
    for size in [10, 100, 1000] {
        group.bench_with_input(
            BenchmarkId::new("guest_build_read", size),
            &size,
            |b, &size| {
                b.iter(|| {
                    let len = build_string.call(&mut store, size as i32).unwrap();
                    let mut out = vec![0u8; len as usize];
                    memory.read(&store, buf_ptr, &mut out).unwrap();
                    out
                });
            },
        );
    }

    group.finish();
}

// ---------------------------------------------------------------------------
// D4/D5: Element construction + decoding
// ---------------------------------------------------------------------------

fn bench_element_construction(c: &mut Criterion) {
    let mut group = c.benchmark_group("element_construction");

    let engine = Engine::default();
    let wasm_bytes = load_wasm_fixture("element-builder.wasm").unwrap();
    let module = Module::new(&engine, &wasm_bytes).unwrap();
    let mut store = Store::new(&engine, ());
    let instance = Instance::new(&mut store, &module, &[]).unwrap();

    let get_buffer_ptr = instance
        .get_typed_func::<(), i32>(&mut store, "get_buffer_ptr")
        .unwrap();
    let build_single_text = instance
        .get_typed_func::<(), i32>(&mut store, "build_single_text")
        .unwrap();
    let build_gutter = instance
        .get_typed_func::<i32, i32>(&mut store, "build_gutter")
        .unwrap();
    let build_nested = instance
        .get_typed_func::<i32, i32>(&mut store, "build_nested")
        .unwrap();

    let memory = instance.get_memory(&mut store, "memory").unwrap();
    let buf_ptr = get_buffer_ptr.call(&mut store, ()).unwrap() as usize;

    // D4a: Single Text element (build + decode)
    group.bench_function("single_text", |b| {
        b.iter(|| {
            let len = build_single_text.call(&mut store, ()).unwrap() as usize;
            let mut data = vec![0u8; len];
            memory.read(&store, buf_ptr, &mut data).unwrap();
            let mut offset = 0;
            decode_element(&data, &mut offset)
        });
    });

    // D4b: 24-line gutter (build + decode)
    group.bench_function("gutter_24", |b| {
        b.iter(|| {
            let len = build_gutter.call(&mut store, 24).unwrap() as usize;
            let mut data = vec![0u8; len];
            memory.read(&store, buf_ptr, &mut data).unwrap();
            let mut offset = 0;
            decode_element(&data, &mut offset)
        });
    });

    // D5a: Nested structure (3 cols × 8 rows = 24 elements)
    group.bench_function("nested_3x8", |b| {
        b.iter(|| {
            let len = build_nested.call(&mut store, 8).unwrap() as usize;
            let mut data = vec![0u8; len];
            memory.read(&store, buf_ptr, &mut data).unwrap();
            let mut offset = 0;
            decode_element(&data, &mut offset)
        });
    });

    // D5b: Nested structure (3 cols × 24 rows = 72 elements)
    group.bench_function("nested_3x24", |b| {
        b.iter(|| {
            let len = build_nested.call(&mut store, 24).unwrap() as usize;
            let mut data = vec![0u8; len];
            memory.read(&store, buf_ptr, &mut data).unwrap();
            let mut offset = 0;
            decode_element(&data, &mut offset)
        });
    });

    // Measure decode cost in isolation (guest builds once, host decodes repeatedly)
    let len = build_gutter.call(&mut store, 24).unwrap() as usize;
    let mut cached_data = vec![0u8; len];
    memory.read(&store, buf_ptr, &mut cached_data).unwrap();
    group.bench_function("decode_only_gutter_24", |b| {
        b.iter(|| {
            let mut offset = 0;
            decode_element(&cached_data, &mut offset)
        });
    });

    group.finish();
}

// ---------------------------------------------------------------------------
// D6/D7: Host function density + state read simulation
// ---------------------------------------------------------------------------

#[derive(Clone)]
struct HostState {
    cursor_line: i32,
    cursor_col: i32,
    line_count: i32,
    cols: i32,
    rows: i32,
    focused: i32,
}

fn bench_host_function_density(c: &mut Criterion) {
    let mut group = c.benchmark_group("host_fn_density");

    let engine = Engine::default();
    let wasm_bytes = load_wasm_fixture("state-reader.wasm").unwrap();
    let module = Module::new(&engine, &wasm_bytes).unwrap();

    let mut linker = Linker::new(&engine);
    linker
        .func_wrap(
            "env",
            "host_get_cursor_line",
            |caller: Caller<'_, HostState>| -> i32 { caller.data().cursor_line },
        )
        .unwrap();
    linker
        .func_wrap(
            "env",
            "host_get_cursor_col",
            |caller: Caller<'_, HostState>| -> i32 { caller.data().cursor_col },
        )
        .unwrap();
    linker
        .func_wrap(
            "env",
            "host_get_line_count",
            |caller: Caller<'_, HostState>| -> i32 { caller.data().line_count },
        )
        .unwrap();
    linker
        .func_wrap(
            "env",
            "host_get_cols",
            |caller: Caller<'_, HostState>| -> i32 { caller.data().cols },
        )
        .unwrap();
    linker
        .func_wrap(
            "env",
            "host_get_rows",
            |caller: Caller<'_, HostState>| -> i32 { caller.data().rows },
        )
        .unwrap();
    linker
        .func_wrap(
            "env",
            "host_is_focused",
            |caller: Caller<'_, HostState>| -> i32 { caller.data().focused },
        )
        .unwrap();

    let host_state = HostState {
        cursor_line: 10,
        cursor_col: 5,
        line_count: 100,
        cols: 80,
        rows: 24,
        focused: 1,
    };

    let mut store = Store::new(&engine, host_state);
    let instance = linker.instantiate(&mut store, &module).unwrap();

    let on_state_changed = instance
        .get_typed_func::<i32, i32>(&mut store, "on_state_changed")
        .unwrap();
    let on_state_changed_heavy = instance
        .get_typed_func::<i32, i32>(&mut store, "on_state_changed_heavy")
        .unwrap();
    let contribute_lines = instance
        .get_typed_func::<(i32, i32), i32>(&mut store, "contribute_lines")
        .unwrap();
    let get_line_buffer_ptr = instance
        .get_typed_func::<(), i32>(&mut store, "get_line_buffer_ptr")
        .unwrap();

    let memory = instance.get_memory(&mut store, "memory").unwrap();

    // D6a: on_state_changed (3 host calls)
    group.bench_function("state_changed_3calls", |b| {
        b.iter(|| {
            // Move cursor to new position each time to ensure work happens
            store.data_mut().cursor_line += 1;
            on_state_changed.call(&mut store, 0x01).unwrap()
        });
    });

    // D6b: on_state_changed_heavy (6 host calls)
    group.bench_function("state_changed_6calls", |b| {
        b.iter(|| {
            store.data_mut().cursor_line += 1;
            on_state_changed_heavy.call(&mut store, 0x01).unwrap()
        });
    });

    // D7: contribute_lines (24 lines) — read result from guest memory
    let line_buf_ptr = get_line_buffer_ptr.call(&mut store, ()).unwrap() as usize;
    group.bench_function("contribute_lines_24", |b| {
        b.iter(|| {
            let count = contribute_lines.call(&mut store, (0, 24)).unwrap();
            // Read results: worst case each entry is 4 bytes (1 tag + 3 rgb)
            let mut out = vec![0u8; count as usize * 4];
            memory.read(&store, line_buf_ptr, &mut out).unwrap();
            out
        });
    });

    // D7b: Full cycle: state_changed + contribute_lines
    group.bench_function("full_cycle_state_lines", |b| {
        b.iter(|| {
            store.data_mut().cursor_line += 1;
            on_state_changed.call(&mut store, 0x01).unwrap();
            let count = contribute_lines.call(&mut store, (0, 24)).unwrap();
            let mut out = vec![0u8; count as usize * 4];
            memory.read(&store, line_buf_ptr, &mut out).unwrap();
            out
        });
    });

    group.finish();
}

criterion_group!(
    benches,
    bench_string_passing,
    bench_element_construction,
    bench_host_function_density,
);
criterion_main!(benches);
