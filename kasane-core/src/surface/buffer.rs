//! KakouneBufferSurface: built-in Surface for the Kakoune buffer area.
//!
//! Builds the buffer content (BufferRef + gutters + above/below slots) without
//! the status bar or overlays. Those are handled by separate surfaces
//! (StatusBarSurface, MenuSurface, InfoSurface).

use crate::element::Element;
use crate::plugin::Command;
use crate::state::{AppState, DirtyFlags};

use super::{
    EventContext, SizeHint, SlotDeclaration, SlotPosition, Surface, SurfaceEvent, SurfaceId,
    ViewContext,
};

/// Built-in surface that renders the Kakoune buffer content.
///
/// Delegates to `build_buffer_content()` which collects plugin slots
/// (buffer_left, buffer_right, above_buffer, below_buffer), builds gutters,
/// and assembles the buffer element tree.
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
                SlotDeclaration::new("kasane.buffer.left", SlotPosition::Left),
                SlotDeclaration::new("kasane.buffer.right", SlotPosition::Right),
                SlotDeclaration::new("kasane.buffer.above", SlotPosition::Before),
                SlotDeclaration::new("kasane.buffer.below", SlotPosition::After),
                SlotDeclaration::new("kasane.buffer.overlay", SlotPosition::Overlay),
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

    fn size_hint(&self) -> SizeHint {
        SizeHint::fill()
    }

    fn view(&self, ctx: &ViewContext<'_>) -> Element {
        crate::render::view::build_buffer_content(ctx.state, ctx.registry)
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
