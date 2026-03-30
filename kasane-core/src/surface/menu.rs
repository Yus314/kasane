//! MenuSurface: built-in Surface for the completion menu overlay.
//!
//! Wraps the existing menu rendering logic from `render::view::menu` as a
//! first-class Surface. Created dynamically when a menu appears and removed
//! when it disappears.
//!
//! NOTE: The menu rendering delegates to `render::view::menu::build_menu_overlay`.
//! A parallel pure implementation exists in `salsa_views::menu::pure_menu_overlay`
//! for the Salsa pipeline. Future consolidation should extract shared helpers.

use crate::element::Element;
use crate::plugin::{AppView, Command, TransformSubject};
use crate::state::{AppState, DirtyFlags};
use compact_str::CompactString;

use super::{EventContext, SizeHint, Surface, SurfaceEvent, SurfaceId, ViewContext};

/// Built-in surface for the completion menu overlay.
///
/// This surface is ephemeral — it is registered in the SurfaceRegistry when
/// `AppState::menu` becomes `Some` and removed when it becomes `None`.
pub struct MenuSurface;

impl Surface for MenuSurface {
    fn id(&self) -> SurfaceId {
        SurfaceId::MENU
    }

    fn surface_key(&self) -> CompactString {
        "kasane.menu".into()
    }

    fn size_hint(&self) -> SizeHint {
        // Menus are overlays — size is determined by content and anchor position
        SizeHint::fill()
    }

    fn view(&self, ctx: &ViewContext<'_>) -> Element {
        // Delegate to the existing menu rendering pipeline.
        // The actual overlay positioning is handled by the Overlay/OverlayAnchor
        // system in the view layer, not by Surface layout.
        if let Some(menu_state) = ctx.state.menu.as_ref() {
            use crate::plugin::TransformTarget;
            use crate::protocol::MenuStyle;
            use crate::render::view::menu;

            let transform_target = match menu_state.style {
                MenuStyle::Prompt => TransformTarget::MENU_PROMPT,
                MenuStyle::Inline => TransformTarget::MENU_INLINE,
                MenuStyle::Search => TransformTarget::MENU_SEARCH,
            };
            // Build default; apply hierarchical transform chain (Menu → style-specific).
            let menu_overlay = menu::build_menu_overlay(menu_state, ctx.state, ctx.registry);
            match menu_overlay {
                Some(overlay) => {
                    let app_view = AppView::new(ctx.state);
                    ctx.registry
                        .apply_transform_chain_hierarchical(
                            transform_target,
                            TransformSubject::Overlay(overlay),
                            &app_view,
                        )
                        .into_element()
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
