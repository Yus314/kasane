use kasane_core::plugin_prelude::*;

kasane_core::impl_migrated_caps_default!(TestExternalPlugin);

#[kasane_plugin]
mod test_external {
    use kasane_core::plugin::{AppView, Effects};
    use kasane_core::plugin_prelude::*;

    #[state]
    #[derive(Default)]
    pub struct State {
        pub init_called: bool,
    }

    pub fn on_init_effects(state: &mut State, _core: &AppView<'_>) -> Effects {
        state.init_called = true;
        Effects::redraw(DirtyFlags::STATUS)
    }
}

#[test]
fn external_plugin_registers_and_inits() {
    let mut registry = PluginRuntime::new();
    registry.register_backend(Box::new(TestExternalPlugin::new()));
    let state = AppState::default();
    let batch = registry.init_all_batch(&AppView::new(&state));
    assert!(batch.effects.redraw.contains(DirtyFlags::STATUS));
    // No panic = success; plugin was registered and initialized
}

#[test]
fn external_plugin_lifecycle() {
    let mut plugin = TestExternalPlugin::new();
    assert!(!plugin.state.init_called);

    let state = AppState::default();
    let effects = plugin.on_init_effects(&AppView::new(&state));
    assert!(plugin.state.init_called);
    assert!(effects.redraw.contains(DirtyFlags::STATUS));
}
