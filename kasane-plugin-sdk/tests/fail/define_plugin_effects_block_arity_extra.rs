// ADR-053 chunk 2: `effects on StateChanged(...)` takes exactly one
// parameter (the dirty-flags bitmask). Supplying a different number of
// parameters must be rejected at macro-expansion time.

kasane_plugin_sdk::define_plugin! {
    id: "effects_block_arity_extra",

    effects on StateChanged(flags, extra) {
        let _ = (flags, extra);
    },
}

fn main() {}
