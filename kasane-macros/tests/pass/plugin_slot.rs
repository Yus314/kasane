use kasane_core::kasane_plugin;

#[kasane_plugin]
mod slot_plugin {
    use kasane_core::element::Element;
    use kasane_core::protocol::Face;
    use kasane_core::state::AppState;

    #[state]
    #[derive(Default)]
    pub struct State;

    #[slot(Slot::BufferLeft)]
    pub fn view(_state: &State, _core: &AppState) -> Option<Element> {
        Some(Element::text("gutter", Face::default()))
    }
}

fn main() {
    let _ = SlotPluginPlugin::new();
}
