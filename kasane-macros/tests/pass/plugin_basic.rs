use kasane_core::kasane_plugin;

#[kasane_plugin]
mod my_plugin {
    use kasane_core::plugin::Command;
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

    pub fn update(state: &mut State, msg: Msg, _core: &AppState) -> Vec<Command> {
        match msg {
            Msg::Increment => state.counter += 1,
        }
        vec![]
    }
}

fn main() {
    let plugin = MyPluginPlugin::new();
    assert_eq!(plugin.state.counter, 0);
}
