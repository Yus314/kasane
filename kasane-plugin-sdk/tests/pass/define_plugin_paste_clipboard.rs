kasane_plugin_sdk::define_plugin! {
    id: "paste_clipboard_helper",

    on_active_session_ready_effects() {
        Effects {
            redraw: dirty::STATUS,
            commands: vec![paste_clipboard()],
            scroll_plans: vec![],
        }
    },
}

fn main() {}
