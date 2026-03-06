use kasane_core::kasane_plugin;

#[kasane_plugin]
mod replace_plugin {
    use kasane_core::element::Element;
    use kasane_core::protocol::Face;
    use kasane_core::state::AppState;

    #[state]
    #[derive(Default)]
    pub struct State;

    #[replace(ReplaceTarget::StatusBar)]
    pub fn replace(_state: &State, _core: &AppState) -> Option<Element> {
        Some(Element::text("custom", Face::default()))
    }
}

fn main() {
    let _ = ReplacePluginPlugin::new();
}
