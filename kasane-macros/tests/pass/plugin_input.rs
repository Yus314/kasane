use kasane_core::kasane_plugin;

#[kasane_plugin]
mod input_plugin {
    use kasane_core::input::{KeyEvent, MouseEvent};
    use kasane_core::plugin::{AppView, Command};

    #[state]
    #[derive(Default)]
    pub struct State {
        pub key_count: u32,
    }

    pub fn observe_key(state: &mut State, _key: &KeyEvent, _core: &AppView<'_>) {
        state.key_count += 1;
    }

    pub fn observe_mouse(_state: &mut State, _event: &MouseEvent, _core: &AppView<'_>) {}

    pub fn handle_key(
        _state: &mut State,
        _key: &KeyEvent,
        _core: &AppView<'_>,
    ) -> Option<Vec<Command>> {
        None
    }

    pub fn handle_mouse(
        _state: &mut State,
        _event: &MouseEvent,
        _id: kasane_core::element::InteractiveId,
        _core: &AppView<'_>,
    ) -> Option<Vec<Command>> {
        None
    }
}

fn main() {
    use kasane_core::plugin::{AppView, PluginBackend};
    use kasane_core::input::{Key, KeyEvent, Modifiers};
    use kasane_core::state::AppState;

    let mut plugin = InputPluginPlugin::new();
    let state = AppState::default();
    let key = KeyEvent {
        key: Key::Char('a'),
        modifiers: Modifiers::empty(),
    };
    plugin.observe_key(&key, &AppView::new(&state));
    assert_eq!(plugin.state.key_count, 1);
}
