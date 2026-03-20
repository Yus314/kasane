kasane_plugin_sdk::define_plugin! {
    id: "bad_on_init",

    on_init() {
        BootstrapEffects::default()
    },
}

fn main() {}
