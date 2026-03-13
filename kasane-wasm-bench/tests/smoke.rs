use wasmtime::*;

#[test]
fn wasmtime_loads_and_runs() {
    let engine = Engine::default();
    let module = Module::new(&engine, r#"(module (func (export "noop")))"#).unwrap();
    let mut store = Store::new(&engine, ());
    let instance = Instance::new(&mut store, &module, &[]).unwrap();
    let noop = instance
        .get_typed_func::<(), ()>(&mut store, "noop")
        .unwrap();
    noop.call(&mut store, ()).unwrap();
}

#[test]
fn wasmtime_integer_roundtrip() {
    let engine = Engine::default();
    let module = Module::new(
        &engine,
        r#"(module
            (func (export "add") (param i32 i32) (result i32)
                local.get 0
                local.get 1
                i32.add
            )
        )"#,
    )
    .unwrap();
    let mut store = Store::new(&engine, ());
    let instance = Instance::new(&mut store, &module, &[]).unwrap();
    let add = instance
        .get_typed_func::<(i32, i32), i32>(&mut store, "add")
        .unwrap();
    assert_eq!(add.call(&mut store, (3, 4)).unwrap(), 7);
}

#[test]
fn wasmtime_host_function() {
    let engine = Engine::default();
    let module = Module::new(
        &engine,
        r#"(module
            (import "host" "get_value" (func $get_value (result i32)))
            (func (export "call_host") (result i32)
                call $get_value
            )
        )"#,
    )
    .unwrap();
    let mut linker = Linker::new(&engine);
    linker
        .func_wrap("host", "get_value", |caller: Caller<'_, i32>| -> i32 {
            *caller.data()
        })
        .unwrap();
    let mut store = Store::new(&engine, 42i32);
    let instance = linker.instantiate(&mut store, &module).unwrap();
    let call_host = instance
        .get_typed_func::<(), i32>(&mut store, "call_host")
        .unwrap();
    assert_eq!(call_host.call(&mut store, ()).unwrap(), 42);
}

#[test]
fn wasmtime_memory_string_roundtrip() {
    let engine = Engine::default();
    // A module that exports memory and a function to write "hello" at offset 0
    let module = Module::new(
        &engine,
        r#"(module
            (memory (export "memory") 1)
            (data (i32.const 0) "hello world")
            (func (export "get_ptr") (result i32) (i32.const 0))
            (func (export "get_len") (result i32) (i32.const 11))
        )"#,
    )
    .unwrap();
    let mut store = Store::new(&engine, ());
    let instance = Instance::new(&mut store, &module, &[]).unwrap();

    let get_ptr = instance
        .get_typed_func::<(), i32>(&mut store, "get_ptr")
        .unwrap();
    let get_len = instance
        .get_typed_func::<(), i32>(&mut store, "get_len")
        .unwrap();

    let ptr = get_ptr.call(&mut store, ()).unwrap() as usize;
    let len = get_len.call(&mut store, ()).unwrap() as usize;

    let memory = instance.get_memory(&mut store, "memory").unwrap();
    let data = &memory.data(&store)[ptr..ptr + len];
    let s = std::str::from_utf8(data).unwrap();
    assert_eq!(s, "hello world");
}
