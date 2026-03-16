use kasane_core::kasane_plugin;

#[kasane_plugin]
mod icon_plugin {
    use kasane_core::protocol::{Atom, Face};
    use kasane_core::state::AppState;

    #[state]
    #[derive(Default)]
    pub struct State;

    pub fn transform_menu_item(
        _state: &State,
        item: &[Atom],
        _index: usize,
        _selected: bool,
        _core: &AppState,
    ) -> Option<Vec<Atom>> {
        let mut result = vec![Atom {
            face: Face::default(),
            contents: "★ ".into(),
        }];
        result.extend(item.iter().cloned());
        Some(result)
    }
}

fn main() {
    use kasane_core::plugin::PluginBackend;
    use kasane_core::protocol::{Atom, Face};
    use kasane_core::state::AppState;

    let plugin = IconPluginPlugin::new();
    let state = AppState::default();
    let item = vec![Atom {
        face: Face::default(),
        contents: "test".into(),
    }];
    let result = plugin.transform_menu_item(&item, 0, false, &state);
    assert!(result.is_some());
    let result = result.unwrap();
    assert_eq!(result[0].contents.as_str(), "★ ");
}
