// ADR-053 chunk 2: an `effects on StateChanged` block and the legacy
// `on_state_changed_effects(...) { ... }` block both emit the same
// Guest method body, so using both is a parser-level error.

use kasane_plugin_sdk::effects::Effect;

kasane_plugin_sdk::define_plugin! {
    id: "effects_block_conflict",

    on_state_changed_effects(flags) {
        let _ = flags;
        KakouneSideEffects::default()
    },

    effects on StateChanged(flags) {
        yield Effect::Redraw(flags);
    },
}

fn main() {}
