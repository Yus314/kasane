// ADR-044 Phase B-3 fixture: exercises the new tier-1 state-changed
// handler emitted by `define_plugin!`. The plugin echoes a fixed
// kakoune-side eval-command per tick so host-side tests can assert
// the tier-1 export wires through and the legacy export stays empty.

kasane_plugin_sdk::define_plugin! {
    manifest: "kasane-plugin.toml",

    on_state_changed_tier1_effects(_dirty) {
        kasane_plugin_sdk::kakoune_side_setup_effects![
            "echo tier1-fired",
        ]
    },
}
