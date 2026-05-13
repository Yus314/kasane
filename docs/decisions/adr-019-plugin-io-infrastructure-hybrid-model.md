# ADR-019: Plugin I/O Infrastructure — Hybrid Model

**Status:** Decided

**Context:**

During Phase P, the design for granting I/O capabilities to plugins was evaluated. Currently, WASM plugins are given no capabilities with `WasiCtxBuilder::new().build()`, and cannot use filesystem, process execution, or network communication. This prevents building plugins such as fuzzy finders, file browsers, and linter integrations.

wasmtime is linked in synchronous mode (`add_to_linker_sync`), and all Plugin trait methods, all WASM calls in adapter.rs, and both event loops (TUI/GUI) operate synchronously.

### 19-1: I/O Architecture Model — Hybrid Model

**Options evaluated:**

| Option | Overview |
|--------|----------|
| A: Host-mediated only | All I/O proxied through `Command` + `update()` |
| B: WASI direct only | Grant all capabilities via `WasiCtxBuilder`, convert wasmtime to async |
| C: Hybrid | Synchronous I/O via WASI direct, asynchronous I/O via host-mediated |

**Decision:** Adopt the hybrid model (C).

**Separation criterion:** "Can it block indefinitely?"

| I/O Operation | Blocking Characteristics | Model |
|---------------|------------------------|-------|
| Filesystem read/write | Typically μs–ms | WASI direct (`preopened_dir`) |
| Environment variable retrieval | ns | WASI direct (`env`) |
| Monotonic clock / random | ns | WASI direct (`inherit_monotonic_clock`) |
| External process execution | Indefinite | Host-mediated (`Command::SpawnProcess`) |
| Network communication | Indefinite | Host-mediated (future) |

**Rationale:**

1. **Avoiding wasmtime async conversion:** Option B (WASI direct only) would require changing from `add_to_linker_sync` → `add_to_linker_async`. This is a large-scale refactoring affecting all 19 methods in adapter.rs, all Plugin trait method signatures, registry.rs, both event loops, and the rendering pipeline — disproportionate in effort and design impact. The hybrid model maintains `add_to_linker_sync` while enabling only the synchronous subset of WASI (`wasi:filesystem`, `wasi:clocks`, `wasi:random`).

2. **WASI specification constraints:** `wasi:cli/command` is a specification for "executing a WASM component as a command," not for "launching arbitrary programs on the host from a guest." Even with option B, process spawning would require a custom host-side WIT interface, arriving at the same structure as host-mediated.

3. **Hot path protection:** With option B, plugins could call `std::process::Command` within `contribute_to()`, causing the rendering thread to block indefinitely. The hybrid model structurally excludes process execution and network communication from the hot path.

4. **Streaming and backpressure:** With host-mediated process execution, the host delivers stdout in 16ms batches and manages buffer size limits and cancellation. These controls are difficult with synchronous pipe processing inside WASM.

5. **Incremental migration path:** The hybrid model is on the incremental migration path toward B. If wasmtime async conversion becomes necessary in the future, the existing `Command::SpawnProcess` + `IoEvent` patterns can be maintained as-is, with additional enabling of `wasi:sockets`, etc.

**Security model:**

| Layer | Mechanism | Control |
|-------|-----------|---------|
| WASI layer (synchronous I/O) | `preopened_dir` | Plugin-dedicated directory only. Manifest declaration + user approval |
| Host layer (asynchronous I/O) | Host-side validation of `Command::SpawnProcess` | Program allow list, argument validation |

**Trade-offs:**

- Plugin authors need to use 2 I/O patterns (files via `std::fs`, processes via `Command`)
- File I/O can still be called within hot paths, requiring documentation warnings and runtime measurement
- NFS / FUSE mounts and other exceptionally slow filesystems risk blocking with synchronous I/O

### 19-2: I/O Event Delivery Method — Unified IoEvent Type

**Options evaluated:**

| Option | Overview |
|--------|----------|
| A: Reuse existing `update(Box<dyn Any>)` | Wrap ProcessEvent in `Box<dyn Any>` and deliver via `deliver_message()` |
| B: Dedicated method `on_process_event()` | Add a ProcessEvent-specific method to Plugin trait |
| C: Unified type `on_io_event(IoEvent)` | Add 1 method to Plugin trait that receives an IoEvent enum |

**Decision:** Adopt the unified IoEvent type (C).

```rust
enum IoEvent {
    Process(ProcessEvent),
    // Future: Http(HttpResponse), FileWatch(FileWatchEvent), ...
}

enum ProcessEvent {
    Stdout { job_id: u64, data: Vec<u8> },
    Stderr { job_id: u64, data: Vec<u8> },
    Exited { job_id: u64, exit_code: i32 },
}

// 1 method added to Plugin trait
fn on_io_event(&mut self, _event: IoEvent, _state: &AppState) -> Vec<Command> {
    vec![]
}
```

**WIT:**

```wit
variant io-event {
    process(process-event),
}

record process-event {
    job-id: u64,
    kind: process-event-kind,
}

variant process-event-kind {
    stdout(list<u8>),
    stderr(list<u8>),
    exited(s32),
}

on-io-event: func(event: io-event) -> list<command>;
```

**Rationale:**

1. **Type safety:** Option A's `Box<dyn Any>` + downcast risks silent ignoring. Option C's structured type enables IDE completion and compile-time verification.

2. **Scalability:** Option B (dedicated methods) would add methods to Plugin trait for each future I/O type (`on_http_response()`, `on_file_changed()`, ...). Option C only adds `IoEvent` variants, leaving Plugin trait unchanged.

3. **Role clarity:** `update()` is dedicated to inter-plugin messages, `on_io_event()` to host I/O completion notifications, `on_state_changed()` to Kakoune protocol state change notifications. Three asynchronous input paths are clearly separated.

4. **WASM compatibility:** Defining `io-event` as a variant type in WIT allows the WASM guest side to receive structured data without tag bytes or serialization conventions.

### 19-3: Sub-phase Structure

Reflecting the decisions of 19-1 and 19-2, the Phase P sub-phases are restructured.

**Problems with the old structure (P-a / P-b / P-c):**

- P-a (async task infrastructure) and P-b (SpawnProcess) have inseparable delivery destination (`IoEvent` type) and delivery source (`ProcessManager`), making separate implementation impossible
- P-c (WASI capabilities) becomes independent of process execution under the hybrid model, enabling earlier implementation

**New structure:**

| Sub-phase | Content | Dependencies |
|-----------|---------|-------------|
| P-1 | WASI capability infrastructure: capability declarations in manifest, per-plugin `WasiCtxBuilder` configuration injection (`preopened_dir`, `env`, `inherit_monotonic_clock`) | None |
| P-2 | Process execution infrastructure: `IoEvent` / `ProcessEvent` type definitions, `Plugin::on_io_event()` + WIT addition, `Command::SpawnProcess` + `ProcessManager`, event loop integration, 16ms batch delivery, job ID / cancel | P-1 (program allow list in manifest) |
| P-3 | Proof-of-concept and stabilization: fuzzy finder reference implementation (WASM guest), runtime frame time measurement, backpressure tuning | P-2 |

**Key changes:**

- Merged P-a/P-b into P-2 (they were inseparable)
- Moved P-c earlier as P-1 (can be implemented independently of process execution)
- Added P-3 (proof-of-concept phase)

**Implementation status:** All sub-phases (P-1, P-2, P-3) are done. For implementation status, see [roadmap.md](./roadmap.md).
