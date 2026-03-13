//! StatusBarSurface: built-in Surface for the status bar.
//!
//! Phase S2 will implement the full Surface trait here.
//! For now this module declares the type so that the crate compiles.

use crate::element::Element;
use crate::plugin::Command;
use crate::state::{AppState, DirtyFlags};

use super::{
    EventContext, SizeHint, SlotDeclaration, SlotPosition, Surface, SurfaceEvent, SurfaceId,
    ViewContext,
};

/// Built-in surface that renders the Kakoune status bar.
///
/// Declares the following extension slots:
/// - `kasane.status.above` — above the status bar
/// - `kasane.status.left` — left side of the status bar
/// - `kasane.status.right` — right side of the status bar
pub struct StatusBarSurface {
    slots: Vec<SlotDeclaration>,
}

impl StatusBarSurface {
    pub fn new() -> Self {
        StatusBarSurface {
            slots: vec![
                SlotDeclaration::new("kasane.status.above", SlotPosition::Before),
                SlotDeclaration::new("kasane.status.left", SlotPosition::Left),
                SlotDeclaration::new("kasane.status.right", SlotPosition::Right),
            ],
        }
    }
}

impl Default for StatusBarSurface {
    fn default() -> Self {
        Self::new()
    }
}

impl Surface for StatusBarSurface {
    fn id(&self) -> SurfaceId {
        SurfaceId::STATUS
    }

    fn size_hint(&self) -> SizeHint {
        SizeHint::fixed_height(1)
    }

    fn view(&self, ctx: &ViewContext<'_>) -> Element {
        // Phase S2: will replicate build_status_bar() logic here.
        let _ = ctx;
        Element::Empty
    }

    fn handle_event(&mut self, _event: SurfaceEvent, _ctx: &EventContext<'_>) -> Vec<Command> {
        vec![]
    }

    fn on_state_changed(&mut self, _state: &AppState, _dirty: DirtyFlags) -> Vec<Command> {
        vec![]
    }

    fn declared_slots(&self) -> &[SlotDeclaration] {
        &self.slots
    }
}
