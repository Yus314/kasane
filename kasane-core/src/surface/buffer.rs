//! KakouneBufferSurface: built-in Surface for the Kakoune buffer area.
//!
//! Builds the buffer content (BufferRef + gutters + above/below slots) without
//! the status bar or overlays. Those are handled by separate surfaces
//! (StatusBarSurface, MenuSurface, InfoSurface).

use crate::element::{BufferRefState, Element, FlexChild};
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

/// Client buffer surface for multi-pane split views.
///
/// Each instance is backed by an independent Kakoune client connection.
/// Has a distinct SurfaceId and no declared slots — builds buffer content
/// directly via `build_buffer_core_parts()` to avoid SlotPlaceholder
/// resolution errors.
///
/// When rendered with a pane-specific `AppState` via `PaneStates`, each
/// instance displays its own buffer, cursor, mode, and status independently.
pub struct ClientBufferSurface {
    surface_id: SurfaceId,
}

/// Backward-compatible alias for `ClientBufferSurface`.
#[deprecated(note = "use `ClientBufferSurface` directly")]
pub type MirrorBufferSurface = ClientBufferSurface;

impl ClientBufferSurface {
    pub fn new(surface_id: SurfaceId) -> Self {
        ClientBufferSurface { surface_id }
    }
}

/// Embed pane-specific AppState data into a BufferRef element for multi-pane rendering.
fn embed_buffer_state(element: Element, state: &AppState) -> Element {
    match element {
        Element::BufferRef {
            line_range,
            line_backgrounds,
            display_map,
            inline_decorations,
            ..
        } => Element::BufferRef {
            line_range,
            line_backgrounds,
            display_map,
            state: Some(Box::new(BufferRefState {
                lines: state.lines.clone(),
                lines_dirty: state.lines_dirty.clone(),
                default_face: state.default_face,
                padding_face: state.padding_face,
                padding_char: state.padding_char.clone(),
            })),
            inline_decorations,
        },
        // If a transform wrapped the BufferRef, pass through unchanged
        other => other,
    }
}

impl Surface for ClientBufferSurface {
    fn id(&self) -> SurfaceId {
        self.surface_id
    }

    fn surface_key(&self) -> CompactString {
        CompactString::new(format!("kasane.buffer.client.{}", self.surface_id.0))
    }

    fn size_hint(&self) -> SizeHint {
        SizeHint::fill()
    }

    fn view(&self, ctx: &ViewContext<'_>) -> Element {
        let parts = crate::render::view::build_buffer_core_parts(ctx.state, ctx.registry);
        // Embed pane state into BufferRef for multi-pane rendering
        let buffer = embed_buffer_state(parts.buffer, ctx.state);
        let mut row_children = Vec::new();
        if let Some(left_gutter) = parts.left_gutter {
            row_children.push(FlexChild::fixed(left_gutter));
        }
        row_children.push(FlexChild::flexible(buffer, 1.0));
        if let Some(right_gutter) = parts.right_gutter {
            row_children.push(FlexChild::fixed(right_gutter));
        }
        Element::row(row_children)
    }

    fn handle_event(&mut self, _event: SurfaceEvent, _ctx: &EventContext<'_>) -> Vec<Command> {
        vec![]
    }
}
