kasane_plugin_sdk::define_plugin! {
    manifest: "kasane-plugin.toml",

    state {
        #[bind(host_state::get_cursor_count(), on: dirty::BUFFER)]
        cursor_count: u32 = 0,
    },

    slots {
        STATUS_RIGHT(dirty::BUFFER) => |_ctx| {
            (state.cursor_count > 1).then(|| {
                auto_contribution(text(&format!(" {} sel ", state.cursor_count), default_face()))
            })
        },
    },
}
