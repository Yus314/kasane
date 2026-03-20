use std::any::Any;

use kasane_core::kasane_plugin;
use kasane_core::state::DirtyFlags;

#[kasane_plugin]
mod runtime_effects_plugin {
    use std::any::Any;

    use kasane_core::plugin::RuntimeEffects;
    use kasane_core::scroll::{ScrollAccumulationMode, ScrollCurve, ScrollPlan};
    use kasane_core::state::{AppState, DirtyFlags};

    #[state]
    #[derive(Default)]
    pub struct State {
        pub changed: bool,
        pub updated: bool,
    }

    #[event]
    pub enum Msg {
        Ping,
    }

    pub fn on_state_changed_effects(
        state: &mut State,
        _core: &AppState,
        dirty: DirtyFlags,
    ) -> RuntimeEffects {
        state.changed = dirty.contains(DirtyFlags::BUFFER);
        RuntimeEffects {
            redraw: DirtyFlags::STATUS,
            commands: vec![],
            scroll_plans: vec![ScrollPlan {
                total_amount: 1,
                line: 1,
                column: 1,
                frame_interval_ms: 16,
                curve: ScrollCurve::Linear,
                accumulation: ScrollAccumulationMode::Add,
            }],
        }
    }

    pub fn update_effects(
        state: &mut State,
        msg: &mut dyn Any,
        _core: &AppState,
    ) -> RuntimeEffects {
        state.updated = msg.downcast_ref::<Msg>().is_some();
        RuntimeEffects {
            redraw: DirtyFlags::BUFFER,
            commands: vec![],
            scroll_plans: vec![],
        }
    }
}

fn main() {
    use kasane_core::plugin::PluginBackend;
    use kasane_core::state::AppState;

    let mut plugin = RuntimeEffectsPluginPlugin::new();
    let state = AppState::default();

    let changed = plugin.on_state_changed_effects(&state, DirtyFlags::BUFFER);
    assert!(plugin.state.changed);
    assert!(changed.redraw.contains(DirtyFlags::STATUS));
    assert_eq!(changed.scroll_plans.len(), 1);

    let mut msg: Box<dyn Any> = Box::new(runtime_effects_plugin::Msg::Ping);
    let updated = plugin.update_effects(msg.as_mut(), &state);
    assert!(plugin.state.updated);
    assert!(updated.redraw.contains(DirtyFlags::BUFFER));
}
