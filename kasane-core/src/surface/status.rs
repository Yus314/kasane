//! StatusBarSurface: built-in Surface for the status bar.
//!
//! Delegates to [`crate::render::view::build_status_surface_abstract`], which
//! returns the abstract status surface skeleton with named slot placeholders.

use crate::element::Element;
use crate::plugin::Command;
use crate::state::{AppState, DirtyFlags};
use compact_str::CompactString;

use super::{
    EventContext, SizeHint, SlotDeclaration, SlotKind, Surface, SurfaceEvent, SurfaceId,
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
                SlotDeclaration::new("kasane.status.above", SlotKind::AboveBand),
                SlotDeclaration::new("kasane.status.left", SlotKind::LeftRail),
                SlotDeclaration::new("kasane.status.right", SlotKind::RightRail),
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

    fn surface_key(&self) -> CompactString {
        "kasane.status".into()
    }

    fn size_hint(&self) -> SizeHint {
        SizeHint {
            min_width: 1,
            min_height: 1,
            preferred_width: None,
            preferred_height: None,
            flex: 0.0,
        }
    }

    fn view(&self, ctx: &ViewContext<'_>) -> Element {
        if ctx
            .state
            .runtime
            .suppressed_builtins
            .contains(&crate::plugin::BuiltinTarget::StatusBar)
        {
            return Element::Empty;
        }
        crate::render::view::build_status_surface_abstract(ctx.state, ctx.registry)
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
