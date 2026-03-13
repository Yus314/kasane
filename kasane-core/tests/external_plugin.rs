#![allow(deprecated)]
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

    #[slot(Slot::StatusLeft)]
    pub fn status(_state: &State, _core: &AppState) -> Option<Element> {
        Some(Element::text("ext", Face::default()))
    }
}

#[test]
fn external_plugin_registers_and_contributes() {
    let mut registry = PluginRegistry::new();
    registry.register(Box::new(TestExternalPlugin::new()));
    let state = AppState::default();
    let _ = registry.init_all(&state);
    let elements = registry.collect_slot(Slot::StatusLeft, &state);
    assert_eq!(elements.len(), 1);
}

#[test]
fn external_plugin_lifecycle() {
    // Verify lifecycle hooks work with the prelude-based plugin
    let mut plugin = TestExternalPlugin::new();
    assert!(!plugin.state.init_called);

    let state = AppState::default();
    plugin.on_init(&state);
    assert!(plugin.state.init_called);
}
