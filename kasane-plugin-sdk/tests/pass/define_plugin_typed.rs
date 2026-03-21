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

    on_workspace_changed(snapshot) {
        let _ = snapshot;
    },

    handle_key_middleware(event) {
        let _ = event;
        KeyHandleResult::Passthrough
    },

    update_effects(payload) {
        let _ = payload;
        RuntimeEffects::default()
    },

    on_io_event_effects(event) {
        let _ = event;
        RuntimeEffects::default()
    },

    capabilities: [Capability::Filesystem],
    authorities: [PluginAuthority::DynamicSurface],

    slots {
        STATUS_RIGHT(dirty::BUFFER) => plain(" typed "),
    },

    display_directives() {
        vec![DisplayDirective::InsertAfter(InsertAfterDirective {
            after: 0,
            content: "typed".to_string(),
            face: default_face(),
        })]
    },
}

fn main() {}
