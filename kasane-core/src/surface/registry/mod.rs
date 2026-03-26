mod access;
mod compose;
mod routing;
mod session;
mod workspace;

use std::collections::HashMap;

use compact_str::CompactString;

use crate::input::{MouseButton, MouseEventKind};
use crate::layout::{Rect, SplitDirection};
use crate::plugin::{AppView, Command, PluginId, PluginView};
use crate::session::SessionId;
use crate::state::{AppState, DirtyFlags};
use crate::workspace::{
    Placement, Workspace, WorkspaceCommand, WorkspaceDivider, WorkspaceDividerId,
};

use super::pane_map::PaneStates;
use super::resolve::{self, SurfaceComposeResult, SurfaceRenderOutcome, SurfaceRenderReport};
use super::{
    EventContext, SlotDeclaration, SourcedSurfaceCommands, Surface, SurfaceDescriptor,
    SurfaceEvent, SurfaceId, SurfacePlacementRequest, SurfaceRegistrationError, ViewContext,
};

pub(crate) struct RegisteredSurface {
    pub(crate) surface: Box<dyn Surface>,
    pub(crate) descriptor: SurfaceDescriptor,
    pub(crate) owner_plugin: Option<PluginId>,
    pub(crate) session_binding: Option<super::types::SessionBindingState>,
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct ActiveDividerDrag {
    divider_id: WorkspaceDividerId,
    direction: SplitDirection,
    start_main: u16,
    start_ratio: f32,
    available_main: u16,
}

/// Manages all Surface instances and the Workspace layout tree.
///
/// Coordinates view composition by calling each Surface's `view()` method
/// with the rectangle allocated by the Workspace, then assembling the
/// results into a single Element tree.
pub struct SurfaceRegistry {
    pub(super) surfaces: HashMap<SurfaceId, RegisteredSurface>,
    pub(super) surface_ids_by_key: HashMap<CompactString, SurfaceId>,
    pub(super) slot_owners_by_name: HashMap<CompactString, SurfaceId>,
    pub(super) workspace: Workspace,
    pub(super) active_divider_drag: Option<ActiveDividerDrag>,
    /// Reverse index: SessionId → SurfaceId (for session-keyed lookups).
    pub(super) session_to_surface: HashMap<SessionId, SurfaceId>,
    /// Kakoune server session name (used for `-c` connections).
    pub(super) server_session_name: Option<String>,
}

impl SurfaceRegistry {
    /// Create a new registry with a default workspace rooted at `SurfaceId::BUFFER`.
    pub fn new() -> Self {
        SurfaceRegistry {
            surfaces: HashMap::new(),
            surface_ids_by_key: HashMap::new(),
            slot_owners_by_name: HashMap::new(),
            workspace: Workspace::default(),
            active_divider_drag: None,
            session_to_surface: HashMap::new(),
            server_session_name: None,
        }
    }

    /// Create a registry with a custom initial workspace.
    pub fn with_workspace(workspace: Workspace) -> Self {
        SurfaceRegistry {
            surfaces: HashMap::new(),
            surface_ids_by_key: HashMap::new(),
            slot_owners_by_name: HashMap::new(),
            workspace,
            active_divider_drag: None,
            session_to_surface: HashMap::new(),
            server_session_name: None,
        }
    }
}

impl Default for SurfaceRegistry {
    fn default() -> Self {
        Self::new()
    }
}
