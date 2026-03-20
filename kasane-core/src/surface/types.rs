use std::collections::HashMap;

use compact_str::CompactString;

use crate::input::KeyEvent;
use crate::input::MouseEvent;
use crate::layout::{Rect, SplitDirection};
use crate::plugin::{Command, PluginId, PluginRegistry};
use crate::state::AppState;
use crate::workspace::DockPosition;

use super::Surface;

/// Unique identifier for a surface within a [`Workspace`](crate::workspace::Workspace).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct SurfaceId(pub u32);

/// Well-known surface IDs for built-in core surfaces.
impl SurfaceId {
    /// The primary Kakoune buffer surface (always present).
    pub const BUFFER: SurfaceId = SurfaceId(0);
    /// The status bar surface (always present).
    pub const STATUS: SurfaceId = SurfaceId(1);
    /// The menu overlay surface (created/destroyed dynamically).
    pub const MENU: SurfaceId = SurfaceId(2);
    /// Base ID for info overlay surfaces. Info `i` uses `SurfaceId(INFO_BASE + i)`.
    pub const INFO_BASE: u32 = 10;
    /// First ID available for plugin-created surfaces.
    pub const PLUGIN_BASE: u32 = 100;
}

/// Size preferences for layout negotiation.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SizeHint {
    pub min_width: u16,
    pub min_height: u16,
    pub preferred_width: Option<u16>,
    pub preferred_height: Option<u16>,
    /// Flex factor for proportional space allocation (0.0 = fixed, >0.0 = flexible).
    pub flex: f32,
}

impl SizeHint {
    /// Fixed-size surface.
    pub fn fixed(w: u16, h: u16) -> Self {
        SizeHint {
            min_width: w,
            min_height: h,
            preferred_width: Some(w),
            preferred_height: Some(h),
            flex: 0.0,
        }
    }

    /// Surface that fills all available space.
    pub fn fill() -> Self {
        SizeHint {
            min_width: 1,
            min_height: 1,
            preferred_width: None,
            preferred_height: None,
            flex: 1.0,
        }
    }

    /// Fixed height, flexible width.
    pub fn fixed_height(h: u16) -> Self {
        SizeHint {
            min_width: 1,
            min_height: h,
            preferred_width: None,
            preferred_height: Some(h),
            flex: 0.0,
        }
    }
}

impl Default for SizeHint {
    fn default() -> Self {
        SizeHint::fill()
    }
}

/// Context provided to a Surface when building its view.
pub struct ViewContext<'a> {
    /// Pane-specific application state (buffer, cursor, mode, status).
    ///
    /// In multi-client mode, each pane has its own AppState from its
    /// independent Kakoune client connection. In single-pane mode this
    /// is the same as `global_state`.
    pub state: &'a AppState,
    /// Global application state (always the focused pane's / primary state).
    ///
    /// Use this for screen dimensions, configuration, and other global settings.
    /// In single-pane mode this is identical to `state`.
    pub global_state: &'a AppState,
    /// The rectangular area allocated to this surface.
    pub rect: Rect,
    /// Whether this surface currently has focus.
    pub focused: bool,
    /// Plugin registry for collecting slot contributions.
    pub registry: &'a PluginRegistry,
    /// This surface's identifier.
    pub surface_id: SurfaceId,
}

/// Context provided to a Surface when handling events.
pub struct EventContext<'a> {
    /// Read-only application state.
    pub state: &'a AppState,
    /// The rectangular area allocated to this surface.
    pub rect: Rect,
    /// Whether this surface currently has focus.
    pub focused: bool,
}

/// Events delivered to a Surface.
#[derive(Debug)]
pub enum SurfaceEvent {
    /// A key event (routed to the focused surface).
    Key(KeyEvent),
    /// A mouse event (routed by hit testing).
    Mouse(MouseEvent),
    /// This surface gained focus.
    FocusGained,
    /// This surface lost focus.
    FocusLost,
    /// This surface was resized.
    Resize(Rect),
}

/// Advisory kind for a surface slot.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SlotKind {
    /// A band above the surface's main content.
    AboveBand,
    /// A band below the surface's main content.
    BelowBand,
    /// A rail on the left side of the surface.
    LeftRail,
    /// A rail on the right side of the surface.
    RightRail,
    /// An overlay slot layered on top of the surface.
    Overlay,
}

/// Static placement request for a surface descriptor.
///
/// Unlike [`Placement`], keyed placements refer to target surfaces by stable
/// `surface_key`, so they can be declared before runtime `SurfaceId`s exist.
#[derive(Debug, Clone, PartialEq)]
pub enum SurfacePlacementRequest {
    SplitFocused {
        direction: SplitDirection,
        ratio: f32,
    },
    SplitFrom {
        target_surface_key: CompactString,
        direction: SplitDirection,
        ratio: f32,
    },
    Tab,
    TabIn {
        target_surface_key: CompactString,
    },
    Dock(DockPosition),
    Float {
        rect: Rect,
    },
}

/// Declaration of an extension point (slot) within a Surface.
#[derive(Debug, Clone)]
pub struct SlotDeclaration {
    /// Fully-qualified slot name (e.g., "kasane.buffer.left").
    pub name: CompactString,
    /// Advisory kind for documentation and discovery.
    pub kind: SlotKind,
}

impl SlotDeclaration {
    pub fn new(name: impl Into<CompactString>, kind: SlotKind) -> Self {
        SlotDeclaration {
            name: name.into(),
            kind,
        }
    }
}

/// Registration-time descriptor for a surface's static contract.
#[derive(Debug, Clone)]
pub struct SurfaceDescriptor {
    pub surface_id: SurfaceId,
    pub surface_key: CompactString,
    pub declared_slots: Vec<SlotDeclaration>,
    pub initial_placement: Option<SurfacePlacementRequest>,
    declared_slot_lookup: HashMap<CompactString, usize>,
}

impl SurfaceDescriptor {
    pub(crate) fn from_surface(surface: &dyn Surface) -> Result<Self, SurfaceRegistrationError> {
        let surface_key = surface.surface_key();
        let declared_slots = surface.declared_slots().to_vec();
        let mut declared_slot_lookup = HashMap::new();
        for (index, slot) in declared_slots.iter().enumerate() {
            if declared_slot_lookup
                .insert(slot.name.clone(), index)
                .is_some()
            {
                return Err(SurfaceRegistrationError::DuplicateDeclaredSlotInSurface {
                    surface_key,
                    slot_name: slot.name.clone(),
                });
            }
        }
        Ok(Self {
            surface_id: surface.id(),
            surface_key,
            declared_slots,
            initial_placement: surface.initial_placement(),
            declared_slot_lookup,
        })
    }

    pub fn declares_slot(&self, slot_name: &str) -> bool {
        self.declared_slot_lookup.contains_key(slot_name)
    }

    pub fn declared_slot(&self, slot_name: &str) -> Option<&SlotDeclaration> {
        self.declared_slot_lookup
            .get(slot_name)
            .and_then(|index| self.declared_slots.get(*index))
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SurfaceRegistrationError {
    DuplicateSurfaceId {
        surface_id: SurfaceId,
        existing_surface_key: CompactString,
        new_surface_key: CompactString,
    },
    DuplicateSurfaceKey {
        surface_key: CompactString,
    },
    DuplicateDeclaredSlot {
        slot_name: CompactString,
        existing_surface_key: CompactString,
        new_surface_key: CompactString,
    },
    DuplicateDeclaredSlotInSurface {
        surface_key: CompactString,
        slot_name: CompactString,
    },
}

pub struct SourcedSurfaceCommands {
    pub source_plugin: Option<PluginId>,
    pub commands: Vec<Command>,
}

impl std::fmt::Debug for SourcedSurfaceCommands {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SourcedSurfaceCommands")
            .field("source_plugin", &self.source_plugin)
            .field("commands_len", &self.commands.len())
            .finish()
    }
}
