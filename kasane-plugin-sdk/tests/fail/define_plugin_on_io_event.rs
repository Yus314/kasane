kasane_plugin_sdk::define_plugin! {
    id: "bad_on_io_event",

    on_io_event(event) {
        let _ = event;
        RuntimeEffects::default()
    },
}

fn main() {}
