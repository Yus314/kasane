//! KakouneBufferSurface: built-in Surface for the Kakoune buffer area.
//!
//! Builds the buffer content (BufferRef + gutters + above/below slots) without
//! the status bar or overlays. Those are handled by separate surfaces
//! (StatusBarSurface, MenuSurface, InfoSurface).

use crate::element::Element;
use crate::plugin::Command;
use crate::state::{AppState, DirtyFlags};
use compact_str::CompactString;

use super::{
    EventContext, SizeHint, SlotDeclaration, SlotKind, Surface, SurfaceEvent, SurfaceId,
    ViewContext,
};

/// Built-in surface that renders the Kakoune buffer content.
///
/// Delegates to `build_buffer_surface_abstract()` which builds the buffer core
/// and places named slot placeholders around it.
///
/// Declares the following extension slots:
/// - `kasane.buffer.left` — left gutter area
/// - `kasane.buffer.right` — right gutter area
/// - `kasane.buffer.above` — above the buffer
/// - `kasane.buffer.below` — below the buffer
/// - `kasane.buffer.overlay` — overlay on top of the buffer
pub struct KakouneBufferSurface {
    slots: Vec<SlotDeclaration>,
}

impl KakouneBufferSurface {
    pub fn new() -> Self {
        KakouneBufferSurface {
            slots: vec![
                SlotDeclaration::new("kasane.buffer.left", SlotKind::LeftRail),
                SlotDeclaration::new("kasane.buffer.right", SlotKind::RightRail),
                SlotDeclaration::new("kasane.buffer.above", SlotKind::AboveBand),
                SlotDeclaration::new("kasane.buffer.below", SlotKind::BelowBand),
                SlotDeclaration::new("kasane.buffer.overlay", SlotKind::Overlay),
            ],
        }
    }
}

impl Default for KakouneBufferSurface {
    fn default() -> Self {
        Self::new()
    }
}

impl Surface for KakouneBufferSurface {
    fn id(&self) -> SurfaceId {
        SurfaceId::BUFFER
    }

    fn surface_key(&self) -> CompactString {
        "kasane.buffer".into()
    }

    fn size_hint(&self) -> SizeHint {
        SizeHint::fill()
    }

    fn view(&self, ctx: &ViewContext<'_>) -> Element {
        crate::render::view::build_buffer_surface_abstract(ctx.state, ctx.registry)
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
