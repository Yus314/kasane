// ADR-053 chunk 2: `effects on StateChanged(flags) { ... }` block lowers
// `yield Effect::X(...)` to the existing tier-1 `KakouneSideEffects`
// return shape via the macro-generated `__KasaneYielder` bridge.

use kasane_plugin_sdk::effects::Effect;

kasane_plugin_sdk::define_plugin! {
    id: "effects_block_smoke",

    effects on StateChanged(flags) {
        yield Effect::Redraw(flags);
        yield Effect::EvalCommand("echo from-effect".into());
    },
}

fn main() {}
