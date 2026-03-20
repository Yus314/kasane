kasane_plugin_sdk::define_plugin! {
    id: "bad_update",

    update(payload) {
        let _ = payload;
        RuntimeEffects::default()
    },
}

fn main() {}
