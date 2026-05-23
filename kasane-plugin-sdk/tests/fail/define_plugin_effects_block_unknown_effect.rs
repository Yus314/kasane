// ADR-053 chunk 3: `yield Effect::<Variant>(...)` must name a real variant
// of the `Effect` enum so the macro can compute REQUIRED_CAPABILITIES.
// An unknown variant must be rejected at macro-expansion time with a
// message that lists the supported chunk-1 variants.

use kasane_plugin_sdk::effects::Effect;

kasane_plugin_sdk::define_plugin! {
    id: "effects_block_unknown_effect",

    effects on StateChanged(flags) {
        let _ = flags;
        yield Effect::Telepathy("hello".into());
    },
}

fn main() {}
