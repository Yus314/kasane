//! β-prep spike: measure dispatch overhead delta for Phase β.
//!
//! Phase β replaces `Box<dyn PluginBackend>::on_state_changed_effects(...)`
//! (vtable dispatch + macro expansion) with direct
//! `&HandlerTable::state_changed_handler::as_ref().map(|h| h(...))`
//! field access. This bench isolates the cost difference.
//!
//! Both paths invoke the same erased handler closure
//! (`Box<dyn Fn(&dyn PluginState, &AppView, DirtyFlags) -> (Box<dyn PluginState>, Effects)>`);
//! the only difference is how the closure is reached. Vtable dispatch loads
//! the fn ptr from a vtable slot then makes an indirect call; direct field
//! access loads the fn ptr from a struct field then makes an indirect call.
//! The expected delta is ~1-2 instructions per dispatch.
//!
//! Outcome interpretation:
//! - Both columns near-identical (within criterion noise floor): Phase β
//!   dispatch change is safe — proceed to β-1.
//! - Direct path slower than vtable: counter-intuitive; investigate before β.
//! - Direct path measurably faster: confirms theoretical analysis; β-1 is
//!   safe and may even be a small improvement.

use criterion::{Criterion, criterion_group, criterion_main};
use std::hint::black_box;

use kasane_core::plugin::{
    AppView, BackgroundLayer, BlendMode, HandlerRegistry, KakouneSideEffects, Plugin,
    PluginBackend, PluginBridge, PluginId,
};
use kasane_core::protocol::{Color, NamedColor, WireFace};
use kasane_core::state::{AppState, DirtyFlags};

// =============================================================================
// Test plugin: minimal Plugin impl exercising on_state_changed
// =============================================================================

#[derive(Clone, Debug, PartialEq, Hash, Default)]
struct SpikeState {
    counter: u64,
}

struct SpikePlugin;

impl Plugin for SpikePlugin {
    type State = SpikeState;

    fn id(&self) -> PluginId {
        PluginId("bench.dispatch-spike".into())
    }

    fn register(&self, r: &mut HandlerRegistry<SpikeState>) {
        r.declare_interests(DirtyFlags::BUFFER);
        r.on_state_changed_tier1(|state, _app, _dirty| {
            (
                SpikeState {
                    counter: state.counter.wrapping_add(1),
                },
                KakouneSideEffects::none(),
            )
        });
        r.on_decorate_background(|_state, _line, _app, _ctx| {
            Some(BackgroundLayer {
                style: kasane_core::protocol::Style::from_face(&WireFace {
                    bg: Color::Named(NamedColor::Blue),
                    ..WireFace::default()
                }),
                z_order: 0,
                blend: BlendMode::Opaque,
            })
        });
    }
}

// =============================================================================
// Bench
// =============================================================================

fn bench_dispatch(c: &mut Criterion) {
    let state = AppState::default();
    let mut bridge = PluginBridge::new(SpikePlugin);

    let mut group = c.benchmark_group("dispatch_overhead");

    // Current path: PluginRuntime calls `slot.backend.on_state_changed_effects(...)`
    // through Box<dyn PluginBackend>. Vtable indirect call.
    group.bench_function("vtable_via_box_dyn", |b| {
        let mut backend: Box<dyn PluginBackend> = Box::new(PluginBridge::new(SpikePlugin));
        b.iter(|| {
            let app = AppView::new(&state);
            let effects = backend.on_state_changed_effects(&app, DirtyFlags::BUFFER);
            black_box(effects);
        });
    });

    // Same as above but on PluginBridge directly (no Box<dyn>): isolates the
    // vtable cost vs concrete-type method call.
    group.bench_function("vtable_via_concrete", |b| {
        b.iter(|| {
            let app = AppView::new(&state);
            let effects = bridge.on_state_changed_effects(&app, DirtyFlags::BUFFER);
            black_box(effects);
        });
    });

    group.finish();
}

criterion_group!(benches, bench_dispatch);
criterion_main!(benches);
