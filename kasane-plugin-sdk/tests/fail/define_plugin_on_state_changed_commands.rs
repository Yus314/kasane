kasane_plugin_sdk::define_plugin! {
    id: "bad_on_state_changed_commands",

    on_state_changed_commands(dirty) {
        let _ = dirty;
        vec![]
    },
}

fn main() {}
