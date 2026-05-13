use super::*;
use crate::input::KeyEvent;
use crate::layout::SplitDirection;
use crate::state::{AppState, DirtyFlags};
use crate::surface::SurfaceId;
use crate::test_support::TestSurfaceBuilder;
use crate::workspace::Placement;

mod command_classification;
mod commands;
mod compose;
mod directive_classification;
mod hooks;
mod io;
mod projection;
mod registry;

struct TestPlugin;

impl crate::plugin::Plugin for TestPlugin {
    type State = ();

    fn id(&self) -> PluginId {
        PluginId::from("test")
    }

    fn register(&self, _r: &mut crate::plugin::HandlerRegistry<()>) {}
}

struct LifecyclePlugin;

impl LifecyclePlugin {
    fn new() -> Self {
        LifecyclePlugin
    }
}

impl crate::plugin::Plugin for LifecyclePlugin {
    type State = ();

    fn id(&self) -> PluginId {
        PluginId::from("lifecycle")
    }

    fn register(&self, r: &mut crate::plugin::HandlerRegistry<()>) {
        r.on_init_tier1(|state, _app| {
            (
                state.clone(),
                crate::plugin::KakouneSideEffects::redraw(DirtyFlags::BUFFER),
            )
        });
        r.on_shutdown(|_state| {});
        r.on_state_changed_tier1(|state, _app, _dirty| {
            (state.clone(), crate::plugin::KakouneSideEffects::none())
        });
    }
}

struct SurfacePlugin;

impl crate::plugin::Plugin for SurfacePlugin {
    type State = ();

    fn id(&self) -> PluginId {
        PluginId::from("surface-plugin")
    }

    fn register(&self, r: &mut crate::plugin::HandlerRegistry<()>) {
        r.declare_surfaces(|_state| {
            vec![
                TestSurfaceBuilder::new(SurfaceId(200)).build(),
                TestSurfaceBuilder::new(SurfaceId(201)).build(),
            ]
        });
        r.declare_workspace_request(Placement::SplitFocused {
            direction: SplitDirection::Vertical,
            ratio: 0.5,
        });
    }
}

struct StatefulPlugin;

impl crate::plugin::Plugin for StatefulPlugin {
    type State = ();

    fn id(&self) -> PluginId {
        PluginId::from("stateful")
    }

    fn register(&self, _r: &mut crate::plugin::HandlerRegistry<()>) {}
}
