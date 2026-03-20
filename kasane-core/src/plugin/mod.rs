//! Plugin infrastructure: `Plugin` trait, `PluginBackend` trait, registry, context, command, I/O.

pub mod bridge;
mod command;
mod context;
mod effects;
pub mod io;
mod registry;
pub mod state;
mod traits;

#[cfg(test)]
mod tests;

pub use crate::session::SessionCommand;
use bitflags::bitflags;
use compact_str::CompactString;

// Re-export command module
pub use command::{
    Command, CommandResult, DeferredCommand, PaintHook, execute_commands,
    extract_deferred_commands, extract_redraw_flags,
};
pub use effects::{
    BootstrapEffects, InitBatch, ReadyBatch, RuntimeBatch, RuntimeEffects, SessionReadyCommand,
    SessionReadyEffects,
};

// Re-export io module types
pub use io::{
    IoEvent, NullProcessDispatcher, ProcessDispatcher, ProcessEvent, ProcessEventSink, StdinMode,
};

// Re-export context module
pub use context::{
    AnnotateContext, AnnotationResult, BackgroundLayer, BlendMode, ContribSizeHint,
    ContributeContext, Contribution, LineAnnotation, OverlayContext, OverlayContribution,
    SourcedContribution, TransformContext, TransformTarget,
};

// Re-export registry module
pub use registry::{PluginRegistry, PluginSurfaceSet};

// Re-export display types for plugin API
pub use crate::display::{DisplayDirective, DisplayMapRef};

// Re-export traits module
pub use traits::PluginBackend;

// Re-export state and bridge modules
pub use bridge::{IsBridgedPlugin, PluginBridge};
pub use state::{Plugin, PluginState};

bitflags! {
    /// Declares which Plugin trait methods a plugin actually implements.
    /// Used by PluginRegistry to skip WASM boundary crossings for non-participating plugins.
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub struct PluginCapabilities: u32 {
        const OVERLAY            = 1 << 2;
        const MENU_TRANSFORM     = 1 << 5;
        const CURSOR_STYLE       = 1 << 6;
        const INPUT_HANDLER      = 1 << 7;
        const SURFACE_PROVIDER   = 1 << 11;
        const WORKSPACE_OBSERVER = 1 << 12;
        const PAINT_HOOK         = 1 << 13;
        const CONTRIBUTOR        = 1 << 14;
        const TRANSFORMER        = 1 << 15;
        const ANNOTATOR          = 1 << 16;
        const IO_HANDLER         = 1 << 17;
        const DISPLAY_TRANSFORM  = 1 << 18;
        const SCROLL_POLICY      = 1 << 19;
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct PluginId(pub String);

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
