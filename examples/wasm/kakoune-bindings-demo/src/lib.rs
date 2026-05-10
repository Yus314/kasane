use kasane_plugin_sdk::kak::{self, OptionKind, Scope};

kasane_plugin_sdk::define_plugin! {
    manifest: "kasane-plugin.toml",

    on_active_session_ready_effects() {
        kasane_plugin_sdk::kakoune_setup_effects![
            kak::declare_option("demo_counter", OptionKind::Int, "0", true),
            kak::define_command(
                "demo-bump",
                None,
                "set-option global demo_counter %sh{ echo $(( kak_opt_demo_counter + 1 )) }",
            ),
            kak::declare_user_mode("demo"),
            kak::map(Scope::Global, "demo", "b", ":demo-bump<ret>", Some("bump counter")),
            kak::map(
                Scope::Global,
                "demo",
                "?",
                ":info 'counter is %opt{demo_counter}'<ret>",
                Some("show counter"),
            ),
        ]
    },
}
