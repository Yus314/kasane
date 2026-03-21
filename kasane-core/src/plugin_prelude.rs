//! Prelude for plugin authors.
//!
//! ```ignore
//! use kasane_core::plugin_prelude::*;
//! ```

pub use crate::kasane_plugin;

// Plugin trait and types
pub use crate::plugin::{
    AnnotateContext, AnnotationResult, BackgroundLayer, BlendMode, BootstrapEffects, BufferEdit,
    BufferPosition, Command, ContribSizeHint, ContributeContext, Contribution, DisplayDirective,
    DisplayMapRef, IsBridgedPlugin, KeyDispatchResult, KeyHandleResult, LineAnnotation,
    OverlayContext, OverlayContribution, PaintHook, PaneContext, Plugin, PluginAuthorities,
    PluginBackend, PluginBridge, PluginCapabilities, PluginDescriptor, PluginFactory, PluginId,
    PluginManager, PluginProvider, PluginRank, PluginRevision, PluginRuntime, PluginSource,
    PluginState, RuntimeEffects, SessionReadyCommand, SessionReadyEffects, SlotId,
    TransformContext, TransformTarget, builtin_plugin, host_plugin, host_plugin_with_provider,
};

// Element tree
pub use crate::element::{Element, FlexChild, InteractiveId, Overlay, OverlayAnchor, StyleToken};

// Protocol types
pub use crate::protocol::{Atom, Color, Coord, Face, Line, NamedColor};

// Scroll policy types
pub use crate::scroll::{
    DefaultScrollCandidate, ResolvedScroll, ScrollAccumulationMode, ScrollConsumption, ScrollCurve,
    ScrollGranularity, ScrollOwner, ScrollPlan, ScrollPolicyResult,
};

// State
pub use crate::state::{AppState, DirtyFlags};

// Input
pub use crate::input::{Key, KeyEvent, Modifiers, MouseButton, MouseEvent, MouseEventKind};

// Layout
pub use crate::layout::SplitDirection;

// Workspace
pub use crate::workspace::{
    DockPosition, FloatingEntry, FocusDirection, Placement, Workspace, WorkspaceCommand,
    WorkspaceNode, WorkspaceQuery,
};

// Surface
pub use crate::surface::{
    EventContext, SizeHint, SlotDeclaration, SlotKind, Surface, SurfaceEvent, SurfaceId,
    SurfacePlacementRequest, SurfaceRegistry, ViewContext,
};
