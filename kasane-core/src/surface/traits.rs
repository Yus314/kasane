use std::any::Any;

use compact_str::CompactString;

use crate::element::Element;
use crate::input::KeyEvent;
use crate::plugin::Command;
use crate::state::{AppState, DirtyFlags};

use super::{
    EventContext, SizeHint, SlotDeclaration, SurfaceEvent, SurfaceId, SurfacePlacementRequest,
    ViewContext,
};

/// A rectangular screen region that can build its own Element tree and handle events.
///
/// Both core components and plugins implement this trait, enabling symmetric
/// extensibility. The core Kakoune buffer view is just one Surface among equals.
pub trait Surface: Any + Send {
    /// Unique identifier for this surface.
    fn id(&self) -> SurfaceId;

    /// Stable semantic key for this surface.
    fn surface_key(&self) -> CompactString;

    /// Size preferences for layout negotiation.
    fn size_hint(&self) -> SizeHint;

    /// Static initial placement request for this surface.
    fn initial_placement(&self) -> Option<SurfacePlacementRequest> {
        None
    }

    /// Build the Element tree for the allocated rectangle.
    fn view(&self, ctx: &ViewContext<'_>) -> Element;

    /// Handle an event within this surface's region.
    fn handle_event(&mut self, event: SurfaceEvent, ctx: &EventContext<'_>) -> Vec<Command>;

    /// Handle a key event while this surface is focused.
    ///
    /// Return `Some(commands)` to consume the key, even when `commands`
    /// is empty. Return `None` to pass the key through to the normal
    /// editor/plugin key pipeline.
    fn handle_key_input(
        &mut self,
        _key: &KeyEvent,
        _ctx: &EventContext<'_>,
    ) -> Option<Vec<Command>> {
        None
    }

    /// Handle committed text input while this surface is focused.
    ///
    /// Return `Some(commands)` to consume the text input, even when `commands`
    /// is empty. Return `None` to pass the text input through to the normal
    /// editor/plugin input pipeline.
    fn handle_text_input(&mut self, _text: &str, _ctx: &EventContext<'_>) -> Option<Vec<Command>> {
        None
    }

    /// Notification that shared application state has changed.
    fn on_state_changed(&mut self, _state: &AppState, _dirty: DirtyFlags) -> Vec<Command> {
        vec![]
    }

    /// Hash of surface-internal state for view caching.
    /// A change in this value invalidates the cached view output.
    fn state_hash(&self) -> u64 {
        0
    }

    /// Extension points (slots) that this surface exposes to plugins.
    fn declared_slots(&self) -> &[SlotDeclaration] {
        &[]
    }
}
