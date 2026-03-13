//! KakouneBufferSurface: built-in Surface for the Kakoune buffer area.
//!
//! Delegates to [`crate::render::view::view_cached`] to produce the full
//! Element tree (buffer + status bar + overlays). This guarantees pixel-identical
//! output with the non-Surface rendering path.

use std::cell::RefCell;

use crate::element::Element;
use crate::plugin::Command;
use crate::render::ViewCache;
use crate::state::{AppState, DirtyFlags};

use super::{
    EventContext, SizeHint, SlotDeclaration, SlotPosition, Surface, SurfaceEvent, SurfaceId,
    ViewContext,
};

/// Built-in surface that renders the full Kakoune UI (buffer + status bar).
///
/// Internally delegates to `view_cached()` so that the existing rendering
/// optimizations (ViewCache, section memoization) are preserved.
///
/// Declares the following extension slots:
/// - `kasane.buffer.left` — left gutter area
/// - `kasane.buffer.right` — right gutter area
/// - `kasane.buffer.above` — above the buffer
/// - `kasane.buffer.below` — below the buffer
/// - `kasane.buffer.overlay` — overlay on top of the buffer
pub struct KakouneBufferSurface {
    slots: Vec<SlotDeclaration>,
    view_cache: RefCell<ViewCache>,
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
            view_cache: RefCell::new(ViewCache::new()),
        }
    }

    /// Invalidate internal caches based on dirty flags.
    pub fn invalidate(&self, dirty: DirtyFlags) {
        self.view_cache.borrow_mut().invalidate(dirty);
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
        crate::render::view::view_cached(ctx.state, ctx.registry, &mut self.view_cache.borrow_mut())
    }

    fn handle_event(&mut self, _event: SurfaceEvent, _ctx: &EventContext<'_>) -> Vec<Command> {
        vec![]
    }

    fn on_state_changed(&mut self, _state: &AppState, _dirty: DirtyFlags) -> Vec<Command> {
        self.invalidate(_dirty);
        vec![]
    }

    fn declared_slots(&self) -> &[SlotDeclaration] {
        &self.slots
    }
}
