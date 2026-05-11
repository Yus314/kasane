// ADR-044 Phase B-5 fixture: exercises the renamed tier-1 state-changed
// handler emitted by `define_plugin!`. The plugin echoes a fixed
// kakoune-side eval-command per tick so host-side tests can assert
// the tier-1 wire export round-trips through the host adapter.

kasane_plugin_sdk::define_plugin! {
    manifest: "kasane-plugin.toml",

    on_state_changed_effects(_dirty) {
        kasane_plugin_sdk::kakoune_side_setup_effects![
            "echo tier1-fired",
        ]
    },
}
