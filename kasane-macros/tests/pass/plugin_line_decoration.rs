use kasane_core::kasane_plugin;

#[kasane_plugin]
mod line_number_plugin {
    use kasane_core::element::Element;
    use kasane_core::plugin::LineDecoration;
    use kasane_core::protocol::Face;
    use kasane_core::state::AppState;

    #[state]
    #[derive(Default)]
    pub struct State;

    pub fn contribute_line(
        _state: &State,
        line: usize,
        _core: &AppState,
    ) -> Option<LineDecoration> {
        Some(LineDecoration {
            left_gutter: Some(Element::text(format!("{:>3}", line + 1), Face::default())),
            right_gutter: None,
            background: None,
        })
    }
}

fn main() {
    use kasane_core::plugin::Plugin;
    use kasane_core::state::AppState;

    let plugin = LineNumberPluginPlugin::new();
    let state = AppState::default();
    let dec = plugin.contribute_line(0, &state);
    assert!(dec.is_some());
    assert!(dec.unwrap().left_gutter.is_some());
}
