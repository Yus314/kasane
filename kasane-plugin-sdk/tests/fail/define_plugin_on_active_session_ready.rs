kasane_plugin_sdk::define_plugin! {
    id: "bad_on_active_session_ready",

    on_active_session_ready() {
        Effects::default()
    },
}

fn main() {}
