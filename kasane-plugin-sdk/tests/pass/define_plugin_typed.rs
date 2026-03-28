kasane_plugin_sdk::define_plugin! {
    id: "typed_define_plugin",

    state {
        counter: u32 = 0,
    },

    on_init_effects() {
        Effects {
            redraw: dirty::STATUS,
            commands: vec![],
            scroll_plans: vec![],
        }
    },

    on_active_session_ready_effects() {
        Effects {
            redraw: dirty::STATUS,
            commands: vec![],
            scroll_plans: vec![],
        }
    },

    on_state_changed_effects(dirty_flags) {
        let _ = dirty_flags;
        Effects {
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
        Effects::default()
    },

    on_io_event_effects(event) {
        let _ = event;
        Effects::default()
    },

    capabilities: [Capability::Filesystem],
    authorities: [PluginAuthority::DynamicSurface],

    slots {
        STATUS_RIGHT(dirty::BUFFER) => plain(" typed "),
    },

    display_directives() {
        vec![DisplayDirective::InsertAfter(InsertAfterDirective {
            after: 0,
            content: vec![Atom { face: default_face(), contents: "typed".to_string() }],
        })]
    },
}

fn main() {}
