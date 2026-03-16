use kasane_core::kasane_plugin;

#[kasane_plugin]
mod counter_plugin {
    use kasane_core::plugin::Command;
    use kasane_core::state::AppState;

    #[state]
    #[derive(Default)]
    pub struct State {
        pub count: u32,
    }

    #[event]
    pub enum Msg {
        Increment,
    }

    pub fn update(state: &mut State, msg: Msg, _core: &AppState) -> Vec<Command> {
        match msg {
            Msg::Increment => state.count += 1,
        }
        vec![]
    }
}

fn main() {
    use kasane_core::plugin::PluginBackend;
    let plugin = CounterPluginPlugin::new();
    // state_hash should be generated and callable
    let h1 = plugin.state_hash();
    let h2 = plugin.state_hash();
    assert_eq!(h1, h2); // same state -> same hash
}
