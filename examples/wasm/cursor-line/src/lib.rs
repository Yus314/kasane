kasane_plugin_sdk::define_plugin! {
    manifest: "kasane-plugin.toml",

    state {
        #[bind(host_state::get_cursor_line(), on: dirty::BUFFER)]
        active_line: i32 = -1,
    },

    display() {
        if state.active_line < 0 {
            return vec![];
        }
        let bg = theme_style_or(
            "cursor.line.bg",
            if is_dark_background() {
                style_bg(rgb(40, 40, 50))
            } else {
                style_bg(rgb(220, 220, 235))
            },
        );
        vec![style_line(state.active_line as u32, bg)]
    },
}
