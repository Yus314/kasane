//! Prelude for plugin authors.
//!
//! ```ignore
//! use kasane_core::plugin_prelude::*;
//! ```

pub use crate::kasane_plugin;

// Plugin trait and types
#[allow(deprecated)]
pub use crate::plugin::Slot;
pub use crate::plugin::{
    AnnotateContext, AnnotationResult, BackgroundLayer, BlendMode, Command, ContribSizeHint,
    ContributeContext, Contribution, DecorateTarget, LineAnnotation, LineDecoration,
    OverlayContext, OverlayContribution, PaintHook, Plugin, PluginCapabilities, PluginId,
    PluginRegistry, ReplaceTarget, SlotId, TransformContext, TransformTarget,
};

// Element tree
pub use crate::element::{Element, FlexChild, InteractiveId, Overlay, OverlayAnchor, StyleToken};

// Protocol types
pub use crate::protocol::{Atom, Color, Coord, Face, Line, NamedColor};

// State
pub use crate::state::{AppState, DirtyFlags};

// Pane types
pub use crate::pane::{
    FocusDirection, NewPaneContent, PaneCommand, PaneContext, PaneId, PanePermissions,
    SplitDirection,
};

// Input
pub use crate::input::{Key, KeyEvent, Modifiers, MouseButton, MouseEvent, MouseEventKind};

// Workspace
pub use crate::workspace::{
    DockPosition, FloatingEntry, Placement, Workspace, WorkspaceCommand, WorkspaceNode,
    WorkspaceQuery,
};

// Surface
pub use crate::surface::{
    EventContext, SizeHint, SlotDeclaration, SlotPosition, Surface, SurfaceEvent, SurfaceId,
    SurfaceRegistry, ViewContext,
};
