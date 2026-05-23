// ADR-053 chunk 2: each trigger admits at most one `effects on <Trigger>`
// block per `define_plugin!` invocation. Authors must compose multiple
// behaviors inside a single block, not by repeating the trigger.

use kasane_plugin_sdk::effects::Effect;

kasane_plugin_sdk::define_plugin! {
    id: "effects_block_duplicate",

    effects on StateChanged(flags) {
        yield Effect::Redraw(flags);
    },

    effects on StateChanged(flags) {
        yield Effect::EvalCommand("echo second".into());
    },
}

fn main() {}
