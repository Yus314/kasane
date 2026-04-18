//! Plugin infrastructure: `Plugin` trait, `PluginBackend` trait, registry, context, command, I/O.

pub mod app_view;
pub mod bridge;
pub mod channel;
mod command;
pub mod compose;
mod context;
pub mod diagnostics;
pub mod effect_footprint;
mod effects;
pub mod element_patch;
pub mod extension_point;
pub mod handler_registry;
pub(crate) mod handler_table;
pub mod io;
mod manager;
pub mod process_task;
pub mod projection_status;
mod provider;
pub mod pubsub;
pub mod recovery_witness;
mod registry;
pub mod render_ornament;
pub mod safe_directive;
pub mod setting;
pub mod state;
mod traits;
pub mod transparent_command;
pub mod transparent_effects;
pub mod variable_store;

#[cfg(test)]
mod tests;

pub use crate::session::SessionCommand;
use bitflags::bitflags;
use compact_str::CompactString;

// Re-export command module
pub use command::{
    BufferEdit, BufferPosition, Command, CommandResult, edits_to_keys, escape_kakoune_insert_text,
    execute_commands, extract_redraw_flags, partition_commands,
};
pub use diagnostics::{
    PluginDiagnostic, PluginDiagnosticKind, PluginDiagnosticOverlayState, PluginDiagnosticSeverity,
    PluginDiagnosticTarget, ProviderArtifactStage, report_plugin_diagnostics,
};
pub use effects::{
    Effects, EffectsBatch, LifecyclePhase, MouseHandleResult, NullEffects, PluginEffects,
    RecordingEffects, TextInputHandleResult,
};
pub use kasane_plugin_model::{PluginId, SettingValue};
pub use manager::{AppliedWinnerDelta, PluginApplyResult, PluginManager, ResolvedPluginSnapshot};
pub use provider::{
    CompositePluginProvider, PluginCollect, PluginDescriptor, PluginFactory, PluginProvider,
    PluginRank, PluginRevision, PluginSource, StaticPluginProvider, builtin_plugin, host_plugin,
    host_plugin_with_provider, plugin_factory,
};

// Re-export io module types
pub use io::{
    IoEvent, NullProcessDispatcher, ProcessDispatcher, ProcessEvent, ProcessEventSink, StdinMode,
};

// Re-export context module
pub use context::{
    AnnotateContext, AnnotationResult, BackgroundLayer, BlendMode, CellDecoration, ContribSizeHint,
    ContributeContext, Contribution, DecorationTarget, FaceMerge, LineAnnotation, OverlayContext,
    OverlayContribution, PaneContext, SourcedContribution, TransformContext, TransformDescriptor,
    TransformScope, TransformSubject, TransformTarget, VirtualTextItem,
};

// Re-export registry module
pub use registry::{
    CollectedOrnaments, ContributionCache, KeyDispatchResult, PluginRuntime, PluginSurfaceSet,
    PluginView,
};

/// Deprecated alias — use [`PluginRuntime`] instead.
#[deprecated(note = "renamed to PluginRuntime")]
pub type PluginRegistry = PluginRuntime;

// Re-export display types for plugin API
pub use crate::display::{
    ActionResult, ContentAnchor, ContentAnnotation, DisplayDirective, DisplayMapRef, DisplayUnit,
    DisplayUnitId, DisplayUnitMap, FoldToggleState, NavigationAction, NavigationDirection,
    NavigationPolicy, ProjectionCategory, ProjectionDescriptor, ProjectionId,
    ProjectionPolicyState, SemanticRole, SourceStrength, UnitSource,
};

// Re-export traits module
pub use crate::input::KeyResponse;
pub use traits::{KeyHandleResult, PluginBackend};

// Re-export projection status plugin (Phase 10)
pub use projection_status::ProjectionStatusPlugin;

// Re-export transparent command types (ADR-030 Level 3)
pub use transparent_command::{TransparentCommand, TransparentKeyResult};

// Re-export transparent effects types (ADR-030 Level 5)
pub use transparent_effects::TransparentEffects;

// Re-export effect footprint types (ADR-030 Level 5)
pub use command::EffectCategory;
pub use effect_footprint::{EffectFootprint, compute_transitive_footprints};

// Re-export recovery witness types (ADR-030 Level 4)
pub use recovery_witness::{RecoveryMechanism, RecoveryWitness};
pub use safe_directive::SafeDisplayDirective;

// Re-export compose module
pub use compose::{
    CommutativeComposable, Composable, ContentAnnotationSet, ContributionSet, FirstWins,
    MenuTransformChain, OverlaySet, TransformChain, TransformChainEntry,
};

// Re-export app_view, state, and bridge modules
pub use crate::state::Truth;
pub use app_view::{AppView, FrameworkAccess};
pub use bridge::{IsBridgedPlugin, PluginBridge};
pub use channel::ChannelValue;
pub use element_patch::ElementPatch;
pub use extension_point::{CompositionRule, ExtensionPointId, ExtensionResults};
pub use handler_registry::HandlerRegistry;
pub use handler_table::GutterSide;
pub use process_task::{ProcessTaskResult, ProcessTaskSpec};
pub use pubsub::{OscillationKind, Topic, TopicBus, TopicId};
pub use render_ornament::{
    CursorEffect, CursorEffectOrn, CursorStyleOrn, OrnamentBatch, OrnamentModality,
    RenderOrnamentContext, SurfaceOrn, SurfaceOrnAnchor, SurfaceOrnKind,
};
pub use state::{Plugin, PluginState};
pub use variable_store::PluginVariableStore;

bitflags! {
    /// Declares which Plugin trait methods a plugin actually implements.
    /// Used by PluginRuntime to skip WASM boundary crossings for non-participating plugins.
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub struct PluginCapabilities: u32 {
        const OVERLAY            = 1 << 2;
        const MENU_TRANSFORM     = 1 << 5;
        const INPUT_HANDLER      = 1 << 7;
        /// NOTE: SURFACE_PROVIDER is declarative metadata only. It is not used
        /// for dispatch gating in PluginRuntime — surface lifecycle is managed
        /// separately via the SurfaceRegistry.
        const SURFACE_PROVIDER   = 1 << 11;
        const WORKSPACE_OBSERVER = 1 << 12;
        const CONTRIBUTOR        = 1 << 14;
        const TRANSFORMER        = 1 << 15;
        const ANNOTATOR          = 1 << 16;
        const IO_HANDLER         = 1 << 17;
        const DISPLAY_TRANSFORM  = 1 << 18;
        const SCROLL_POLICY      = 1 << 19;
        const NAVIGATION_POLICY  = 1 << 21;
        const NAVIGATION_ACTION  = 1 << 22;
        const DROP_HANDLER       = 1 << 23;
        const RENDER_ORNAMENT    = 1 << 24;
        const CONTENT_ANNOTATOR  = 1 << 25;
    }
}

bitflags! {
    /// Host-resolved authority set for privileged plugin effects.
    ///
    /// Unlike [`PluginCapabilities`], these bits are a security boundary used
    /// by the event loop when executing deferred commands.
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub struct PluginAuthorities: u32 {
        const DYNAMIC_SURFACE = 1 << 0;
        const PTY_PROCESS     = 1 << 1;
        const WORKSPACE       = 1 << 2;
    }
}

/// Open slot identifier that supports both well-known and custom plugin-defined slots.
///
/// Well-known slots have `const` definitions matching the legacy `Slot` enum variants.
/// Plugins can define custom slots using arbitrary names (e.g., `SlotId::new("myplugin.sidebar")`).
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct SlotId(pub CompactString);

impl SlotId {
    pub const BUFFER_LEFT: Self = Self(CompactString::const_new("kasane.buffer.left"));
    pub const BUFFER_RIGHT: Self = Self(CompactString::const_new("kasane.buffer.right"));
    pub const ABOVE_BUFFER: Self = Self(CompactString::const_new("kasane.buffer.above"));
    pub const BELOW_BUFFER: Self = Self(CompactString::const_new("kasane.buffer.below"));
    pub const ABOVE_STATUS: Self = Self(CompactString::const_new("kasane.status.above"));
    pub const STATUS_LEFT: Self = Self(CompactString::const_new("kasane.status.left"));
    pub const STATUS_RIGHT: Self = Self(CompactString::const_new("kasane.status.right"));
    pub const OVERLAY: Self = Self(CompactString::const_new("kasane.overlay"));

    const WELL_KNOWN: [SlotId; 8] = [
        Self::BUFFER_LEFT,
        Self::BUFFER_RIGHT,
        Self::ABOVE_BUFFER,
        Self::BELOW_BUFFER,
        Self::ABOVE_STATUS,
        Self::STATUS_LEFT,
        Self::STATUS_RIGHT,
        Self::OVERLAY,
    ];

    /// Create a custom slot identifier.
    pub fn new(name: impl Into<CompactString>) -> Self {
        Self(name.into())
    }

    /// Get the slot name.
    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// Check if this is a well-known (built-in) slot.
    pub fn is_well_known(&self) -> bool {
        Self::WELL_KNOWN.contains(self)
    }

    /// Return the well-known slot index (0..8), or None for custom slots.
    pub fn well_known_index(&self) -> Option<usize> {
        Self::WELL_KNOWN.iter().position(|wk| wk == self)
    }
}

/// Scope of an annotation handler.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AnnotationScope {
    LeftGutter,
    RightGutter,
    Background,
    Inline,
    VirtualText,
}

/// Structural metadata describing a plugin's capabilities.
///
/// Complements [`PluginCapabilities`] bitflags with structured information
/// about which targets, slots, topics, and extension points a plugin interacts
/// with. Used for interference detection and dispatch optimization.
#[derive(Debug, Clone, Default)]
pub struct CapabilityDescriptor {
    pub transform_targets: Vec<TransformTarget>,
    pub contribution_slots: Vec<SlotId>,
    pub annotation_scopes: Vec<AnnotationScope>,
    pub publish_topics: Vec<TopicId>,
    pub subscribe_topics: Vec<TopicId>,
    pub extensions_defined: Vec<extension_point::ExtensionPointId>,
    pub extensions_consumed: Vec<extension_point::ExtensionPointId>,
}

impl CapabilityDescriptor {
    /// Check if this plugin may interfere with another.
    ///
    /// Interference is detected when:
    /// - Both plugins transform the same target
    /// - Both plugins contribute to the same slot
    /// - One publishes a topic the other subscribes to (coupling)
    pub fn may_interfere(&self, other: &Self) -> bool {
        // Transform target overlap
        if self
            .transform_targets
            .iter()
            .any(|t| other.transform_targets.contains(t))
        {
            return true;
        }
        // Contribution slot overlap
        if self
            .contribution_slots
            .iter()
            .any(|s| other.contribution_slots.contains(s))
        {
            return true;
        }
        // Pub/sub coupling: one publishes what the other subscribes
        if self
            .publish_topics
            .iter()
            .any(|t| other.subscribe_topics.contains(t))
        {
            return true;
        }
        if other
            .publish_topics
            .iter()
            .any(|t| self.subscribe_topics.contains(t))
        {
            return true;
        }
        false
    }
}
