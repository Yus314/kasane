use super::*;
use crate::input::KeyEvent;
use crate::layout::SplitDirection;
use crate::plugin::Effects;
use crate::protocol::Face;
use crate::state::{AppState, DirtyFlags};
use crate::surface::{Surface, SurfaceId};
use crate::test_support::TestSurfaceBuilder;
use crate::workspace::Placement;

mod command_classification;
mod commands;
mod compose;
mod hooks;
mod io;
mod registry;

struct TestPlugin;

impl PluginBackend for TestPlugin {
    fn id(&self) -> PluginId {
        PluginId("test".to_string())
    }
}

struct LifecyclePlugin {
    init_called: bool,
    shutdown_called: bool,
    state_changes: Vec<DirtyFlags>,
}

impl LifecyclePlugin {
    fn new() -> Self {
        LifecyclePlugin {
            init_called: false,
            shutdown_called: false,
            state_changes: Vec::new(),
        }
    }
}

impl PluginBackend for LifecyclePlugin {
    fn id(&self) -> PluginId {
        PluginId("lifecycle".to_string())
    }

    fn on_init_effects(&mut self, _state: &AppView<'_>) -> Effects {
        self.init_called = true;
        Effects::redraw(DirtyFlags::BUFFER)
    }

    fn on_shutdown(&mut self) {
        self.shutdown_called = true;
    }

    fn on_state_changed_effects(&mut self, _state: &AppView<'_>, dirty: DirtyFlags) -> Effects {
        self.state_changes.push(dirty);
        Effects::default()
    }
}

struct SurfacePlugin;

impl PluginBackend for SurfacePlugin {
    fn id(&self) -> PluginId {
        PluginId("surface-plugin".to_string())
    }

    fn surfaces(&mut self) -> Vec<Box<dyn Surface>> {
        vec![
            TestSurfaceBuilder::new(SurfaceId(200)).build(),
            TestSurfaceBuilder::new(SurfaceId(201)).build(),
        ]
    }

    fn workspace_request(&self) -> Option<Placement> {
        Some(Placement::SplitFocused {
            direction: SplitDirection::Vertical,
            ratio: 0.5,
        })
    }
}

struct StatefulPlugin {
    hash: u64,
}

impl PluginBackend for StatefulPlugin {
    fn id(&self) -> PluginId {
        PluginId("stateful".to_string())
    }

    fn state_hash(&self) -> u64 {
        self.hash
    }
}
