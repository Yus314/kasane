kasane_plugin_sdk::define_plugin! {
    id: "bad_on_state_changed",

    on_state_changed(dirty) {
        let _ = dirty;
        RuntimeEffects::default()
    },
}

fn main() {}
