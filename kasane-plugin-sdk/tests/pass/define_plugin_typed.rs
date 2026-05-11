// ADR-044 Phase B-5: handler-return tiers. Init / session-ready /
// state-changed are tier-1 (`KakouneSideEffects`). Update / io-event are
// tier-2 (`Effects` = `ProcessCapableEffects`).
kasane_plugin_sdk::define_plugin! {
    id: "typed_define_plugin",

    state {
        counter: u32 = 0,
    },

    on_init_effects() {
        KakouneSideEffects {
            redraw: dirty::STATUS,
            commands: vec![],
            scroll_plans: vec![],
        }
    },

    on_active_session_ready_effects() {
        KakouneSideEffects {
            redraw: dirty::STATUS,
            commands: vec![],
            scroll_plans: vec![],
        }
    },

    on_state_changed_effects(dirty_flags) {
        let _ = dirty_flags;
        KakouneSideEffects {
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
        vec![DisplayDirective::InsertAfter(InterlineDirective {
            line: 0,
            content: 0,
            priority: 0,
        })]
    },
}

fn main() {}
