// ADR-053 chunk 2: the trigger name in `effects on <Trigger>(...)` must be
// one of the supported triggers (StateChanged, Init, SessionReady). A typo
// or unknown name must be rejected at macro-expansion time with a message
// that lists the valid set.

kasane_plugin_sdk::define_plugin! {
    id: "effects_block_unknown_trigger",

    effects on StateChange(flags) {
        let _ = flags;
    },
}

fn main() {}
