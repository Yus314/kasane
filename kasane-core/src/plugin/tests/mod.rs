use super::*;
use crate::input::KeyEvent;
use crate::layout::SplitDirection;
use crate::protocol::Face;
use crate::state::{AppState, DirtyFlags};
use crate::surface::{EventContext, SizeHint, Surface, SurfaceEvent, SurfaceId, ViewContext};
use crate::workspace::Placement;

mod commands;
mod hooks;
mod io;
mod registry;

struct TestSurface {
    id: SurfaceId,
}

impl Surface for TestSurface {
    fn id(&self) -> SurfaceId {
        self.id
    }

    fn surface_key(&self) -> compact_str::CompactString {
        format!("test.surface.{}", self.id.0).into()
    }

    fn size_hint(&self) -> SizeHint {
        SizeHint::fill()
    }

    fn view(&self, _ctx: &ViewContext<'_>) -> crate::element::Element {
        crate::element::Element::Empty
    }

    fn handle_event(&mut self, _event: SurfaceEvent, _ctx: &EventContext<'_>) -> Vec<Command> {
        vec![]
    }
}

struct TestPlugin;

impl Plugin for TestPlugin {
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

impl Plugin for LifecyclePlugin {
    fn id(&self) -> PluginId {
        PluginId("lifecycle".to_string())
    }

    fn on_init(&mut self, _state: &AppState) -> Vec<Command> {
        self.init_called = true;
        vec![Command::RequestRedraw(DirtyFlags::BUFFER)]
    }

    fn on_shutdown(&mut self) {
        self.shutdown_called = true;
    }

    fn on_state_changed(&mut self, _state: &AppState, dirty: DirtyFlags) -> Vec<Command> {
        self.state_changes.push(dirty);
        vec![]
    }
}

struct SurfacePlugin;

impl Plugin for SurfacePlugin {
    fn id(&self) -> PluginId {
        PluginId("surface-plugin".to_string())
    }

    fn surfaces(&mut self) -> Vec<Box<dyn Surface>> {
        vec![
            Box::new(TestSurface { id: SurfaceId(200) }),
            Box::new(TestSurface { id: SurfaceId(201) }),
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

impl Plugin for StatefulPlugin {
    fn id(&self) -> PluginId {
        PluginId("stateful".to_string())
    }

    fn state_hash(&self) -> u64 {
        self.hash
    }
}
