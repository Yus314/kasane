use kasane_core::kasane_plugin;

#[kasane_plugin]
mod my_plugin {
    use kasane_core::plugin::RuntimeEffects;
    use kasane_core::state::AppState;

    #[state]
    #[derive(Default)]
    pub struct State {
        pub counter: u32,
    }

    #[event]
    pub enum Msg {
        Increment,
    }

    pub fn update_effects(
        state: &mut State,
        msg: &mut dyn std::any::Any,
        _core: &AppState,
    ) -> RuntimeEffects {
        if let Some(Msg::Increment) = msg.downcast_ref::<Msg>() {
            state.counter += 1;
        }
        RuntimeEffects::default()
    }
}

fn main() {
    let plugin = MyPluginPlugin::new();
    assert_eq!(plugin.state.counter, 0);
}
