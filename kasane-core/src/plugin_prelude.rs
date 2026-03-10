//! Prelude for plugin authors.
//!
//! ```ignore
//! use kasane_core::plugin_prelude::*;
//! ```

pub use crate::kasane_plugin;

// Plugin trait and types
pub use crate::plugin::{
    Command, DecorateTarget, LineDecoration, Plugin, PluginId, PluginRegistry, ReplaceTarget, Slot,
};

// Element tree
pub use crate::element::{Element, FlexChild, InteractiveId, Overlay, OverlayAnchor};

// Protocol types
pub use crate::protocol::{Atom, Color, Coord, Face, Line, NamedColor};

// State
pub use crate::state::{AppState, DirtyFlags};

// Input
pub use crate::input::{Key, KeyEvent, Modifiers, MouseButton, MouseEvent, MouseEventKind};
