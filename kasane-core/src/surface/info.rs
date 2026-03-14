//! InfoSurface: built-in Surface for info popup overlays.
//!
//! Each info popup (`AppState::infos[i]`) is represented as a separate Surface
//! with ID `SurfaceId(SurfaceId::INFO_BASE + i)`. These surfaces are created
//! and destroyed dynamically as infos appear and disappear.

use crate::element::Element;
use crate::plugin::Command;
use crate::state::{AppState, DirtyFlags};

use super::{EventContext, SizeHint, Surface, SurfaceEvent, SurfaceId, ViewContext};

/// Built-in surface for a single info popup overlay.
///
/// The `index` field identifies which `AppState::infos[index]` this surface renders.
pub struct InfoSurface {
    index: usize,
}

impl InfoSurface {
    pub fn new(index: usize) -> Self {
        InfoSurface { index }
    }
}

impl Surface for InfoSurface {
    fn id(&self) -> SurfaceId {
        SurfaceId(SurfaceId::INFO_BASE + self.index as u32)
    }

    fn size_hint(&self) -> SizeHint {
        SizeHint::fill()
    }

    fn view(&self, ctx: &ViewContext<'_>) -> Element {
        if let Some(info_state) = ctx.state.infos.get(self.index) {
            use crate::element::OverlayAnchor;
            use crate::protocol::InfoStyle;
            use crate::render::view::info;

            // Build avoid rects (menu + cursor + previous infos)
            let menu_rect = crate::layout::get_menu_rect(ctx.state);
            let mut avoid_rects: Vec<crate::layout::Rect> = Vec::new();
            if let Some(mr) = menu_rect {
                avoid_rects.push(mr);
            }
            avoid_rects.push(crate::layout::Rect {
                x: ctx.state.cursor_pos.column as u16,
                y: ctx.state.cursor_pos.line as u16,
                w: 1,
                h: 1,
            });
            // Add rects from prior infos for collision avoidance
            for prior in ctx.state.infos.iter().take(self.index) {
                if let Some(overlay) = info::build_info_overlay_indexed(
                    prior,
                    ctx.state,
                    &avoid_rects,
                    0, // index doesn't affect rect computation
                ) && let OverlayAnchor::Absolute { x, y, w, h } = &overlay.anchor
                {
                    avoid_rects.push(crate::layout::Rect {
                        x: *x,
                        y: *y,
                        w: *w,
                        h: *h,
                    });
                }
            }

            // Build default; apply_transform_chain handles replacement internally.
            let info_overlay =
                info::build_info_overlay_indexed(info_state, ctx.state, &avoid_rects, self.index);
            match info_overlay {
                Some(mut overlay) => {
                    use crate::plugin::TransformTarget;
                    overlay.element = ctx.registry.apply_transform_chain(
                        TransformTarget::Info,
                        || overlay.element.clone(),
                        ctx.state,
                    );
                    if let Some(tt) = match info_state.style {
                        InfoStyle::Prompt => Some(TransformTarget::InfoPrompt),
                        InfoStyle::Modal => Some(TransformTarget::InfoModal),
                        _ => None,
                    } {
                        overlay.element = ctx.registry.apply_transform_chain(
                            tt,
                            || overlay.element.clone(),
                            ctx.state,
                        );
                    }
                    overlay.element
                }
                None => Element::Empty,
            }
        } else {
            Element::Empty
        }
    }

    fn handle_event(&mut self, _event: SurfaceEvent, _ctx: &EventContext<'_>) -> Vec<Command> {
        vec![]
    }

    fn on_state_changed(&mut self, _state: &AppState, _dirty: DirtyFlags) -> Vec<Command> {
        vec![]
    }
}
