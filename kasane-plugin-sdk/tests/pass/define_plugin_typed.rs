kasane_plugin_sdk::define_plugin! {
    id: "typed_define_plugin",

    state {
        counter: u32 = 0,
    },

    on_init_effects() {
        BootstrapEffects {
            redraw: dirty::STATUS,
        }
    },

    on_active_session_ready_effects() {
        SessionReadyEffects {
            redraw: dirty::STATUS,
            commands: vec![],
            scroll_plans: vec![],
        }
    },

    on_state_changed_effects(dirty_flags) {
        let _ = dirty_flags;
        RuntimeEffects {
            redraw: dirty::BUFFER,
            commands: vec![],
            scroll_plans: vec![],
        }
    },

    update_effects(payload) {
        let _ = payload;
        RuntimeEffects::default()
    },

    on_io_event_effects(event) {
        let _ = event;
        RuntimeEffects::default()
    },

    slots {
        STATUS_RIGHT(dirty::BUFFER) => plain(" typed "),
    },
}

fn main() {}
