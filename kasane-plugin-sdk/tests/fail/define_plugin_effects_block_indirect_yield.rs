// ADR-053 chunk 3: each `yield` site must literally name an
// `Effect::<Variant>(...)` constructor so the macro can project the
// required capability set at compile time. A `yield` whose RHS is an
// indirect expression (function call, variable reference) is rejected.

use kasane_plugin_sdk::effects::Effect;

fn make_effect() -> Effect {
    Effect::Redraw(0)
}

kasane_plugin_sdk::define_plugin! {
    id: "effects_block_indirect_yield",

    effects on StateChanged(flags) {
        let _ = flags;
        yield make_effect();
    },
}

fn main() {}
