//! ADR-052 buffer-view host-side overhead microbenchmark.
//!
//! Measures the cost the capability-resource subsystem adds *on top
//! of* the bare Component Model call. The bench drives the host
//! trait directly — no WASM crossing — so the numbers isolate the
//! ADR-052 surface (broker check, `ResourceTable::push` /
//! `get` / `delete`, line slice copy) from Wasmtime overhead.
//!
//! Pair with `kasane-wasm-bench/benches/component_model.rs` (Update 1
//! baseline) to estimate the full round-trip a plugin sees:
//!
//!   round-trip ≈ component-model-call + this-bench
//!   ≈ 430–574 ns + this-bench
//!
//! ADR-052 §Exit criterion sets a 5 µs ceiling on the round trip and
//! a >10 µs abandon threshold; the published Wasmtime resource-method
//! cost (~50–200 ns table-lookup) is well inside the budget. This
//! bench ratifies that estimate against Kasane's actual host code.

use criterion::{Criterion, criterion_group, criterion_main};
use kasane_plugin_package::manifest::{
    CapabilitiesSection, PluginManifest, PluginSection, ServiceDeclaration,
};
use kasane_wasm::broker::CapabilityBroker;
use kasane_wasm::buffer_view::BufferViewRep;

fn manifest_declaring_buffer() -> PluginManifest {
    PluginManifest {
        manifest_version: None,
        plugin: PluginSection {
            id: "bench".into(),
            abi_version: "6.5.0".into(),
        },
        capabilities: CapabilitiesSection {
            wasi: Vec::new(),
            env_vars: Vec::new(),
            services: vec![ServiceDeclaration {
                name: "buffer".into(),
            }],
        },
        authorities: Default::default(),
        handlers: Default::default(),
        view: Default::default(),
        settings: Default::default(),
    }
}

use kasane_wasm::bench_api::{Host, HostBufferView, HostState};

fn bench_open_and_drop(c: &mut Criterion) {
    c.bench_function("buffer_view_open_then_drop", |b| {
        // The ResourceTable persists across iterations; the bench
        // measures the steady-state cost of one full acquisition
        // + drop cycle, which is the membrane pattern's hot path.
        b.iter_with_setup(
            || {
                let mut host_state = HostState::default();
                host_state.capability_broker =
                    CapabilityBroker::from_manifest(&manifest_declaring_buffer());
                host_state
            },
            |mut host_state| {
                let handle = host_state.open_buffer_view().expect("declared");
                HostBufferView::drop(&mut host_state, handle).expect("drop");
                std::hint::black_box(host_state);
            },
        );
    });
}

fn bench_get_lines_text(c: &mut Criterion) {
    use kasane_core::protocol::{Atom, Style};
    use std::sync::Arc;

    // Pre-populate 100 short lines — covers the typical buffer-view
    // call shape (1–10 lines requested from a hundreds-of-lines
    // buffer). The host's line slice path is what we want to time.
    let lines: Vec<Vec<Atom>> = (0..100)
        .map(|i| vec![Atom::with_style(format!("line {i:03}"), Style::default())])
        .collect();
    let lines = Arc::new(lines);

    let mut group = c.benchmark_group("buffer_view_get_lines_text");
    for &count in &[1usize, 8, 64] {
        let lines = Arc::clone(&lines);
        group.bench_function(format!("range_{count}"), |b| {
            b.iter_with_setup(
                || {
                    let mut host_state = HostState::default();
                    host_state.capability_broker =
                        CapabilityBroker::from_manifest(&manifest_declaring_buffer());
                    host_state.lines = Arc::clone(&lines);
                    let handle = host_state.open_buffer_view().expect("declared");
                    (host_state, handle)
                },
                |(mut host_state, handle)| {
                    let out = host_state.get_lines_text(handle, 0, count as u32);
                    std::hint::black_box(out);
                },
            );
        });
    }
    group.finish();
}

criterion_group!(benches, bench_open_and_drop, bench_get_lines_text);
criterion_main!(benches);
