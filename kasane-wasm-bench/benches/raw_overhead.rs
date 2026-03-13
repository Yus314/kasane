use criterion::{Criterion, criterion_group, criterion_main};
use wasmtime::*;

const WAT_NOOP: &str = r#"(module (func (export "noop")))"#;

const WAT_ADD: &str = r#"(module
    (func (export "add") (param i32 i32) (result i32)
        local.get 0
        local.get 1
        i32.add
    )
)"#;

const WAT_HOST_IMPORT: &str = r#"(module
    (import "host" "get_value" (func $get_value (result i32)))
    (func (export "call_host") (result i32)
        call $get_value
    )
    (func (export "call_host_10x") (result i32)
        (local $sum i32)
        (local $i i32)
        (local.set $i (i32.const 10))
        (block $break
            (loop $loop
                (local.set $sum (i32.add (local.get $sum) (call $get_value)))
                (local.set $i (i32.sub (local.get $i) (i32.const 1)))
                (br_if $break (i32.eqz (local.get $i)))
                (br $loop)
            )
        )
        (local.get $sum)
    )
)"#;

// ---------------------------------------------------------------------------
// R6: Native baseline
// ---------------------------------------------------------------------------

#[inline(never)]
fn native_noop() {}

#[inline(never)]
fn native_add(a: i32, b: i32) -> i32 {
    a + b
}

// ---------------------------------------------------------------------------
// Benchmarks
// ---------------------------------------------------------------------------

fn bench_instantiation(c: &mut Criterion) {
    let mut group = c.benchmark_group("instantiation");

    group.bench_function("engine_new", |b| {
        b.iter(|| Engine::default());
    });

    let engine = Engine::default();
    group.bench_function("module_new_noop", |b| {
        b.iter(|| Module::new(&engine, WAT_NOOP).unwrap());
    });

    let module = Module::new(&engine, WAT_NOOP).unwrap();
    group.bench_function("instance_new_noop", |b| {
        b.iter(|| {
            let mut store = Store::new(&engine, ());
            Instance::new(&mut store, &module, &[]).unwrap()
        });
    });

    group.finish();
}

fn bench_empty_call(c: &mut Criterion) {
    let mut group = c.benchmark_group("empty_call");

    // R1: WASM empty call
    let engine = Engine::default();
    let module = Module::new(&engine, WAT_NOOP).unwrap();
    let mut store = Store::new(&engine, ());
    let instance = Instance::new(&mut store, &module, &[]).unwrap();
    let noop = instance
        .get_typed_func::<(), ()>(&mut store, "noop")
        .unwrap();

    group.bench_function("wasm_noop", |b| {
        b.iter(|| noop.call(&mut store, ()).unwrap());
    });

    // R6: Native baseline
    group.bench_function("native_noop", |b| {
        b.iter(|| native_noop());
    });

    group.finish();
}

fn bench_integer_call(c: &mut Criterion) {
    let mut group = c.benchmark_group("integer_call");

    // R2: WASM integer call
    let engine = Engine::default();
    let module = Module::new(&engine, WAT_ADD).unwrap();
    let mut store = Store::new(&engine, ());
    let instance = Instance::new(&mut store, &module, &[]).unwrap();
    let add = instance
        .get_typed_func::<(i32, i32), i32>(&mut store, "add")
        .unwrap();

    group.bench_function("wasm_add", |b| {
        b.iter(|| add.call(&mut store, (1, 2)).unwrap());
    });

    // R6: Native baseline
    group.bench_function("native_add", |b| {
        b.iter(|| native_add(1, 2));
    });

    group.finish();
}

fn bench_host_import(c: &mut Criterion) {
    let mut group = c.benchmark_group("host_import");

    let engine = Engine::default();
    let module = Module::new(&engine, WAT_HOST_IMPORT).unwrap();
    let mut linker = Linker::new(&engine);
    linker
        .func_wrap("host", "get_value", |caller: Caller<'_, u32>| -> i32 {
            *caller.data() as i32
        })
        .unwrap();

    let mut store = Store::new(&engine, 42u32);
    let instance = linker.instantiate(&mut store, &module).unwrap();

    let call_host = instance
        .get_typed_func::<(), i32>(&mut store, "call_host")
        .unwrap();
    let call_host_10x = instance
        .get_typed_func::<(), i32>(&mut store, "call_host_10x")
        .unwrap();

    // R3: Single host import call
    group.bench_function("1x", |b| {
        b.iter(|| call_host.call(&mut store, ()).unwrap());
    });

    // R4: 10x host import calls in loop
    group.bench_function("10x", |b| {
        b.iter(|| call_host_10x.call(&mut store, ()).unwrap());
    });

    group.finish();
}

criterion_group!(
    benches,
    bench_instantiation,
    bench_empty_call,
    bench_integer_call,
    bench_host_import,
);
criterion_main!(benches);
