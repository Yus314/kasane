use kasane_core::kasane_plugin;

#[kasane_plugin]
mod lifecycle_plugin {
    use kasane_core::plugin::{AppView, BootstrapEffects, RuntimeEffects};
    use kasane_core::state::DirtyFlags;

    #[state]
    #[derive(Default)]
    pub struct State {
        pub initialized: bool,
    }

    pub fn on_init_effects(state: &mut State, _core: &AppView<'_>) -> BootstrapEffects {
        state.initialized = true;
        BootstrapEffects::default()
    }

    pub fn on_shutdown(state: &mut State) {
        state.initialized = false;
    }

    pub fn on_state_changed_effects(
        _state: &mut State,
        _core: &AppView<'_>,
        _dirty: DirtyFlags,
    ) -> RuntimeEffects {
        RuntimeEffects::default()
    }
}

fn main() {
    use kasane_core::plugin::{AppView, PluginBackend};
    use kasane_core::state::AppState;

    let mut plugin = LifecyclePluginPlugin::new();
    assert!(!plugin.state.initialized);

    let state = AppState::default();
    plugin.on_init_effects(&AppView::new(&state));
    assert!(plugin.state.initialized);

    plugin.on_shutdown();
    assert!(!plugin.state.initialized);
}
