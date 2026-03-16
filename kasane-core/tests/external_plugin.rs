use kasane_core::plugin_prelude::*;

#[kasane_plugin]
mod test_external {
    use kasane_core::plugin::Command;
    use kasane_core::plugin_prelude::*;

    #[state]
    #[derive(Default)]
    pub struct State {
        pub init_called: bool,
    }

    pub fn on_init(state: &mut State, _core: &AppState) -> Vec<Command> {
        state.init_called = true;
        vec![]
    }
}

#[test]
fn external_plugin_registers_and_inits() {
    let mut registry = PluginRegistry::new();
    registry.register_backend(Box::new(TestExternalPlugin::new()));
    let state = AppState::default();
    let _ = registry.init_all(&state);
    // No panic = success; plugin was registered and initialized
}

#[test]
fn external_plugin_lifecycle() {
    let mut plugin = TestExternalPlugin::new();
    assert!(!plugin.state.init_called);

    let state = AppState::default();
    plugin.on_init(&state);
    assert!(plugin.state.init_called);
}
