//! ADR-053 chunk 5: SDK-side overhead of a single `yield Effect::X(...)`.
//!
//! The macro CPS-lowers each yield site to `__yielder.emit(...)?`. This
//! benchmark measures the per-call cost of that emit against the
//! `MockHandler` reference yielder — purely host-side, no wasmtime
//! involved. The wasmtime host-call cost (~115 ns from
//! `component_model.rs`) stacks on top of this number to give the
//! end-to-end per-yield budget the ADR's ≤5 µs threshold gates.

use std::hint::black_box;

use criterion::{Criterion, criterion_group, criterion_main};
use kasane_plugin_sdk::effects::{
    CapabilityName, Effect, EffectError, EffectReply, MockHandler, Yielder,
};

fn bench_emit_unmatched(c: &mut Criterion) {
    c.bench_function("effect_yield/emit_unmatched_redraw", |b| {
        // No rules — every emit falls through to EffectReply::Unit.
        let mut handler = MockHandler::new();
        b.iter(|| {
            let reply = handler.emit(black_box(Effect::Redraw(0))).unwrap();
            black_box(reply);
        });
        // Drain so the next bench restart doesn't grow the log forever.
        handler.clear();
    });
}

fn bench_emit_matched_respond(c: &mut Criterion) {
    c.bench_function("effect_yield/emit_matched_respond", |b| {
        let mut handler = MockHandler::new().respond(
            |e| matches!(e, Effect::PasteClipboard),
            EffectReply::ClipboardText("bench".into()),
        );
        b.iter(|| {
            let reply = handler.emit(black_box(Effect::PasteClipboard)).unwrap();
            black_box(reply);
        });
        handler.clear();
    });
}

fn bench_emit_matched_reject(c: &mut Criterion) {
    c.bench_function("effect_yield/emit_matched_reject", |b| {
        let mut handler = MockHandler::new().reject(
            |e| matches!(e, Effect::SetClipboard(_)),
            EffectError::MissingCapability(CapabilityName("clipboard")),
        );
        b.iter(|| {
            let result = handler.emit(black_box(Effect::SetClipboard("x".into())));
            black_box(result.is_err());
        });
        handler.clear();
    });
}

criterion_group!(
    benches,
    bench_emit_unmatched,
    bench_emit_matched_respond,
    bench_emit_matched_reject,
);
criterion_main!(benches);
