use kasane_core::kasane_plugin;
use kasane_core::state::DirtyFlags;

#[kasane_plugin]
mod lifecycle_effects_plugin {
    use kasane_core::plugin::{AppView, Command, Effects};
    use kasane_core::protocol::KasaneRequest;
    use kasane_core::state::DirtyFlags;

    #[state]
    #[derive(Default)]
    pub struct State {
        pub initialized: bool,
        pub ready: bool,
    }

    pub fn on_init_effects(state: &mut State, _core: &AppView<'_>) -> Effects {
        state.initialized = true;
        Effects::redraw(DirtyFlags::STATUS)
    }

    pub fn on_active_session_ready_effects(
        state: &mut State,
        _core: &AppView<'_>,
    ) -> Effects {
        state.ready = true;
        Effects {
            redraw: DirtyFlags::BUFFER,
            commands: vec![Command::SendToKakoune(KasaneRequest::Scroll {
                amount: 1,
                line: 1,
                column: 1,
            })],
            scroll_plans: vec![],
        }
    }
}

fn main() {
    use kasane_core::plugin::{AppView, PluginBackend};
    use kasane_core::state::AppState;

    let mut plugin = LifecycleEffectsPluginPlugin::new();
    let state = AppState::default();
    let view = AppView::new(&state);

    let init = plugin.on_init_effects(&view);
    assert!(plugin.state.initialized);
    assert!(init.redraw.contains(DirtyFlags::STATUS));

    let ready = plugin.on_active_session_ready_effects(&view);
    assert!(plugin.state.ready);
    assert!(ready.redraw.contains(DirtyFlags::BUFFER));
    assert_eq!(ready.commands.len(), 1);
}
