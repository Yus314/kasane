kasane_plugin_sdk::define_plugin! {
    id: "cursor_line",

    state {
        #[bind(host_state::get_cursor_line(), on: dirty::BUFFER)]
        active_line: i32 = -1,
    },

    annotate(line, _ctx) {
        if line as i32 != state.active_line {
            return None;
        }
        let bg = theme_face_or(
            "cursor.line.bg",
            if is_dark_background() {
                face_bg(rgb(40, 40, 50))
            } else {
                face_bg(rgb(220, 220, 235))
            },
        );
        Some(bg_annotation(bg))
    },
}
