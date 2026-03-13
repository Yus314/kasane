//! StatusBarSurface: built-in Surface for the status bar.
//!
//! Delegates to [`crate::render::view::build_status_bar_surface`] which collects
//! plugin slots (status_left, status_right, above_status), applies replacement
//! and decorator, and assembles the status bar element.

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
        crate::render::view::build_status_bar_surface(ctx.state, ctx.registry)
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
