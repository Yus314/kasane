use kasane_core::kasane_plugin;

#[kasane_plugin]
mod counter_plugin {
    use kasane_core::element::Element;
    use kasane_core::protocol::Face;
    use kasane_core::state::AppState;

    #[state]
    #[derive(Default)]
    pub struct State {
        pub count: u32,
    }

    #[slot(Slot::StatusRight)]
    pub fn view(state: &State, _core: &AppState) -> Option<Element> {
        Some(Element::text(format!("[{}]", state.count), Face::default()))
    }
}

fn main() {
    use kasane_core::plugin::Plugin;
    let plugin = CounterPluginPlugin::new();
    // state_hash should be generated and callable
    let h1 = plugin.state_hash();
    let h2 = plugin.state_hash();
    assert_eq!(h1, h2); // same state → same hash
}
