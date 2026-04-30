//! Prelude for plugin authors.
//!
//! ```ignore
//! use kasane_core::plugin_prelude::*;
//! ```

pub use crate::kasane_plugin;

// Plugin trait and types
pub use crate::plugin::{
    ActionResult, AnnotateContext, AnnotationResult, AppView, BackgroundLayer, BlendMode,
    BufferEdit, BufferPosition, CellDecoration, Command, CompositionRule, ContentAnchor,
    ContentAnnotation, ContentAnnotationSet, ContribSizeHint, ContributeContext, Contribution,
    CursorEffect, CursorEffectOrn, CursorStyleOrn, DecorationTarget, DisplayDirective,
    DisplayMapRef, DisplayUnit, DisplayUnitId, DisplayUnitMap, Effects, EffectsBatch, ElementPatch,
    ExtensionPointId, ExtensionResults, FaceMerge, FoldToggleState, GutterSide, HandlerRegistry,
    IsBridgedPlugin, KeyDispatchResult, KeyHandleResult, LifecyclePhase, LineAnnotation,
    NavigationAction, NavigationDirection, NavigationPolicy, NullEffects, OrnamentBatch,
    OrnamentModality, OverlayContext, OverlayContribution, PaneContext, Plugin, PluginAuthorities,
    PluginBackend, PluginBridge, PluginCapabilities, PluginDescriptor, PluginEffects,
    PluginFactory, PluginId, PluginManager, PluginProvider, PluginRank, PluginRevision,
    PluginRuntime, PluginSource, PluginState, RecoveryMechanism, RecoveryWitness,
    RenderOrnamentContext, SafeDisplayDirective, SemanticRole, SlotId, SourceStrength, SurfaceOrn,
    SurfaceOrnAnchor, SurfaceOrnKind, Topic, TopicBus, TopicId, TransformContext,
    TransformDescriptor, TransformScope, TransformSubject, TransformTarget, Transparency,
    TransparentCommand, TransparentEffects, TransparentKeyResult, UnitSource, VirtualTextItem,
    builtin_plugin, debug_overlay::DebugOverlayPlugin, host_plugin, host_plugin_with_provider,
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
//
// `Style` / `Brush` / `TextDecoration` / `FontWeight` / `FontSlant` /
// `FontVariation` are the post-resolve canonical types; new plugin code
// should construct atoms / element styles via these. `WireFace` / `Color` /
// `Attributes` are wire-format types kept in the prelude for the
// `detect_cursors`-style code paths that observe Kakoune `final_*`
// resolution flags (no `Style` equivalent). The host preserves both
// forms; pick by intent (post-resolve vs. wire-aware).
pub use crate::protocol::{
    Atom, Attributes, Brush, Color, Coord, FontSlant, FontVariation, FontWeight, Line, NamedColor,
    StatusStyle, Style, TextDecoration, WireFace,
};

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

// Syntax
pub use crate::syntax::{Declaration, DeclarationKind, SyntaxNode, SyntaxProvider};
