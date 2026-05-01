use kasane_core::kasane_plugin;
use kasane_core::plugin::{Plugin, PluginId};
use kasane_core::state::DirtyFlags;

#[kasane_plugin(v2)]
mod highlight {
    use kasane_core::plugin::{AppView, BackgroundLayer, BlendMode, Effects};
    use kasane_core::protocol::{Color, WireFace, NamedColor};
    use kasane_core::state::DirtyFlags;

    #[state]
    #[derive(Clone, Default, PartialEq, Debug)]
    #[dirty(DirtyFlags::BUFFER_CURSOR)]
    pub struct State {
        pub active_line: usize,
    }

    pub fn on_state_changed(
        state: &State,
        _app: &AppView<'_>,
        _dirty: DirtyFlags,
    ) -> (State, Effects) {
        (
            State {
                active_line: state.active_line,
            },
            Effects::default(),
        )
    }

    pub fn decorate_background(
        state: &State,
        line: usize,
        _app: &AppView<'_>,
        _ctx: &kasane_core::plugin::AnnotateContext,
    ) -> Option<BackgroundLayer> {
        if line == state.active_line {
            Some(BackgroundLayer {
                style: kasane_core::protocol::Style::from_face(&WireFace {
                    bg: Color::Named(NamedColor::Blue),
                    ..WireFace::default()
                }),
                z_order: 0,
                blend: BlendMode::Opaque,
            })
        } else {
            None
        }
    }
}

fn main() {
    let plugin = HighlightPlugin;
    assert_eq!(plugin.id(), PluginId("highlight".to_string()));
}
