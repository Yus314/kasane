//! Prelude for plugin authors.
//!
//! ```ignore
//! use kasane_core::plugin_prelude::*;
//! ```

pub use crate::kasane_plugin;

// Plugin trait and types
pub use crate::plugin::{
    ActionResult, AnnotateContext, AnnotationResult, AppView, BackgroundLayer, BlendMode,
    BufferEdit, BufferPosition, CellDecoration, Command, CompositionRule, ContribSizeHint,
    ContributeContext, Contribution, CursorOrn, CursorOrnKind, DecorationTarget, DisplayDirective,
    DisplayMapRef, DisplayUnit, DisplayUnitId, DisplayUnitMap, Effects, EffectsBatch, ElementPatch,
    EmphasisOrn, ExtensionPointId, ExtensionResults, FaceMerge, FoldToggleState, GutterSide,
    HandlerRegistry, IsBridgedPlugin, KeyDispatchResult, KeyHandleResult, LifecyclePhase,
    LineAnnotation, NavigationAction, NavigationDirection, NavigationPolicy, NullEffects,
    OrnamentBatch, OrnamentModality, OverlayContext, OverlayContribution, PaintHook, PaneContext,
    Plugin, PluginAuthorities, PluginBackend, PluginBridge, PluginCapabilities, PluginDescriptor,
    PluginEffects, PluginFactory, PluginId, PluginManager, PluginProvider, PluginRank,
    PluginRevision, PluginRuntime, PluginSource, PluginState, RenderOrnamentContext, SemanticRole,
    SlotId, SourceStrength, SurfaceOrn, SurfaceOrnAnchor, SurfaceOrnKind, Topic, TopicBus, TopicId,
    TransformContext, TransformDescriptor, TransformScope, TransformSubject, TransformTarget,
    UnitSource, VirtualTextItem, builtin_plugin, host_plugin, host_plugin_with_provider,
};

// Element tree
pub use crate::element::{
    Element, FlexChild, ImageFit, ImageSource, InteractiveId, Overlay, OverlayAnchor, PluginTag,
    StyleToken,
};

// Rendering types
pub use crate::render::{
    BlinkHint, CursorStyleHint, EasingCurve, InlineDecoration, InlineOp, MovementHint,
};

// Protocol types
pub use crate::protocol::{Atom, Color, Coord, Face, Line, NamedColor, StatusStyle};

// Scroll policy types
pub use crate::scroll::{
    DefaultScrollCandidate, ResolvedScroll, ScrollAccumulationMode, ScrollConsumption, ScrollCurve,
    ScrollGranularity, ScrollOwner, ScrollPlan, ScrollPolicyResult,
};

// State
pub use crate::state::derived::{EditorMode, Selection};
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
pub use crate::surface::buffer::ClientBufferSurface;
pub use crate::surface::{
    EventContext, SizeHint, SlotDeclaration, SlotKind, Surface, SurfaceEvent, SurfaceId,
    SurfacePlacementRequest, SurfaceRegistry, ViewContext,
};

// Session
pub use crate::session::SessionId;
