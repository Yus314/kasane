use kasane_core::kasane_plugin;

#[kasane_plugin]
mod deco_plugin {
    use kasane_core::element::Element;
    use kasane_core::state::AppState;

    #[state]
    #[derive(Default)]
    pub struct State;

    #[decorate(DecorateTarget::Buffer, priority = 10)]
    pub fn decorate(_state: &State, element: Element, _core: &AppState) -> Element {
        element
    }
}

fn main() {
    let _ = DecoPluginPlugin::new();
}
